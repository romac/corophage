//! Two execution loops share one dispatch protocol and state owner: a
//! cooperative async loop, and an inline loop for synchronous dispatch.

use std::future::Future;
use std::marker::PhantomData;
use std::task::{Context, Poll, Waker};

use fauxgen::{AsyncGenerator, GeneratorState};
use frunk_core::coproduct::Coproduct;

use crate::coproduct::dispatch::Dispatch;
use crate::effect::{Effects, Resumes, Start};
use crate::{Cancelled, Control};

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

pub(crate) async fn drive<G, S, H, D, E, A>(
    generator: G,
    mut state: S,
    mut handlers: H,
    dispatch: D,
) -> Result<G::Return, Cancelled>
where
    G: AsyncGenerator<Coproduct<Start, A>, Yield = Coproduct<Start, E>>,
    S: State,
    D: for<'call> Dispatcher<CallContext<'call, S::Value, H>, E, A>,
{
    let mut generator = std::pin::pin!(generator);
    let mut step = generator.as_mut().resume(Coproduct::Inl(Start)).await;
    let mut budget = crate::ASYNC_DISPATCH_BUDGET;
    loop {
        match step {
            GeneratorState::Complete(value) => return Ok(value),
            GeneratorState::Yielded(effect) => {
                let effect = match effect {
                    Coproduct::Inr(effect) => effect,
                    Coproduct::Inl(_) => unreachable!("Start must not be yielded"),
                };
                // Completion at the budget boundary needs no extra poll.
                if budget == 0 {
                    crate::yield_to_executor().await;
                    budget = crate::ASYNC_DISPATCH_BUDGET;
                }
                budget -= 1;
                match dispatch
                    .dispatch(
                        effect,
                        CallContext {
                            state: state.value_mut(),
                            handlers: &mut handlers,
                        },
                    )
                    .await
                {
                    Control::Cancel => return Err(Cancelled),
                    Control::Resume(value) => {
                        step = generator.as_mut().resume(Coproduct::Inr(value)).await;
                    }
                }
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
pub(crate) fn drive_inline<G, S, H, D, E, A>(
    generator: G,
    mut state: S,
    mut handlers: H,
    dispatch: D,
) -> Result<G::Return, Cancelled>
where
    G: AsyncGenerator<Coproduct<Start, A>, Yield = Coproduct<Start, E>>,
    S: State,
    D: for<'call> Dispatcher<CallContext<'call, S::Value, H>, E, A>,
{
    let mut generator = std::pin::pin!(generator);
    let mut context = Context::from_waker(Waker::noop());
    let mut argument = Some(Coproduct::Inl(Start));
    loop {
        let effect = match generator
            .as_mut()
            .poll_resume(&mut context, argument.take())
        {
            Poll::Ready(GeneratorState::Complete(value)) => return Ok(value),
            Poll::Ready(GeneratorState::Yielded(Coproduct::Inr(effect))) => effect,
            Poll::Ready(GeneratorState::Yielded(Coproduct::Inl(_))) => {
                unreachable!("Start must not be yielded")
            }
            Poll::Pending => panic!(
                "computation suspended on a non-effect future; use an async runner instead. \
                 Nested `.run().await` calls also suspend after a bounded number of effects; \
                 use `invoke` or `.run_sync()` for sub-programs of a synchronous program"
            ),
        };
        let future = std::pin::pin!(dispatch.dispatch(
            effect,
            CallContext {
                state: state.value_mut(),
                handlers: &mut handlers,
            },
        ));
        match future.poll(&mut context) {
            Poll::Ready(Control::Cancel) => return Err(Cancelled),
            Poll::Ready(Control::Resume(value)) => argument = Some(Coproduct::Inr(value)),
            Poll::Pending => unreachable!("synchronous dispatch is always ready"),
        }
    }
}

/// Preserve the explicit guarantee of the legacy sendable low-level functions.
/// All proofs here concern concrete types; the main loop has no Send bounds.
pub(crate) fn drive_send<G, S, H, D, E, A>(
    generator: G,
    state: S,
    handlers: H,
    dispatch: D,
) -> impl Future<Output = Result<G::Return, Cancelled>> + Send
where
    G: AsyncGenerator<Coproduct<Start, A>, Yield = Coproduct<Start, E>> + Send,
    G::Return: Send,
    S: State + Send,
    S::Value: Send,
    H: Send,
    D: Send + for<'c> Dispatcher<CallContext<'c, S::Value, H>, E, A>,
    for<'c> <D as Dispatcher<CallContext<'c, S::Value, H>, E, A>>::Future: Send,
    E: Send,
    A: Send,
{
    drive(generator, state, handlers, dispatch)
}
