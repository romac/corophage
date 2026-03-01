use std::fmt;
use std::marker::PhantomData;

use frunk_core::coproduct::CoprodInjector;

use crate::effect::{Effects, Resumes};

pub enum CoControl<'a, Effs>
where
    Effs: Effects<'a>,
{
    Cancel,
    Resume(Resumes<'a, Effs>),
    #[doc(hidden)]
    _Phantom(std::convert::Infallible, PhantomData<&'a ()>),
}

impl<'a, Effs> CoControl<'a, Effs>
where
    Effs: Effects<'a>,
{
    pub fn cancel() -> Self {
        Self::Cancel
    }

    pub fn resume<R, Index>(r: R) -> Self
    where
        Resumes<'a, Effs>: CoprodInjector<R, Index>,
    {
        Self::Resume(Resumes::<'a, Effs>::inject(r))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("computation was cancelled")
    }
}

impl std::error::Error for Cancelled {}
