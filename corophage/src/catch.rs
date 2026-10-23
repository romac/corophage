//! Typed errors as an effect: [`Throw`] aborts a computation with an error, and
//! [`Program::catch`] returns that error from the run.

use std::future::{Future, Ready, ready};
use std::marker::PhantomData;
use std::ops::Add;

use frunk_core::coproduct::{CNil, Coproduct, CoproductSubsetter};
use frunk_core::hlist::{HCons, HNil, Selector};

use crate::coproduct::HandlersToEffects;
use crate::coproduct::dispatch::{Call, Dispatch, Synchronous};
use crate::effect::{Effect, Effects, InjectResume};
use crate::execution::{CoproductDispatch, State, drive, drive_inline};
use crate::program::{HandleEffects, Program};
use crate::{Cancelled, Control, Never};

/// An effect that aborts the computation with an error.
///
/// `Throw<E>` never resumes. Throw from a body with [`Yielder::throw`] or,
/// inside an `#[effectful]` function, with `throw!(error)`. Attach
/// [`Program::catch`] to return the error from the run as `Ok(Err(error))`,
/// or handle `Throw<E>` like any other effect.
///
/// [`Yielder::throw`]: crate::Yielder::throw
pub struct Throw<E>(pub E);

impl<E> Effect for Throw<E> {
    type Resume<'r> = Never;

    #[inline]
    fn shorten_resume<'a: 'b, 'b>(resume: Never) -> Never {
        resume
    }
}

/// Handler installed by [`Program::catch`]. It stores the thrown error and
/// cancels; the caught runner reads the error back after execution.
#[doc(hidden)]
pub struct CatchHandler<E> {
    thrown: Option<E>,
}

/// Dispatch kind of [`CatchHandler`].
#[doc(hidden)]
pub struct Caught;

impl Synchronous for Caught {}

impl<State, E> Call<State, Throw<E>, Never, Caught> for &mut CatchHandler<E> {
    type Future = Ready<Control<Never>>;
    fn call(self, _: State, Throw(error): Throw<E>) -> Self::Future {
        self.thrown = Some(error);
        ready(Control::Cancel)
    }
}

impl<'a, Effs, E, Tail, Index, ITail>
    HandlersToEffects<'a, Effs, HCons<(Throw<E>, Index, Caught), ITail>>
    for HCons<CatchHandler<E>, Tail>
where
    Effs: Effects<'a> + InjectResume<'a, Throw<E>, Index>,
    Tail: HandlersToEffects<'a, Effs, ITail>,
{
    type Effects = Coproduct<Throw<E>, Tail::Effects>;
}

/// A program whose thrown errors of type `E` are returned by its run.
///
/// Created by [`Program::catch`]. Run it with [`Catch::run`] or [`Catch::run_sync`].
pub struct Catch<P, E> {
    program: P,
    error: PhantomData<fn() -> E>,
}

impl<'a, Effs: Effects<'a>, F, Remaining, Handlers, S, Mode>
    Program<'a, Effs, F, Remaining, Handlers, S, Mode>
{
    /// Handle [`Throw<E>`] by returning the error from the run.
    ///
    /// The run returns `Ok(Ok(value))` on completion, `Ok(Err(error))` when the
    /// body throws, and `Err(Cancelled)` when another handler cancels. Use `?`
    /// to propagate cancellation and keep the thrown error:
    ///
    /// ```
    /// use corophage::prelude::*;
    ///
    /// struct NotFound;
    ///
    /// let program = Program::new(|mut y: Yielder<'_, Effects![Throw<NotFound>]>| async move {
    ///     let user: &str = y.throw(NotFound).await;
    ///     user.len()
    /// });
    ///
    /// let result = program.catch().run_sync();
    /// assert!(matches!(result, Ok(Err(NotFound))));
    /// ```
    ///
    /// Attach other handlers and bind state before calling `catch`.
    #[allow(clippy::type_complexity)]
    pub fn catch<E, HandleIdx, SubsetIdx>(
        self,
    ) -> Catch<
        Program<
            'a,
            Effs,
            F,
            HandleEffects<'a, Remaining, HCons<CatchHandler<E>, HNil>, Effs, HandleIdx, SubsetIdx>,
            <Handlers as Add<HCons<CatchHandler<E>, HNil>>>::Output,
            S,
            Mode,
        >,
        E,
    >
    where
        HCons<CatchHandler<E>, HNil>: HandlersToEffects<'a, Effs, HandleIdx>,
        Remaining: CoproductSubsetter<
                <HCons<CatchHandler<E>, HNil> as HandlersToEffects<'a, Effs, HandleIdx>>::Effects,
                SubsetIdx,
            >,
        Handlers: Add<HCons<CatchHandler<E>, HNil>>,
    {
        Catch {
            program: self.handle(CatchHandler { thrown: None }),
            error: PhantomData,
        }
    }
}

impl<'a, Effs: Effects<'a>, F: Future, H, S: State, Mode, E>
    Catch<Program<'a, Effs, F, CNil, H, S, Mode>, E>
{
    /// Execute with cooperative scheduling, returning a thrown error as
    /// `Ok(Err(error))`. `Send` follows the stored components, as for
    /// [`Program::run`].
    pub fn run<I, Sel>(self) -> impl Future<Output = Result<Result<F::Output, E>, Cancelled>>
    where
        Effs: for<'call> Dispatch<'a, Effs, &'call mut S::Value, &'call mut H, I>,
        H: Selector<CatchHandler<E>, Sel>,
    {
        let Program {
            co,
            state,
            mut handlers,
            ..
        } = self.program;
        async move {
            let result = drive(
                co.generator(),
                state,
                &mut handlers,
                CoproductDispatch::<'a, Effs, I>::new(),
            )
            .await;
            caught(result, handlers.get_mut().thrown.take())
        }
    }

    /// Execute inline with synchronous handlers, returning a thrown error as
    /// `Ok(Err(error))`.
    ///
    /// # Panics
    ///
    /// Panics if the computation suspends on an ordinary future, as in
    /// [`Program::run_sync`].
    pub fn run_sync<I, Sel>(self) -> Result<Result<F::Output, E>, Cancelled>
    where
        Effs: for<'call> Dispatch<'a, Effs, &'call mut S::Value, &'call mut H, I>,
        I: Synchronous,
        H: Selector<CatchHandler<E>, Sel>,
    {
        let Program {
            co,
            state,
            mut handlers,
            ..
        } = self.program;
        let result = drive_inline(
            co.generator(),
            state,
            &mut handlers,
            CoproductDispatch::<'a, Effs, I>::new(),
        );
        caught(result, handlers.get_mut().thrown.take())
    }
}

/// A cancellation that stored an error was a throw; any other is a cancellation.
fn caught<R, E>(
    result: Result<R, Cancelled>,
    thrown: Option<E>,
) -> Result<Result<R, E>, Cancelled> {
    match (result, thrown) {
        (Ok(value), _) => Ok(Ok(value)),
        (Err(Cancelled), Some(error)) => Ok(Err(error)),
        (Err(Cancelled), None) => Err(Cancelled),
    }
}
