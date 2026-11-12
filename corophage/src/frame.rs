//! Frame transport between a computation body and the runner that owns it.
//!
//! Each run owns a pinned [`Frame`] with a unique id and one effect/resume slot.
//! While the owner polls the body, the frame is registered in a thread-local
//! list. A [`Yielder`](crate::Yielder) stores only the frame id and finds its
//! frame in that list, so effect operations work inside any combinator that is
//! polled within the body's own poll. An invocation also uses the slot of its
//! parent's frame to send control [`Signal`]s, which travel up to the root.
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

use std::cell::Cell;
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

/// A control signal from an invocation to the owner of its parent frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Signal {
    /// A local handler cancelled: abort the whole computation.
    Cancel,
    /// A run of local dispatches used its budget: let the root reschedule.
    Reschedule,
}

enum Slot<Y, A> {
    Empty,
    // Written by an operation, taken by the owner.
    Yielded(Y),
    // Written by the owner, taken by the operation.
    Resumed(A),
    // Written by an operation, taken by the owner. Signals share the slot, so
    // at most one effect or signal per frame is in flight.
    Signalled(Signal),
}

/// The header comes first, so a header pointer can be cast back to the whole
/// frame once its id matches (I3).
#[repr(C)]
struct Frame<Y, A> {
    header: FrameHeader,
    slot: Cell<Slot<Y, A>>,
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
            slot: Cell::new(Slot::Empty),
            _pinned: PhantomPinned,
        }
    }

    fn replace(&self, slot: Slot<Y, A>) -> Slot<Y, A> {
        self.slot.replace(slot)
    }

    fn is_empty(&self) -> bool {
        // Moving out and restoring optimizes to just a discriminant check
        // (verified on rustc 1.99.1/aarch64 with payloads up to 4 KiB).
        let slot = self.slot.replace(Slot::Empty);
        let empty = matches!(&slot, Slot::Empty);
        // Restore before any user code or destructor can observe the slot (I4).
        self.slot.set(slot);
        empty
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

/// The id of a frame of type `Frame<Effs, Resumes<'a, Effs>>` (I3).
pub(crate) struct FrameRef<'a, Effs> {
    id: FrameId,
    marker: Invariant<'a, Effs>,
}

impl<'a, Effs> Clone for FrameRef<'a, Effs> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, Effs> Copy for FrameRef<'a, Effs> {}

