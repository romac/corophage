#![doc = include_str!("../README.md")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod coproduct;
use coproduct::{FoldMut, FoldWith};

mod control;
mod coroutine;
mod effect;
mod locality;
pub mod program;

#[macro_use]
mod macros;

pub mod prelude;

pub use control::{Cancelled, CoControl};
pub use coroutine::{Co, CoSend, GenericCo, Yielder};
pub use effect::Effect;
pub use locality::{Local, Locality, Sendable};
pub use program::Program;

/// An uninhabited type for effects that never resume.
///
/// Use this as `Effect::Resume` for effects that always cancel the computation
/// (e.g., `Cancel`) and therefore can never resume.
pub enum Never {}

pub use asynk::*;

mod asynk {
    use crate::coproduct::{AsyncFoldMut, AsyncFoldWith};
    use crate::effect::Effects;
    use crate::locality::Locality;

    use super::*;

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

pub mod sync {
    use crate::effect::Effects;
    use crate::locality::Locality;

    use super::*;

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
