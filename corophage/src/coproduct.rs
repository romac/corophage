use std::future::Future;

pub use frunk_core::coproduct::{CNil, CoprodInjector, CoprodUninjector, Coproduct};
pub use frunk_core::hlist::{HCons, HNil};
use frunk_core::indices::{Here, There};

use crate::control::{CoControl, Control};
use crate::effect::{Effect, Effects, InjectResume, MapResume, Resumes};

// ---------------------------------------------------------------------------
// declare_find_handler!  –  generates a Find*Handler trait + Here/There impls
// ---------------------------------------------------------------------------

macro_rules! declare_find_handler {
    ($trait_name:ident, sync, mut, $fn_bound:ident) => {
        #[doc(hidden)]
        pub trait $trait_name<'a, Effs: Effects<'a>, CH: Effect, InjectIdx, FindIdx> {
            fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs>;
        }

        impl<'a, Effs, CH, F, FTail, InjectIdx> $trait_name<'a, Effs, CH, InjectIdx, Here>
            for HCons<F, FTail>
        where
            Effs: Effects<'a> + InjectResume<'a, CH, InjectIdx>,
            CH: Effect,
            F: $fn_bound(CH) -> Control<CH::Resume<'a>>,
        {
            #[inline]
            fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs> {
                match (self.head)(effect) {
                    Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
                    Control::Cancel => CoControl::Cancel,
                }
            }
        }

        impl<'a, Effs, CH, FHead, FTail, InjectIdx, I>
            $trait_name<'a, Effs, CH, InjectIdx, There<I>> for HCons<FHead, FTail>
        where
            Effs: Effects<'a>,
            CH: Effect,
            FTail: $trait_name<'a, Effs, CH, InjectIdx, I>,
        {
            #[inline]
            fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs> {
                self.tail.call_handler(effect)
            }
        }
    };

    ($trait_name:ident, sync, with, $fn_bound:ident) => {
        #[doc(hidden)]
        pub trait $trait_name<'a, Effs: Effects<'a>, CH: Effect, S, InjectIdx, FindIdx> {
            fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs>;
        }

        impl<'a, Effs, CH, F, FTail, S, InjectIdx> $trait_name<'a, Effs, CH, S, InjectIdx, Here>
            for HCons<F, FTail>
        where
            Effs: Effects<'a> + InjectResume<'a, CH, InjectIdx>,
            CH: Effect,
            F: $fn_bound(&mut S, CH) -> Control<CH::Resume<'a>>,
        {
            #[inline]
            fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs> {
                match (self.head)(state, effect) {
                    Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
                    Control::Cancel => CoControl::Cancel,
                }
            }
        }

        impl<'a, Effs, CH, FHead, FTail, S, InjectIdx, I>
            $trait_name<'a, Effs, CH, S, InjectIdx, There<I>> for HCons<FHead, FTail>
        where
            Effs: Effects<'a>,
            CH: Effect,
            FTail: $trait_name<'a, Effs, CH, S, InjectIdx, I>,
        {
            #[inline]
            fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs> {
                self.tail.call_handler(state, effect)
            }
        }
    };

    ($trait_name:ident, async, mut, $fn_bound:ident) => {
        #[doc(hidden)]
        pub trait $trait_name<'a, Effs: Effects<'a>, CH: Effect, InjectIdx, FindIdx> {
            fn call_handler(&mut self, effect: CH) -> impl Future<Output = CoControl<'a, Effs>>;
        }

        impl<'a, Effs, CH, F, FTail, InjectIdx> $trait_name<'a, Effs, CH, InjectIdx, Here>
            for HCons<F, FTail>
        where
            Effs: Effects<'a> + InjectResume<'a, CH, InjectIdx>,
            CH: Effect,
            F: $fn_bound(CH) -> Control<CH::Resume<'a>>,
        {
            #[inline]
            async fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs> {
                match (self.head)(effect).await {
                    Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
                    Control::Cancel => CoControl::Cancel,
                }
            }
        }

        impl<'a, Effs, CH, FHead, FTail, InjectIdx, I>
            $trait_name<'a, Effs, CH, InjectIdx, There<I>> for HCons<FHead, FTail>
        where
            Effs: Effects<'a>,
            CH: Effect,
            FTail: $trait_name<'a, Effs, CH, InjectIdx, I>,
        {
            #[inline]
            async fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs> {
                self.tail.call_handler(effect).await
            }
        }
    };

    ($trait_name:ident, async, with, $fn_bound:ident) => {
        #[doc(hidden)]
        pub trait $trait_name<'a, Effs: Effects<'a>, CH: Effect, S, InjectIdx, FindIdx> {
            fn call_handler(
                &self,
                state: &mut S,
                effect: CH,
            ) -> impl Future<Output = CoControl<'a, Effs>>;
        }

        impl<'a, Effs, CH, F, FTail, S, InjectIdx> $trait_name<'a, Effs, CH, S, InjectIdx, Here>
            for HCons<F, FTail>
        where
            Effs: Effects<'a> + InjectResume<'a, CH, InjectIdx>,
            CH: Effect,
            F: $fn_bound(&mut S, CH) -> Control<CH::Resume<'a>>,
        {
            #[inline]
            async fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs> {
                match (self.head)(state, effect).await {
                    Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
                    Control::Cancel => CoControl::Cancel,
                }
            }
        }

        impl<'a, Effs, CH, FHead, FTail, S, InjectIdx, I>
            $trait_name<'a, Effs, CH, S, InjectIdx, There<I>> for HCons<FHead, FTail>
        where
            Effs: Effects<'a>,
            CH: Effect,
            FTail: $trait_name<'a, Effs, CH, S, InjectIdx, I>,
        {
            #[inline]
            async fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs> {
                self.tail.call_handler(state, effect).await
            }
        }
    };
}

