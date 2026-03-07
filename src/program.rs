use std::marker::PhantomData;
use std::ops::Add;

use frunk_core::coproduct::{CNil, Coproduct};
use frunk_core::hlist::{HCons, HNil};

use crate::CoControl;
use crate::control::Cancelled;
use crate::coproduct::{AsyncFoldMut, AsyncFoldWith, FoldMut, FoldWith};
use crate::coroutine::GenericCo;
use crate::effect::Effects;
use crate::locality::Locality;

/// A computation with incrementally attached effect handlers.
///
/// Handlers are added one at a time via [`handle`](Program::handle),
/// in the same order as the effects in the `Effects![...]` list.
/// Once all effects are handled (`Remaining = CNil`), the computation
/// can be executed via [`run`](Program::run) or [`run_sync`](Program::run_sync).
pub struct Program<'a, Effs: Effects<'a>, R, L: Locality, Remaining, Handlers> {
    co: GenericCo<'a, Effs, R, L>,
    handlers: Handlers,
    _remaining: PhantomData<Remaining>,
}

impl<'a, Effs, R, L> Program<'a, Effs, R, L, Effs, HNil>
where
    Effs: Effects<'a>,
    L: Locality,
{
    pub fn new(co: GenericCo<'a, Effs, R, L>) -> Self {
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

impl<'a, Effs, R, L, Handlers> Program<'a, Effs, R, L, CNil, Handlers>
where
    Effs: Effects<'a>,
    L: Locality,
{
    /// Run the computation synchronously.
    pub fn run_sync(self) -> Result<R, Cancelled>
    where
        Effs: FoldMut<Handlers, CoControl<'a, Effs>>,
    {
        let mut handlers = self.handlers;
        crate::sync::run(self.co, &mut handlers)
    }

    /// Run the computation synchronously with shared state.
    pub fn run_sync_stateful<S>(self, state: &mut S) -> Result<R, Cancelled>
    where
        Effs: FoldWith<Handlers, S, CoControl<'a, Effs>>,
    {
        let mut handlers = self.handlers;
        crate::sync::run_stateful(self.co, state, &mut handlers)
    }

    /// Run the computation asynchronously.
    pub async fn run(self) -> Result<R, Cancelled>
    where
        Effs: AsyncFoldMut<Handlers, CoControl<'a, Effs>>,
    {
        let mut handlers = self.handlers;
        crate::run(self.co, &mut handlers).await
    }

    /// Run the computation asynchronously with shared state.
    pub async fn run_stateful<S>(self, state: &mut S) -> Result<R, Cancelled>
    where
        Effs: AsyncFoldWith<Handlers, S, CoControl<'a, Effs>>,
    {
        let mut handlers = self.handlers;
        crate::run_stateful(self.co, state, &mut handlers).await
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
