use std::future::Future;
use std::marker::PhantomData;
use std::ops::Add;

use frunk_core::coproduct::{CNil, Coproduct, CoproductSubsetter};
use frunk_core::hlist::{HCons, HNil};

use crate::control::Cancelled;
use crate::coproduct::{AsyncHandleMut, AsyncHandleWith, HandleMut, HandleWith, HandlersToEffects};
use crate::coroutine::{Co, CoSend, GenericCo, Yielder};
use crate::effect::{CanStart, Effects, Resumes};
use crate::locality::{Local, Locality, Sendable};

/// A computation with incrementally attached effect handlers.
///
/// Handlers are added one at a time via [`handle`](Program::handle),
/// or in bulk via [`handle_all`](Program::handle_all).
/// Once all effects are handled (`Remaining = CNil`), the computation
/// can be executed via [`run`](Program::run) or [`run_sync`](Program::run_sync).
pub struct Program<'a, Effs: Effects<'a>, R, L: Locality, Remaining, Handlers> {
    co: GenericCo<'a, Effs, R, L>,
    handlers: Handlers,
    _remaining: PhantomData<Remaining>,
}

impl<'a, Effs, R> Program<'a, Effs, R, Local, Effs, HNil>
where
    Effs: Effects<'a>,
{
    /// Create a new program from a computation closure.
    pub fn new<F>(f: impl FnOnce(Yielder<'a, Effs>) -> F + 'a) -> Self
    where
        F: Future<Output = R>,
    {
        Self::from_co(Co::new(f))
    }
}

impl<'a, Effs, R> Program<'a, Effs, R, Sendable, Effs, HNil>
where
    Effs: Effects<'a>,
    for<'r> Resumes<'r, CanStart<Effs>>: Send + Sync,
{
    /// Create a new `Send`-able program from a computation closure.
    pub fn new_send<F>(f: impl FnOnce(Yielder<'a, Effs>) -> F + Send + 'a) -> Self
    where
        F: Future<Output = R> + Send,
    {
        Self::from_co(CoSend::new(f))
    }
}

impl<'a, Effs, R, L> Program<'a, Effs, R, L, Effs, HNil>
where
    Effs: Effects<'a>,
    L: Locality,
{
    /// Create a program from an existing coroutine.
    pub fn from_co(co: GenericCo<'a, Effs, R, L>) -> Self {
        Program {
            co,
            handlers: HNil,
            _remaining: PhantomData,
        }
    }
}

impl<'a, Effs, R, L, Head, Tail, Handlers> Program<'a, Effs, R, L, Coproduct<Head, Tail>, Handlers>
where
    Effs: Effects<'a>,
    L: Locality,
{
    /// Attach a handler for the next unhandled effect.
    ///
    /// Handlers must be attached in the same order as the effects
    /// appear in the `Effects![...]` list.
    pub fn handle<F>(
        self,
        handler: F,
    ) -> Program<'a, Effs, R, L, Tail, <Handlers as Add<HCons<F, HNil>>>::Output>
    where
        Handlers: Add<HCons<F, HNil>>,
    {
        Program {
            co: self.co,
            handlers: self.handlers
                + HCons {
                    head: handler,
                    tail: HNil,
                },
            _remaining: PhantomData,
        }
    }
}

type HandleEffects<'a, Remaining, H, Effs, HandleIdx, SubsetIdx> =
    <Remaining as CoproductSubsetter<
        <H as HandlersToEffects<'a, Effs, HandleIdx>>::Effects,
        SubsetIdx,
    >>::Remainder;

impl<'a, Effs, R, L, Remaining, Handlers> Program<'a, Effs, R, L, Remaining, Handlers>
where
    Effs: Effects<'a>,
    L: Locality,
{
    /// Attach multiple handlers at once from an HList.
    ///
    /// The handlers can be for any subset of the remaining effects,
    /// in any order. The effect types are inferred from the handler
    /// closure signatures, and `CoproductSubsetter` removes those
    /// effects from the `Remaining` set.
    #[allow(clippy::type_complexity)]
    pub fn handle_all<H, HandleIdx, SubsetIdx>(
        self,
        handlers: H,
    ) -> Program<
        'a,
        Effs,
        R,
        L,
        HandleEffects<'a, Remaining, H, Effs, HandleIdx, SubsetIdx>,
        <Handlers as Add<H>>::Output,
    >
    where
        H: HandlersToEffects<'a, Effs, HandleIdx>,
        Remaining: CoproductSubsetter<H::Effects, SubsetIdx>,
        Handlers: Add<H>,
    {
        Program {
            co: self.co,
            handlers: self.handlers + handlers,
            _remaining: PhantomData,
        }
    }
}

impl<'a, Effs, R, L, Handlers> Program<'a, Effs, R, L, CNil, Handlers>
where
    Effs: Effects<'a>,
    L: Locality,
{
    /// Run the computation synchronously.
    pub fn run_sync<Indices>(self) -> Result<R, Cancelled>
    where
        Effs: HandleMut<'a, Effs, Handlers, Indices>,
    {
        let mut handlers = self.handlers;
        crate::sync::run(self.co, &mut handlers)
    }

    /// Run the computation synchronously with shared state.
    pub fn run_sync_stateful<S, Indices>(self, state: &mut S) -> Result<R, Cancelled>
    where
        Effs: HandleWith<'a, Effs, Handlers, S, Indices>,
    {
        let handlers = self.handlers;
        crate::sync::run_stateful(self.co, state, &handlers)
    }

    /// Run the computation asynchronously.
    pub async fn run<Indices>(self) -> Result<R, Cancelled>
    where
        Effs: AsyncHandleMut<'a, Effs, Handlers, Indices>,
    {
        let mut handlers = self.handlers;
        crate::asynk::run(self.co, &mut handlers).await
    }

    /// Run the computation asynchronously with shared state.
    pub async fn run_stateful<S, Indices>(self, state: &mut S) -> Result<R, Cancelled>
    where
        Effs: AsyncHandleWith<'a, Effs, Handlers, S, Indices>,
    {
        let handlers = self.handlers;
        crate::asynk::run_stateful(self.co, state, &handlers).await
    }
}

/// Attach a handler for the next unhandled effect of a [`Program`].
///
/// This is a free-function equivalent of [`Program::handle`].
pub fn handle<'a, Effs, R, L, Head, Tail, Handlers, F>(
    program: Program<'a, Effs, R, L, Coproduct<Head, Tail>, Handlers>,
    handler: F,
) -> Program<'a, Effs, R, L, Tail, <Handlers as Add<HCons<F, HNil>>>::Output>
where
    Effs: Effects<'a>,
    L: Locality,
    Handlers: Add<HCons<F, HNil>>,
{
    program.handle(handler)
}
