use std::future::Future;
use std::pin::Pin;

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Local {}
    impl Sealed for super::Sendable {}
}

/// Marker for coroutines that are not `Send`.
///
/// This is the default locality for [`Co`](crate::Co).
pub struct Local;

/// Marker for coroutines that are `Send`.
///
/// Use this as the third type parameter of [`Co`](crate::Co) to make the
/// coroutine `Send`, allowing it to be used with `tokio::spawn` and other
/// multi-threaded executors.
pub struct Sendable;

/// Sealed trait that controls whether a [`Co`](crate::Co) coroutine is `Send`.
///
/// This trait is sealed and cannot be implemented outside of this crate.
/// The only valid implementations are [`Local`] and [`Sendable`].
pub trait Locality: sealed::Sealed + 'static {
    #[doc(hidden)]
    type PinBoxFuture<'a, A>: Future<Output = A>;
}

impl Locality for Local {
    type PinBoxFuture<'a, A> = Pin<Box<dyn Future<Output = A> + 'a>>;
}

impl Locality for Sendable {
    type PinBoxFuture<'a, A> = Pin<Box<dyn Future<Output = A> + Send + 'a>>;
}
