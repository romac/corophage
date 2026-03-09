use std::future::Future;

pub use frunk_core::coproduct::{CNil, Coproduct};
pub use frunk_core::hlist::{HCons, HNil};
use frunk_core::indices::{Here, There};

use crate::control::{CoControl, Control};
use crate::effect::{Effect, Effects, InjectResume};

// --- FindHandler: locate a handler for effect CH in an HList by type ---

#[doc(hidden)]
pub trait FindHandler<'a, Effs: Effects<'a>, CH: Effect, InjectIdx, FindIdx> {
    fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs>;
}

impl<'a, Effs, CH, F, FTail, InjectIdx> FindHandler<'a, Effs, CH, InjectIdx, Here>
    for HCons<F, FTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, InjectIdx>,
    CH: Effect,
    F: FnMut(CH) -> Control<CH::Resume<'a>>,
{
    fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs> {
        match (self.head)(effect) {
            Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
            Control::Cancel => CoControl::Cancel,
        }
    }
}

impl<'a, Effs, CH, FHead, FTail, InjectIdx, I> FindHandler<'a, Effs, CH, InjectIdx, There<I>>
    for HCons<FHead, FTail>
where
    Effs: Effects<'a>,
    CH: Effect,
    FTail: FindHandler<'a, Effs, CH, InjectIdx, I>,
{
    fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs> {
        self.tail.call_handler(effect)
    }
}

// --- FindHandlerWith: locate a stateful handler for effect CH ---

#[doc(hidden)]
pub trait FindHandlerWith<'a, Effs: Effects<'a>, CH: Effect, S, InjectIdx, FindIdx> {
    fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs>;
}

impl<'a, Effs, CH, F, FTail, S, InjectIdx> FindHandlerWith<'a, Effs, CH, S, InjectIdx, Here>
    for HCons<F, FTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, InjectIdx>,
    CH: Effect,
    F: Fn(&mut S, CH) -> Control<CH::Resume<'a>>,
{
    fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs> {
        match (self.head)(state, effect) {
            Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
            Control::Cancel => CoControl::Cancel,
        }
    }
}

impl<'a, Effs, CH, FHead, FTail, S, InjectIdx, I>
    FindHandlerWith<'a, Effs, CH, S, InjectIdx, There<I>> for HCons<FHead, FTail>
where
    Effs: Effects<'a>,
    CH: Effect,
    FTail: FindHandlerWith<'a, Effs, CH, S, InjectIdx, I>,
{
    fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs> {
        self.tail.call_handler(state, effect)
    }
}

// --- AsyncFindHandler: locate an async handler for effect CH ---

#[doc(hidden)]
pub trait AsyncFindHandler<'a, Effs: Effects<'a>, CH: Effect, InjectIdx, FindIdx> {
    fn call_handler(&mut self, effect: CH) -> impl Future<Output = CoControl<'a, Effs>>;
}

impl<'a, Effs, CH, F, FTail, InjectIdx> AsyncFindHandler<'a, Effs, CH, InjectIdx, Here>
    for HCons<F, FTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, InjectIdx>,
    CH: Effect,
    F: AsyncFnMut(CH) -> Control<CH::Resume<'a>>,
{
    async fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs> {
        match (self.head)(effect).await {
            Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
            Control::Cancel => CoControl::Cancel,
        }
    }
}

impl<'a, Effs, CH, FHead, FTail, InjectIdx, I> AsyncFindHandler<'a, Effs, CH, InjectIdx, There<I>>
    for HCons<FHead, FTail>
where
    Effs: Effects<'a>,
    CH: Effect,
    FTail: AsyncFindHandler<'a, Effs, CH, InjectIdx, I>,
{
    async fn call_handler(&mut self, effect: CH) -> CoControl<'a, Effs> {
        self.tail.call_handler(effect).await
    }
}

// --- AsyncFindHandlerWith: locate an async stateful handler for effect CH ---

#[doc(hidden)]
pub trait AsyncFindHandlerWith<'a, Effs: Effects<'a>, CH: Effect, S, InjectIdx, FindIdx> {
    fn call_handler(&self, state: &mut S, effect: CH) -> impl Future<Output = CoControl<'a, Effs>>;
}

impl<'a, Effs, CH, F, FTail, S, InjectIdx> AsyncFindHandlerWith<'a, Effs, CH, S, InjectIdx, Here>
    for HCons<F, FTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, InjectIdx>,
    CH: Effect,
    F: AsyncFn(&mut S, CH) -> Control<CH::Resume<'a>>,
{
    async fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs> {
        match (self.head)(state, effect).await {
            Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
            Control::Cancel => CoControl::Cancel,
        }
    }
}

impl<'a, Effs, CH, FHead, FTail, S, InjectIdx, I>
    AsyncFindHandlerWith<'a, Effs, CH, S, InjectIdx, There<I>> for HCons<FHead, FTail>
where
    Effs: Effects<'a>,
    CH: Effect,
    FTail: AsyncFindHandlerWith<'a, Effs, CH, S, InjectIdx, I>,
{
    async fn call_handler(&self, state: &mut S, effect: CH) -> CoControl<'a, Effs> {
        self.tail.call_handler(state, effect).await
    }
}

