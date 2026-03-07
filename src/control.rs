use std::fmt;

use frunk_core::coproduct::CoprodInjector;

use crate::effect::{Effects, Resumes};

/// The control flow decision returned by an effect handler.
///
/// After handling an effect, the handler returns either:
/// - [`Cancel`](CoControl::Cancel) to abort the computation, or
/// - [`Resume`](CoControl::Resume) to continue the computation with a value.
pub enum CoControl<'a, Effs>
where
    Effs: Effects<'a>,
{
    /// Cancel the computation. The runner will return [`Err(Cancelled)`](Cancelled).
    Cancel,
    /// Resume the computation with the given value.
    Resume(Resumes<'a, Effs>),
}

impl<'a, Effs> CoControl<'a, Effs>
where
    Effs: Effects<'a>,
{
    /// Create a [`Cancel`](CoControl::Cancel) control flow value.
    pub fn cancel() -> Self {
        Self::Cancel
    }

    /// Create a [`Resume`](CoControl::Resume) control flow value
    /// by injecting the resume value into the correct coproduct position.
    pub fn resume<R, Index>(r: R) -> Self
    where
        Resumes<'a, Effs>: CoprodInjector<R, Index>,
    {
        Self::Resume(Resumes::<'a, Effs>::inject(r))
    }
}

/// Error returned when a computation is cancelled by a handler.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("computation was cancelled")
    }
}

impl std::error::Error for Cancelled {}
