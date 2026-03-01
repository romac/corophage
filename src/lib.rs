#![doc = include_str!("../README.md")]

mod coproduct;
use coproduct::{FoldMut, FoldWith};

mod control;
mod coroutine;
mod effect;
mod locality;

#[macro_use]
mod macros;

pub mod prelude;

pub use control::{Cancelled, CoControl};
pub use coroutine::{Co, CoSend, GenericCo, Yielder};
pub use effect::Effect;
pub use locality::{Local, Locality, Sendable};

/// An uninhabited type for effects that never resume.
///
/// Use this as `Effect::Resume` for effects that always cancel the computation
/// (e.g., `Cancel`) and therefore can never resume.
pub enum Never {}

pub use asynk::*;

mod asynk {
    use crate::coproduct::{AsyncFoldMut, AsyncFoldWith};
    use crate::locality::Locality;

    use super::*;

    pub async fn run<'a, Effs, Return, L, F>(
        co: GenericCo<'a, Effs, Return, L>,
        handler: &mut F,
    ) -> Result<Return, Cancelled>
    where
        L: Locality,
        Effs: effect::Effects<'a> + AsyncFoldMut<F, CoControl<'a, Effs>>,
    {
        run!('a, Effs, co, effect => effect.fold_mut(handler).await)
    }

    pub async fn run_with<'a, Effs, Return, L, State, F>(
        co: GenericCo<'a, Effs, Return, L>,
        state: &mut State,
        handler: &mut F,
    ) -> Result<Return, Cancelled>
    where
        L: Locality,
        Effs: effect::Effects<'a> + AsyncFoldWith<F, State, CoControl<'a, Effs>>,
    {
        run!('a, Effs, co, effect => effect.fold_with(state, handler).await)
    }
}

pub mod sync {
    use crate::locality::Locality;

    use super::*;

    pub fn run<'a, Effs, Return, L, F>(
        co: GenericCo<'a, Effs, Return, L>,
        handler: &mut F,
    ) -> Result<Return, Cancelled>
    where
        L: Locality,
        Effs: effect::Effects<'a> + FoldMut<F, CoControl<'a, Effs>>,
    {
        run!('a, Effs, co, effect => effect.fold_mut(handler))
    }

    pub fn run_with<'a, Effs, Return, L, State, F>(
        co: GenericCo<'a, Effs, Return, L>,
        state: &mut State,
        handler: &mut F,
    ) -> Result<Return, Cancelled>
    where
        L: Locality,
        Effs: effect::Effects<'a> + FoldWith<F, State, CoControl<'a, Effs>>,
    {
        run!('a, Effs, co, effect => effect.fold_with(state, handler))
    }
}