// --- HandleMut: dispatch effects to handlers (handlers can be in any order) ---

#[doc(hidden)]
pub trait HandleMut<'a, Effs: Effects<'a>, Handlers, Indices> {
    fn handle_mut(self, handlers: &mut Handlers) -> CoControl<'a, Effs>;
}

impl<'a, Effs: Effects<'a>, Handlers> HandleMut<'a, Effs, Handlers, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn handle_mut(self, _: &mut Handlers) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs, CH, CTail, Handlers, InjectIdx, FindIdx, ITail>
    HandleMut<'a, Effs, Handlers, HCons<(InjectIdx, FindIdx), ITail>> for Coproduct<CH, CTail>
where
    Effs: Effects<'a>,
    CH: Effect,
    Handlers: FindHandler<'a, Effs, CH, InjectIdx, FindIdx>,
    CTail: HandleMut<'a, Effs, Handlers, ITail>,
{
    fn handle_mut(self, handlers: &mut Handlers) -> CoControl<'a, Effs> {
        match self {
            Coproduct::Inl(head) => handlers.call_handler(head),
            Coproduct::Inr(rest) => rest.handle_mut(handlers),
        }
    }
}

// --- HandleWith: dispatch effects to stateful handlers (any order) ---

#[doc(hidden)]
pub trait HandleWith<'a, Effs: Effects<'a>, Handlers, S, Indices> {
    fn handle_with(self, state: &mut S, handlers: &Handlers) -> CoControl<'a, Effs>;
}

impl<'a, Effs: Effects<'a>, Handlers, S> HandleWith<'a, Effs, Handlers, S, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn handle_with(self, _: &mut S, _: &Handlers) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs, CH, CTail, Handlers, S, InjectIdx, FindIdx, ITail>
    HandleWith<'a, Effs, Handlers, S, HCons<(InjectIdx, FindIdx), ITail>> for Coproduct<CH, CTail>
where
    Effs: Effects<'a>,
    CH: Effect,
    Handlers: FindHandlerWith<'a, Effs, CH, S, InjectIdx, FindIdx>,
    CTail: HandleWith<'a, Effs, Handlers, S, ITail>,
{
    fn handle_with(self, state: &mut S, handlers: &Handlers) -> CoControl<'a, Effs> {
        match self {
            Coproduct::Inl(head) => handlers.call_handler(state, head),
            Coproduct::Inr(rest) => rest.handle_with(state, handlers),
        }
    }
}

// --- AsyncHandleMut: async dispatch (any order) ---

#[doc(hidden)]
pub trait AsyncHandleMut<'a, Effs: Effects<'a>, Handlers, Indices> {
    fn handle_mut(self, handlers: &mut Handlers) -> impl Future<Output = CoControl<'a, Effs>>;
}

impl<'a, Effs: Effects<'a>, Handlers> AsyncHandleMut<'a, Effs, Handlers, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn handle_mut(self, _: &mut Handlers) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs, CH, CTail, Handlers, InjectIdx, FindIdx, ITail>
    AsyncHandleMut<'a, Effs, Handlers, HCons<(InjectIdx, FindIdx), ITail>> for Coproduct<CH, CTail>
where
    Effs: Effects<'a>,
    CH: Effect,
    Handlers: AsyncFindHandler<'a, Effs, CH, InjectIdx, FindIdx>,
    CTail: AsyncHandleMut<'a, Effs, Handlers, ITail>,
{
    async fn handle_mut(self, handlers: &mut Handlers) -> CoControl<'a, Effs> {
        match self {
            Coproduct::Inl(head) => handlers.call_handler(head).await,
            Coproduct::Inr(rest) => rest.handle_mut(handlers).await,
        }
    }
}

// --- AsyncHandleWith: async stateful dispatch (any order) ---

#[doc(hidden)]
pub trait AsyncHandleWith<'a, Effs: Effects<'a>, Handlers, S, Indices> {
    fn handle_with(
        self,
        state: &mut S,
        handlers: &Handlers,
    ) -> impl Future<Output = CoControl<'a, Effs>>;
}

impl<'a, Effs: Effects<'a>, Handlers, S> AsyncHandleWith<'a, Effs, Handlers, S, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn handle_with(self, _: &mut S, _: &Handlers) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs, CH, CTail, Handlers, S, InjectIdx, FindIdx, ITail>
    AsyncHandleWith<'a, Effs, Handlers, S, HCons<(InjectIdx, FindIdx), ITail>>
    for Coproduct<CH, CTail>
where
    Effs: Effects<'a>,
    CH: Effect,
    Handlers: AsyncFindHandlerWith<'a, Effs, CH, S, InjectIdx, FindIdx>,
    CTail: AsyncHandleWith<'a, Effs, Handlers, S, ITail>,
{
    async fn handle_with(self, state: &mut S, handlers: &Handlers) -> CoControl<'a, Effs> {
        match self {
            Coproduct::Inl(head) => handlers.call_handler(state, head).await,
            Coproduct::Inr(rest) => rest.handle_with(state, handlers).await,
        }
    }
}

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
