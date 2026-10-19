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
use std::sync::Arc;
use std::task::{Context, Poll};

use fauxgen::__private::AsyncGenerator as GeneratorStorage;
use fauxgen::__private::RawGeneratorToken;
use fauxgen::AsyncGenerator;
use fauxgen::GeneratorState;
use frunk_core::coproduct::{CoprodInjector, CoprodUninjector, Coproduct};

use crate::coproduct::{EmbedEffect, ProjectResume};
use crate::effect::{CanStart, Effect, Effects, MapResume, Resumes, Start};
use crate::locality::{Local, Locality, Sendable};

/// A coroutine body whose future type has not been erased.
///
/// Construct this through [`Program::new`](crate::Program::new).
/// Its `Send` capability follows its body and resume transport, without a
/// locality marker. The body is pinned only when execution or invocation starts.
pub struct Coroutine<'a, Effs: Effects<'a>, F> {
    body: F,
    // fauxgen's generator has an unchecked Send impl that only checks F.
    // Retain the token's Send + Sync requirements for the actual resume lifetime,
    // including before the first poll. Arc here is only a zero-sized marker.
    transport: PhantomData<std::sync::Arc<Resumes<'a, CanStart<Effs>>>>,
}

impl<'a, Effs: Effects<'a>, F: Future> Coroutine<'a, Effs, F> {
    pub(crate) fn map<R>(
        self,
        map: impl FnOnce(F::Output) -> R,
    ) -> Coroutine<'a, Effs, impl Future<Output = R>> {
        Coroutine {
            // Reuse the body and its token; mapping needs no new generator or box.
            body: async move { map(self.body.await) },
            transport: self.transport,
        }
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn generator(
        self,
    ) -> Transport<
        GeneratorStorage<F, CanStart<Effs>, Resumes<'a, CanStart<Effs>>>,
        Resumes<'a, CanStart<Effs>>,
    > {
        Transport {
            generator: fauxgen::__private::gen_async(Default::default(), self.body),
            transport: self.transport,
        }
    }

    /// Erase the body for an existing local coroutine signature.
    pub fn boxed(self) -> Co<'a, Effs, F::Output>
    where
        F: 'a,
    {
        GenericCo {
            body: Box::pin(self.body) as Pin<Box<dyn Future<Output = F::Output> + 'a>>,
            transport: PhantomData,
            locality: PhantomData,
        }
    }

    /// Erase the body while retaining the sendable coroutine guarantee.
    pub fn boxed_send(self) -> CoSend<'a, Effs, F::Output>
    where
        F: Send + 'a,
        for<'r> Resumes<'r, CanStart<Effs>>: Send + Sync,
    {
        GenericCo {
            body: Box::pin(self.body) as Pin<Box<dyn Future<Output = F::Output> + Send + 'a>>,
            transport: PhantomData,
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
            // Allocate on first poll so constructing a program stays allocation-free.
            let token: Token<'a, Effs> = Arc::pin(fauxgen::__private::token());
            fauxgen::__private::register(token.as_ref()).await;
            let _ = token.as_ref().argument().await;
            // fauxgen identifies the generator by the token's address. Keep the
            // token alive until the body completes so that address cannot be
            // reused by another token, even if the yielder is dropped early.
            let registered = token.clone();
            let output = build(Yielder::new(token)).await;
            drop(registered);
            output
        },
        transport: PhantomData,
    }
}

pin_project_lite::pin_project! {
    // Keep resume storage constraints visible after the body becomes a generator.
    pub(crate) struct Transport<G, A> {
        #[pin]
        generator: G,
        transport: PhantomData<std::sync::Arc<A>>,
    }
}

