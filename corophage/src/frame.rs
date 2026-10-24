//! Frame transport between a computation body and the runner that owns it.
//!
//! Each run owns a pinned [`Frame`] with a unique id and one effect/resume slot.
//! While the owner polls the body, the frame is registered in a thread-local
//! list. A [`Yielder`](crate::Yielder) stores only the frame id and finds its
//! frame in that list, so effect operations work inside any combinator that is
//! polled within the body's own poll.
//!
//! The unsafe code relies on these invariants:
//!
//! - **I1 Registration.** A frame is in the list only while its owner is inside
//!   the call that polls the frame's body, on the thread making that call. A
//!   guard removes it on every exit path, and aborts unless it is the head.
//! - **I2 Unique ids.** A frame takes its id at its first registration, and
//!   ids are never reused, so a stale id matches no frame.
//! - **I3 Type correspondence.** A `Yielder<'a, Effs>` only holds the id of a
//!   `Frame<Effs, Resumes<'a, Effs>>`. A body takes its id from its owner's
//!   frame at its first poll, and yielders, coroutines, and programs are
//!   invariant in `'a` and `Effs`, so neither side can change these types.
//! - **I4 Exclusive slot access.** The owner touches the slot only while the
//!   body is not being polled; operations touch it only during their own poll
//!   or drop, inside the owner's poll. No reference into the slot outlives one
//!   access, frames are only reached through shared references, and a slot
//!   value is always moved out before it is dropped.
//! - **I5 Drop order.** A generator drops its body before its frame.

use std::cell::{Cell, UnsafeCell};
use std::future::Future;
use std::marker::{PhantomData, PhantomPinned};
use std::num::NonZeroU64;
use std::pin::Pin;
use std::ptr;
use std::task::{Context, Poll, Waker};

use crate::effect::{MapResume, Resumes};

/// A zero-sized marker that is invariant in `'a` and `Effs`, and `Send + Sync`.
pub(crate) type Invariant<'a, Effs> = PhantomData<fn(&'a (), Effs) -> (&'a (), Effs)>;

/// Identifies one frame for the lifetime of the process (I2).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameId(NonZeroU64);

/// Each thread reserves ids in blocks, so most runs need no atomic operation.
const ID_BLOCK: u64 = 1024;

thread_local! {
    // The next id of this thread's block, and the end of the block.
    static IDS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
    // The most recently registered frame. Each header links to the previous one.
    static FRAMES: Cell<*const FrameHeader> = const { Cell::new(ptr::null()) };
}

impl FrameId {
    fn fresh() -> Self {
        IDS.with(|ids| {
            let (mut next, mut end) = ids.get();
            if next == end {
                next = reserve_block();
                end = next + ID_BLOCK;
            }
            ids.set((next + 1, end));
            Self(NonZeroU64::new(next).expect("frame ids start at one"))
        })
    }
}

/// Reserve the next block of ids. The counter never wraps around (I2).
#[cfg(target_has_atomic = "64")]
fn reserve_block() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut start = NEXT.load(Ordering::Relaxed);
    loop {
        let end = start.checked_add(ID_BLOCK).expect("frame ids exhausted");
        match NEXT.compare_exchange_weak(start, end, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return start,
            Err(current) => start = current,
        }
    }
}

#[cfg(not(target_has_atomic = "64"))]
fn reserve_block() -> u64 {
    static NEXT: std::sync::Mutex<u64> = std::sync::Mutex::new(1);

    let mut next = NEXT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let start = *next;
    *next = start.checked_add(ID_BLOCK).expect("frame ids exhausted");
    start
}

/// The untyped part of a frame, shared by every frame type.
#[repr(C)]
struct FrameHeader {
    // Assigned at first registration. Constructing a frame then has no side
    // effects, so the compiler can elide moves of the large unstarted body
    // that surrounds it, for example in deep invocation chains.
    id: Cell<Option<FrameId>>,
    prev: Cell<*const FrameHeader>,
    // Set until the body takes its yielder.
    starting: Cell<bool>,
    // The owner's waker during the current poll.
    waker: Cell<*const Waker>,
}

// SAFETY: The pointers are only dereferenced while the frame is registered, on
// the thread that registered it (I1).
unsafe impl Send for FrameHeader {}

