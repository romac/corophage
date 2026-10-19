use std::future::Future;
use std::pin::Pin;

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Local {}
    impl Sealed for super::Sendable {}
}

/// Marker for coroutines that are not `Send`.
///
/// This is the default locality for [`Co`](crate::coroutine::Co).
pub struct Local;

/// Marker for coroutines that are `Send`.
///
/// Use [`CoSend`](crate::coroutine::CoSend) or
/// `GenericCo<'a, Effs, Return, Sendable>` for a sendable erased coroutine.
/// [`Co`](crate::coroutine::Co) always uses [`Local`] and has no locality parameter.
pub struct Sendable;

/// Sealed trait that controls whether a [`Co`](crate::coroutine::Co) coroutine is `Send`.
///
/// This trait is sealed and cannot be implemented outside of this crate.
/// The only valid implementations are [`Local`] and [`Sendable`].
pub trait Locality: sealed::Sealed + 'static {
    #[doc(hidden)]
    type PinBoxFuture<'a, A>: Future<Output = A>;

    /// Execution mode of erased programs with this locality. Local programs
    /// share the inferred terminal methods; sendable programs keep their
    /// explicit `Send` guarantees.
    #[doc(hidden)]
    type Mode;
}

impl Locality for Local {
    type PinBoxFuture<'a, A> = Pin<Box<dyn Future<Output = A> + 'a>>;
    type Mode = crate::program::Inferred;
}

impl Locality for Sendable {
    type PinBoxFuture<'a, A> = Pin<Box<dyn Future<Output = A> + Send + 'a>>;
    type Mode = Sendable;
}