impl<'a, Effs: MapResume> FrameRef<'a, Effs> {
    /// # Safety
    ///
    /// `id` must belong to a frame of type `Frame<Effs, Resumes<'a, Effs>>`.
    unsafe fn new(id: FrameId) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }

    /// Start an operation on this frame.
    pub(crate) fn operation(self) -> Operation<'a, Effs> {
        Operation {
            frame: self,
            stage: Stage::Idle,
        }
    }

    /// Run `f` on this frame, which must be registered.
    fn with<R>(self, f: impl FnOnce(&Frame<Effs, Resumes<'a, Effs>>) -> R) -> R {
        // SAFETY: The constructor's caller guarantees the frame type.
        unsafe { with_frame(self.id, f) }.unwrap_or_else(|| not_registered())
    }

    /// Like [`with`](Self::with), but does nothing if the frame is not
    /// registered.
    fn with_registered<R>(self, f: impl FnOnce(&Frame<Effs, Resumes<'a, Effs>>) -> R) -> Option<R> {
        // SAFETY: As in `with`.
        unsafe { with_frame(self.id, f) }
    }
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

/// Take the frame whose owner polls a body for the first time.
///
/// Only the owner polls a body, so its frame is the most recently registered.
///
/// # Safety
///
/// Only a body may call this, at its first poll, and its runner's frame must
/// have type `Frame<Effs, Resumes<'a, Effs>>` (I3).
pub(crate) unsafe fn start<'a, Effs: MapResume>() -> FrameRef<'a, Effs> {
    // SAFETY: The caller guarantees the frame type.
    unsafe { FrameRef::new(starting(true)) }
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

/// What an [`Operation`] waits for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Idle,
    /// The resume of the effect it yielded.
    Resume,
    /// The owner taking the signal it sent.
    Signal(Signal),
}

/// One operation on a frame: it hands an effect or a signal to the frame's
/// owner, then waits for the resume or for the owner to take the signal.
///
/// Dropping the operation while it waits withdraws its effect or signal, or
/// discards its resume if the handler already ran.
pub(crate) struct Operation<'a, Effs: MapResume> {
    frame: FrameRef<'a, Effs>,
    stage: Stage,
}

impl<'a, Effs: MapResume> Operation<'a, Effs> {
    pub(crate) fn stage(&self) -> Stage {
        self.stage
    }

    /// Hand an effect to the frame's owner. The caller must then return
    /// `Pending` up to the owner.
    pub(crate) fn yield_(&mut self, cx: &Context<'_>, effect: Effs) {
        self.hand_over(cx, Slot::Yielded(effect), Stage::Resume);
    }

    /// Hand a signal to the frame's owner. The caller must then return
    /// `Pending` up to the owner.
    pub(crate) fn signal(&mut self, cx: &Context<'_>, signal: Signal) {
        self.hand_over(cx, Slot::Signalled(signal), Stage::Signal(signal));
    }

    fn hand_over(&mut self, cx: &Context<'_>, slot: Slot<Effs, Resumes<'a, Effs>>, stage: Stage) {
        self.frame.with(|frame| {
            if !frame.is_empty() {
                panic!("another effect operation of this computation is in flight");
            }
            frame.replace(slot);
            self.stage = stage;
            frame.wake_unless_owner(cx);
        });
    }

    /// Take the resume of the yielded effect once the owner provides it.
    pub(crate) fn poll_resume(&mut self, cx: &Context<'_>) -> Poll<Resumes<'a, Effs>> {
        if self.stage != Stage::Resume {
            panic!("effect operation polled after completion");
        }
        self.frame.with(|frame| match frame.replace(Slot::Empty) {
            Slot::Resumed(value) => {
                self.stage = Stage::Idle;
                Poll::Ready(value)
            }
            // Polled again before the owner took the effect.
            yielded @ Slot::Yielded(_) => {
                frame.replace(yielded);
                frame.wake_unless_owner(cx);
                Poll::Pending
            }
            _ => panic!("effect operation lost its pending effect"),
        })
    }

    /// Complete once the owner has taken the signal.
    pub(crate) fn poll_signal(&mut self, cx: &Context<'_>) -> Poll<()> {
        if !matches!(self.stage, Stage::Signal(_)) {
            panic!("signal polled after completion");
        }
        self.frame.with(|frame| match frame.replace(Slot::Empty) {
            // Polled again before the owner took the signal.
            signalled @ Slot::Signalled(_) => {
                frame.replace(signalled);
                frame.wake_unless_owner(cx);
                Poll::Pending
            }
            Slot::Empty => {
                self.stage = Stage::Idle;
                Poll::Ready(())
            }
            _ => panic!("signal operation found another operation's value"),
        })
    }
}

impl<'a, Effs: MapResume> Drop for Operation<'a, Effs> {
    fn drop(&mut self) {
        if self.stage != Stage::Idle {
            // At most one operation per frame is in flight, so the slot holds
            // this operation's effect, resume, or signal, or nothing once the
            // owner took the signal. An unregistered frame keeps it until the
            // frame itself is dropped.
            let withdrawn = self
                .frame
                .with_registered(|frame| frame.replace(Slot::Empty));
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
    frame: Option<FrameRef<'a, Effs>>,
}

impl<'a, Effs: MapResume> Interceptor<'a, Effs> {
    /// # Safety
    ///
    /// The interceptor must only be polled as part of a body whose runner's
    /// frame has type `Frame<Effs, Resumes<'a, Effs>>` (I3).
    pub(crate) unsafe fn new() -> Self {
        Self { frame: None }
    }

    /// Record the frame of the runner that polls the body for the first time.
    pub(crate) fn attach(&mut self) {
        if self.frame.is_none() {
            // SAFETY: The constructor's caller guarantees the frame type.
            self.frame = Some(unsafe { FrameRef::new(starting(false)) });
        }
    }

    /// Take the effect that the body just yielded, if `select` accepts it.
    /// Otherwise the effect stays in the slot for the owner.
    pub(crate) fn take<T>(&self, select: impl FnOnce(Effs) -> Result<T, Effs>) -> Option<T> {
        let frame = self.frame.expect("interceptor polled before attaching");
        frame.with(|frame| match frame.replace(Slot::Empty) {
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

    // Keep effect transport in the body's poll; an extra call slows catch loops.
    #[inline(always)]
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
    /// A nested invocation signalled the owner.
    Signalled(Signal),
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
                Slot::Signalled(signal) => Poll::Ready(GeneratorState::Signalled(signal)),
                Slot::Empty => Poll::Pending,
                // The body waits on something else before reading its resume.
                resumed @ Slot::Resumed(_) => {
                    frame.replace(resumed);
                    Poll::Pending
                }
            },
        }
    }
}
