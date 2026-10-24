//! Typed errors as an effect: [`Throw`] aborts a computation with an error, and
//! [`Program::catch`] makes that error the program's result.

use std::future::{Future, Ready};
use std::marker::PhantomData;
use std::ops::Add;

use frunk_core::coproduct::{Coproduct, CoproductSubsetter};
use frunk_core::hlist::{HCons, HNil};

use crate::coproduct::dispatch::{Call, Synchronous};
use crate::coproduct::{CoprodAt, HandlersToEffects};
use crate::effect::{Effect, Effects, InjectResume};
use crate::program::{HandleEffects, Program};
use crate::{Control, Never};

/// An effect that aborts the computation with an error.
///
/// `Throw<E>` never resumes. Throw from a body with [`Yielder::throw`] or,
/// inside an `#[effectful]` function, with `throw!(error)`. Attach
/// [`Program::catch`] to make the error the program's result, or handle
/// `Throw<E>` like any other effect.
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

/// Handler entry installed by [`Program::catch`]. The caught body takes every
/// throw before dispatch, so this handler is never called.
#[doc(hidden)]
pub struct CatchHandler<E>(PhantomData<fn() -> E>);

/// Dispatch kind of [`CatchHandler`].
#[doc(hidden)]
pub struct Caught;

impl Synchronous for Caught {}

impl<State, E> Call<State, Throw<E>, Never, Caught> for &mut CatchHandler<E> {
    type Future = Ready<Control<Never>>;
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn call(self, _: State, _: Throw<E>) -> Self::Future {
        unreachable!("a caught throw is taken by its program body")
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

impl<'a, Effs: Effects<'a>, F: Future, Remaining, Handlers, S, Mode>
    Program<'a, Effs, F, Remaining, Handlers, S, Mode>
{
    /// Handle [`Throw<E>`] by completing the program with the error.
    ///
    /// The caught program returns `Ok(value)` on completion and `Err(error)`
    /// as soon as its body throws, dropping the rest of the body. A throw
    /// from an invoked sub-program is caught too. Its run returns
    /// `Ok(Ok(value))`, `Ok(Err(error))`, or `Err(Cancelled)` when another
    /// handler cancels. Use `?` to propagate cancellation and keep the thrown
    /// error:
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
    /// Handlers and state can be attached before or after `catch`. Like
    /// [`map`](Program::map), the caught program uses inferred execution.
    #[allow(clippy::type_complexity)]
    pub fn catch<E, ThrowIdx, HandleIdx, SubsetIdx>(
        self,
    ) -> Program<
        'a,
        Effs,
        impl Future<Output = Result<F::Output, E>>,
        HandleEffects<'a, Remaining, HCons<CatchHandler<E>, HNil>, Effs, HandleIdx, SubsetIdx>,
        <Handlers as Add<HCons<CatchHandler<E>, HNil>>>::Output,
        S,
    >
    where
        Effs: CoprodAt<Throw<E>, ThrowIdx>,
        HCons<CatchHandler<E>, HNil>: HandlersToEffects<'a, Effs, HandleIdx>,
        Remaining: CoproductSubsetter<
                <HCons<CatchHandler<E>, HNil> as HandlersToEffects<'a, Effs, HandleIdx>>::Effects,
                SubsetIdx,
            >,
        Handlers: Add<HCons<CatchHandler<E>, HNil>>,
    {
        self.handle(CatchHandler(PhantomData))
            .with_body(|co| co.intercept(|effect: Effs| effect.at().map(|Throw(error)| error)))
    }
}
