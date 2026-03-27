use frunk_core::coproduct::{CNil, Coproduct};
use frunk_core::indices::{Here, There};

/// An algebraic effect that can be yielded from a computation.
///
/// Each effect defines a [`Resume`](Effect::Resume) type that determines
/// what value the handler must provide to resume the computation.
///
/// The [`shorten_resume`](Effect::shorten_resume) method witnesses that
/// `Resume<'r>` is covariant in `'r`, allowing resume values with a longer
/// lifetime to be safely used where a shorter lifetime is expected.  This is
/// needed by [`Yielder::invoke`](crate::Yielder::invoke) when invoking
/// sub-programs that borrow from shorter-lived data than the outer program.
///
/// The `#[effect]` macro derives both items automatically.  If you implement
/// `Effect` by hand you must also provide `shorten_resume`; for most types
/// the body is simply `resume`:
///
/// ```ignore
/// struct MyEffect;
/// impl Effect for MyEffect {
///     type Resume<'r> = &'r str;
///     fn shorten_resume<'a: 'b, 'b>(resume: &'a str) -> &'b str { resume }
/// }
/// ```
pub trait Effect {
    /// The type of value that the handler must provide to resume the computation
    /// after this effect is yielded.
    ///
    /// This is a generic associated type parameterized by a lifetime `'r`,
    /// allowing handlers to resume with borrowed data (e.g., `&'r str`).
    type Resume<'r>;

    /// Convert a resume value from a longer lifetime to a shorter one.
    ///
    /// This witnesses the covariance of [`Resume`](Effect::Resume) in its
    /// lifetime parameter.
    fn shorten_resume<'a: 'b, 'b>(resume: Self::Resume<'a>) -> Self::Resume<'b>;
}

/// Maps a coproduct of effects to a coproduct of their resume types.
///
/// This trait is automatically implemented for coproducts of [`Effect`] types
/// and is used internally to compute the resume type for a set of effects.
pub trait MapResume {
    /// The coproduct of resume types corresponding to the effects.
    type Output<'r>;

    /// Convert all resume values in the coproduct from lifetime `'a` to `'b`.
    ///
    /// Used by [`Yielder::invoke`](crate::Yielder::invoke) to convert resume
    /// values from the outer handler's lifetime to the sub-program's shorter
    /// lifetime.
    #[doc(hidden)]
    fn shorten_resumes<'a: 'b, 'b>(resumes: Self::Output<'a>) -> Self::Output<'b>;
}

impl MapResume for CNil {
    type Output<'r> = CNil;

    #[inline]
    fn shorten_resumes<'a: 'b, 'b>(resumes: CNil) -> CNil {
        resumes
    }
}

impl<H: Effect, T: MapResume> MapResume for Coproduct<H, T> {
    type Output<'r> = Coproduct<H::Resume<'r>, <T as MapResume>::Output<'r>>;

    #[inline]
    fn shorten_resumes<'a: 'b, 'b>(
        resumes: Coproduct<H::Resume<'a>, T::Output<'a>>,
    ) -> Coproduct<H::Resume<'b>, T::Output<'b>> {
        match resumes {
            Coproduct::Inl(head) => Coproduct::Inl(H::shorten_resume(head)),
            Coproduct::Inr(tail) => Coproduct::Inr(T::shorten_resumes(tail)),
        }
    }
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

/// Inject a resume value into the resume coproduct at the position
/// corresponding to effect `E`.
///
/// This trait resolves the coproduct index from the *effect type* rather than
/// the resume type, avoiding ambiguity when multiple effects share the same
/// resume type.
pub trait InjectResume<'a, E: Effect, Index>: MapResume {
    /// Inject the resume value at the correct position.
    fn inject_resume(r: E::Resume<'a>) -> Resumes<'a, Self>;
}

impl<'a, E: Effect, T: MapResume> InjectResume<'a, E, Here> for Coproduct<E, T> {
    #[inline]
    fn inject_resume(r: E::Resume<'a>) -> Resumes<'a, Self> {
        Coproduct::Inl(r)
    }
}

impl<'a, H: Effect, E: Effect, T: MapResume, TailIndex> InjectResume<'a, E, There<TailIndex>>
    for Coproduct<H, T>
where
    T: InjectResume<'a, E, TailIndex>,
{
    #[inline]
    fn inject_resume(r: E::Resume<'a>) -> Resumes<'a, Self> {
        Coproduct::Inr(T::inject_resume(r))
    }
}

/// Internal start signal used to kick off a coroutine.
///
/// This is prepended to the effect coproduct via [`CanStart`] so that
/// the generator can distinguish its initial activation from effect resumes.
#[derive(Copy, Clone, Debug)]
pub struct Start;

impl Effect for Start {
    type Resume<'r> = Start;

    #[inline]
    fn shorten_resume<'a: 'b, 'b>(resume: Start) -> Start {
        resume
    }
}

/// Wraps an effect coproduct with a [`Start`] signal at the head.
///
/// Used internally by the coroutine machinery to bootstrap the generator.
pub type CanStart<Effs> = Coproduct<Start, Effs>;