enum Slot<Y, A> {
    Empty,
    // Written by an operation, taken by the owner.
    Yielded(Y),
    // Written by the owner, taken by the operation.
    Resumed(A),
}

/// The header comes first, so a header pointer can be cast back to the whole
/// frame once its id matches (I3).
#[repr(C)]
struct Frame<Y, A> {
    header: FrameHeader,
    slot: UnsafeCell<Slot<Y, A>>,
    _pinned: PhantomPinned,
}

impl<Y, A> Frame<Y, A> {
    fn new() -> Self {
        Self {
            header: FrameHeader {
                id: Cell::new(None),
                prev: Cell::new(ptr::null()),
                starting: Cell::new(true),
                waker: Cell::new(ptr::null()),
            },
            slot: UnsafeCell::new(Slot::Empty),
            _pinned: PhantomPinned,
        }
    }

    fn replace(&self, slot: Slot<Y, A>) -> Slot<Y, A> {
        // SAFETY: Only one thread reaches the frame at a time, and this access
        // creates no reference into the slot. The old value is moved out, so
        // its destructor cannot run while the slot is being accessed (I4).
        unsafe { ptr::replace(self.slot.get(), slot) }
    }

    fn is_empty(&self) -> bool {
        // SAFETY: As in `replace`; this only reads the discriminant.
        unsafe { matches!(*self.slot.get(), Slot::Empty) }
    }

    fn register<'f>(&'f self, waker: &'f Waker) -> Registration<'f> {
        let header = &self.header;
        if header.id.get().is_none() {
            header.id.set(Some(FrameId::fresh()));
        }
        header.waker.set(waker);
        // Derive the pointer from the whole frame, so it can be cast back.
        let frame = ptr::from_ref(self).cast::<FrameHeader>();
        FRAMES.with(|frames| header.prev.set(frames.replace(frame)));
        Registration { header }
    }

    fn wake_unless_owner(&self, cx: &Context<'_>) {
        // SAFETY: The frame is registered, so its owner's waker is alive (I1).
        let owner = unsafe { &*self.header.waker.get() };
        // A combinator that re-polls only woken children needs a wake-up to
        // poll this operation again once the owner resumes it.
        if !cx.waker().will_wake(owner) {
            cx.waker().wake_by_ref();
        }
    }
}

/// Keeps a frame registered while its owner polls the body (I1).
struct Registration<'f> {
    header: &'f FrameHeader,
}

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        FRAMES.with(|frames| {
            if !ptr::eq(frames.get(), self.header) {
                out_of_order();
            }
            frames.set(self.header.prev.get());
        });
    }
}

/// Continuing would leave a dead frame registered, so abort with a message.
#[cold]
fn out_of_order() -> ! {
    struct Abort;
    impl Drop for Abort {
        fn drop(&mut self) {
            std::process::abort();
        }
    }
    let _abort = Abort;
    panic!("corophage frames were unregistered out of order");
}

/// Run `f` on the registered frame with this id, if there is one.
///
/// # Safety
///
/// A frame with this id must have type `Frame<Y, A>` (I3).
unsafe fn with_frame<Y, A, R>(id: FrameId, f: impl FnOnce(&Frame<Y, A>) -> R) -> Option<R> {
    let mut header = FRAMES.with(Cell::get);
    // SAFETY: Registered frames are alive (I1).
    while let Some(current) = unsafe { header.as_ref() } {
        if current.id.get() == Some(id) {
            // SAFETY: The pointer was derived from the whole frame, whose type
            // the caller guarantees.
            return Some(f(unsafe { &*header.cast::<Frame<Y, A>>() }));
        }
        header = current.prev.get();
    }
    None
}

/// Take the id of the frame whose owner polls a body for the first time.
///
/// Only the owner polls a body, so its frame is the most recently registered.
pub(crate) fn start() -> FrameId {
    starting(true)
}

/// The id of the frame whose owner polls a body for the first time. A wrapper
/// around the body keeps the frame starting, so the body can take its yielder.
fn starting(start: bool) -> FrameId {
    let header = FRAMES.with(Cell::get);
    // SAFETY: Registered frames are alive (I1).
    match unsafe { header.as_ref() } {
        Some(header) if header.starting.get() => {
            header.starting.set(!start);
            header.id.get().expect("registered frames have an id")
        }
        _ => panic!("a computation body was polled outside its runner"),
    }
}

