use frunk::{HCons, HNil};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Coproduct<H, T> {
    Here(H),
    There(T),
}

impl<H, T> Coproduct<H, T> {
    pub fn inject<A, Idx>(a: A) -> Self
    where
        Self: Inject<A, Idx>,
    {
        Inject::inject(a)
    }

    pub fn extract<A, Idx>(self) -> A
    where
        Self: Extract<A, Idx>,
    {
        Extract::extract(self)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CNil {}

pub struct Here;
pub struct There<T>(T);

#[macro_export]
macro_rules! Coproduct {
    () => {
        CNil
    };
    ($head:ty) => {
        Coproduct<$head, CNil>
    };
    ($head:ty, $($tail:ty),+) => {
        Coproduct<$head, Coproduct!($($tail),*)>
    };
}

pub trait Inject<A, Idx> {
    fn inject(a: A) -> Self;
}

impl<A, Tail> Inject<A, Here> for Coproduct<A, Tail> {
    fn inject(a: A) -> Self {
        Coproduct::Here(a)
    }
}

impl<A, Head, Tail, Idx> Inject<A, There<Idx>> for Coproduct<Head, Tail>
where
    Tail: Inject<A, Idx>,
{
    fn inject(a: A) -> Self {
        Coproduct::There(<Tail as Inject<A, Idx>>::inject(a))
    }
}

pub trait Extract<A, Idx> {
    fn extract(self) -> A;
}

impl<A, Tail> Extract<A, Here> for Coproduct<A, Tail> {
    fn extract(self) -> A {
        match self {
            Coproduct::Here(a) => a,
            _ => unreachable!(),
        }
    }
}

impl<A, B, Tail, Idx> Extract<A, There<Idx>> for Coproduct<B, Tail>
where
    Tail: Extract<A, Idx>,
{
    fn extract(self) -> A {
        match self {
            Coproduct::There(tail) => tail.extract(),
            _ => unreachable!(),
        }
    }
}

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
            Self::Here(r) => (f_head)(r),
            Self::There(rest) => rest.fold_mut(f_tail),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Abc = Coproduct!(usize, bool, char);

    #[test]
    fn it_works() {
        let _: Coproduct!(char) = Coproduct::inject('c');
        let _: Coproduct!(char, bool) = Coproduct::inject('c');
        let _: Coproduct!(char, bool) = Coproduct::inject(true);

        let a = Abc::inject::<usize, Here>(42);
        let b = Abc::inject::<bool, There<Here>>(1 == 1);
        let c = Abc::inject::<char, There<There<Here>>>('c');

        assert_eq!(a.extract::<usize, _>(), 42);
        assert_eq!(b.extract::<bool, _>(), 1 == 1);
        assert_eq!(c.extract::<char, _>(), 'c');
    }
}
