//! Workloads shared by execution benchmarks and their semantic checks.
//! Chain depths count invocation edges; no chain boxes its child bodies.

use std::future::Future;
use std::future::poll_fn;
use std::task::Poll;

use corophage::prelude::*;
use divan::black_box;

#[effect(usize)]
pub struct Tick(pub usize);

pub fn tick(Tick(n): Tick) -> Control<usize> {
    Control::resume(black_box(n))
}

/// A real suspension with one wake, independent of Tokio's scheduling policy.
pub async fn suspend_once() {
    let mut pending = true;
    poll_fn(|cx| {
        if std::mem::take(&mut pending) {
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}

#[effectful(Tick)]
pub fn ticks(n: usize) -> usize {
    let mut sum = 0;
    for _ in 0..n {
        sum += yield_!(Tick(black_box(1)));
    }
    sum
}

#[effectful(Tick)]
pub fn suspending_body(n: usize) -> usize {
    let mut sum = 0;
    for _ in 0..n {
        suspend_once().await;
        sum += yield_!(Tick(black_box(1)));
    }
    sum
}

#[effect(usize)]
pub struct LocalTick(pub usize);

pub fn local_tick(LocalTick(level): LocalTick, cancel: bool) -> Control<usize> {
    if cancel && level == 0 {
        Control::cancel()
    } else {
        Control::resume(black_box(1))
    }
}

#[effectful(Tick, LocalTick)]
pub fn chain_0(n: usize, _cancel: bool) -> usize {
    let mut sum = yield_!(LocalTick(0));
    for _ in 0..n {
        sum += yield_!(Tick(black_box(1)));
    }
    sum
}

// Both paths perform n Tick effects and depth + 1 LocalTick effects. Only the
// placement of LocalTick handlers differs; Tick always reaches the root.
macro_rules! chain {
    ($plain:ident, $handled:ident, $plain_child:ident, $handled_child:ident, $depth:expr) => {
        #[effectful(Tick, LocalTick)]
        pub fn $plain(n: usize, cancel: bool) -> usize {
            let local = yield_!(LocalTick($depth));
            local + invoke!($plain_child(n, cancel))
        }

        #[effectful(Tick, LocalTick)]
        pub fn $handled(n: usize, cancel: bool) -> usize {
            let local = yield_!(LocalTick($depth));
            local
                + invoke!(
                    $handled_child(n, cancel)
                        .handle(move |effect: LocalTick| { local_tick(effect, cancel) })
                )
        }
    };
}

chain!(plain_1, handled_1, chain_0, chain_0, 1);
chain!(plain_2, handled_2, plain_1, handled_1, 2);
chain!(plain_3, handled_3, plain_2, handled_2, 3);
chain!(plain_4, handled_4, plain_3, handled_3, 4);
chain!(plain_5, handled_5, plain_4, handled_4, 5);
chain!(plain_6, handled_6, plain_5, handled_5, 6);
chain!(plain_7, handled_7, plain_6, handled_6, 7);
chain!(plain_8, handled_8, plain_7, handled_7, 8);

#[derive(Debug, PartialEq, Eq)]
pub struct Inner;
#[derive(Debug, PartialEq, Eq)]
pub struct Outer;

#[effectful(Tick, Throw<Inner>)]
pub fn fallible(n: usize, throw_at: usize) -> usize {
    let mut sum = 0;
    for i in 0..n {
        if i == throw_at {
            throw!(Inner);
        }
        sum += yield_!(Tick(black_box(1)));
    }
    sum
}

/// Ordinary Tick effects pass through an invoked catch before the outer catch.
#[effectful(Tick, Throw<Outer>)]
pub fn catch_layer(n: usize, throw_at: usize) -> usize {
    match invoke!(fallible(n, throw_at).catch()) {
        Ok(sum) => sum,
        Err(Inner) => throw!(Outer),
    }
}

pub struct Payload<const N: usize>(pub [u8; N]);

impl<const N: usize> Effect for Payload<N> {
    type Resume<'r> = [u8; N];
    fn shorten_resume<'a: 'b, 'b>(value: [u8; N]) -> [u8; N] {
        value
    }
}

pub fn payload<const N: usize>(Payload(bytes): Payload<N>) -> Control<[u8; N]> {
    Control::resume(black_box(bytes))
}

#[effectful(Payload<N>)]
pub fn payload_0<const N: usize>(n: usize) -> [u8; N] {
    let mut bytes = black_box([7; N]);
    for _ in 0..n {
        bytes = yield_!(Payload(black_box(bytes)));
    }
    black_box(bytes)
}

macro_rules! payload_chain {
    ($name:ident, $child:ident) => {
        #[effectful(Payload<N>)]
        pub fn $name<const N: usize>(n: usize) -> [u8; N] {
            invoke!($child::<N>(n))
        }
    };
}
payload_chain!(payload_1, payload_0);
payload_chain!(payload_2, payload_1);
payload_chain!(payload_3, payload_2);
payload_chain!(payload_4, payload_3);

pub struct Borrow<'a>(pub &'a [u8]);
impl Effect for Borrow<'_> {
    type Resume<'r> = &'r [u8];
    fn shorten_resume<'a: 'b, 'b>(value: &'a [u8]) -> &'b [u8] {
        value
    }
}

