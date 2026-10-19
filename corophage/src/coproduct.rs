pub use frunk_core::coproduct::{CNil, CoprodInjector, Coproduct};
pub use frunk_core::hlist::{HCons, HNil};
use frunk_core::indices::{Here, There};

use crate::control::Control;
use crate::effect::{Effect, Effects, InjectResume, MapResume, Resumes};

pub(crate) mod dispatch;

// --- HandlersToEffects: compute the effects coproduct from a handler HList ---

#[doc(hidden)]
pub trait HandlersToEffects<'a, Effs: Effects<'a>, Indices> {
    type Effects;
}

impl<'a, Effs: Effects<'a>> HandlersToEffects<'a, Effs, HNil> for HNil {
    type Effects = CNil;
}

#[doc(hidden)]
pub struct SyncHandler;

impl<'a, Effs, F, FTail, CH, Index, ITail>
    HandlersToEffects<'a, Effs, HCons<(CH, Index, SyncHandler), ITail>> for HCons<F, FTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, Index>,
    CH: Effect,
    F: FnMut(CH) -> Control<CH::Resume<'a>>,
    FTail: HandlersToEffects<'a, Effs, ITail>,
{
    type Effects = Coproduct<CH, <FTail as HandlersToEffects<'a, Effs, ITail>>::Effects>;
}

#[doc(hidden)]
pub struct SyncStatefulHandler<S>(std::marker::PhantomData<S>);

impl<'a, Effs, F, FTail, CH, S, Index, ITail>
    HandlersToEffects<'a, Effs, HCons<(CH, Index, SyncStatefulHandler<S>), ITail>>
    for HCons<F, FTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, Index>,
    CH: Effect,
    F: FnMut(&mut S, CH) -> Control<CH::Resume<'a>>,
    FTail: HandlersToEffects<'a, Effs, ITail>,
{
    type Effects = Coproduct<CH, <FTail as HandlersToEffects<'a, Effs, ITail>>::Effects>;
}

#[doc(hidden)]
pub struct AsyncHandler;

impl<'a, Effs, F, FTail, CH, Index, ITail>
    HandlersToEffects<'a, Effs, HCons<(CH, Index, AsyncHandler), ITail>> for HCons<F, FTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, Index>,
    CH: Effect,
    F: AsyncFnMut(CH) -> Control<CH::Resume<'a>>,
    FTail: HandlersToEffects<'a, Effs, ITail>,
{
    type Effects = Coproduct<CH, <FTail as HandlersToEffects<'a, Effs, ITail>>::Effects>;
}

#[doc(hidden)]
pub struct AsyncStatefulHandler<S>(std::marker::PhantomData<S>);

impl<'a, Effs, F, FTail, CH, S, Index, ITail>
    HandlersToEffects<'a, Effs, HCons<(CH, Index, AsyncStatefulHandler<S>), ITail>>
    for HCons<F, FTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, Index>,
    CH: Effect,
    F: AsyncFnMut(&mut S, CH) -> Control<CH::Resume<'a>>,
    FTail: HandlersToEffects<'a, Effs, ITail>,
{
    type Effects = Coproduct<CH, <FTail as HandlersToEffects<'a, Effs, ITail>>::Effects>;
}

// ---------------------------------------------------------------------------
// EmbedEffect + ProjectResume  –  synchronous coproduct conversions for invoke
// ---------------------------------------------------------------------------

/// Embed a sub-effect coproduct into a larger outer-effect coproduct.
///
/// Use this bound together with [`ProjectResume`] in generic helpers that call
/// [`Yielder::invoke`](crate::Yielder::invoke). `Indices` selects each sub-effect's
/// position in the outer set; keep it generic so callers can infer the positions.
/// Both bounds must use the same indices, even when the effect sets are identical.
///
/// Implementations are provided for coproducts constructed with
/// [`Effects!`](crate::Effects!). Ordinary invocation infers these bounds.
pub trait EmbedEffect<OuterEffs, Indices> {
    /// Convert an effect to the corresponding variant of the outer effect set.
    fn embed(self) -> OuterEffs;
}

