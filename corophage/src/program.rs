use std::future::{Future, Ready};
use std::marker::PhantomData;
use std::ops::Add;
use std::pin::Pin;

use frunk_core::coproduct::{CNil, CoproductSubsetter};
use frunk_core::hlist::{HCons, HNil};

use crate::control::Cancelled;
use crate::coproduct::dispatch::{Dispatch, Synchronous};
use crate::coproduct::{AsyncHandleMut, AsyncHandleWith, HandlersToEffects};
use crate::coroutine::{Co, CoSend, Coroutine, GenericCo, Yielder};
use crate::effect::{CanStart, Effects, Resumes};
use crate::execution::State;
use crate::locality::{Local, Locality, Sendable};

/// An unhandled program with an explicitly boxed body.
///
/// Use `.boxed()` or `.boxed_send()` when returning this erased alias. Inferred
/// programs and `#[effectful]` functions retain their concrete body by default.
pub type Effectful<'a, Effs, R, L = Local> =
    Program<'a, Effs, <L as Locality>::PinBoxFuture<'a, R>, Effs, HNil, (), L>;

/// Deprecated alias for [`Effectful`].
#[deprecated(since = "0.3.0", note = "renamed to `Effectful`")]
pub type Eff<'a, Effs, R, L = Local> = Effectful<'a, Effs, R, L>;

impl<'a, R> Program<'a, CNil, Ready<R>> {
    /// Create a program without erasing the body future's type or auto traits.
    ///
    /// Both `Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move { ... })`
    /// and `Program::new::<Effects![Ask], _>(|mut y| async move { ... })` work.
    /// Sync and async handlers can share one list. For async closures that lend
    /// their captures, use [`crate::local`] or [`crate::local_stateful`].
    pub fn new<Effs, F>(
        f: impl FnOnce(Yielder<'a, Effs>) -> F + 'a,
    ) -> Program<'a, Effs, impl Future<Output = R> + 'a>
    where
        Effs: Effects<'a>,
        F: Future<Output = R> + 'a,
    {
        Program {
            co: crate::coroutine::concrete(f),
            state: (),
            handlers: HNil,
            remaining: PhantomData,
            mode: PhantomData,
        }
    }

    /// Construct an explicitly erased sendable body for compatibility.
    ///
    /// Prefer [`Program::new`] for inferred programs. The execution future's
    /// `Send` capability still depends on the handlers and state attached later.
    pub fn new_send<Effs, F>(
        f: impl FnOnce(Yielder<'a, Effs>) -> F + Send + 'a,
    ) -> Effectful<'a, Effs, R, Sendable>
    where
        Effs: Effects<'a>,
        for<'r> Resumes<'r, CanStart<Effs>>: Send + Sync,
        F: Future<Output = R> + Send,
    {
        Self::from_co(CoSend::new(f))
    }

    /// Wrap an existing erased coroutine, including callers generic over locality.
    pub fn from_co<Effs: Effects<'a>, L: Locality>(
        co: GenericCo<'a, Effs, R, L>,
    ) -> Effectful<'a, Effs, R, L> {
        Program {
            co: co.into_concrete(),
            state: (),
            handlers: HNil,
            remaining: PhantomData,
            mode: PhantomData,
        }
    }
}

type HandleEffects<'a, Remaining, H, Effs, HandleIdx, SubsetIdx> =
    <Remaining as CoproductSubsetter<
        <H as HandlersToEffects<'a, Effs, HandleIdx>>::Effects,
        SubsetIdx,
    >>::Remainder;

/// Conditional execution mode used by inferred programs.
#[doc(hidden)]
pub struct Inferred;

/// A program with a concrete coroutine body and conditional `Send`.
///
/// Construct with [`Program::new`]. Use `impl Future<Output = R>` for `F`
/// in a function's return type, or [box](Program::boxed) an unhandled
/// program to retain an existing [`Effectful`] signature.
pub struct Program<
    'a,
    Effs: Effects<'a>,
    F,
    Remaining = Effs,
    Handlers = HNil,
    S = (),
    Mode = Inferred,
> {
    pub(crate) co: Coroutine<'a, Effs, F>,
    state: S,
    handlers: Handlers,
    remaining: PhantomData<Remaining>,
    mode: PhantomData<fn() -> Mode>,
}

