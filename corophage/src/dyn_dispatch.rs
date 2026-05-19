//! Dyn-handler escape hatch for effect sets that hit rust-lang/rust#100013.
//!
//! The default hlist-of-closures dispatch (`Program::handle(...).run_stateful(...)`)
//! cannot prove `Send` for `tokio::spawn` once the effect chain grows past
//! ~20 entries — see #100013. This module bypasses the recursive
//! `AsyncFindHandlerWith` / `AsyncHandleWith` chain entirely: the user
//! implements a single trait method whose body is one big `match`. There is
//! no per-effect closure type, so no HRTB-over-impl-chain Send proof.
//!
//! Cost: one heap allocation per yielded effect (the boxed handler future),
//! and the user writes resume-coproduct construction by hand instead of
//! returning per-effect `Control<E::Resume<'a>>`.

use std::future::Future;
use std::pin::Pin;

use fauxgen::GeneratorState;
use frunk_core::coproduct::Coproduct;
use frunk_core::hlist::HNil;

pub use crate::control::CoControl;

use crate::control::Cancelled;
use crate::coroutine::GenericCo;
use crate::effect::{Effect, Effects, InjectResume, Start};
use crate::locality::Locality;
use crate::program::Program;

/// Match an effect coproduct against per-effect patterns.
///
/// Hides the `Coproduct::Inr(...)` cascade users would otherwise write by hand
/// in [`EffectHandler::handle`]. Effects must be listed in the same order as
/// in the `Effects![...]` type definition.
///
/// # Example
///
/// ```ignore
/// match_effect!(effect => {
///     E01(n) => resume::<_, E01, _>(n + 1),
///     E02(n) => resume::<_, E02, _>(n * 2),
///     E03(_) => CoControl::Cancel,
/// })
/// ```
///
/// expands to
///
/// ```ignore
/// match effect {
///     Coproduct::Inl(E01(n)) => resume::<_, E01, _>(n + 1),
///     Coproduct::Inr(Coproduct::Inl(E02(n))) => resume::<_, E02, _>(n * 2),
///     Coproduct::Inr(Coproduct::Inr(Coproduct::Inl(E03(_)))) => CoControl::Cancel,
///     Coproduct::Inr(Coproduct::Inr(Coproduct::Inr(cnil))) => match cnil {},
/// }
/// ```
#[macro_export]
macro_rules! match_effect {
    // Entry: peel off the first arm, recurse on the Inr branch.
    ($effect:expr => { $pat0:pat => $body0:expr $(, $pat:pat => $body:expr )* $(,)? }) => {
        match $effect {
            $crate::__frunk_core::coproduct::Coproduct::Inl($pat0) => $body0,
            $crate::__frunk_core::coproduct::Coproduct::Inr(__inner) => {
                $crate::match_effect!(__inner => { $($pat => $body),* })
            }
        }
    };
    // Base case: no arms left, the remaining coproduct is CNil (uninhabited).
    ($effect:expr => { $(,)? }) => {
        match $effect {}
    };
}

/// Build a `CoControl::Resume(...)` for an effect at a given coproduct index.
///
/// Convenience helper for [`EffectHandler::handle`] implementations: instead
/// of constructing the resume coproduct by hand, write `resume::<Effs, E01, _>(value)`
/// (Effs is usually inferred from the function return type).
#[inline]
pub fn resume<'a, Effs, E, Idx>(value: E::Resume<'a>) -> CoControl<'a, Effs>
where
    Effs: Effects<'a> + InjectResume<'a, E, Idx>,
    E: Effect,
{
    CoControl::Resume(<Effs as InjectResume<'a, E, Idx>>::inject_resume(value))
}

/// A single-method effect handler that handles every effect in `Effs` via a
/// hand-written match.
///
/// Use this when the default hlist-of-closures dispatch hits
/// rust-lang/rust#100013 (typically with ~20+ effects under `tokio::spawn`).
pub trait EffectHandler<'a, Effs, S>: Send + Sync
where
    Effs: Effects<'a>,
{
    /// Handle one yielded effect, optionally mutating `state`, returning the
    /// raw resume coproduct (or `CoControl::Cancel`).
    ///
    /// The user is responsible for constructing the matching coproduct variant
    /// for the resume value of each effect. See the example in
    /// `tests/it/dyn_dispatch.rs`.
    fn handle<'h>(
        &'h self,
        state: &'h mut S,
        effect: Effs,
    ) -> Pin<Box<dyn Future<Output = CoControl<'a, Effs>> + Send + 'h>>
    where
        Self: 'h,
        Effs: 'h,
        S: 'h;
}

/// Drive a coroutine to completion against a single dyn `EffectHandler`.
///
/// Equivalent to `asynk::run_stateful`, but the dispatch goes through one
/// trait-object method instead of the recursive hlist chain.
pub async fn run_dyn_stateful<'a, Effs, R, L, S, H>(
    co: GenericCo<'a, Effs, R, L>,
    handler: &H,
    state: &mut S,
) -> Result<R, Cancelled>
where
    L: Locality,
    Effs: Effects<'a>,
    H: EffectHandler<'a, Effs, S> + ?Sized,
{
    let mut co = std::pin::pin!(co);
    let mut yielded = co.as_mut().resume_with(Start);

    loop {
        match yielded {
            GeneratorState::Complete(value) => break Ok(value),
            GeneratorState::Yielded(effect) => {
                let subeffect = match effect {
                    Coproduct::Inl(_) => debug_unreachable!(
                        "Start (Inl) arm should never be yielded after initialization"
                    ),
                    Coproduct::Inr(subeffect) => subeffect,
                };

                let resume = handler.handle(state, subeffect).await;
                match resume {
                    CoControl::Cancel => break Err(Cancelled),
                    CoControl::Resume(r) => {
                        yielded = co.as_mut().resume(Coproduct::Inr(r));
                    }
                }
            }
        }
    }
}

impl<'a, Effs, R, L> Program<'a, Effs, R, L, Effs, HNil>
where
    Effs: Effects<'a>,
    L: Locality,
{
    /// Run the program against a single dyn effect handler.
    ///
    /// This is the escape hatch for effect sets that hit
    /// rust-lang/rust#100013 with the default hlist dispatch.
    pub async fn run_dyn_stateful<S, H>(self, handler: &H, state: &mut S) -> Result<R, Cancelled>
    where
        H: EffectHandler<'a, Effs, S> + ?Sized,
    {
        run_dyn_stateful(self.co, handler, state).await
    }
}
