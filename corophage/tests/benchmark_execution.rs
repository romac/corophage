#![cfg(feature = "macros")]
#![recursion_limit = "256"]

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Wake;
use std::task::Waker;

use corophage::prelude::*;

#[path = "../benches/support/execution.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn catch_fixtures_stop_at_the_labelled_effect() {
    for throw_at in [0, 999, 1000] {
        let mut calls = 0;
        let result = fallible(1000, throw_at)
            .handle(|effect| {
                calls += 1;
                tick(effect)
            })
            .catch()
            .run_sync();
        assert_eq!(calls, throw_at);
        assert_eq!(
            result,
            Ok(if throw_at == 1000 {
                Ok(1000)
            } else {
                Err(Inner)
            })
        );
        #[cfg(not(comparison_v070))]
        {
            calls = 0;
            let result = catch_layer(1000, throw_at)
                .catch()
                .handle(|effect| {
                    calls += 1;
                    tick(effect)
                })
                .run_sync();
            assert_eq!(calls, throw_at);
            assert_eq!(
                result,
                Ok(if throw_at == 1000 {
                    Ok(1000)
                } else {
                    Err(Outer)
                })
            );
        }
    }
}

#[test]
fn invocation_fixtures_do_equal_work_with_different_handler_placement() {
    macro_rules! check {
        ($fixture:ident, $depth:expr, $root_local:expr) => {{
            let mut forwarded = 0;
            let mut local = 0;
            let result = $fixture(100, false)
                .handle(|effect| {
                    forwarded += 1;
                    tick(effect)
                })
                .handle(|effect| {
                    local += 1;
                    local_tick(effect, false)
                })
                .run_sync();
            assert_eq!(result, Ok(100 + $depth + 1));
            assert_eq!(forwarded, 100);
            assert_eq!(local, $root_local);
        }};
    }
    check!(plain_1, 1, 2);
    check!(plain_4, 4, 5);
    check!(plain_8, 8, 9);
    #[cfg(not(comparison_v070))]
    check!(handled_1, 1, 1);
    #[cfg(not(comparison_v070))]
    check!(handled_4, 4, 1);
    #[cfg(not(comparison_v070))]
    check!(handled_8, 8, 1);
}

#[tokio::test]
async fn cancellation_fixtures_never_reach_the_leaf_ticks() {
    macro_rules! check {
        ($fixture:ident) => {{
            assert_eq!(
                $fixture(100, true)
                    .handle(|_: Tick| -> Control<usize> { panic!("tick after cancellation") })
                    .handle(|effect| local_tick(effect, true))
                    .run_sync(),
                Err(Cancelled)
            );
            assert_eq!(
                $fixture(100, true)
                    .handle(|_: Tick| -> Control<usize> { panic!("tick after cancellation") })
                    .handle(|effect| local_tick(effect, true))
                    .run()
                    .await,
                Err(Cancelled)
            );
        }};
    }
    check!(plain_1);
    check!(plain_4);
    check!(plain_8);
    #[cfg(not(comparison_v070))]
    check!(handled_1);
    #[cfg(not(comparison_v070))]
    check!(handled_4);
    #[cfg(not(comparison_v070))]
    check!(handled_8);
}

#[test]
fn inline_payloads_survive_each_transport_size() {
    fn check<const N: usize>() {
        let mut calls = 0;
        assert_eq!(
            payload_0::<N>(100).handle(payload::<N>).run_sync(),
            Ok([7; N])
        );
        assert_eq!(
            payload_4::<N>(100)
                .handle(|value| {
                    calls += 1;
                    payload::<N>(value)
                })
                .run_sync(),
            Ok([7; N])
        );
        assert_eq!(calls, 100);
    }
    check::<8>();
    check::<64>();
    check::<1024>();
    check::<4096>();
}

#[derive(Default)]
struct Wakes(AtomicUsize);
impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn suspension_fixture_returns_pending_and_wakes_exactly_once() {
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(wakes.clone());
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(suspend_once());
    assert_eq!(future.as_mut().poll(&mut cx), Poll::Pending);
    assert_eq!(wakes.0.load(Ordering::Relaxed), 1);
    assert_eq!(future.as_mut().poll(&mut cx), Poll::Ready(()));
    assert_eq!(wakes.0.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn async_workloads_preserve_results_and_handler_loans() {
    for n in [1, 1000] {
        assert_eq!(suspending_body(n).handle(tick).run().await, Ok(n));
        assert_eq!(
            ticks(n)
                .handle(|effect| async move {
                    suspend_once().await;
                    tick(effect)
                })
                .run()
                .await,
            Ok(n)
        );
        #[cfg(not(comparison_v070))]
        assert_eq!(short_children(n).run().await, Ok(n));

        let bytes = vec![7; 64];
        let mut captured = 0;
        let mut state = 0;
        let result = borrowed(&bytes, n)
            .with_state(&mut state)
            .handle(corophage::local_stateful(
                async |state: &mut usize, Borrow(value)| {
                    let loan = &mut captured;
                    *loan += 1;
                    *state += 1;
                    suspend_once().await;
                    *loan += 1;
                    *state += 1;
                    Control::resume(value)
                },
            ))
            .run()
            .await;
        assert_eq!(result, Ok(7 * n));
        assert_eq!((captured, state), (2 * n, 2 * n));
    }
}

#[tokio::test]
async fn latency_probe_observes_an_active_competing_task() {
    #[cfg(comparison_v070)]
    let loads = [0, 1];
    #[cfg(not(comparison_v070))]
    let loads = [0, 1, 2];
    for load in loads {
        let input = PeerLatency::new(load);
        let progress = input.local.run_until(input.peer).await.unwrap();
        if load == 0 {
            assert_eq!(progress, 0);
        } else {
            assert!(progress > 0, "the load must run before the probe");
            assert!(
                progress < PeerLatency::WORK,
                "the probe must run before completion"
            );
        }
    }
}

struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn suspended_drop_fixtures_keep_both_payloads_alive_until_drop() {
    macro_rules! check {
        ($fixture:ident, $handler_pending:expr) => {{
            let dropped = Arc::new(AtomicUsize::new(0));
            let owned = || Owned {
                bytes: vec![7; 4096],
                guard: Dropped(dropped.clone()),
            };
            let mut future = Box::pin(
                $fixture(owned(), owned())
                    .handle(|effect| async move {
                        if $handler_pending {
                            std::future::pending::<()>().await;
                        }
                        Control::resume(effect)
                    })
                    .run(),
            );
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(dropped.load(Ordering::Relaxed), 0);
            drop(future);
            assert_eq!(dropped.load(Ordering::Relaxed), 2);
        }};
    }
    check!(held_0, true);
    check!(held_0, false);
    check!(held_4, true);
    check!(held_4, false);
}