declare_find_handler!(FindHandler, sync, mut, FnMut);
declare_find_handler!(FindHandlerWith, sync, with, Fn);
declare_find_handler!(AsyncFindHandler, async, mut, AsyncFnMut);
declare_find_handler!(AsyncFindHandlerWith, async, with, AsyncFn);

// ---------------------------------------------------------------------------
// declare_handle_dispatch!  –  generates a Handle* trait + CNil/Coproduct impls
// ---------------------------------------------------------------------------

macro_rules! declare_handle_dispatch {
    ($trait_name:ident, sync, $find_trait:ident, mut) => {
        #[doc(hidden)]
        pub trait $trait_name<'a, Effs: Effects<'a>, Handlers, Indices> {
            fn handle_mut(self, handlers: &mut Handlers) -> CoControl<'a, Effs>;
        }

        impl<'a, Effs: Effects<'a>, Handlers> $trait_name<'a, Effs, Handlers, HNil> for CNil {
            #[cfg_attr(coverage_nightly, coverage(off))]
            #[inline]
            fn handle_mut(self, _: &mut Handlers) -> CoControl<'a, Effs> {
                match self {}
            }
        }

        impl<'a, Effs, CH, CTail, Handlers, InjectIdx, FindIdx, ITail>
            $trait_name<'a, Effs, Handlers, HCons<(InjectIdx, FindIdx), ITail>>
            for Coproduct<CH, CTail>
        where
            Effs: Effects<'a>,
            CH: Effect,
            Handlers: $find_trait<'a, Effs, CH, InjectIdx, FindIdx>,
            CTail: $trait_name<'a, Effs, Handlers, ITail>,
        {
            #[inline]
            fn handle_mut(self, handlers: &mut Handlers) -> CoControl<'a, Effs> {
                match self {
                    Coproduct::Inl(head) => handlers.call_handler(head),
                    Coproduct::Inr(rest) => rest.handle_mut(handlers),
                }
            }
        }
    };

    ($trait_name:ident, sync, $find_trait:ident, with) => {
        #[doc(hidden)]
        pub trait $trait_name<'a, Effs: Effects<'a>, Handlers, S, Indices> {
            fn handle_with(self, state: &mut S, handlers: &Handlers) -> CoControl<'a, Effs>;
        }

        impl<'a, Effs: Effects<'a>, Handlers, S> $trait_name<'a, Effs, Handlers, S, HNil> for CNil {
            #[cfg_attr(coverage_nightly, coverage(off))]
            #[inline]
            fn handle_with(self, _: &mut S, _: &Handlers) -> CoControl<'a, Effs> {
                match self {}
            }
        }

        impl<'a, Effs, CH, CTail, Handlers, S, InjectIdx, FindIdx, ITail>
            $trait_name<'a, Effs, Handlers, S, HCons<(InjectIdx, FindIdx), ITail>>
            for Coproduct<CH, CTail>
        where
            Effs: Effects<'a>,
            CH: Effect,
            Handlers: $find_trait<'a, Effs, CH, S, InjectIdx, FindIdx>,
            CTail: $trait_name<'a, Effs, Handlers, S, ITail>,
        {
            #[inline]
            fn handle_with(self, state: &mut S, handlers: &Handlers) -> CoControl<'a, Effs> {
                match self {
                    Coproduct::Inl(head) => handlers.call_handler(state, head),
                    Coproduct::Inr(rest) => rest.handle_with(state, handlers),
                }
            }
        }
    };

    ($trait_name:ident, async, $find_trait:ident, mut) => {
        #[doc(hidden)]
        pub trait $trait_name<'a, Effs: Effects<'a>, Handlers, Indices> {
            fn handle_mut(
                self,
                handlers: &mut Handlers,
            ) -> impl Future<Output = CoControl<'a, Effs>>;
        }

        impl<'a, Effs: Effects<'a>, Handlers> $trait_name<'a, Effs, Handlers, HNil> for CNil {
            #[cfg_attr(coverage_nightly, coverage(off))]
            #[inline]
            async fn handle_mut(self, _: &mut Handlers) -> CoControl<'a, Effs> {
                match self {}
            }
        }

        impl<'a, Effs, CH, CTail, Handlers, InjectIdx, FindIdx, ITail>
            $trait_name<'a, Effs, Handlers, HCons<(InjectIdx, FindIdx), ITail>>
            for Coproduct<CH, CTail>
        where
            Effs: Effects<'a>,
            CH: Effect,
            Handlers: $find_trait<'a, Effs, CH, InjectIdx, FindIdx>,
            CTail: $trait_name<'a, Effs, Handlers, ITail>,
        {
            #[inline]
            async fn handle_mut(self, handlers: &mut Handlers) -> CoControl<'a, Effs> {
                match self {
                    Coproduct::Inl(head) => handlers.call_handler(head).await,
                    Coproduct::Inr(rest) => rest.handle_mut(handlers).await,
                }
            }
        }
    };

    ($trait_name:ident, async, $find_trait:ident, with) => {
        #[doc(hidden)]
        pub trait $trait_name<'a, Effs: Effects<'a>, Handlers, S, Indices> {
            fn handle_with(
                self,
                state: &mut S,
                handlers: &Handlers,
            ) -> impl Future<Output = CoControl<'a, Effs>>;
        }

        impl<'a, Effs: Effects<'a>, Handlers, S> $trait_name<'a, Effs, Handlers, S, HNil> for CNil {
            #[cfg_attr(coverage_nightly, coverage(off))]
            #[inline]
            async fn handle_with(self, _: &mut S, _: &Handlers) -> CoControl<'a, Effs> {
                match self {}
            }
        }

        impl<'a, Effs, CH, CTail, Handlers, S, InjectIdx, FindIdx, ITail>
            $trait_name<'a, Effs, Handlers, S, HCons<(InjectIdx, FindIdx), ITail>>
            for Coproduct<CH, CTail>
        where
            Effs: Effects<'a>,
            CH: Effect,
            Handlers: $find_trait<'a, Effs, CH, S, InjectIdx, FindIdx>,
            CTail: $trait_name<'a, Effs, Handlers, S, ITail>,
        {
            #[inline]
            async fn handle_with(self, state: &mut S, handlers: &Handlers) -> CoControl<'a, Effs> {
                match self {
                    Coproduct::Inl(head) => handlers.call_handler(state, head).await,
                    Coproduct::Inr(rest) => rest.handle_with(state, handlers).await,
                }
            }
        }
    };
}

