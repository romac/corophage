//! Concrete, mutable dispatch for both inline and cooperative execution.

use std::future::{Future, Ready, ready};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use frunk_core::coproduct::{CNil, Coproduct};
use frunk_core::hlist::HCons;
use frunk_core::hlist::HNil;
use frunk_core::indices::{Here, There};

use crate::Control;
use crate::effect::{Effect, Effects, InjectResume, Resumes};

pub struct Sync;
pub struct Async;
pub struct Stateful<K>(PhantomData<K>);
pub struct Lending;

/// An explicitly boxed local lending handler. Each dispatch allocates one future.
#[allow(clippy::type_complexity)]
pub struct LocalHandler<'a, E: Effect, F> {
    handler: F,
    marker: PhantomData<fn() -> (&'a (), E, E::Resume<'a>)>,
}

/// A local lending handler that can also borrow the shared state across awaits.
#[allow(clippy::type_complexity)]
pub struct LocalStatefulHandler<'a, S, E: Effect, F> {
    handler: F,
    marker: PhantomData<fn() -> (&'a (), S, E, E::Resume<'a>)>,
}

/// Adapt a lending async closure with an explicit mutable state argument.
/// Each call boxes one non-`Send` future; ordinary handlers do not allocate.
pub fn local_stateful<'a, S, E: Effect, F>(handler: F) -> LocalStatefulHandler<'a, S, E, F>
where
    F: AsyncFnMut(&mut S, E) -> Control<E::Resume<'a>>,
{
    LocalStatefulHandler {
        handler,
        marker: PhantomData,
    }
}

impl<'a, 'h, S, E: Effect, F> Call<&'h mut S, E, E::Resume<'a>, Stateful<Lending>>
    for &'h mut LocalStatefulHandler<'a, S, E, F>
where
    F: AsyncFnMut(&mut S, E) -> Control<E::Resume<'a>>,
{
    type Future = Pin<Box<dyn Future<Output = Control<E::Resume<'a>>> + 'h>>;
    fn call(self, state: &'h mut S, effect: E) -> Self::Future {
        Box::pin((self.handler)(state, effect))
    }
}

impl<'a, Effs, S, E, F, Tail, Index, ITail>
    super::HandlersToEffects<'a, Effs, HCons<(E, Index, Stateful<Lending>), ITail>>
    for HCons<LocalStatefulHandler<'a, S, E, F>, Tail>
where
    Effs: Effects<'a> + InjectResume<'a, E, Index>,
    E: Effect,
    F: AsyncFnMut(&mut S, E) -> Control<E::Resume<'a>>,
    Tail: super::HandlersToEffects<'a, Effs, ITail>,
{
    type Effects = Coproduct<E, Tail::Effects>;
}

/// Adapt an async closure that borrows its captures across awaits.
pub fn local<'a, E: Effect, F>(handler: F) -> LocalHandler<'a, E, F>
where
    F: AsyncFnMut(E) -> Control<E::Resume<'a>>,
{
    LocalHandler {
        handler,
        marker: PhantomData,
    }
}

impl<'a, 'h, State, E: Effect, F> Call<State, E, E::Resume<'a>, Lending>
    for &'h mut LocalHandler<'a, E, F>
where
    F: AsyncFnMut(E) -> Control<E::Resume<'a>>,
{
    type Future = Pin<Box<dyn Future<Output = Control<E::Resume<'a>>> + 'h>>;
    fn call(self, _: State, effect: E) -> Self::Future {
        Box::pin((self.handler)(effect))
    }
}

impl<'a, Effs, E, F, Tail, Index, ITail>
    super::HandlersToEffects<'a, Effs, HCons<(E, Index, Lending), ITail>>
    for HCons<LocalHandler<'a, E, F>, Tail>
where
    Effs: Effects<'a> + InjectResume<'a, E, Index>,
    E: Effect,
    F: AsyncFnMut(E) -> Control<E::Resume<'a>>,
    Tail: super::HandlersToEffects<'a, Effs, ITail>,
{
    type Effects = Coproduct<E, Tail::Effects>;
}

