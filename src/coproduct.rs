use frunk::coproduct::CNil;
use frunk::{Coproduct, HCons, HNil};

pub trait CoproductFoldableMut<F, R> {
    fn fold_mut(self, f: &mut F) -> R;
}

impl<R> CoproductFoldableMut<CNil, R> for CNil {
    fn fold_mut(self, _: &mut CNil) -> R {
        match self {}
    }
}

impl<R> CoproductFoldableMut<HNil, R> for CNil {
    fn fold_mut(self, _: &mut HNil) -> R {
        unreachable!()
    }
}

impl<F, R, FTail, CH, CTail> CoproductFoldableMut<HCons<F, FTail>, R> for Coproduct<CH, CTail>
where
    F: FnMut(CH) -> R,
    CTail: CoproductFoldableMut<FTail, R>,
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