declare_handle_dispatch!(HandleMut, sync, FindHandler, mut);
declare_handle_dispatch!(HandleWith, sync, FindHandlerWith, with);
declare_handle_dispatch!(AsyncHandleMut, async, AsyncFindHandler, mut);
declare_handle_dispatch!(AsyncHandleWith, async, AsyncFindHandlerWith, with);

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
    F: Fn(&mut S, CH) -> Control<CH::Resume<'a>>,
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
    F: AsyncFn(&mut S, CH) -> Control<CH::Resume<'a>>,
    FTail: HandlersToEffects<'a, Effs, ITail>,
{
    type Effects = Coproduct<CH, <FTail as HandlersToEffects<'a, Effs, ITail>>::Effects>;
}

// ---------------------------------------------------------------------------
// EmbedEffect + ProjectResume  –  synchronous coproduct conversions for invoke
// ---------------------------------------------------------------------------

/// Embed a sub-effect coproduct into a larger outer-effect coproduct.
///
/// This is the synchronous "injection" half of effect forwarding, used by
/// [`Yielder::invoke`] to work around [rust-lang/rust#100013].
///
/// [rust-lang/rust#100013]: https://github.com/rust-lang/rust/issues/100013
/// [`Yielder::invoke`]: crate::coroutine::Yielder::invoke
#[doc(hidden)]
pub trait EmbedEffect<OuterEffs, Indices> {
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
/// This is the synchronous "uninject" half of effect forwarding, used by
/// [`Yielder::invoke`] to work around [rust-lang/rust#100013].
///
/// [rust-lang/rust#100013]: https://github.com/rust-lang/rust/issues/100013
/// [`Yielder::invoke`]: crate::coroutine::Yielder::invoke
/// Project an outer resume value at a known index back into a sub-effect
/// resume coproduct.
///
/// Unlike [`EmbedEffect`] which walks the sub-effect coproduct at runtime,
/// `ProjectResume` handles a single resume value whose position in the outer
/// coproduct is known at compile time (determined by [`EmbedEffect`]'s index).
/// It tries each sub-effect's index in turn: if the resume is at that index,
/// it injects into the sub-resume coproduct; otherwise it tries the next.
///
/// [rust-lang/rust#100013]: https://github.com/rust-lang/rust/issues/100013
/// [`Yielder::invoke`]: crate::coroutine::Yielder::invoke
#[doc(hidden)]
pub trait ProjectResume<'a, SubEffs: MapResume, Indices> {
    fn project(self) -> Resumes<'a, SubEffs>;
}

// Base case: when all sub-effects have been projected, any remaining outer
// resume variants are unreachable (the handler only resumes at indices that
// correspond to effects we actually yielded).
impl<'a, OuterResumes> ProjectResume<'a, CNil, HNil> for OuterResumes {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn project(self) -> Resumes<'a, CNil> {
        debug_unreachable!(
            "ProjectResume: no sub-effect matched — handler resumed at an unexpected index"
        )
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
