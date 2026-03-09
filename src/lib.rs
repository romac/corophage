#![doc = include_str!("../README.md")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod coproduct;
use coproduct::{AsyncHandleMut, AsyncHandleWith, HandleMut, HandleWith};

mod control;
mod effect;
mod locality;
mod program;

#[macro_use]
mod macros;

/// Re-exports of the most commonly used types and traits.
pub mod prelude;

pub mod coroutine;

pub use control::{Cancelled, Control};
pub use coroutine::Yielder;
pub use effect::Effect;
pub use locality::{Local, Locality, Sendable};
pub use program::Program;

/// Internal macro for running a coroutine with effect handlers.
macro_rules! run {
    ($lt:lifetime, $effs:ty, $co:expr, $effect:pat => $handle:expr) => {{
        let mut co = ::std::pin::pin!($co);

        let mut yielded = co.as_mut().resume_with($crate::effect::Start);

        loop {
            match yielded {
                ::fauxgen::GeneratorState::Complete(value) => break Ok(value),

                ::fauxgen::GeneratorState::Yielded(effect) => {
                    let $effect = match effect {
                        ::frunk_core::coproduct::Coproduct::Inl(_) => unreachable!(),
                        ::frunk_core::coproduct::Coproduct::Inr(subeffect) => subeffect,
                    };

                    let resume: $crate::control::CoControl<$lt, $effs> = $handle;
                    match resume {
                        $crate::control::CoControl::Cancel => {
                            break Err($crate::control::Cancelled);
                        }
                        $crate::control::CoControl::Resume(r) => {
                            yielded = co
                                .as_mut()
                                .resume(::frunk_core::coproduct::Coproduct::Inr(r))
                        }
                    }
                }
            }
        }
    }};
}

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
    use crate::coroutine::GenericCo;
    use crate::effect::Effects;
    use crate::locality::Locality;

    use super::*;

    /// Run a coroutine with an hlist of async handlers.
    #[doc(hidden)]
    pub async fn run<'a, ES, R, L, F, Indices>(
        co: GenericCo<'a, ES, R, L>,
        handler: &mut F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + AsyncHandleMut<'a, ES, F, Indices>,
    {
        run!('a, ES, co, effect => effect.handle_mut(handler).await)
    }

    /// Run a coroutine with an hlist of async handlers and shared mutable state.
    #[doc(hidden)]
    pub async fn run_stateful<'a, ES, R, L, S, F, Indices>(
        co: GenericCo<'a, ES, R, L>,
        state: &mut S,
        handler: &F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + AsyncHandleWith<'a, ES, F, S, Indices>,
    {
        run!('a, ES, co, effect => effect.handle_with(state, handler).await)
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
    #[doc(hidden)]
    pub fn run<'a, ES, R, L, F, Indices>(
        co: GenericCo<'a, ES, R, L>,
        handler: &mut F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + HandleMut<'a, ES, F, Indices>,
    {
        run!('a, ES, co, effect => effect.handle_mut(handler))
    }

    /// Run a coroutine with an hlist of synchronous handlers and shared mutable state.
    #[doc(hidden)]
    pub fn run_stateful<'a, ES, R, L, S, F, Indices>(
        co: GenericCo<'a, ES, R, L>,
        state: &mut S,
        handler: &F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + HandleWith<'a, ES, F, S, Indices>,
    {
        run!('a, ES, co, effect => effect.handle_with(state, handler))
    }
}