impl<'a, Effs: Effects<'a>, F, Remaining, Handlers, Mode>
    Program<'a, Effs, F, Remaining, Handlers, (), Mode>
{
    /// Bind shared state now and lend a fresh reborrow to each handler.
    ///
    /// The borrow ends on completion, cancellation, or drop. State-free handlers
    /// may be attached before or after this call and ignore the bound state.
    pub fn with_state<S>(self, state: &mut S) -> Program<'a, Effs, F, Remaining, Handlers, &mut S> {
        Program {
            co: self.co,
            state,
            handlers: self.handlers,
            remaining: PhantomData,
            mode: PhantomData,
        }
    }
}

impl<'a, Effs: Effects<'a>, F: Future> Program<'a, Effs, F> {
    /// Convert an unhandled program to the existing local erased representation.
    ///
    /// The erased local `.run()` requires async callables and accepts lending
    /// closures directly. Call [`Program::with_state`] before `.run()` to use
    /// mixed sync/async handlers or [`crate::local`] / [`crate::local_stateful`].
    /// A unit state is sufficient if no handler needs shared state.
    /// Synchronous handlers remain supported by `.run_sync()`.
    pub fn boxed(self) -> Effectful<'a, Effs, F::Output>
    where
        F: 'a,
    {
        Program::from_co(self.co.boxed())
    }

    /// Convert an unhandled program to the existing sendable erased representation.
    pub fn boxed_send(self) -> Effectful<'a, Effs, F::Output, Sendable>
    where
        F: Send + 'a,
        for<'r> Resumes<'r, CanStart<Effs>>: Send + Sync,
    {
        Program::from_co(self.co.boxed_send())
    }
}

impl<'a, Effs: Effects<'a>, F, Remaining, Handlers, S, Mode>
    Program<'a, Effs, F, Remaining, Handlers, S, Mode>
{
    /// Attach a handler in any effect order.
    ///
    /// Inferred execution can mix sync and async handlers. Local erased async
    /// execution has the compatibility restrictions described by [`Program::boxed`].
    #[allow(clippy::type_complexity)]
    pub fn handle<H, HandleIdx, SubsetIdx>(
        self,
        handler: H,
    ) -> Program<
        'a,
        Effs,
        F,
        HandleEffects<'a, Remaining, HCons<H, HNil>, Effs, HandleIdx, SubsetIdx>,
        <Handlers as Add<HCons<H, HNil>>>::Output,
        S,
        Mode,
    >
    where
        HCons<H, HNil>: HandlersToEffects<'a, Effs, HandleIdx>,
        Remaining: CoproductSubsetter<
                <HCons<H, HNil> as HandlersToEffects<'a, Effs, HandleIdx>>::Effects,
                SubsetIdx,
            >,
        Handlers: Add<HCons<H, HNil>>,
    {
        self.handle_all(HCons {
            head: handler,
            tail: HNil,
        })
    }

    /// Attach several handlers, retaining compile-time missing/duplicate checks.
    #[allow(clippy::type_complexity)]
    pub fn handle_all<H, HandleIdx, SubsetIdx>(
        self,
        handlers: H,
    ) -> Program<
        'a,
        Effs,
        F,
        HandleEffects<'a, Remaining, H, Effs, HandleIdx, SubsetIdx>,
        <Handlers as Add<H>>::Output,
        S,
        Mode,
    >
    where
        H: HandlersToEffects<'a, Effs, HandleIdx>,
        Remaining: CoproductSubsetter<H::Effects, SubsetIdx>,
        Handlers: Add<H>,
    {
        Program {
            co: self.co,
            state: self.state,
            handlers: self.handlers + handlers,
            remaining: PhantomData,
            mode: PhantomData,
        }
    }
}

impl<'a, Effs: Effects<'a>, F: Future, H, S: State> Program<'a, Effs, F, CNil, H, S> {
    /// Execute with cooperative scheduling. `Send` follows the stored components.
    pub fn run<I>(self) -> impl Future<Output = Result<F::Output, Cancelled>>
    where
        Effs: for<'call> Dispatch<'a, Effs, &'call mut S::Value, &'call mut H, I>,
    {
        crate::execution::drive::<true, _, _, _, _, _, _>(
            self.co.generator(),
            self.state,
            self.handlers,
            crate::execution::CoproductDispatch::<'a, Effs, I>::new(),
        )
    }
}

impl<'a, Effs: Effects<'a>, F: Future, H, S: State, Mode> Program<'a, Effs, F, CNil, H, S, Mode> {
    /// Execute inline with synchronous handlers through the same dispatch loop.
    ///
    /// # Panics
    ///
    /// Panics if the computation suspends on an ordinary future. It never spins
    /// or blocks on `Pending`; async handlers are rejected at compile time.
    pub fn run_sync<I>(self) -> Result<F::Output, Cancelled>
    where
        Effs: for<'call> Dispatch<'a, Effs, &'call mut S::Value, &'call mut H, I>,
        I: Synchronous,
    {
        crate::execution::inline(crate::execution::drive::<false, _, _, _, _, _, _>(
            self.co.generator(),
            self.state,
            self.handlers,
            crate::execution::CoproductDispatch::<'a, Effs, I>::new(),
        ))
    }
}

