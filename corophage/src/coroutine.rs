//! Coroutine types for defining effectful computations.
//!
//! The primary types are [`Co`] (non-`Send`) and [`CoSend`] (`Send`-able),
//! both of which are type aliases for [`GenericCo`] parameterized by a
//! [`Locality`] marker.
//!
//! For most use cases, prefer [`Program`](crate::Program) over constructing
//! coroutines directly.

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use frunk_core::coproduct::{CoprodInjector, CoprodUninjector};

use crate::coproduct::{EmbedEffect, ProjectResume};
use crate::effect::{Effect, Effects, MapResume, Resumes};
use crate::frame::{FrameId, Generator, GeneratorState, Invariant, RawYield};
use crate::locality::{Local, Locality, Sendable};

/// A coroutine body whose future type has not been erased.
///
/// Construct this through [`Program::new`](crate::Program::new).
/// Its `Send` capability follows its body, without a locality marker. The body
/// is pinned only when execution or invocation starts.
pub struct Coroutine<'a, Effs: Effects<'a>, F> {
    body: F,
    // The runner creates its frame from these parameters, and the body's
    // yielder from the same ones, so they must not change by variance.
    marker: Invariant<'a, Effs>,
}

impl<'a, Effs: Effects<'a>, F: Future> Coroutine<'a, Effs, F> {
    pub(crate) fn map<R>(
        self,
        map: impl FnOnce(F::Output) -> R,
    ) -> Coroutine<'a, Effs, impl Future<Output = R>> {
        Coroutine {
            // Reuse the body and its yielder; mapping needs no new frame or box.
            body: async move { map(self.body.await) },
            marker: self.marker,
        }
    }

    pub(crate) fn generator(self) -> Generator<F, Effs, Resumes<'a, Effs>> {
        // SAFETY: Coroutines are only built by `concrete` and `make_co!`, whose
        // bodies start a `Yielder<'a, Effs>`. Invariance keeps both parameters.
        unsafe { Generator::new(self.body) }
    }

    /// Erase the body for an existing local coroutine signature.
    pub fn boxed(self) -> Co<'a, Effs, F::Output>
    where
        F: 'a,
    {
        GenericCo {
            body: Box::pin(self.body) as Pin<Box<dyn Future<Output = F::Output> + 'a>>,
            marker: self.marker,
            locality: PhantomData,
        }
    }

    /// Erase the body while retaining the sendable coroutine guarantee.
    pub fn boxed_send(self) -> CoSend<'a, Effs, F::Output>
    where
        F: Send + 'a,
    {
        GenericCo {
            body: Box::pin(self.body) as Pin<Box<dyn Future<Output = F::Output> + Send + 'a>>,
            marker: self.marker,
            locality: PhantomData,
        }
    }
}