pub fn borrowed<'a>(
    bytes: &'a [u8],
    n: usize,
) -> Program<'a, Effects![Borrow<'a>], impl Future<Output = usize> + 'a> {
    Program::new(move |mut y: Yielder<'_, Effects![Borrow<'a>]>| async move {
        let mut sum = 0;
        for _ in 0..n {
            let value = y.yield_(Borrow(black_box(bytes))).await;
            sum += black_box(value)[0] as usize;
        }
        sum
    })
}

#[effectful()]
pub fn short_children(n: usize) -> usize {
    let mut sum = 0;
    for _ in 0..n {
        sum += invoke!(ticks(1).handle(tick));
    }
    sum
}

/// Tasks are queued in load-then-peer order. The peer records how much load
/// completed before it ran; tests check that it ran while the load was active.
pub struct PeerLatency {
    pub local: tokio::task::LocalSet,
    pub peer: tokio::task::JoinHandle<usize>,
}

impl PeerLatency {
    pub const WORK: usize = 1_000_000;

    pub fn new(load: usize) -> Self {
        use std::cell::Cell;
        use std::rc::Rc;

        let local = tokio::task::LocalSet::new();
        let progress = Rc::new(Cell::new(0));
        let observed = progress.clone();
        match load {
            0 => {}
            1 => {
                local.spawn_local(async move {
                    ticks(Self::WORK)
                        .handle(|effect| {
                            progress.set(progress.get() + 1);
                            tick(effect)
                        })
                        .run()
                        .await
                        .unwrap();
                });
            }
            2 => {
                local.spawn_local(async move {
                    Program::new(|mut y: Yielder<'_, Effects![]>| async move {
                        for _ in 0..Self::WORK {
                            y.invoke_handled(ticks(1).handle(|effect| {
                                progress.set(progress.get() + 1);
                                tick(effect)
                            }))
                            .await;
                        }
                    })
                    .run()
                    .await
                    .unwrap();
                });
            }
            _ => unreachable!(),
        }
        let peer = local.spawn_local(async move { observed.get() });
        Self { local, peer }
    }
}

/// A payload with an optional test guard. Benchmarks use (), so no reference
/// counting or destructor instrumentation enters their timed cleanup path.
pub struct Owned<G> {
    pub bytes: Vec<u8>,
    pub guard: G,
}
impl<G: Send + Sync> Effect for Owned<G> {
    type Resume<'r> = Self;
    fn shorten_resume<'a: 'b, 'b>(value: Self) -> Self {
        value
    }
}

pub fn held_0<G: Send + Sync + 'static>(
    body: Owned<G>,
    effect: Owned<G>,
) -> Program<'static, Effects![Owned<G>], impl Future<Output = ()>> {
    Program::new(move |mut y: Yielder<'_, Effects![Owned<G>]>| async move {
        let resumed = y.yield_(effect).await;
        std::future::pending::<()>().await;
        // Both allocations must stay live across the suspension.
        black_box((body.bytes, body.guard, resumed.bytes, resumed.guard));
    })
}

macro_rules! held_chain {
    ($name:ident, $child:ident) => {
        pub fn $name<G: Send + Sync + 'static>(
            body: Owned<G>,
            effect: Owned<G>,
        ) -> Program<'static, Effects![Owned<G>], impl Future<Output = ()>> {
            Program::new(move |mut y: Yielder<'_, Effects![Owned<G>]>| async move {
                y.invoke($child(body, effect)).await
            })
        }
    };
}
held_chain!(held_1, held_0);
held_chain!(held_2, held_1);
held_chain!(held_3, held_2);
held_chain!(held_4, held_3);
