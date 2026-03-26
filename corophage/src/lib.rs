#![doc = include_str!("../README.md")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

/// Unsafe unreachable hint that panics in debug builds instead of causing UB.
///
/// In release builds, this compiles to `core::hint::unreachable_unchecked()`.
/// In debug builds, it panics with the provided message, making invariant
/// violations easier to diagnose.
macro_rules! debug_unreachable {
    ($($msg:tt)*) => {
        if cfg!(debug_assertions) {
            unreachable!($($msg)*)
        } else {
            unsafe { ::core::hint::unreachable_unchecked() }
        }
    }
}

mod coproduct;
use coproduct::{AsyncHandleMut, AsyncHandleWith, HandleMut, HandleWith};

#[doc(hidden)]
pub use coproduct::ForwardEffects;

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
#[allow(deprecated)]
pub use program::{Eff, Effectful, Program};

#[cfg(feature = "macros")]
pub use corophage_macros::{effect, effectful};

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
                        // INVARIANT: Yielder::yield_ always wraps effects in Inr,
                        // so the Inl (Start) arm is never yielded after init.
                        ::frunk_core::coproduct::Coproduct::Inl(_) => debug_unreachable!(
                            "Start (Inl) arm should never be yielded after initialization"
                        ),
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
