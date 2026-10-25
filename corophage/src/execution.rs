//! Two execution loops share one dispatch protocol and state owner: a
//! cooperative async loop, and an inline loop for synchronous dispatch.

use std::cell::Cell;
use std::future::{Future, poll_fn};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use crate::coproduct::dispatch::Dispatch;
use crate::effect::{Effects, Resumes};
use crate::frame::{Generator, GeneratorState, Signal};
use crate::{Cancelled, Control};

thread_local! {
    // Invocations execute within their root's poll, even when a short child
    // completes without sending an effect or signal back to the root.
    static DISPATCH_BUDGET: Cell<usize> = const { Cell::new(0) };
}

/// Scope the budget shared by a root's invocations. Independent nested runners
/// restore the caller's budget on every exit, including unwinding. Async roots
/// enter once per poll, so no thread-local state survives a suspended poll.
struct Budget {
    previous: usize,
}

impl Budget {
    fn enter() -> Self {
        Self {
            previous: DISPATCH_BUDGET.with(|budget| budget.replace(crate::ASYNC_DISPATCH_BUDGET)),
        }
    }

    fn reset() {
        DISPATCH_BUDGET.with(|budget| budget.set(crate::ASYNC_DISPATCH_BUDGET));
    }

    #[inline(always)]
    fn consume() -> bool {
        DISPATCH_BUDGET.with(|budget| match budget.get().checked_sub(1) {
            Some(remaining) => {
                budget.set(remaining);
                true
            }
            None => false,
        })
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        DISPATCH_BUDGET.with(|budget| budget.set(self.previous));
    }
}

pin_project_lite::pin_project! {
    /// Establish the budget around the whole root poll, including dispatches.
    struct Cooperative<F> {
        #[pin]
        future: F,
    }
}

impl<F: Future> Future for Cooperative<F> {
    type Output = F::Output;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let _budget = Budget::enter();
        self.project().future.poll(cx)
    }
}

/// One state owner lends a fresh mutable reference to each handler call.
#[doc(hidden)]
pub trait State {
    type Value;
    fn value_mut(&mut self) -> &mut Self::Value;
}

impl State for () {
    type Value = ();
    fn value_mut(&mut self) -> &mut () {
        self
    }
}

impl<S> State for &mut S {
    type Value = S;
    fn value_mut(&mut self) -> &mut S {
        self
    }
}

/// The owner of a handler list: the list itself, or a borrow of it. A runner
/// that lends its list can inspect the handlers after execution.
pub(crate) trait HandlerOwner {
    type List;
    fn list(&mut self) -> &mut Self::List;
}

/// An owned handler list, consumed by the execution loop.
pub(crate) struct Owned<H>(pub(crate) H);

impl<H> HandlerOwner for Owned<H> {
    type List = H;
    fn list(&mut self) -> &mut H {
        &mut self.0
    }
}

// Lending the list lets a runner read handler state after execution.
impl<H> HandlerOwner for &mut H {
    type List = H;
    fn list(&mut self) -> &mut H {
        self
    }
}

/// Keep resume and effect types independent of GAT projections in the async
/// loop. This makes the concrete dispatch and resume futures' auto traits
/// available without rustc's higher-ranked opaque-future limitation (#100013).
pub(crate) trait Dispatcher<Context, E, A> {
    type Future: Future<Output = Control<A>>;
    fn dispatch(&self, effect: E, context: Context) -> Self::Future;
}

#[allow(clippy::type_complexity)]
pub(crate) struct CoproductDispatch<'a, Effs, I>(PhantomData<fn() -> (&'a (), Effs, I)>);

impl<'a, Effs, I> CoproductDispatch<'a, Effs, I> {
    pub(crate) fn new() -> Self {
        Self(PhantomData)
    }
}

/// A single dispatch loan. Its state and handler references can come from
/// independently borrowed owners; both are reborrowed only for this call.
/// Keeping the loan in one type also avoids rustc generalizing its two reference
/// lifetimes independently when proving the enclosing future's auto traits.
pub(crate) struct CallContext<'call, S, H> {
    state: &'call mut S,
    handlers: &'call mut H,
}

impl<'a, 'call, Effs, S, H, I> Dispatcher<CallContext<'call, S, H>, Effs, Resumes<'a, Effs>>
    for CoproductDispatch<'a, Effs, I>
