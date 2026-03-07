use frunk_core::coproduct::{CNil, Coproduct};

/// An algebraic effect that can be yielded from a computation.
///
/// Each effect defines a [`Resume`](Effect::Resume) type that determines
/// what value the handler must provide to resume the computation.
pub trait Effect {
    /// The type of value that the handler must provide to resume the computation
    /// after this effect is yielded.
    ///
    /// This is a generic associated type parameterized by a lifetime `'r`,
    /// allowing handlers to resume with borrowed data (e.g., `&'r str`).
    type Resume<'r>: Sync + Send;
}

/// Maps a coproduct of effects to a coproduct of their resume types.
///
/// This trait is automatically implemented for coproducts of [`Effect`] types
/// and is used internally to compute the resume type for a set of effects.
pub trait MapResume {
    /// The coproduct of resume types corresponding to the effects.
    type Output<'r>: Sync + Send;
}

impl MapResume for CNil {
    type Output<'r> = CNil;
}

impl<H: Effect, T: MapResume> MapResume for Coproduct<H, T> {
    type Output<'r> = Coproduct<H::Resume<'r>, <T as MapResume>::Output<'r>>;
}

/// A bound combining [`MapResume`], `Send`, `Sync`, and a lifetime,
/// satisfied by any coproduct of [`Effect`] types.
pub trait Effects<'a>: MapResume + Send + Sync + 'a {}

impl<'a, E> Effects<'a> for E where E: MapResume + Send + Sync + 'a {}

/// The coproduct of resume types for an effect set `E`.
///
/// Given `E = Effects![A, B, C]`, `Resumes<'r, E>` is
/// `Coproduct<A::Resume<'r>, Coproduct<B::Resume<'r>, Coproduct<C::Resume<'r>, CNil>>>`.
pub type Resumes<'r, E> = <E as MapResume>::Output<'r>;

/// Internal start signal used to kick off a coroutine.
///
/// This is prepended to the effect coproduct via [`CanStart`] so that
/// the generator can distinguish its initial activation from effect resumes.
#[derive(Copy, Clone, Debug)]
pub struct Start;

impl Effect for Start {
    type Resume<'r> = Start;
}

/// Wraps an effect coproduct with a [`Start`] signal at the head.
///
/// Used internally by the coroutine machinery to bootstrap the generator.
pub type CanStart<Effs> = Coproduct<Start, Effs>;
