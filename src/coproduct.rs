use std::future::Future;

pub use frunk_core::coproduct::{CNil, CoprodInjector, CoprodUninjector, Coproduct};
pub use frunk_core::hlist::{HCons, HNil};
use frunk_core::indices::{Here, There};

use crate::control::{CoControl, Control};
use crate::coroutine::Yielder;
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
// ForwardEffects  –  forwards each effect variant through an outer Yielder
// ---------------------------------------------------------------------------

/// Forward each effect in a coproduct through an outer [`Yielder`].
///
/// This trait is used by [`Yielder::invoke`] to forward effects from an inner
/// (sub-)program through the outer program's yielder.
#[doc(hidden)]
pub trait ForwardEffects<'a, OuterEffs: MapResume, Indices>: MapResume {
    fn forward(self, yielder: &Yielder<'a, OuterEffs>) -> impl Future<Output = Resumes<'a, Self>>;
}

impl<'a, OuterEffs: MapResume> ForwardEffects<'a, OuterEffs, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn forward(self, _yielder: &Yielder<'a, OuterEffs>) -> Resumes<'a, Self> {
        match self {}
    }
}

impl<'a, OuterEffs, E, Tail, OuterIdx, TailIndices>
    ForwardEffects<'a, OuterEffs, HCons<OuterIdx, TailIndices>> for Coproduct<E, Tail>
where
    E: Effect,
    Tail: ForwardEffects<'a, OuterEffs, TailIndices>,
    OuterEffs: MapResume + CoprodInjector<E, OuterIdx>,
    <OuterEffs as MapResume>::Output<'a>: CoprodUninjector<E::Resume<'a>, OuterIdx>,
{
    async fn forward(self, yielder: &Yielder<'a, OuterEffs>) -> Resumes<'a, Self> {
        match self {
            Coproduct::Inl(effect) => Coproduct::Inl(yielder.yield_(effect).await),
            Coproduct::Inr(tail) => Coproduct::Inr(tail.forward(yielder).await),
        }
    }
}