/// Metadata only: synchronous execution uses the same dispatch futures.
pub trait Synchronous {}
impl Synchronous for Sync {}
impl Synchronous for Stateful<Sync> {}
impl Synchronous for HNil {}
impl<I, F, K: Synchronous, T: Synchronous> Synchronous for HCons<(I, F, K), T> {}

/// The reference types carry independent state and handler borrow lifetimes.
/// Avoid a GAT bound of the form `for<'s> Self::Future<'s>`: its required
/// `Self: 's` bound can impose an unintended `'static` requirement.
pub trait Call<State, E, R, Kind> {
    type Future: Future<Output = Control<R>>;
    fn call(self, state: State, effect: E) -> Self::Future;
}

impl<State, E, R, F> Call<State, E, R, Sync> for &mut F
where
    F: FnMut(E) -> Control<R>,
{
    type Future = Ready<Control<R>>;
    fn call(self, _: State, effect: E) -> Self::Future {
        ready(self(effect))
    }
}

impl<State, E, R, F, Fut> Call<State, E, R, Async> for &mut F
where
    F: FnMut(E) -> Fut,
    Fut: Future<Output = Control<R>>,
{
    type Future = Fut;
    fn call(self, _: State, effect: E) -> Self::Future {
        self(effect)
    }
}

impl<State, E, R, F> Call<State, E, R, Stateful<Sync>> for &mut F
where
    F: FnMut(State, E) -> Control<R>,
{
    type Future = Ready<Control<R>>;
    fn call(self, state: State, effect: E) -> Self::Future {
        ready(self(state, effect))
    }
}

impl<State, E, R, F, Fut> Call<State, E, R, Stateful<Async>> for &mut F
where
    F: FnMut(State, E) -> Fut,
    Fut: Future<Output = Control<R>>,
{
    type Future = Fut;
    fn call(self, state: State, effect: E) -> Self::Future {
        self(state, effect)
    }
}

pub trait Find<State, E, R, Index, Kind> {
    type Future: Future<Output = Control<R>>;
    fn call(self, state: State, effect: E) -> Self::Future;
}

// Borrowed low-level handler lists use the same lookup and branch futures.
impl<'h, H: ?Sized, State, E, R, I, K> Find<State, E, R, I, K> for &'h mut &mut H
where
    &'h mut H: Find<State, E, R, I, K>,
{
    type Future = <&'h mut H as Find<State, E, R, I, K>>::Future;
    fn call(self, state: State, effect: E) -> Self::Future {
        (&mut **self).call(state, effect)
    }
}

impl<'h, H: ?Sized, State, E, R, I, K> Find<State, E, R, I, K> for &mut &'h H
where
    &'h H: Find<State, E, R, I, K>,
{
    type Future = <&'h H as Find<State, E, R, I, K>>::Future;
    fn call(self, state: State, effect: E) -> Self::Future {
        (*self).call(state, effect)
    }
}

impl<'h, State, E, R, F, Tail, K> Find<State, E, R, Here, K> for &'h HCons<F, Tail>
where
    &'h F: Call<State, E, R, K>,
{
    type Future = <&'h F as Call<State, E, R, K>>::Future;
    fn call(self, state: State, effect: E) -> Self::Future {
        (&self.head).call(state, effect)
    }
}

impl<'h, State, E, R, F, Tail, I, K> Find<State, E, R, There<I>, K> for &'h HCons<F, Tail>
where
    &'h Tail: Find<State, E, R, I, K>,
{
    type Future = <&'h Tail as Find<State, E, R, I, K>>::Future;
    fn call(self, state: State, effect: E) -> Self::Future {
        (&self.tail).call(state, effect)
    }
}

impl<State, E, R, F> Call<State, E, R, Stateful<Sync>> for &F
where
    F: Fn(State, E) -> Control<R>,
{
    type Future = Ready<Control<R>>;
    fn call(self, state: State, effect: E) -> Self::Future {
        ready(self(state, effect))
    }
}

