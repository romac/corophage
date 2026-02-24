use std::future::Future;

pub use frunk_core::coproduct::{CNil, Coproduct};
pub use frunk_core::hlist::{HCons, HNil};

pub trait FoldMut<F, R> {
    fn fold_mut(self, f: &mut F) -> R;
}

impl<R> FoldMut<CNil, R> for CNil {
    fn fold_mut(self, _: &mut CNil) -> R {
        match self {}
    }
}

impl<R> FoldMut<HNil, R> for CNil {
    fn fold_mut(self, _: &mut HNil) -> R {
        match self {}
    }
}

impl<F, R, FTail, CH, CTail> FoldMut<HCons<F, FTail>, R> for Coproduct<CH, CTail>
where
    F: FnMut(CH) -> R,
    CTail: FoldMut<FTail, R>,
{
    fn fold_mut(self, f: &mut HCons<F, FTail>) -> R {
        let f_head = &mut f.head;
        let f_tail = &mut f.tail;

        match self {
            Coproduct::Inl(r) => (f_head)(r),
            Coproduct::Inr(rest) => rest.fold_mut(f_tail),
        }
    }
}

pub trait FoldWith<F, S, R> {
    fn fold_with(self, s: &mut S, f: &F) -> R;
}

impl<S, R> FoldWith<CNil, S, R> for CNil {
    fn fold_with(self, _: &mut S, _: &CNil) -> R {
        match self {}
    }
}

impl<S, R> FoldWith<HNil, S, R> for CNil {
    fn fold_with(self, _: &mut S, _: &HNil) -> R {
        match self {}
    }
}

impl<F, S, R, FTail, CH, CTail> FoldWith<HCons<F, FTail>, S, R> for Coproduct<CH, CTail>
where
    F: Fn(&mut S, CH) -> R,
    CTail: FoldWith<FTail, S, R>,
{
    fn fold_with(self, s: &mut S, f: &HCons<F, FTail>) -> R {
        let f_head = &f.head;
        let f_tail = &f.tail;

        match self {
            Coproduct::Inl(r) => (f_head)(s, r),
            Coproduct::Inr(rest) => rest.fold_with(s, f_tail),
        }
    }
}

pub trait AsyncFoldMut<F, R> {
    fn fold_mut(self, f: &mut F) -> impl Future<Output = R>;
}

impl<R> AsyncFoldMut<CNil, R> for CNil {
    async fn fold_mut(self, _: &mut CNil) -> R {
        match self {}
    }
}

impl<R> AsyncFoldMut<HNil, R> for CNil {
    async fn fold_mut(self, _: &mut HNil) -> R {
        match self {}
    }
}

impl<F, R, FTail, CH, CTail> AsyncFoldMut<HCons<F, FTail>, R> for Coproduct<CH, CTail>
where
    F: AsyncFnMut(CH) -> R,
    CTail: AsyncFoldMut<FTail, R>,
{
    async fn fold_mut(self, f: &mut HCons<F, FTail>) -> R {
        let f_head = &mut f.head;
        let f_tail = &mut f.tail;

        match self {
            Coproduct::Inl(r) => (f_head)(r).await,
            Coproduct::Inr(rest) => rest.fold_mut(f_tail).await,
        }
    }
}

pub trait AsyncFoldWith<F, S, R> {
    fn fold_with(self, s: &mut S, f: &F) -> impl Future<Output = R>;
}

impl<S, R> AsyncFoldWith<CNil, S, R> for CNil {
    async fn fold_with(self, _: &mut S, _: &CNil) -> R {
        match self {}
    }
}

impl<S, R> AsyncFoldWith<HNil, S, R> for CNil {
    async fn fold_with(self, _: &mut S, _: &HNil) -> R {
        match self {}
    }
}

impl<F, S, R, FTail, CH, CTail> AsyncFoldWith<HCons<F, FTail>, S, R> for Coproduct<CH, CTail>
where
    F: AsyncFn(&mut S, CH) -> R,
    CTail: AsyncFoldWith<FTail, S, R>,
{
    async fn fold_with(self, s: &mut S, f: &HCons<F, FTail>) -> R {
        let f_head = &f.head;
        let f_tail = &f.tail;

        match self {
            Coproduct::Inl(r) => (f_head)(s, r).await,
            Coproduct::Inr(rest) => rest.fold_with(s, f_tail).await,
        }
    }
}
