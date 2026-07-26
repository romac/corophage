#![doc = include_str!("../README.md")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

// Gives proc-macro expansions a stable absolute path when invoked in this package.
extern crate self as corophage;

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
pub use coproduct::{EmbedEffect, ProjectResume};

mod control;
mod effect;
mod locality;
mod program;

#[macro_use]
mod macros;

/// Re-exports of the most commonly used types and traits.
pub mod prelude;

pub mod coroutine;

#[doc(hidden)]
pub use frunk_core as __frunk_core;

pub use control::{Cancelled, Control};
pub use coroutine::{Co, CoSend, Yielder};
pub use effect::Effect;
pub use locality::{Local, Locality, Sendable};
#[allow(deprecated)]
pub use program::{Eff, Effectful, Program};

#[cfg(feature = "macros")]
pub use corophage_macros::{effect, effectful};

macro_rules! resume_co {
    (sync, $co:expr, $resume:expr) => {
        $co.resume($resume)
    };
    (asynk, $co:expr, $resume:expr) => {
        $co.resume_async($resume).await
    };
}

macro_rules! resume_co_with {
    (sync, $co:expr, $resume:expr) => {
        $co.resume_with($resume)
    };
    (asynk, $co:expr, $resume:expr) => {
        $co.resume_with_async($resume).await
    };
}

/// Internal macro for running a coroutine with effect handlers.
macro_rules! run {
    ($mode:ident, $lt:lifetime, $effs:ty, $co:expr, $effect:pat => $handle:expr) => {{
        let mut co = ::std::pin::pin!($co);

        let mut yielded = resume_co_with!($mode, co.as_mut(), $crate::effect::Start);

        loop {
            match yielded {
                ::fauxgen::GeneratorState::Complete(value) => break Ok(value),

                ::fauxgen::GeneratorState::Yielded(effect) => {
                    let $effect = match effect {
                        // INVARIANT: Yielder::yield_ always wraps effects in Inr,
                        // so the Inl (Start) arm is never yielded after init.
                        $crate::__frunk_core::coproduct::Coproduct::Inl(_) => debug_unreachable!(
                            "Start (Inl) arm should never be yielded after initialization"
                        ),
                        $crate::__frunk_core::coproduct::Coproduct::Inr(subeffect) => subeffect,
                    };

                    let resume: $crate::control::CoControl<$lt, $effs> = $handle;
                    match resume {
                        $crate::control::CoControl::Cancel => {
                            break Err($crate::control::Cancelled);
                        }
                        $crate::control::CoControl::Resume(r) => {
                            yielded = resume_co!(
                                $mode,
                                co.as_mut(),
                                $crate::__frunk_core::coproduct::Coproduct::Inr(r)
                            )
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
    use crate::coproduct::{SendAsyncHandleMut, SendFuture};
    use crate::coroutine::GenericCo;
    use crate::effect::Effects;
    use crate::locality::{Locality, Sendable};

    use super::*;

    /// Run a coroutine with an hlist of async handlers.
    ///
    /// Both the computation body and its handlers may await ordinary futures.
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
        run!(asynk, 'a, ES, co, effect => effect.handle_mut(handler).await)
    }

    /// Run a sendable coroutine with async handlers whose futures are `Send`.
    #[doc(hidden)]
    #[inline]
    pub fn run_send<'a: 'h, 'h, ES, R, F, Indices>(
        co: GenericCo<'a, ES, R, Sendable>,
        handler: &'h mut F,
    ) -> impl Future<Output = Result<R, Cancelled>> + Send + use<'a, 'h, ES, R, F, Indices>
    where
        R: Send + 'h,
        F: Send + 'h,
        ES: Effects<'a> + SendAsyncHandleMut<'a, ES, F, Indices>,
        GenericCo<'a, ES, R, Sendable>: Send,
    {
        let future =
            async move { run!(asynk, 'a, ES, co, effect => effect.handle_mut_send(handler).await) };

        // SAFETY: the future captures a `Send` coroutine and handler reference,
        // its runner state (including fauxgen's resume future) is `Send`, and
        // `SendAsyncHandleMut` guarantees every awaited dispatch future is `Send`.
        // rustc cannot yet carry those proofs through #100013.
        unsafe { SendFuture::new_unchecked(future) }
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
        run!(asynk, 'a, ES, co, effect => effect.handle_with(state, handler).await)
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
    ///
    /// # Panics
    ///
    /// Panics if the computation body suspends on a non-effect future. Use
    /// [`crate::asynk::run`] when the body needs to await ordinary futures.
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
        run!(sync, 'a, ES, co, effect => effect.handle_mut(handler))
    }

    /// Run a coroutine with an hlist of synchronous handlers and shared mutable state.
    ///
    /// # Panics
    ///
    /// Panics if the computation body suspends on a non-effect future. Use
    /// [`crate::asynk::run_stateful`] when the body needs to await ordinary futures.
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
        run!(sync, 'a, ES, co, effect => effect.handle_with(state, handler))
    }
}
