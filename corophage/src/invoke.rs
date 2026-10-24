//! Invoking sub-programs from a computation body.
//!
//! An unhandled program forwards each effect directly from its frame into the
//! parent's. A program with handlers attached runs its own dispatch loop: an
//! internal [`Forward`] handler serves exactly its remaining effects by
//! yielding them on the parent's frame, and the attached handlers serve the
//! rest. A local cancellation, or a run of local dispatches that uses its
//! budget, sends a signal through the parent's frame to the root runner.

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use frunk_core::coproduct::{CoprodInjector, CoprodUninjector};
use frunk_core::hlist::{HCons, HNil};

use crate::coproduct::dispatch::{Call, Dispatch};
use crate::coproduct::{CoprodAt, EmbedEffect, ProjectResume};
use crate::coroutine::Yielder;
use crate::effect::{Effect, Effects, InjectResume, MapResume, Resumes};
use crate::execution::{CoproductDispatch, Owned, Scheduler, State, drive_with};
use crate::frame::{FrameRef, Generator, GeneratorState, Operation, RawYield, Signal, Stage};
use crate::program::Program;
use crate::{Cancelled, Control};

// Invocation futures are returned through inherent functions, never through
// trait projections: an auto-trait proof of a body that awaits one then sees
// a concrete type, and needs none of the lifetime relations used to build it.

/// Invoke an unhandled program, forwarding every effect.
#[inline]
pub(crate) fn unhandled<'a, 'b, Effs, SubEffs, F, Mode, Indices>(
    program: Program<'b, SubEffs, F, SubEffs, HNil, (), Mode>,
    parent: &Yielder<'a, Effs>,
) -> impl Future<Output = F::Output> + use<'a, 'b, Effs, SubEffs, F, Mode, Indices>
where
    'a: 'b,
    Effs: MapResume,
    SubEffs: Effects<'b> + EmbedEffect<Effs, Indices>,
    Resumes<'a, Effs>: ProjectResume<'a, SubEffs, Indices>,
    F: Future,
{
    Direct::<'a, 'b, Effs, SubEffs, F, Indices> {
        generator: program.co.generator(),
        parent: parent.frame().operation(),
        indices: PhantomData,
    }
}

/// Invoke a program with handlers attached: they handle its effects locally,
/// and [`Forward`] forwards its remaining ones.
#[allow(clippy::type_complexity)]
pub(crate) fn handled<'a, 'b, Effs, SubEffs, F, Remaining, Handlers, S, Mode, Indices>(
    program: Program<'b, SubEffs, F, Remaining, Handlers, S, Mode>,
    parent: &Yielder<'a, Effs>,
) -> impl Future<Output = F::Output> + use<'a, 'b, Effs, SubEffs, F, Remaining, Handlers, S, Mode, Indices>
where
    'a: 'b,
    Effs: MapResume,
    SubEffs: Effects<'b>
        + for<'call> Dispatch<
            'b,
            SubEffs,
            &'call mut S::Value,
            &'call mut HCons<Forward<'a, 'b, Effs, Remaining>, Handlers>,
            Indices,
        >,
    Remaining: MapResume,
    F: Future,
    S: State,
{
    let Program {
        co,
        state,
        handlers,
        ..
    } = program;
    let handlers = HCons {
        head: Forward {
            parent: parent.frame(),
            shorten: Remaining::shorten_resumes::<'a, 'b>,
        },
        tail: handlers,
    };
    Handled {
        run: drive_with(
            co.generator(),
            state,
            Owned(handlers),
            CoproductDispatch::<'b, SubEffs, Indices>::new(),
            Relay(parent.frame().operation()),
        ),
    }
}

// `invoke!` calls `__invoke` on the program. The two impls are disjoint, so
// method resolution selects the path by whether handlers are attached.
impl<'b, SubEffs: Effects<'b>, F: Future, Mode> Program<'b, SubEffs, F, SubEffs, HNil, (), Mode> {
    #[doc(hidden)]
    #[inline]
    pub fn __invoke<'y, 'a, Effs, Indices>(
        self,
        yielder: &'y mut Yielder<'a, Effs>,
    ) -> impl Future<Output = F::Output> + use<'y, 'a, 'b, Effs, SubEffs, F, Mode, Indices>
    where
        'a: 'b,
        Effs: MapResume,
        SubEffs: EmbedEffect<Effs, Indices>,
        Resumes<'a, Effs>: ProjectResume<'a, SubEffs, Indices>,
    {
        yielder.invoke(self)
    }
}