impl<State, E, R, F, Fut> Call<State, E, R, Stateful<Async>> for &F
where
    F: Fn(State, E) -> Fut,
    Fut: Future<Output = Control<R>>,
{
    type Future = Fut;
    fn call(self, state: State, effect: E) -> Self::Future {
        self(state, effect)
    }
}

impl<'h, State, E, R, F, Tail, K> Find<State, E, R, Here, K> for &'h mut HCons<F, Tail>
where
    &'h mut F: Call<State, E, R, K>,
{
    type Future = <&'h mut F as Call<State, E, R, K>>::Future;
    fn call(self, state: State, effect: E) -> Self::Future {
        (&mut self.head).call(state, effect)
    }
}

impl<'h, State, E, R, F, Tail, I, K> Find<State, E, R, There<I>, K> for &'h mut HCons<F, Tail>
where
    &'h mut Tail: Find<State, E, R, I, K>,
{
    type Future = <&'h mut Tail as Find<State, E, R, I, K>>::Future;
    fn call(self, state: State, effect: E) -> Self::Future {
        (&mut self.tail).call(state, effect)
    }
}

pin_project_lite::pin_project! {
    #[project = BranchProj]
    pub enum Branch<L, R> {
        Left { #[pin] future: L },
        Right { #[pin] future: R },
    }
}

impl<L: Future, R: Future<Output = L::Output>> Future for Branch<L, R> {
    type Output = L::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            BranchProj::Left { future } => future.poll(cx),
            BranchProj::Right { future } => future.poll(cx),
        }
    }
}

pin_project_lite::pin_project! {
    pub struct Inject<'a, Effs, E, I, F> {
        #[pin]
        future: F,
        // These types describe a conversion, not stored values.
        marker: PhantomData<fn() -> (&'a (), Effs, E, I)>,
    }
}

impl<'a, Effs, E, I, F> Future for Inject<'a, Effs, E, I, F>
where
    Effs: Effects<'a> + InjectResume<'a, E, I>,
    E: Effect,
    F: Future<Output = Control<E::Resume<'a>>>,
{
    type Output = Control<Resumes<'a, Effs>>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().future.poll(cx).map(|control| match control {
            Control::Resume(value) => Control::Resume(Effs::inject_resume(value)),
            Control::Cancel => Control::Cancel,
        })
    }
}

pub trait Dispatch<'a, Effs: Effects<'a>, State, Handlers, Indices> {
    type Future: Future<Output = Control<Resumes<'a, Effs>>>;
    fn dispatch(self, state: State, handlers: Handlers) -> Self::Future;
}

impl<'a, Effs: Effects<'a>, State, Handlers> Dispatch<'a, Effs, State, Handlers, HNil> for CNil {
    type Future = Ready<Control<Resumes<'a, Effs>>>;
    fn dispatch(self, _: State, _: Handlers) -> Self::Future {
        match self {}
    }
}

impl<'a, Effs, E, Tail, State, Handlers, I, F, K, ITail>
    Dispatch<'a, Effs, State, Handlers, HCons<(I, F, K), ITail>> for Coproduct<E, Tail>
where
    Effs: Effects<'a> + InjectResume<'a, E, I>,
    E: Effect,
    Handlers: Find<State, E, E::Resume<'a>, F, K>,
    Tail: Dispatch<'a, Effs, State, Handlers, ITail>,
{
    type Future = Branch<Inject<'a, Effs, E, I, Handlers::Future>, Tail::Future>;
    fn dispatch(self, state: State, handlers: Handlers) -> Self::Future {
        match self {
            Coproduct::Inl(effect) => Branch::Left {
                future: Inject {
                    future: handlers.call(state, effect),
                    marker: PhantomData,
                },
            },
            Coproduct::Inr(tail) => Branch::Right {
                future: tail.dispatch(state, handlers),
            },
        }
    }
}