#[cold]
fn not_registered() -> ! {
    panic!(
        "effect operation polled outside its computation; a yielder cannot leave its \
         computation, and its operations cannot run on another task or thread"
    )
}

/// One effect operation on a frame: it hands an effect to the frame's owner,
/// then waits for the resume.
///
/// Dropping the operation while it waits withdraws its effect, or discards its
/// resume if the handler already ran.
pub(crate) struct Operation<'a, Effs: MapResume> {
    frame: FrameId,
    waiting: bool,
    marker: Invariant<'a, Effs>,
}

impl<'a, Effs: MapResume> Operation<'a, Effs> {
    /// # Safety
    ///
    /// `frame` must be the id of a `Frame<Effs, Resumes<'a, Effs>>` (I3).
    pub(crate) unsafe fn new(frame: FrameId) -> Self {
        Self {
            frame,
            waiting: false,
            marker: PhantomData,
        }
    }

    pub(crate) fn is_waiting(&self) -> bool {
        self.waiting
    }

    /// Hand an effect to the frame's owner. The caller must then return
    /// `Pending` up to the owner.
    pub(crate) fn yield_(&mut self, cx: &Context<'_>, effect: Effs) {
        // SAFETY: The constructor's caller guarantees the frame type.
        let found = unsafe {
            with_frame::<Effs, Resumes<'a, Effs>, _>(self.frame, |frame| {
                if !frame.is_empty() {
                    panic!("another effect operation of this computation is in flight");
                }
                frame.replace(Slot::Yielded(effect));
                self.waiting = true;
                frame.wake_unless_owner(cx);
            })
        };
        found.unwrap_or_else(|| not_registered())
    }

    /// Take the resume of the yielded effect once the owner provides it.
    pub(crate) fn poll_resume(&mut self, cx: &Context<'_>) -> Poll<Resumes<'a, Effs>> {
        if !self.waiting {
            panic!("effect operation polled after completion");
        }
        // SAFETY: As in `yield_`.
        let poll = unsafe {
            with_frame::<Effs, Resumes<'a, Effs>, _>(self.frame, |frame| {
                match frame.replace(Slot::Empty) {
                    Slot::Resumed(value) => {
                        self.waiting = false;
                        Poll::Ready(value)
                    }
                    // Polled again before the owner took the effect.
                    yielded @ Slot::Yielded(_) => {
                        frame.replace(yielded);
                        frame.wake_unless_owner(cx);
                        Poll::Pending
                    }
                    Slot::Empty => panic!("effect operation lost its pending effect"),
                }
            })
        };
        poll.unwrap_or_else(|| not_registered())
    }
}

impl<'a, Effs: MapResume> Drop for Operation<'a, Effs> {
    fn drop(&mut self) {
        if self.waiting {
            // At most one operation per frame is in flight, so the slot holds
            // this operation's effect or resume. An unregistered frame keeps
            // it until the frame itself is dropped.
            // SAFETY: As in `yield_`.
            let withdrawn = unsafe {
                with_frame::<Effs, Resumes<'a, Effs>, _>(self.frame, |frame| {
                    frame.replace(Slot::Empty)
                })
            };
            drop(withdrawn);
        }
    }
}

/// Lets a wrapper around a body take effects of its own computation before
/// the frame's owner sees them.
///
/// The wrapper attaches before it polls the body for the first time, and
/// inspects the slot each time the body returns `Pending`.
pub(crate) struct Interceptor<'a, Effs: MapResume> {
    frame: Option<FrameId>,
    marker: Invariant<'a, Effs>,
}

impl<'a, Effs: MapResume> Interceptor<'a, Effs> {
    /// # Safety
    ///
    /// The interceptor must only be polled as part of a body whose runner's
    /// frame has type `Frame<Effs, Resumes<'a, Effs>>` (I3).
    pub(crate) unsafe fn new() -> Self {
        Self {
            frame: None,
            marker: PhantomData,
        }
    }