pub(crate) fn concrete<'a, Effs: Effects<'a>, F>(
    build: impl FnOnce(Yielder<'a, Effs>) -> F + 'a,
) -> Coroutine<'a, Effs, impl Future<Output = F::Output> + 'a>
where
    F: Future + 'a,
{
    Coroutine {
        body: async move {
            // SAFETY: Only the generator built from this coroutine polls the
            // body, and it creates a frame of the matching type.
            let yielder = unsafe { Yielder::start() };
            build(yielder).await
        },
        marker: PhantomData,
    }
}

/// A non-`Send` coroutine that can yield effects from `Effs` and return `Return`.
///
/// This is the default coroutine type. Use [`CoSend`] if you need a `Send`-able
/// coroutine for use with multi-threaded executors like `tokio::spawn`.
///
/// For most use cases, prefer [`Program`](crate::Program) over using `Co` directly.
pub type Co<'a, Effs, Return> = GenericCo<'a, Effs, Return, Local>;

/// A `Send`-able coroutine that can yield effects from `Effs` and return `Return`.
///
/// Use this instead of [`Co`] when the coroutine needs to be `Send`,
/// e.g., for use with `tokio::spawn` or other multi-threaded executors.
///
/// For most use cases, prefer [`Program::new`](crate::Program::new), which retains
/// the body's concrete type and conditional `Send`. Use
/// [`Program::new_send`](crate::Program::new_send) for an erased sendable body.
pub type CoSend<'a, Effs, Return> = GenericCo<'a, Effs, Return, Sendable>;

/// A coroutine parameterized by a [`Locality`] marker that controls `Send`-ness.
///
/// You typically use this through the type aliases [`Co`] (non-`Send`) or
/// [`CoSend`] (`Send`-able) rather than using `GenericCo` directly.
pub struct GenericCo<'a, Effs, Return, L: Locality = Local>
where
    Effs: Effects<'a>,
{
    body: L::PinBoxFuture<'a, Return>,
    // The body hides its yielder type, so this is the only link between the
    // runner's frame and the yielder (see `Coroutine`).
    marker: Invariant<'a, Effs>,
    locality: PhantomData<fn() -> L>,
}

macro_rules! make_co {
    ($f:expr, $cast:ty) => {{
        let fut = Box::pin(async move {
            // SAFETY: See `concrete`.
            let yielder = unsafe { Yielder::start() };
            $f(yielder).await
        }) as $cast;

        Self {
            body: fut,
            marker: PhantomData,
            locality: PhantomData,
        }
    }};
}

impl<'a, Effs, Return> Co<'a, Effs, Return>
where
    Effs: Effects<'a>,
{
    /// Create a new non-`Send` coroutine from a closure that receives a [`Yielder`].
    pub fn new<F>(f: impl FnOnce(Yielder<'a, Effs>) -> F + 'a) -> Self
    where
        F: Future<Output = Return>,
    {
        make_co!(f, Pin<Box<dyn Future<Output = Return> + 'a>>)
    }
}

impl<'a, Effs, Return> CoSend<'a, Effs, Return>
where
    Effs: Effects<'a>,
{
    /// Create a new `Send`-able coroutine from a closure that receives a [`Yielder`].
    pub fn new<F>(f: impl FnOnce(Yielder<'a, Effs>) -> F + Send + 'a) -> Self
    where
        F: Future<Output = Return> + Send,
    {
        make_co!(f, Pin<Box<dyn Future<Output = Return> + Send + 'a>>)
    }
}

impl<'a, Effs: Effects<'a>, R, L: Locality> GenericCo<'a, Effs, R, L> {
    pub(crate) fn into_concrete(self) -> Coroutine<'a, Effs, L::PinBoxFuture<'a, R>> {
        Coroutine {
            body: self.body,
            marker: self.marker,
        }
    }

    pub(crate) fn generator(self) -> Generator<L::PinBoxFuture<'a, R>, Effs, Resumes<'a, Effs>> {
        self.into_concrete().generator()
    }
}

pin_project_lite::pin_project! {
    // Store each invoked generator once. Moving a concrete program into an
    // async fn and then pinning it inside that function duplicates its storage
    // in the future layout, which grows exponentially in deep invoke chains.
    struct Invocation<F, Y, A, Forward, Fut> {
        #[pin]
        generator: Generator<F, Y, A>,
        forward: Forward,
        #[pin]
        forwarding: Option<Fut>,
        resume: Option<A>,
    }
}

impl<F, Y, A, Forward, Fut> Future for Invocation<F, Y, A, Forward, Fut>
where
    F: Future,
    Forward: FnMut(Y) -> Fut,
    Fut: Future<Output = A>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        loop {
            if let Some(forwarding) = this.forwarding.as_mut().as_pin_mut() {
                *this.resume = Some(std::task::ready!(forwarding.poll(cx)));
                this.forwarding.set(None);
            }
            match std::task::ready!(this.generator.as_mut().poll_resume(cx, this.resume.take())) {
                GeneratorState::Complete(value) => return Poll::Ready(value),
                GeneratorState::Yielded(effect) => {
                    // The forwarding future borrows the outer yielder, which
                    // lives outside this object. It never borrows our fields.
                    this.forwarding.set(Some((this.forward)(effect)));
                }
            }
        }
    }
}

/// Handle passed to computation closures for yielding effects.
///
/// Use [`yield_`](Yielder::yield_) to perform an effect and receive the handler's
/// resume value. Effect operations require exclusive access, so safe Rust cannot
/// start another [`yield_`](Yielder::yield_) or [`invoke`](Yielder::invoke) call
/// while one is still in flight.
pub struct Yielder<'a, Effs>
where
    Effs: MapResume,
{
    // The yielder only names its frame, which it finds while its owner polls
    // the body. This keeps it `Send` and `Sync` without any resume bounds.
    frame: FrameId,
    marker: Invariant<'a, Effs>,
}

impl<'a, Effs> Yielder<'a, Effs>
where
    Effs: MapResume,
{
    /// Take the frame of the runner polling this body for the first time.
    ///
    /// # Safety
    ///
    /// Only a coroutine body may call this, at its first poll, and its runner's
    /// frame must have type `Frame<Effs, Resumes<'a, Effs>>`.
    #[inline]
    unsafe fn start() -> Self {
        Self {
            frame: crate::frame::start(),
            marker: PhantomData,
        }
    }

    #[inline]
    fn raw(&self, effect: Effs) -> RawYield<'a, Effs> {
        // SAFETY: This yielder's frame has the matching type (see `start`).
        unsafe { RawYield::new(self.frame, effect) }
    }

    /// Yield an effect to the handler and suspend until resumed.
    ///
    /// Returns the resume value provided by the handler for this effect.
    ///
    /// The operation can be polled inside any combinator that runs within the
    /// computation's own poll, such as `select!`, `join!`, or
    /// `FuturesUnordered`. Polling it anywhere else panics.
    ///
    /// # Cancellation
    ///
    /// Dropping the operation before it completes withdraws its effect. If the
    /// handler has not taken the effect yet, it is never handled. If the
    /// handler already resumed, the resume value is discarded; the handler's
    /// side effects remain.
    ///
    /// The computation is not polled while a handler runs, so a competing
    /// branch cannot interrupt the handler. It can only win in the poll that
    /// yielded the effect, or after the handler resumed. In `select!`, use
    /// `biased;` with the effect branch first to rule out the second case.
    #[inline]
    pub fn yield_<E, Index>(
        &mut self,
        effect: E,
    ) -> impl Future<Output = E::Resume<'a>> + use<'_, 'a, Effs, E, Index>
    where
        E: Effect,
        Effs: CoprodInjector<E, Index>,
        <Effs as MapResume>::Output<'a>: CoprodUninjector<E::Resume<'a>, Index>,
    {
        Yield::<'a, Effs, E, Index> {
            raw: self.raw(Effs::inject(effect)),
            marker: PhantomData,
        }
    }

    /// Throw an error, aborting the computation.
    ///
    /// Yields [`Throw`](crate::Throw) and never returns, so the call can stand in
    /// for a value of any type. [`Program::catch`](crate::Program::catch) returns
    /// the error from the run. In statement position, write
    /// `return y.throw(error).await` so the type can be inferred.
    #[inline]
    pub async fn throw<T, E, Index>(&mut self, error: E) -> T
    where
        Effs: CoprodInjector<crate::Throw<E>, Index>,
        <Effs as MapResume>::Output<'a>: CoprodUninjector<crate::Never, Index>,
    {
        match self.yield_(crate::Throw(error)).await {}
    }

    /// Invoke a sub-program, forwarding its effects through this yielder.
    ///
    /// The sub-program's effects must be a subset of this yielder's effects.
    /// Each effect yielded by the sub-program is forwarded to the outer handler
    /// via this yielder, and the resume value is passed back to the sub-program.
    ///
    /// Effect forwarding uses synchronous coproduct conversions
    /// ([`EmbedEffect`] / [`ProjectResume`]) combined with a raw yield on this
    /// yielder's frame. This works in both `Send` and non-`Send` contexts.
    ///
    /// Returns the sub-program's result directly. If the outer handler cancels,
    /// the entire coroutine is dropped, so `invoke` never returns in that case.
    ///
    /// The sub-program may use a lifetime `'b` shorter than the outer program's `'a`.
    /// This allows sequential invocations that borrow from the same mutable reference,
    /// since each invocation only borrows for its own duration rather than for the
    /// entire outer computation lifetime.
    ///
    /// Like [`yield_`](Yielder::yield_), the returned future withdraws a
    /// forwarded effect when dropped, and drops the sub-program.
    #[inline]
    pub fn invoke<'b, SubEffs, R, F, Mode, Indices>(
        &mut self,
        program: crate::Program<'b, SubEffs, F, SubEffs, frunk_core::hlist::HNil, (), Mode>,
    ) -> impl Future<Output = R>
    where
        'a: 'b,
        SubEffs: Effects<'b> + EmbedEffect<Effs, Indices>,
        Resumes<'a, Effs>: ProjectResume<'a, SubEffs, Indices>,
        F: Future<Output = R>,
    {
        let yielder = &*self;
        Invocation {
            generator: program.co.generator(),
            resume: None,
            forwarding: None,
            forward: move |effect: SubEffs| {
                // Embed sub-effect into the outer coproduct (synchronous).
                let outer = yielder.raw(effect.embed());
                async move {
                    // Project the outer resume back to sub-effect resume
                    // (synchronous). This produces Resumes<'a, SubEffs> (the
                    // outer handler's lifetime).
                    let resume: Resumes<'a, SubEffs> = outer.await.project();

                    // Convert from the outer lifetime 'a to the sub-program's
                    // lifetime 'b. Effect::shorten_resume witnesses covariance
                    // for every effect.
                    SubEffs::shorten_resumes(resume)
                }
            },
        }
    }
}

/// The future of [`Yielder::yield_`]. Its fields name no resume projection,
/// so bodies that hold it need no resume bounds to be `Send`.
struct Yield<'a, Effs: MapResume, E, Index> {
    raw: RawYield<'a, Effs>,
    marker: PhantomData<fn() -> (E, Index)>,
}

impl<'a, Effs, E, Index> Future for Yield<'a, Effs, E, Index>
where
    Effs: MapResume,
    E: Effect,
    <Effs as MapResume>::Output<'a>: CoprodUninjector<E::Resume<'a>, Index>,
{
    type Output = E::Resume<'a>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().raw)
            .poll(cx)
            .map(|resume| match resume.uninject() {
                Ok(value) => value,
                // Dispatch normally injects at the yielded effect's index. The
                // index types are part of public generic APIs, though, so keep
                // this boundary checked in every build.
                Err(_) => panic!("handler resumed at the wrong effect index"),
            })
    }
}
