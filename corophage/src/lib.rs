#![doc = include_str!("../README.md")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

// Gives proc-macro expansions a stable absolute path when invoked in this package.
extern crate self as corophage;

mod coproduct;
use coproduct::{AsyncHandleMut, AsyncHandleWith};

pub use coproduct::{EmbedEffect, ProjectResume};

mod control;
mod effect;
mod execution;
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
pub use coproduct::dispatch::{LocalHandler, LocalStatefulHandler, local, local_stateful};
pub use coroutine::{Co, CoSend, Yielder};
pub use effect::{Effect, Effects, Resumes};
pub use locality::{Local, Locality, Sendable};
#[allow(deprecated)]
pub use program::{Eff, Effectful, Program};

#[cfg(feature = "macros")]
pub use corophage_macros::{effect, effectful};

macro_rules! resume_co {
    (asynk, $co:expr, $resume:expr) => {
        $co.resume_async($resume).await
    };
}

macro_rules! resume_co_with {
    (asynk, $co:expr, $resume:expr) => {
        $co.resume_with_async($resume).await
    };
}

// Keep short computations in one poll while bounding runs of ready handlers.
const ASYNC_DISPATCH_BUDGET: usize = 128;

#[inline]
async fn yield_to_executor() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
}

/// Internal macro for running a coroutine with effect handlers.
macro_rules! run {
    (asynk, $lt:lifetime, $effs:ty, $co:expr, $effect:pat => $handle:expr) => {{
        let mut budget = $crate::ASYNC_DISPATCH_BUDGET;
        run!(@loop asynk, $lt, $effs, $co, $effect => {
            // Check before dispatch so completion does not require an extra poll.
            // Other suspension points may yield sooner; this counter bounds each
            // run of ready handlers without depending on a particular executor.
            if budget == 0 {
                $crate::yield_to_executor().await;
                budget = $crate::ASYNC_DISPATCH_BUDGET;
            }
            budget -= 1;
            $handle
        })
    }};
    (@loop $mode:ident, $lt:lifetime, $effs:ty, $co:expr, $effect:pat => $handle:expr) => {{
        let mut co = ::std::pin::pin!($co.generator());

        let mut yielded = resume_co_with!($mode, co.as_mut(), $crate::effect::Start);

        loop {
            match yielded {
                ::fauxgen::GeneratorState::Complete(value) => break Ok(value),

                ::fauxgen::GeneratorState::Yielded(effect) => {
                    let $effect = match effect {
                        // INVARIANT: Yielder::yield_ always wraps effects in Inr,
                        // so the Inl (Start) arm is never yielded after init.
                        $crate::__frunk_core::coproduct::Coproduct::Inl(_) => unreachable!(
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
///
/// Runners yield to the executor after a bounded number of effect dispatches,
/// even when handlers are immediately ready. Each runner has its own budget.
/// This provides scheduling opportunities, but does not guarantee task ordering
/// or preempt a computation body or handler that does not return from its poll.
pub mod asynk {
    use crate::coproduct::dispatch::Dispatch;
    use crate::coroutine::GenericCo;
    use crate::effect::Effects;
    use crate::effect::Resumes;
    use crate::execution::{CoproductDispatch, drive_send};
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
    pub fn run_send<'a: 'h, 'h, ES, R, F, I>(
        co: GenericCo<'a, ES, R, Sendable>,
        handler: &'h mut F,
    ) -> impl Future<Output = Result<R, Cancelled>> + Send + use<'a, 'h, ES, R, F, I>
    where
        R: Send + 'h,
        F: Send + 'h,
        ES: Effects<'a> + for<'s, 'f> Dispatch<'a, ES, &'s mut (), &'f mut &'h mut F, I>,
        for<'s, 'f> <ES as Dispatch<'a, ES, &'s mut (), &'f mut &'h mut F, I>>::Future: Send,
        Resumes<'a, ES>: Send + Sync,
    {
        drive_send(
            co.generator(),
            (),
            handler,
            CoproductDispatch::<'a, ES, I>::new(),
        )
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

    /// Run a sendable coroutine with shared state and `Send` handler futures.
    #[doc(hidden)]
    #[inline]
    pub fn run_stateful_send<'a: 'h, 'h, ES, R, S, F, I>(
        co: GenericCo<'a, ES, R, Sendable>,
        state: &'h mut S,
        handler: &'h F,
    ) -> impl Future<Output = Result<R, Cancelled>> + Send + use<'a, 'h, ES, R, S, F, I>
    where
        R: Send + 'h,
        S: Send + 'h,
        F: Sync + 'h,
        ES: Effects<'a> + for<'s, 'f> Dispatch<'a, ES, &'s mut S, &'f mut &'h F, I>,
        for<'s, 'f> <ES as Dispatch<'a, ES, &'s mut S, &'f mut &'h F, I>>::Future: Send,
        Resumes<'a, ES>: Send + Sync,
    {
        drive_send(
            co.generator(),
            state,
            handler,
            CoproductDispatch::<'a, ES, I>::new(),
        )
    }
}

/// Synchronous effect runners.
///
/// Use these functions to run a coroutine with synchronous effect handlers.
/// For most use cases, prefer [`Program::run_sync`] instead.
pub mod sync {
    use crate::coproduct::dispatch::{Dispatch, Synchronous};
    use crate::coroutine::GenericCo;
    use crate::effect::Effects;
    use crate::execution::{CoproductDispatch, drive, inline};
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
    pub fn run<'a, 'h, ES, R, L, F, Indices>(
        co: GenericCo<'a, ES, R, L>,
        handler: &'h mut F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + for<'c> Dispatch<'a, ES, &'c mut (), &'c mut &'h mut F, Indices>,
        Indices: Synchronous,
    {
        inline(drive::<false, _, _, _, _, _, _>(
            co.generator(),
            (),
            handler,
            CoproductDispatch::<'a, ES, Indices>::new(),
        ))
    }

    /// Run a coroutine with an hlist of synchronous handlers and shared mutable state.
    ///
    /// # Panics
    ///
    /// Panics if the computation body suspends on a non-effect future. Use
    /// [`crate::asynk::run_stateful`] when the body needs to await ordinary futures.
    #[doc(hidden)]
    #[inline]
    pub fn run_stateful<'a, 'h, ES, R, L, S, F, Indices>(
        co: GenericCo<'a, ES, R, L>,
        state: &mut S,
        handler: &'h F,
    ) -> Result<R, Cancelled>
    where
        L: Locality,
        ES: Effects<'a> + for<'c> Dispatch<'a, ES, &'c mut S, &'c mut &'h F, Indices>,
        Indices: Synchronous,
    {
        inline(drive::<false, _, _, _, _, _, _>(
            co.generator(),
            state,
            handler,
            CoproductDispatch::<'a, ES, Indices>::new(),
        ))
    }
}