    /// Record the frame of the runner that polls the body for the first time.
    pub(crate) fn attach(&mut self) {
        if self.frame.is_none() {
            self.frame = Some(starting(false));
        }
    }

    /// Take the effect that the body just yielded, if `select` accepts it.
    /// Otherwise the effect stays in the slot for the owner.
    pub(crate) fn take<T>(&self, select: impl FnOnce(Effs) -> Result<T, Effs>) -> Option<T> {
        let id = self.frame.expect("interceptor polled before attaching");
        // SAFETY: The constructor's caller guarantees the frame type.
        let taken = unsafe {
            with_frame::<Effs, Resumes<'a, Effs>, _>(id, |frame| match frame.replace(Slot::Empty) {
                Slot::Yielded(effect) => match select(effect) {
                    Ok(taken) => Some(taken),
                    Err(effect) => {
                        frame.replace(Slot::Yielded(effect));
                        None
                    }
                },
                other => {
                    frame.replace(other);
                    None
                }
            })
        };
        taken.unwrap_or_else(|| not_registered())
    }
}

/// Yields a whole effect coproduct on a frame and returns its resume.
pub(crate) struct RawYield<'a, Effs: MapResume> {
    operation: Operation<'a, Effs>,
    // Present until the first poll.
    effect: Option<Effs>,
}

// The effect is moved out on the first poll and never pinned.
impl<'a, Effs: MapResume> Unpin for RawYield<'a, Effs> {}

impl<'a, Effs: MapResume> RawYield<'a, Effs> {
    pub(crate) fn new(operation: Operation<'a, Effs>, effect: Effs) -> Self {
        Self {
            operation,
            effect: Some(effect),
        }
    }
}

impl<'a, Effs: MapResume> Future for RawYield<'a, Effs> {
    type Output = Resumes<'a, Effs>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.effect.take() {
            Some(effect) => {
                this.operation.yield_(cx, effect);
                Poll::Pending
            }
            None => this.operation.poll_resume(cx),
        }
    }
}

/// A step of a [`Generator`].
pub(crate) enum GeneratorState<Y, R> {
    Yielded(Y),
    Complete(R),
}

pin_project_lite::pin_project! {
    /// A body and the frame that its yielder uses.
    pub(crate) struct Generator<F, Y, A> {
        // Declared first, so the body is dropped before the frame (I5).
        #[pin]
        body: F,
        #[pin]
        frame: Frame<Y, A>,
    }
}

impl<F: Future, Y, A> Generator<F, Y, A> {
    /// # Safety
    ///
    /// The body must only start yielders of type `Yielder<'a, Effs>`, where
    /// `Y` is `Effs` and `A` is `Resumes<'a, Effs>` (I3).
    pub(crate) unsafe fn new(body: F) -> Self {
        Self {
            body,
            frame: Frame::new(),
        }
    }

    /// Resume the body with the handler's value, if any, and poll it until it
    /// yields, completes, or waits on an ordinary future.
    pub(crate) fn poll_resume(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        resume: Option<A>,
    ) -> Poll<GeneratorState<Y, F::Output>> {
        let this = self.project();
        let frame = this.frame.into_ref().get_ref();
        if let Some(resume) = resume {
            drop(frame.replace(Slot::Resumed(resume)));
        }
        let poll = {
            let _registration = frame.register(cx.waker());
            this.body.poll(cx)
        };
        match poll {
            // A value left by a forgotten operation is dropped with the frame.
            Poll::Ready(value) => Poll::Ready(GeneratorState::Complete(value)),
            Poll::Pending => match frame.replace(Slot::Empty) {
                Slot::Yielded(effect) => Poll::Ready(GeneratorState::Yielded(effect)),
                Slot::Empty => Poll::Pending,
                // The body waits on something else before reading its resume.
                resumed @ Slot::Resumed(_) => {
                    frame.replace(resumed);
                    Poll::Pending
                }
            },
        }
    }

    pub(crate) fn resume(
        mut self: Pin<&mut Self>,
        mut resume: Option<A>,
    ) -> impl Future<Output = GeneratorState<Y, F::Output>> {
        std::future::poll_fn(move |cx| self.as_mut().poll_resume(cx, resume.take()))
    }
}
