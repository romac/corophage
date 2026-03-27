//! Coroutine types for defining effectful computations.
//!
//! The primary types are [`Co`] (non-`Send`) and [`CoSend`] (`Send`-able),
//! both of which are type aliases for [`GenericCo`] parameterized by a
//! [`Locality`] marker.
//!
//! For most use cases, prefer [`Program`](crate::Program) over constructing
//! coroutines directly.

use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;

use fauxgen::__private::SyncGenerator;
use fauxgen::Generator;
use fauxgen::GeneratorState;
use fauxgen::GeneratorToken;
use frunk_core::coproduct::{CoprodInjector, CoprodUninjector, Coproduct};

use crate::coproduct::{EmbedEffect, ProjectResume};
use crate::effect::{CanStart, Effect, Effects, MapResume, Resumes, Start};
use crate::locality::{Local, Locality, Sendable};
use crate::program::Effectful;

type Gen<'a, Effs, Return, L> = SyncGenerator<
    <L as Locality>::PinBoxFuture<'a, Return>,
    CanStart<Effs>,
    Resumes<'a, CanStart<Effs>>,
>;

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
/// For most use cases, prefer [`Program::new_send`](crate::Program::new_send) over
/// using `CoSend` directly.
pub type CoSend<'a, Effs, Return> = GenericCo<'a, Effs, Return, Sendable>;

/// A coroutine parameterized by a [`Locality`] marker that controls `Send`-ness.
///
/// You typically use this through the type aliases [`Co`] (non-`Send`) or
/// [`CoSend`] (`Send`-able) rather than using `GenericCo` directly.
pub struct GenericCo<'a, Effs, Return, L: Locality = Local>
where
    Effs: Effects<'a>,
{
    generator: Gen<'a, Effs, Return, L>,
    _pin: PhantomPinned,
}

macro_rules! make_co {
    ($f:expr, $cast:ty) => {{
        let token = fauxgen::__private::token();
        let marker = token.marker();

        let fut = Box::pin(async move {
            let token = fauxgen::__private::register_owned(token).await;

            // Consume the initial Start argument. The value is always
            // CanStart::Inl(Start) — enforced by resume_with(Start) in run!/invoke.
            let _ = token.argument().await;

            $f(Yielder::new(token)).await
        }) as $cast;

        let generator = fauxgen::__private::gen_sync(marker, fut);
        Self {
            generator,
            _pin: PhantomPinned,
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

impl<'a, Effs, Return, L: Locality> GenericCo<'a, Effs, Return, L>
where
    Effs: Effects<'a>,
{
    #[inline]
    pub(crate) fn resume(
        self: Pin<&mut Self>,
        resume: Resumes<'a, CanStart<Effs>>,
    ) -> GeneratorState<CanStart<Effs>, Return> {
        // SAFETY: This is a structural pin projection from `Pin<&mut GenericCo>` to
        // `Pin<&mut Gen>`. This is sound because:
        // 1. `GenericCo` is `!Unpin` (contains `PhantomPinned`), so it is never moved after pinning.
        // 2. The `generator` field is structurally pinned (not behind an indirection).
        // 3. `GenericCo` has no `Drop` impl that could move the field.
        let mut g = unsafe { self.map_unchecked_mut(|s| &mut s.generator) };
        Generator::resume(g.as_mut(), resume)
    }

    #[inline]
    pub(crate) fn resume_with<R, Index>(
        self: Pin<&mut Self>,
        resume: R,
    ) -> GeneratorState<CanStart<Effs>, Return>
    where
        Resumes<'a, CanStart<Effs>>: CoprodInjector<R, Index>,
    {
        self.resume(Resumes::<'a, CanStart<Effs>>::inject(resume))
    }
}

/// Handle passed to computation closures for yielding effects.
///
/// Use [`yield_`](Yielder::yield_) to perform an effect and receive the handler's
/// resume value.
pub struct Yielder<'a, Effs>
where
    Effs: MapResume,
{
    token: GeneratorToken<CanStart<Effs>, Resumes<'a, CanStart<Effs>>>,
}

impl<'a, Effs> Yielder<'a, Effs>
where
    Effs: MapResume,
{
    #[inline]
    fn new(token: GeneratorToken<CanStart<Effs>, Resumes<'a, CanStart<Effs>>>) -> Self {
        Self { token }
    }

    /// Yield an effect to the handler and suspend until resumed.
    ///
    /// Returns the resume value provided by the handler for this effect.
    #[inline]
    pub async fn yield_<E, Index>(&self, effect: E) -> E::Resume<'a>
    where
        E: Effect,
        Effs: CoprodInjector<E, Index>,
        <Effs as MapResume>::Output<'a>: CoprodUninjector<E::Resume<'a>, Index>,
    {
        let resume = self
            .token
            .yield_(Coproduct::Inr(Effs::inject(effect)))
            .await;

        match resume {
            Coproduct::Inr(value) => match value.uninject() {
                Ok(value) => value,
                // INVARIANT: InjectResume guarantees the handler resumes at the
                // same coproduct index as the yielded effect, so uninject
                // always succeeds.
                Err(_) => {
                    debug_unreachable!("uninject failed: handler resumed at wrong coproduct index")
                }
            },
            // INVARIANT: The Start (Inl) arm is never sent as a resume value.
            // The generator receives Start only once during initialization
            // (in make_co!); all subsequent resumes use Inr via InjectResume.
            Coproduct::Inl(_) => {
                debug_unreachable!("Start (Inl) arm should never be sent as a resume value")
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
    #[inline]
    pub async fn invoke<'b, SubEffs, R, L, Indices>(
        &self,
        program: Effectful<'b, SubEffs, R, L>,
    ) -> R
    where
        'a: 'b,
        SubEffs: Effects<'b> + EmbedEffect<Effs, Indices>,
        Resumes<'a, Effs>: ProjectResume<'a, SubEffs, Indices>,
        L: Locality,
    {
        let mut co = std::pin::pin!(program.co);
        let mut yielded = co.as_mut().resume_with(Start);

        loop {
            match yielded {
                GeneratorState::Complete(value) => break value,
                GeneratorState::Yielded(effect) => {
                    // INVARIANT: Yielder::yield_ always wraps effects in Inr,
                    // so the Inl (Start) arm is never yielded after init.
                    let subeffect = match effect {
                        Coproduct::Inl(_) => debug_unreachable!(
                            "Start (Inl) arm should never be yielded after initialization"
                        ),
                        Coproduct::Inr(subeffect) => subeffect,
                    };

                    // Embed sub-effect into the outer coproduct (synchronous).
                    let outer_effect: Effs = subeffect.embed();

                    // Yield through the raw generator token.
                    let resume = self.token.yield_(Coproduct::Inr(outer_effect)).await;

                    // Project the outer resume back to sub-effect resume (synchronous).
                    // This produces Resumes<'a, SubEffs> (the outer handler's lifetime).
                    let resume_long: Resumes<'a, SubEffs> = match resume {
                        Coproduct::Inr(value) => value.project(),
                        Coproduct::Inl(_) => {
                            debug_unreachable!(
                                "Start (Inl) arm should never be sent as a resume value"
                            )
                        }
                    };

                    // Convert from the outer lifetime 'a to the sub-program's lifetime 'b.
                    // This is safe because all effect resume types implement CovariantResume.
                    let resume_short: Resumes<'b, SubEffs> = SubEffs::shorten_resumes(resume_long);

                    yielded = co.as_mut().resume(Coproduct::Inr(resume_short));
                }
            }
        }
    }
}