impl<'a, Effs: Effects<'a>, F: Future, H> Program<'a, Effs, F, CNil, H> {
    /// Compatibility shorthand for binding state at this terminal call and running.
    pub fn run_stateful<S, I>(
        self,
        state: &mut S,
    ) -> impl Future<Output = Result<F::Output, Cancelled>>
    where
        Effs: for<'call> Dispatch<'a, Effs, &'call mut S, &'call mut H, I>,
    {
        self.with_state(state).run()
    }
}

impl<'a, Effs: Effects<'a>, F: Future, H, Mode> Program<'a, Effs, F, CNil, H, (), Mode> {
    /// Compatibility shorthand for [`Program::with_state`] followed by inline execution.
    ///
    /// # Panics
    /// Panics if the body suspends on an ordinary future, as in [`Program::run_sync`].
    pub fn run_sync_stateful<S, I>(self, state: &mut S) -> Result<F::Output, Cancelled>
    where
        Effs: for<'call> Dispatch<'a, Effs, &'call mut S, &'call mut H, I>,
        I: Synchronous,
    {
        self.with_state(state).run_sync()
    }
}

// Erased aliases keep their established execution contracts. These wrappers
// reuse the core or the two local lending paths; they introduce no dispatch loop.
impl<'a, Effs: Effects<'a>, R, H>
    Program<'a, Effs, Pin<Box<dyn Future<Output = R> + 'a>>, CNil, H, (), Local>
{
    /// Execute an erased local program, retaining lending async closure support.
    ///
    /// Every handler must be an async callable. To mix sync/async handlers or use
    /// the explicit local adapters, call [`Program::with_state`] before `.run()`.
    pub async fn run<I>(self) -> Result<R, Cancelled>
    where
        Effs: AsyncHandleMut<'a, Effs, H, I>,
    {
        let mut handlers = self.handlers;
        crate::asynk::run(Co::from_concrete(self.co), &mut handlers).await
    }

    /// Execute an erased local program with state borrowed at this call site.
    ///
    /// Every handler must be a stateful async callable compatible with `AsyncFn`.
    /// Call [`Program::with_state`] followed by `.run()` to use mixed handlers
    /// or the explicit local adapters.
    pub async fn run_stateful<S, I>(self, state: &mut S) -> Result<R, Cancelled>
    where
        Effs: AsyncHandleWith<'a, Effs, H, S, I>,
    {
        crate::asynk::run_stateful(Co::from_concrete(self.co), state, &self.handlers).await
    }
}

impl<'a, Effs: Effects<'a>, R, H>
    Program<'a, Effs, Pin<Box<dyn Future<Output = R> + Send + 'a>>, CNil, H, (), Sendable>
{
    /// Execute an erased sendable program with its explicit `Send` guarantee.
    pub fn run<I>(self) -> impl Future<Output = Result<R, Cancelled>> + Send
    where
        R: Send,
        H: Send,
        Resumes<'a, Effs>: Send + Sync,
        Effs: for<'s, 'h> Dispatch<'a, Effs, &'s mut (), &'h mut H, I>,
        for<'s, 'h> <Effs as Dispatch<'a, Effs, &'s mut (), &'h mut H, I>>::Future: Send,
    {
        crate::execution::drive_send(
            self.co.generator(),
            (),
            self.handlers,
            crate::execution::CoproductDispatch::<'a, Effs, I>::new(),
        )
    }

    /// Execute an erased sendable program with `Send` state and `Send + Sync`
    /// handlers, retaining the old terminal method's capability checks.
    pub fn run_stateful<S, I>(
        self,
        state: &mut S,
    ) -> impl Future<Output = Result<R, Cancelled>> + Send
    where
        R: Send,
        S: Send,
        H: Send + Sync,
        Resumes<'a, Effs>: Send + Sync,
        Effs: for<'s, 'h> Dispatch<'a, Effs, &'s mut S, &'h mut H, I>,
        for<'s, 'h> <Effs as Dispatch<'a, Effs, &'s mut S, &'h mut H, I>>::Future: Send,
    {
        crate::execution::drive_send(
            self.co.generator(),
            state,
            self.handlers,
            crate::execution::CoproductDispatch::<'a, Effs, I>::new(),
        )
    }
}