impl<'b, SubEffs: Effects<'b>, F: Future, Remaining, H, T, S, Mode>
    Program<'b, SubEffs, F, Remaining, HCons<H, T>, S, Mode>
{
    #[doc(hidden)]
    #[inline]
    #[allow(clippy::type_complexity)]
    pub fn __invoke<'y, 'a, Effs, Indices>(
        self,
        yielder: &'y mut Yielder<'a, Effs>,
    ) -> impl Future<Output = F::Output>
    + use<'y, 'a, 'b, Effs, SubEffs, F, Remaining, H, T, S, Mode, Indices>
    where
        'a: 'b,
        Effs: MapResume,
        SubEffs: for<'call> Dispatch<
                'b,
                SubEffs,
                &'call mut S::Value,
                &'call mut HCons<Forward<'a, 'b, Effs, Remaining>, HCons<H, T>>,
                Indices,
            >,
        Remaining: MapResume,
        S: State,
    {
        yielder.invoke_handled(self)
    }
}

pin_project_lite::pin_project! {
    // Store each invoked generator once. Moving a concrete program into an
    // async fn and then pinning it inside that function duplicates its storage
    // in the future layout, which grows exponentially in deep invoke chains.
    struct Direct<'a, 'b, Effs: MapResume, SubEffs: MapResume, F, Indices> {
        #[pin]
        generator: Generator<F, SubEffs, Resumes<'b, SubEffs>>,
        // Moves each child effect or signal into the parent's slot, and each
        // resume back.
        parent: Operation<'a, Effs>,
        indices: PhantomData<fn() -> Indices>,
    }
}

impl<'a, 'b, Effs, SubEffs, F, Indices> Future for Direct<'a, 'b, Effs, SubEffs, F, Indices>
where
    'a: 'b,
    Effs: MapResume,
    SubEffs: MapResume + EmbedEffect<Effs, Indices>,
    Resumes<'a, Effs>: ProjectResume<'a, SubEffs, Indices>,
    F: Future,
{
    type Output = F::Output;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let resume = match this.parent.stage() {
            Stage::Idle => None,
            Stage::Resume => {
                // Project the outer resume back to the sub-effect resume. This
                // produces Resumes<'a, SubEffs> (the outer handler's lifetime).
                let resume: Resumes<'a, SubEffs> = ready!(this.parent.poll_resume(cx)).project();
                // Convert from the outer lifetime 'a to the sub-program's
                // lifetime 'b. Effect::shorten_resume witnesses covariance for
                // every effect.
                Some(SubEffs::shorten_resumes(resume))
            }
            Stage::Signal(Signal::Reschedule) => {
                ready!(this.parent.poll_signal(cx));
                None
            }
            // The root runner aborts the whole computation.
            Stage::Signal(Signal::Cancel) => return Poll::Pending,
        };
        match ready!(this.generator.poll_resume(cx, resume)) {
            GeneratorState::Complete(value) => Poll::Ready(value),
            GeneratorState::Yielded(effect) => {
                // Hand the child's effect to the parent's owner directly; no
                // level of the chain needs a future of its own.
                this.parent.yield_(cx, effect.embed());
                Poll::Pending
            }
            GeneratorState::Signalled(signal) => {
                this.parent.signal(cx, signal);
                Poll::Pending
            }
        }
    }
}

pin_project_lite::pin_project! {
    struct Handled<R> {
        #[pin]
        run: R,
    }
}

impl<R, T> Future for Handled<R>
where
    R: Future<Output = Result<T, Cancelled>>,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        match ready!(self.project().run.poll(cx)) {
            Ok(value) => Poll::Ready(value),
            Err(Cancelled) => unreachable!("a relayed cancellation never completes"),
        }
    }
}

/// Relays a handled sub-program's cancellation and rescheduling to the owner
/// of the parent's frame, which passes them on to the root runner.
struct Relay<'a, Effs: MapResume>(Operation<'a, Effs>);

