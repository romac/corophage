#![doc = include_str!("../README.md")]

mod coproduct;
use coproduct::{FoldMut, FoldWith};

mod effect;
use effect::{Effects, Start};

mod control;
mod coroutine;

pub mod prelude;

pub use control::{Cancelled, CoControl};
pub use coroutine::{Co, Yielder};
pub use effect::Effect;

/// An uninhabited type for effects that never resume.
///
/// Use this as `Effect::Resume` for effects that always cancel the computation
/// (e.g., `Cancel`) and therefore can never resume.
pub enum Never {}

#[macro_export]
macro_rules! Effects {
    [$($effect:ty),*] => {
        ::frunk_core::Coprod!($($effect),*)
    };
}

macro_rules! run {
    ($effs:ty, $co:expr, $effect:pat => $handle:expr) => {{
        use ::frunk_core::coproduct::Coproduct;

        let mut co = std::pin::pin!($co);

        let mut yielded = co.as_mut().resume_with(Start);

        loop {
            match yielded {
                ::fauxgen::GeneratorState::Complete(value) => break Ok(value),

                ::fauxgen::GeneratorState::Yielded(effect) => {
                    let $effect = match effect {
                        Coproduct::Inl(_) => unreachable!(),
                        Coproduct::Inr(subeffect) => subeffect,
                    };

                    let resume: CoControl<$effs> = $handle;
                    match resume {
                        CoControl::Cancel => break Err(Cancelled),
                        CoControl::Resume(r) => yielded = co.as_mut().resume(Coproduct::Inr(r)),
                    }
                }
            }
        }
    }};
}

pub use asynk::*;

mod asynk {
    use crate::coproduct::{AsyncFoldMut, AsyncFoldWith};

    use super::*;

    pub async fn run<Effs, Return, F>(
        co: Co<Effs, Return>,
        handler: &mut F,
    ) -> Result<Return, Cancelled>
    where
        Effs: Effects + AsyncFoldMut<F, CoControl<Effs>>,
    {
        run!(Effs, co, effect => effect.fold_mut(handler).await)
    }

    pub async fn run_with<Effs, Return, State, F>(
        co: Co<Effs, Return>,
        state: &mut State,
        handler: &mut F,
    ) -> Result<Return, Cancelled>
    where
        Effs: Effects + AsyncFoldWith<F, State, CoControl<Effs>>,
    {
        run!(Effs, co, effect => effect.fold_with(state, handler).await)
    }
}

pub mod sync {
    use super::*;

    pub fn run<Effs, Return, F>(co: Co<Effs, Return>, handler: &mut F) -> Result<Return, Cancelled>
    where
        Effs: Effects + FoldMut<F, CoControl<Effs>>,
    {
        run!(Effs, co, effect => effect.fold_mut(handler))
    }

    pub fn run_with<Effs, Return, State, F>(
        co: Co<Effs, Return>,
        state: &mut State,
        handler: &mut F,
    ) -> Result<Return, Cancelled>
    where
        Effs: Effects + FoldWith<F, State, CoControl<Effs>>,
    {
        run!(Effs, co, effect => effect.fold_with(state, handler))
    }
}
