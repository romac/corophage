use frunk::coproduct::CNil;
use frunk::{Coproduct, HCons, HNil};

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
        unreachable!()
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
        unreachable!()
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
