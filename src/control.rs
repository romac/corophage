use std::fmt;

use crate::effect::{Effects, Resumes};

/// The result returned by an effect handler.
///
/// After handling an effect, the handler returns either:
/// - [`Resume`](Control::Resume) to continue the computation with a value, or
/// - [`Cancel`](Control::Cancel) to abort the computation.
///
// NOTE: Unlike the internal `CoControl` type, `Control` is parameterized
// by the resume type `R` rather than the full effect set, making handlers
// reusable across different effect sets.
pub enum Control<R> {
    /// Resume the computation with the given value.
    Resume(R),
    /// Cancel the computation. The runner will return [`Err(Cancelled)`](Cancelled).
    Cancel,
}

impl<R> Control<R> {
    /// Create a [`Resume`](Control::Resume) result to continue the computation.
    pub fn resume(r: R) -> Self {
        Self::Resume(r)
    }

    /// Create a [`Cancel`](Control::Cancel) result to abort the computation.
    pub fn cancel() -> Self {
        Self::Cancel
    }
}

impl<R> From<R> for Control<R> {
    fn from(r: R) -> Self {
        Control::Resume(r)
    }
}

#[doc(hidden)]
pub enum CoControl<'a, Effs>
where
    Effs: Effects<'a>,
{
    Cancel,
    Resume(Resumes<'a, Effs>),
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
