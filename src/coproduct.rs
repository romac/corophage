use std::future::Future;

pub use frunk_core::coproduct::{CNil, Coproduct};
pub use frunk_core::hlist::{HCons, HNil};

use crate::control::{CoControl, Control};
use crate::effect::{Effect, Effects, InjectResume};

#[doc(hidden)]
pub trait HandleMut<'a, Effs: Effects<'a>, F, Indices> {
    fn handle_mut(self, f: &mut F) -> CoControl<'a, Effs>;
}

impl<'a, Effs: Effects<'a>> HandleMut<'a, Effs, CNil, CNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn handle_mut(self, _: &mut CNil) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs: Effects<'a>> HandleMut<'a, Effs, HNil, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn handle_mut(self, _: &mut HNil) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs, F, FTail, CH, CTail, Index, ITail>
    HandleMut<'a, Effs, HCons<F, FTail>, HCons<Index, ITail>> for Coproduct<CH, CTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, Index>,
    CH: Effect,
    F: FnMut(CH) -> Control<CH::Resume<'a>>,
    CTail: HandleMut<'a, Effs, FTail, ITail>,
{
    fn handle_mut(self, f: &mut HCons<F, FTail>) -> CoControl<'a, Effs> {
        match self {
            Coproduct::Inl(head) => match (f.head)(head) {
                Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
                Control::Cancel => CoControl::Cancel,
            },
            Coproduct::Inr(rest) => rest.handle_mut(&mut f.tail),
        }
    }
}

#[doc(hidden)]
pub trait HandleWith<'a, Effs: Effects<'a>, F, S, Indices> {
    fn handle_with(self, s: &mut S, f: &F) -> CoControl<'a, Effs>;
}

impl<'a, Effs: Effects<'a>, S> HandleWith<'a, Effs, CNil, S, CNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn handle_with(self, _: &mut S, _: &CNil) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs: Effects<'a>, S> HandleWith<'a, Effs, HNil, S, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn handle_with(self, _: &mut S, _: &HNil) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs, F, S, FTail, CH, CTail, Index, ITail>
    HandleWith<'a, Effs, HCons<F, FTail>, S, HCons<Index, ITail>> for Coproduct<CH, CTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, Index>,
    CH: Effect,
    F: Fn(&mut S, CH) -> Control<CH::Resume<'a>>,
    CTail: HandleWith<'a, Effs, FTail, S, ITail>,
{
    fn handle_with(self, s: &mut S, f: &HCons<F, FTail>) -> CoControl<'a, Effs> {
        match self {
            Coproduct::Inl(head) => match (f.head)(s, head) {
                Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
                Control::Cancel => CoControl::Cancel,
            },
            Coproduct::Inr(rest) => rest.handle_with(s, &f.tail),
        }
    }
}

#[doc(hidden)]
pub trait AsyncHandleMut<'a, Effs: Effects<'a>, F, Indices> {
    fn handle_mut(self, f: &mut F) -> impl Future<Output = CoControl<'a, Effs>>;
}

impl<'a, Effs: Effects<'a>> AsyncHandleMut<'a, Effs, CNil, CNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn handle_mut(self, _: &mut CNil) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs: Effects<'a>> AsyncHandleMut<'a, Effs, HNil, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn handle_mut(self, _: &mut HNil) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs, F, FTail, CH, CTail, Index, ITail>
    AsyncHandleMut<'a, Effs, HCons<F, FTail>, HCons<Index, ITail>> for Coproduct<CH, CTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, Index>,
    CH: Effect,
    F: AsyncFnMut(CH) -> Control<CH::Resume<'a>>,
    CTail: AsyncHandleMut<'a, Effs, FTail, ITail>,
{
    async fn handle_mut(self, f: &mut HCons<F, FTail>) -> CoControl<'a, Effs> {
        match self {
            Coproduct::Inl(head) => match (f.head)(head).await {
                Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
                Control::Cancel => CoControl::Cancel,
            },
            Coproduct::Inr(rest) => rest.handle_mut(&mut f.tail).await,
        }
    }
}

#[doc(hidden)]
pub trait AsyncHandleWith<'a, Effs: Effects<'a>, F, S, Indices> {
    fn handle_with(self, s: &mut S, f: &F) -> impl Future<Output = CoControl<'a, Effs>>;
}

impl<'a, Effs: Effects<'a>, S> AsyncHandleWith<'a, Effs, CNil, S, CNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn handle_with(self, _: &mut S, _: &CNil) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs: Effects<'a>, S> AsyncHandleWith<'a, Effs, HNil, S, HNil> for CNil {
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn handle_with(self, _: &mut S, _: &HNil) -> CoControl<'a, Effs> {
        match self {}
    }
}

impl<'a, Effs, F, S, FTail, CH, CTail, Index, ITail>
    AsyncHandleWith<'a, Effs, HCons<F, FTail>, S, HCons<Index, ITail>> for Coproduct<CH, CTail>
where
    Effs: Effects<'a> + InjectResume<'a, CH, Index>,
    CH: Effect,
    F: AsyncFn(&mut S, CH) -> Control<CH::Resume<'a>>,
    CTail: AsyncHandleWith<'a, Effs, FTail, S, ITail>,
{
    async fn handle_with(self, s: &mut S, f: &HCons<F, FTail>) -> CoControl<'a, Effs> {
        match self {
            Coproduct::Inl(head) => match (f.head)(s, head).await {
                Control::Resume(r) => CoControl::Resume(Effs::inject_resume(r)),
                Control::Cancel => CoControl::Cancel,
            },
            Coproduct::Inr(rest) => rest.handle_with(s, &f.tail).await,
        }
    }
}