impl<OuterEffs> EmbedEffect<OuterEffs, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn embed(self) -> OuterEffs {
        match self {}
    }
}

impl<OuterEffs, E, Tail, OuterIdx, TailIndices> EmbedEffect<OuterEffs, HCons<OuterIdx, TailIndices>>
    for Coproduct<E, Tail>
where
    OuterEffs: CoprodInjector<E, OuterIdx>,
    Tail: EmbedEffect<OuterEffs, TailIndices>,
{
    #[inline]
    fn embed(self) -> OuterEffs {
        match self {
            Coproduct::Inl(effect) => OuterEffs::inject(effect),
            Coproduct::Inr(tail) => tail.embed(),
        }
    }
}

/// Project an outer resume coproduct back to a sub-effect resume coproduct.
///
/// Pair this bound on [`Resumes<'a, OuterEffs>`](crate::Resumes) with
/// `SubEffs: EmbedEffect<OuterEffs, Indices>` when calling
/// [`Yielder::invoke`](crate::Yielder::invoke) from a generic helper.
/// Use the same `Indices` for both bounds so each effect receives its own resume,
/// including when several effects share the same resume type.
///
/// Implementations are provided for the resume coproducts of
/// [`Effects!`](crate::Effects!) sets. The lifetime `'a` describes the incoming
/// resumes; invocation subsequently shortens it for the child when necessary.
pub trait ProjectResume<'a, SubEffs: MapResume, Indices> {
    /// Convert a resume to the corresponding variant of the sub-effect set.
    ///
    /// # Panics
    ///
    /// Panics if the resume's variant does not match any of the selected indices.
    fn project(self) -> Resumes<'a, SubEffs>;
}

// Base case: when all sub-effects have been projected, any remaining outer
// resume variants indicate an invalid projection. This trait is publicly
// callable because it appears in Yielder::invoke's bounds, so keep the failure
// checked even though correct internal dispatch never reaches it.
impl<'a, OuterResumes> ProjectResume<'a, CNil, HNil> for OuterResumes {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn project(self) -> Resumes<'a, CNil> {
        panic!("ProjectResume: no sub-effect matched — handler resumed at an unexpected index")
    }
}

/// Helper trait: try to extract a value at index `Idx` from a coproduct.
/// Returns `Ok(value)` if found, `Err(self)` if not.
#[doc(hidden)]
pub trait CoprodAt<T, Idx> {
    fn at(self) -> Result<T, Self>
    where
        Self: Sized;
}

impl<T, Tail> CoprodAt<T, Here> for Coproduct<T, Tail> {
    #[inline]
    fn at(self) -> Result<T, Self> {
        match self {
            Coproduct::Inl(val) => Ok(val),
            Coproduct::Inr(_) => Err(self),
        }
    }
}

impl<Head, Tail, T, TailIdx> CoprodAt<T, There<TailIdx>> for Coproduct<Head, Tail>
where
    Tail: CoprodAt<T, TailIdx>,
{
    #[inline]
    fn at(self) -> Result<T, Self> {
        match self {
            Coproduct::Inl(_) => Err(self),
            Coproduct::Inr(tail) => match tail.at() {
                Ok(val) => Ok(val),
                Err(tail) => Err(Coproduct::Inr(tail)),
            },
        }
    }
}

impl<'a, OuterResumes, E, SubTail, OuterIdx, TailIndices>
    ProjectResume<'a, Coproduct<E, SubTail>, HCons<OuterIdx, TailIndices>> for OuterResumes
where
    E: Effect,
    SubTail: MapResume,
    OuterResumes: CoprodAt<E::Resume<'a>, OuterIdx>,
    OuterResumes: ProjectResume<'a, SubTail, TailIndices>,
{
    #[inline]
    fn project(self) -> Resumes<'a, Coproduct<E, SubTail>> {
        match self.at() {
            Ok(resume) => Coproduct::Inl(resume),
            Err(remainder) => Coproduct::Inr(remainder.project()),
        }
    }
}