impl<G: AsyncGenerator<A>, A> AsyncGenerator<A> for Transport<G, A> {
    type Yield = G::Yield;
    type Return = G::Return;
    fn poll_resume(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        arg: Option<A>,
    ) -> Poll<GeneratorState<Self::Yield, Self::Return>> {
        self.project().generator.poll_resume(cx, arg)
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
    transport: PhantomData<std::sync::Arc<Resumes<'a, CanStart<Effs>>>>,
    locality: PhantomData<fn() -> L>,
}

macro_rules! make_co {
    ($f:expr, $cast:ty) => {{
        let fut = Box::pin(async move {
            let token: Token<'a, Effs> = Arc::pin(fauxgen::__private::token());
            fauxgen::__private::register(token.as_ref()).await;

            // Consume the initial Start argument. The value is always
            // CanStart::Inl(Start), sent first by the execution loops and invoke.
            let _ = token.as_ref().argument().await;

            // Keep the registered token address reserved; see `concrete`.
            let registered = token.clone();
            let output = $f(Yielder::new(token)).await;
            drop(registered);
            output
        }) as $cast;

        Self {
            body: fut,
            transport: PhantomData,
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
    for<'r> Resumes<'r, CanStart<Effs>>: Send + Sync,
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
            transport: self.transport,
        }
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn generator(
        self,
    ) -> Transport<
        GeneratorStorage<L::PinBoxFuture<'a, R>, CanStart<Effs>, Resumes<'a, CanStart<Effs>>>,
        Resumes<'a, CanStart<Effs>>,
    > {
        self.into_concrete().generator()
    }
}

pin_project_lite::pin_project! {
    // Store each invoked generator once. Moving a concrete program into an
    // async fn and then pinning it inside that function duplicates its storage
    // in the future layout, which grows exponentially in deep invoke chains.
    struct Invocation<G, Forward, Fut, A> {
        #[pin]
        generator: G,
        forward: Forward,
        #[pin]
        forwarding: Option<Fut>,
        resume: Option<A>,
    }
}

impl<G, Forward, Fut, A> Future for Invocation<G, Forward, Fut, A>
where
    G: AsyncGenerator<A>,
    Forward: FnMut(G::Yield) -> Fut,
    Fut: Future<Output = A>,
{
    type Output = G::Return;

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

/// A registered generator token. fauxgen identifies the generator by its address.
type Token<'a, Effs> = Pin<Arc<RawGeneratorToken<CanStart<Effs>, Resumes<'a, CanStart<Effs>>>>>;

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
    token: Token<'a, Effs>,
}

impl<'a, Effs> Yielder<'a, Effs>
where
    Effs: MapResume,
{
    #[inline]
    fn new(token: Token<'a, Effs>) -> Self {
        Self { token }
    }

    /// Yield an effect to the handler and suspend until resumed.
    ///
    /// Returns the resume value provided by the handler for this effect.
    ///
    /// # Cancellation
    ///
    /// This future is not cancel-safe. Its first poll hands the effect to the
    /// runner. If it is dropped before completing, for example by losing a
    /// `select!` race, the abandoned effect may still be handled at the next
    /// suspension, discarding its resume value, or it may be silently dropped.
    /// Await each operation to completion.
    #[inline]
    pub async fn yield_<E, Index>(&mut self, effect: E) -> E::Resume<'a>
    where
        E: Effect,
        Effs: CoprodInjector<E, Index>,
        <Effs as MapResume>::Output<'a>: CoprodUninjector<E::Resume<'a>, Index>,
    {
        let resume = self
            .token
            .as_ref()
            .yield_(Coproduct::Inr(Effs::inject(effect)))
            .await;

        match resume {
            Coproduct::Inr(value) => match value.uninject() {
                Ok(value) => value,
                // Dispatch normally injects at the yielded effect's index. The
                // index types are part of public generic APIs, though, so keep
                // this boundary checked in every build.
                Err(_) => panic!("handler resumed at the wrong effect index"),
            },
            // INVARIANT: The Start (Inl) arm is never sent as a resume value.
            // The generator receives Start only once during initialization
            // (in make_co!); all subsequent resumes use Inr via InjectResume.
            Coproduct::Inl(_) => {
                unreachable!("Start (Inl) arm should never be sent as a resume value")
            }
        }
    }

    /// Invoke a sub-program, forwarding its effects through this yielder.
    ///
    /// The sub-program's effects must be a subset of this yielder's effects.
    /// Each effect yielded by the sub-program is forwarded to the outer handler
    /// via this yielder, and the resume value is passed back to the sub-program.
    ///
    /// Effect forwarding uses synchronous coproduct conversions
    /// ([`EmbedEffect`] / [`ProjectResume`]) combined with the raw generator
    /// yield. This works in both `Send` and non-`Send` contexts.
    ///
    /// Returns the sub-program's result directly. If the outer handler cancels,
    /// the entire coroutine is dropped, so `invoke` never returns in that case.
    ///
    /// The sub-program may use a lifetime `'b` shorter than the outer program's `'a`.
    /// This allows sequential invocations that borrow from the same mutable reference,
    /// since each invocation only borrows for its own duration rather than for the
    /// entire outer computation lifetime.
    ///
    /// Like [`yield_`](Yielder::yield_), the returned future is not cancel-safe:
    /// dropping it while it forwards an effect has the same consequences.
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
        let token = self.token.as_ref();
        Invocation {
            generator: program.co.generator(),
            resume: Some(Coproduct::Inl(Start)),
            forwarding: None,
            forward: move |effect: CanStart<SubEffs>| async move {
                // INVARIANT: Yielder::yield_ always wraps effects in Inr,
                // so the Inl (Start) arm is never yielded after init.
                let subeffect = match effect {
                    Coproduct::Inl(_) => {
                        unreachable!("Start (Inl) arm should never be yielded after initialization")
                    }
                    Coproduct::Inr(subeffect) => subeffect,
                };

                // Embed sub-effect into the outer coproduct (synchronous).
                let outer_effect: Effs = subeffect.embed();

                // Yield through the raw generator token.
                let resume = token.yield_(Coproduct::Inr(outer_effect)).await;

                // Project the outer resume back to sub-effect resume (synchronous).
                // This produces Resumes<'a, SubEffs> (the outer handler's lifetime).
                let resume_long: Resumes<'a, SubEffs> = match resume {
                    Coproduct::Inr(value) => value.project(),
                    Coproduct::Inl(_) => {
                        unreachable!("Start (Inl) arm should never be sent as a resume value")
                    }
                };

                // Convert from the outer lifetime 'a to the sub-program's lifetime 'b.
                // Effect::shorten_resume witnesses covariance for every effect.
                let resume_short: Resumes<'b, SubEffs> = SubEffs::shorten_resumes(resume_long);

                Coproduct::Inr(resume_short)
            },
        }
    }
}
