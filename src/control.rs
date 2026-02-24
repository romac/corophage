use std::fmt;

use frunk_core::coproduct::CoprodInjector;

use crate::effect::{Effects, Resumes};

pub enum CoControl<Effs>
where
    Effs: Effects,
{
    Cancel,
    Resume(Resumes<Effs>),
}

impl<Effs> CoControl<Effs>
where
    Effs: Effects,
{
    pub fn cancel() -> Self {
        Self::Cancel
    }

    pub fn resume<R, Index>(r: R) -> Self
    where
        Resumes<Effs>: CoprodInjector<R, Index>,
    {
        Self::Resume(Resumes::<Effs>::inject(r))
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
