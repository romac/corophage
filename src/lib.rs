#![doc = include_str!("../README.md")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod coproduct;
use coproduct::{FoldMut, FoldWith};

mod control;
mod coroutine;
mod effect;
mod locality;
mod program;

#[macro_use]
mod macros;

/// Re-exports of the most commonly used types and traits.
pub mod prelude;

pub use control::{Cancelled, CoControl};
pub use coroutine::{Co, CoSend, Yielder};
pub use effect::Effect;
pub use program::{Program, handle};

/// An uninhabited type for effects that never resume.
///
/// Use this as `Effect::Resume` for effects that always cancel the computation
/// (e.g., `Cancel`) and therefore can never resume.
pub enum Never {}

/// Async effect runners.
///
/// Use these functions to run a coroutine with async effect handlers.
/// For most use cases, prefer [`Program::run`] instead.
pub mod asynk {
    use crate::coproduct::{AsyncFoldMut, AsyncFoldWith};
    use crate::coroutine::GenericCo;
    use crate::effect::Effects;
    use crate::locality::Locality;

    use super::*;

    /// Run a coroutine with an hlist of async handlers.
    pub async fn run<'a, ES, R, L, F>(
        co: GenericCo<'a, ES, R, L>,
        handler: &mut F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + AsyncFoldMut<F, CoControl<'a, ES>>,
    {
        run!('a, ES, co, effect => effect.fold_mut(handler).await)
    }

    /// Run a coroutine with an hlist of async handlers and shared mutable state.
    pub async fn run_stateful<'a, ES, R, L, S, F>(
        co: GenericCo<'a, ES, R, L>,
        state: &mut S,
        handler: &mut F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + AsyncFoldWith<F, S, CoControl<'a, ES>>,
    {
        run!('a, ES, co, effect => effect.fold_with(state, handler).await)
    }
}

/// Synchronous effect runners.
///
/// Use these functions to run a coroutine with synchronous effect handlers.
/// For most use cases, prefer [`Program::run_sync`] instead.
pub mod sync {
    use crate::coroutine::GenericCo;
    use crate::effect::Effects;
    use crate::locality::Locality;

    use super::*;

    /// Run a coroutine with an hlist of synchronous handlers.
    pub fn run<'a, ES, R, L, F>(
        co: GenericCo<'a, ES, R, L>,
        handler: &mut F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + FoldMut<F, CoControl<'a, ES>>,
    {
        run!('a, ES, co, effect => effect.fold_mut(handler))
    }

    /// Run a coroutine with an hlist of synchronous handlers and shared mutable state.
    pub fn run_stateful<'a, ES, R, L, S, F>(
        co: GenericCo<'a, ES, R, L>,
        state: &mut S,
        handler: &mut F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + FoldWith<F, S, CoControl<'a, ES>>,
    {
        run!('a, ES, co, effect => effect.fold_with(state, handler))
    }
}