impl<'a, Effs: MapResume> Scheduler for Relay<'a, Effs> {
    fn poll_reschedule(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if self.0.stage() == Stage::Idle {
            self.0.signal(cx, Signal::Reschedule);
            return Poll::Pending;
        }
        self.0.poll_signal(cx)
    }

    fn poll_cancel(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if self.0.stage() == Stage::Idle {
            self.0.signal(cx, Signal::Cancel);
        }
        // The root runner aborts the whole computation.
        Poll::Pending
    }
}

/// Serves the remaining effects of a handled sub-program by yielding them on
/// the invoking computation's frame.
#[doc(hidden)]
pub struct Forward<'a, 'b, Effs, Remaining: MapResume> {
    parent: FrameRef<'a, Effs>,
    // Created where 'a: 'b holds, so that no impl involving this handler needs
    // that bound. Auto-trait proofs of bodies cannot prove lifetime relations.
    shorten: fn(Resumes<'a, Remaining>) -> Resumes<'b, Remaining>,
}

/// Dispatch kind of [`Forward`]: the effect's index in the remaining effects
/// and in the parent's effects.
#[doc(hidden)]
pub struct Forwarded<RemainingIndex, ParentIndex>(
    PhantomData<fn() -> (RemainingIndex, ParentIndex)>,
);

impl<'a, 'b, State, E, Effs, Remaining, RemainingIndex, ParentIndex>
    Call<State, E, E::Resume<'b>, Forwarded<RemainingIndex, ParentIndex>>
    for &mut Forward<'a, 'b, Effs, Remaining>
where
    E: Effect,
    Effs: MapResume + CoprodInjector<E, ParentIndex>,
    Resumes<'a, Effs>: CoprodUninjector<E::Resume<'a>, ParentIndex>,
    // Only remaining effects are forwarded, so no effect has both a local
    // handler and this one.
    Remaining: InjectResume<'a, E, RemainingIndex>,
    Resumes<'b, Remaining>: CoprodAt<E::Resume<'b>, RemainingIndex>,
{
    type Future = ForwardCall<'a, 'b, Effs, E, Remaining, RemainingIndex, ParentIndex>;

    fn call(self, _: State, effect: E) -> Self::Future {
        ForwardCall {
            raw: RawYield::new(self.parent.operation(), Effs::inject(effect)),
            shorten: self.shorten,
            marker: PhantomData,
        }
    }
}

/// A forwarded effect, waiting for the invoking computation's handler.
#[doc(hidden)]
#[allow(clippy::type_complexity)]
pub struct ForwardCall<
    'a,
    'b,
    Effs: MapResume,
    E,
    Remaining: MapResume,
    RemainingIndex,
    ParentIndex,
> {
    raw: RawYield<'a, Effs>,
    shorten: fn(Resumes<'a, Remaining>) -> Resumes<'b, Remaining>,
    marker: PhantomData<fn() -> (E, RemainingIndex, ParentIndex)>,
}

impl<'a, 'b, Effs, E, Remaining, RemainingIndex, ParentIndex> Future
    for ForwardCall<'a, 'b, Effs, E, Remaining, RemainingIndex, ParentIndex>
where
    Effs: MapResume,
    E: Effect,
    Resumes<'a, Effs>: CoprodUninjector<E::Resume<'a>, ParentIndex>,
    Remaining: InjectResume<'a, E, RemainingIndex>,
    Resumes<'b, Remaining>: CoprodAt<E::Resume<'b>, RemainingIndex>,
{
    type Output = Control<E::Resume<'b>>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let shorten = this.shorten;
        Pin::new(&mut this.raw).poll(cx).map(|resume| {
            let resume = match resume.uninject() {
                Ok(value) => value,
                // The parent's dispatch injects at the forwarded effect's
                // index; keep the boundary checked as in `Yielder::yield_`.
                Err(_) => panic!("handler resumed at the wrong effect index"),
            };
            match shorten(Remaining::inject_resume(resume)).at() {
                Ok(value) => Control::Resume(value),
                Err(_) => unreachable!("shortening keeps the effect's position"),
            }
        })
    }
}
