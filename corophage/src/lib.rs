#![doc = include_str!("../README.md")]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

// Gives proc-macro expansions a stable absolute path when invoked in this package.
extern crate self as corophage;

mod coproduct;

pub use coproduct::{EmbedEffect, ProjectResume};

mod catch;
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

pub use catch::{Catch, Throw};
pub use control::{Cancelled, Control};
pub use coproduct::dispatch::{LocalHandler, LocalStatefulHandler, local, local_stateful};
pub use coroutine::{Co, CoSend, Yielder};
pub use effect::{Effect, Effects, Resumes};
pub use locality::{Local, Locality, Sendable};
#[allow(deprecated)]
pub use program::{Eff, Effectful, Program};

#[cfg(feature = "macros")]
pub use corophage_macros::{effect, effectful};

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

/// An uninhabited type for effects that never resume.
///
/// Use this as `Effect::Resume` for effects that always cancel the computation
/// (e.g., `Cancel`) and therefore can never resume.
pub enum Never {}

/// Async effect runners.
///
/// Use these functions to run a coroutine with sync or async effect handlers.
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
    use crate::execution::{CoproductDispatch, Owned, drive, drive_send};
    use crate::locality::{Locality, Sendable};

    use super::*;

    /// Run a coroutine with an hlist of sync or async handlers.
    ///
    /// Both the computation body and its handlers may await ordinary futures.
    /// Wrap async closures that borrow their captures across awaits with
    /// [`crate::local`].
    #[doc(hidden)]
    #[inline]
    pub fn run<'a, 'h, ES, R, L, F, Indices>(
        co: GenericCo<'a, ES, R, L>,
        handler: &'h mut F,
    ) -> impl Future<Output = Result<R, Cancelled>>
    where
        L: Locality,
        ES: Effects<'a> + for<'c> Dispatch<'a, ES, &'c mut (), &'c mut &'h mut F, Indices>,
    {
        drive(
            co.generator(),
            (),
            Owned(handler),
            CoproductDispatch::<'a, ES, Indices>::new(),
        )
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
            Owned(handler),
            CoproductDispatch::<'a, ES, I>::new(),
        )
    }

    /// Run a coroutine with an hlist of sync or async handlers and shared
    /// mutable state.
    ///
    /// Wrap async closures that borrow their captures across awaits with
    /// [`crate::local_stateful`].
    #[doc(hidden)]
    #[inline]
    pub fn run_stateful<'a, 'h, ES, R, L, S, F, Indices>(
        co: GenericCo<'a, ES, R, L>,
        state: &mut S,
        handler: &'h F,
    ) -> impl Future<Output = Result<R, Cancelled>>
    where
        L: Locality,
        ES: Effects<'a> + for<'c> Dispatch<'a, ES, &'c mut S, &'c mut &'h F, Indices>,
    {
        drive(
            co.generator(),
            state,
            Owned(handler),
            CoproductDispatch::<'a, ES, Indices>::new(),
        )
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
            Owned(handler),
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
    use crate::execution::{CoproductDispatch, Owned, drive_inline};
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
        drive_inline(
            co.generator(),
            (),
            Owned(handler),
            CoproductDispatch::<'a, ES, Indices>::new(),
        )
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
        drive_inline(
            co.generator(),
            state,
            Owned(handler),
            CoproductDispatch::<'a, ES, Indices>::new(),
        )
    }
}