where
    Effs: Effects<'a> + Dispatch<'a, Effs, &'call mut S, &'call mut H, I>,
{
    type Future = <Effs as Dispatch<'a, Effs, &'call mut S, &'call mut H, I>>::Future;
    fn dispatch(&self, effect: Effs, context: CallContext<'call, S, H>) -> Self::Future {
        effect.dispatch(context.state, context.handlers)
    }
}

/// What the cooperative loop does when it must give other work a chance to
/// run, or when the computation is cancelled.
pub(crate) trait Scheduler {
    /// Complete once other work had a chance to run.
    fn poll_reschedule(&mut self, cx: &mut Context<'_>) -> Poll<()>;

    /// Complete once the loop may return `Err(Cancelled)`.
    fn poll_cancel(&mut self, cx: &mut Context<'_>) -> Poll<()>;
}

/// A root runner yields to the executor and returns cancellations at once.
#[derive(Default)]
pub(crate) struct Root {
    yielded: bool,
}

impl Scheduler for Root {
    fn poll_reschedule(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if std::mem::take(&mut self.yielded) {
            return Poll::Ready(());
        }
        self.yielded = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }

    fn poll_cancel(&mut self, _: &mut Context<'_>) -> Poll<()> {
        Poll::Ready(())
    }
}

pub(crate) fn drive<F, S, H, D, E, A>(
    generator: Generator<F, E, A>,
    state: S,
    handlers: H,
    dispatch: D,
) -> impl Future<Output = Result<F::Output, Cancelled>>
where
    F: Future,
    S: State,
    H: HandlerOwner,
    D: for<'call> Dispatcher<CallContext<'call, S::Value, H::List>, E, A>,
{
    Cooperative {
        future: drive_with(generator, state, handlers, dispatch, Root::default()),
    }
}

/// The cooperative loop, shared by root runners and handled invocations.
// An `async fn` would move the generator argument into a pinned local and keep
// both copies in its state, about doubling each run and invocation future.
#[allow(clippy::manual_async_fn)]
pub(crate) fn drive_with<F, S, H, D, E, A, C>(
    mut generator: Generator<F, E, A>,
    mut state: S,
    mut handlers: H,
    dispatch: D,
    mut scheduler: C,
) -> impl Future<Output = Result<F::Output, Cancelled>>
where
    F: Future,
    S: State,
    H: HandlerOwner,
    D: for<'call> Dispatcher<CallContext<'call, S::Value, H::List>, E, A>,
    C: Scheduler,
{
    async move {
        // SAFETY: The block is pinned once polled, and the shadowed generator
        // is never moved again. Pinning it where it was captured keeps a
        // single copy of a possibly large body in the block's state.
        let mut generator = unsafe { Pin::new_unchecked(&mut generator) };
        let mut stepper = Stepper {
            resume: None,
            pending: None,
            rescheduling: false,
        };
        loop {
            match poll_fn(|cx| stepper.poll_step(generator.as_mut(), &mut scheduler, cx)).await {
                Next::Complete(value) => return Ok(value),
                Next::Effect(effect) => {
                    let context = CallContext {
                        state: state.value_mut(),
                        handlers: handlers.list(),
                    };
                    match dispatch.dispatch(effect, context).await {
                        Control::Resume(value) => stepper.resume = Some(value),
                        Control::Cancel => break,
                    }
                }
                Next::Cancel => break,
            }
        }
        poll_fn(|cx| scheduler.poll_cancel(cx)).await;
        Err(Cancelled)
    }
}

/// What the cooperative loop does after a step.
enum Next<E, R> {
    Effect(E),
    Complete(R),
    Cancel,
}

/// Resumes the body and polls it to its next effect, completion, or
/// cancellation. Rescheduling, whether the dispatch budget runs out or a
/// nested invocation asks for it, happens inside the step, so the loop only
/// awaits steps and dispatches.
struct Stepper<E, A> {
    resume: Option<A>,
    // An effect held back while the scheduler runs at the budget boundary.
    pending: Option<E>,
    rescheduling: bool,
}

impl<E, A> Stepper<E, A> {
    // Inlining the step into the loop's poll keeps both concrete and boxed
    // bodies fast; without it, measured loops lost up to 15%.
    #[inline(always)]
    fn poll_step<F: Future, C: Scheduler>(
        &mut self,
        mut generator: Pin<&mut Generator<F, E, A>>,
        scheduler: &mut C,
        cx: &mut Context<'_>,
    ) -> Poll<Next<E, F::Output>> {
        loop {
            if self.rescheduling {
                std::task::ready!(scheduler.poll_reschedule(cx));
                self.rescheduling = false;
                if let Some(effect) = self.pending.take() {
                    if Budget::consume() {
                        return Poll::Ready(Next::Effect(effect));
                    }
                    // Another invocation may have spent the renewed budget
                    // before a combinator re-polled this one.
                    self.pending = Some(effect);
                    self.rescheduling = true;
                    continue;
                }
            }
            match std::task::ready!(generator.as_mut().poll_resume(cx, self.resume.take())) {
                GeneratorState::Complete(value) => return Poll::Ready(Next::Complete(value)),
                GeneratorState::Yielded(effect) => {
                    // Completion at the budget boundary needs no extra poll.
                    if Budget::consume() {
                        return Poll::Ready(Next::Effect(effect));
                    } else {
                        self.pending = Some(effect);
                        self.rescheduling = true;
                    }
                }
                // A nested invocation used its budget of local dispatches.
                GeneratorState::Signalled(Signal::Reschedule) => self.rescheduling = true,
                GeneratorState::Signalled(Signal::Cancel) => return Poll::Ready(Next::Cancel),
            }
        }
    }
}

/// Poll the generator and each dispatch future directly, without an async
/// state machine. The generator is pinned once, in place, which avoids moving
/// large concrete bodies into an enclosing future.
///
/// Callers must restrict dispatch to [`Synchronous`] handlers, whose futures
/// are always ready.
///
/// [`Synchronous`]: crate::coproduct::dispatch::Synchronous
pub(crate) fn drive_inline<F, S, H, D, E, A>(
    generator: Generator<F, E, A>,
    mut state: S,
    mut handlers: H,
    dispatch: D,
) -> Result<F::Output, Cancelled>
where
    F: Future,
    S: State,
    H: HandlerOwner,
    D: for<'call> Dispatcher<CallContext<'call, S::Value, H::List>, E, A>,
{
    let mut generator = std::pin::pin!(generator);
    let mut context = Context::from_waker(Waker::noop());
    let mut argument = None;
    let _budget = Budget::enter();
    loop {
        let effect = match generator
            .as_mut()
            .poll_resume(&mut context, argument.take())
        {
            Poll::Ready(GeneratorState::Complete(value)) => return Ok(value),
            Poll::Ready(GeneratorState::Yielded(effect)) => effect,
            Poll::Ready(GeneratorState::Signalled(Signal::Cancel)) => return Err(Cancelled),
            // Local invocations still signal at budget boundaries, allowing
            // combinators to observe them. A sync root resumes immediately.
            Poll::Ready(GeneratorState::Signalled(Signal::Reschedule)) => {
                Budget::reset();
                continue;
            }
            Poll::Pending => panic!(
                "computation suspended on a non-effect future; use an async runner instead. \
                 So do async handlers attached to an invoked sub-program when they suspend. \
                 Nested `.run().await` calls also suspend after a bounded number of effects; \
                 use `invoke` or `.run_sync()` for sub-programs of a synchronous program"
            ),
        };
        let future = std::pin::pin!(dispatch.dispatch(
            effect,
            CallContext {
                state: state.value_mut(),
                handlers: handlers.list(),
            },
        ));
        match future.poll(&mut context) {
            Poll::Ready(Control::Cancel) => return Err(Cancelled),
            Poll::Ready(Control::Resume(value)) => argument = Some(value),
            Poll::Pending => unreachable!("synchronous dispatch is always ready"),
        }
    }
}

/// Preserve the explicit guarantee of the legacy sendable low-level functions.
/// All proofs here concern concrete types; the main loop has no Send bounds.
pub(crate) fn drive_send<F, S, H, D, E, A>(
    generator: Generator<F, E, A>,
    state: S,
    handlers: H,
    dispatch: D,
) -> impl Future<Output = Result<F::Output, Cancelled>> + Send
where
    F: Future + Send,
    F::Output: Send,
    S: State + Send,
    S::Value: Send,
    H: HandlerOwner + Send,
    D: Send + for<'c> Dispatcher<CallContext<'c, S::Value, H::List>, E, A>,
    for<'c> <D as Dispatcher<CallContext<'c, S::Value, H::List>, E, A>>::Future: Send,
    E: Send,
    A: Send,
{
    drive(generator, state, handlers, dispatch)
}
