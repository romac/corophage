use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use corophage::prelude::*;

const EFFECT_COUNT: usize = 10_000;

struct Tick(usize);

impl Effect for Tick {
    type Resume<'r> = usize;

    fn shorten_resume<'a: 'b, 'b>(resume: usize) -> usize {
        resume
    }
}

#[derive(Default)]
struct WakeCount(AtomicUsize);

impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

async fn ready_effects(mut y: Yielder<'_, Effects![Tick]>) -> usize {
    let mut sum = 0;
    for i in 0..EFFECT_COUNT {
        sum += y.yield_(Tick(i)).await;
    }
    sum
}

fn handle_tick(handled: &AtomicUsize, Tick(i): Tick) -> Control<usize> {
    // A scheduling boundary must neither duplicate nor skip an effect.
    assert_eq!(handled.fetch_add(1, Ordering::Relaxed), i);
    Control::resume(i + 1)
}

fn drive_in_batches(future: impl Future<Output = Result<usize, Cancelled>>, handled: &AtomicUsize) {
    let wakes = Arc::new(WakeCount::default());
    let waker = Waker::from(wakes.clone());
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);

    // Manual polling checks the runner itself, independent of executor scheduling.
    for poll in 0..=EFFECT_COUNT {
        let before = handled.load(Ordering::Relaxed);
        let wakes_before = wakes.0.load(Ordering::Relaxed);
        let result = future.as_mut().poll(&mut cx);
        let after = handled.load(Ordering::Relaxed);
        assert!(
            after - before <= 128,
            "one poll dispatched too many effects"
        );

        match result {
            Poll::Pending => {
                assert!(after > before, "the runner made no progress");
                assert!(wakes.0.load(Ordering::Relaxed) > wakes_before);
            }
            Poll::Ready(result) => {
                assert!(poll > 0, "all effects completed in one poll");
                assert_eq!(after, EFFECT_COUNT);
                assert_eq!(result, Ok(EFFECT_COUNT * (EFFECT_COUNT + 1) / 2));
                return;
            }
        }
    }
    panic!("the runner never completed");
}

#[test]
fn ready_local_handlers_yield() {
    let handled = AtomicUsize::new(0);
    let future = Program::new(ready_effects)
        .handle(async |tick: Tick| handle_tick(&handled, tick))
        .run();

    drive_in_batches(future, &handled);
}

#[test]
fn ready_stateful_handlers_yield() {
    let handled = AtomicUsize::new(0);
    let mut state = 0;
    let program = Program::new(ready_effects).handle(async |state: &mut usize, tick: Tick| {
        *state += 1;
        handle_tick(&handled, tick)
    });

    drive_in_batches(program.run_stateful(&mut state), &handled);
    assert_eq!(state, EFFECT_COUNT);
}

#[test]
fn ready_sendable_handlers_yield() {
    let handled = AtomicUsize::new(0);
    let future = Program::new_send(ready_effects)
        .handle(async |tick: Tick| handle_tick(&handled, tick))
        .run();

    fn assert_send(_: &impl Send) {}
    assert_send(&future);

    // Move the future to another thread while the budget yield is suspended.
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    std::thread::scope(|scope| {
        scope
            .spawn(|| drive_in_batches(future, &handled))
            .join()
            .unwrap();
    });
}

#[test]
fn ready_sendable_stateful_handlers_yield() {
    let handled = AtomicUsize::new(0);
    let mut state = 0;
    let program = Program::new_send(ready_effects).handle(async |state: &mut usize, tick: Tick| {
        *state += 1;
        handle_tick(&handled, tick)
    });

    // Move the runner, including its state borrow, after a scheduling yield.
    let mut future = Box::pin(program.run_stateful(&mut state));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    std::thread::scope(|scope| {
        scope
            .spawn(|| drive_in_batches(future, &handled))
            .join()
            .unwrap();
    });
    assert_eq!(state, EFFECT_COUNT);
}

#[test]
fn invoked_effects_share_the_outer_runner_budget() {
    let handled = AtomicUsize::new(0);
    let future = Program::new::<Effects![Tick], _>(|mut y| async move {
        y.invoke(Program::new(ready_effects)).await
    })
    .handle(async |tick: Tick| handle_tick(&handled, tick))
    .run();

    drive_in_batches(future, &handled);
}

#[test]
fn locally_handled_effects_share_the_outer_runner_budget() {
    let handled = AtomicUsize::new(0);
    let handled = &handled;
    let future = Program::new::<Effects![], _>(|mut y| async move {
        y.invoke_handled(
            Program::new(ready_effects).handle(|tick: Tick| handle_tick(handled, tick)),
        )
        .await
    })
    .run();

    drive_in_batches(future, handled);
}

// Each child completes after one dispatch. Its unused budget must not be
// replaced by a fresh budget when the parent invokes the next child.
fn short_children<'a>(
    handled: &'a AtomicUsize,
) -> Program<'a, Effects![], impl Future<Output = usize> + 'a> {
    Program::new(move |mut y: Yielder<'_, Effects![]>| async move {
        let mut sum = 0;
        for i in 0..EFFECT_COUNT {
            let child = Program::new(|mut y: Yielder<'_, Effects![Tick]>| async move {
                y.yield_(Tick(i)).await
            });
            sum += y
                .invoke_handled(child.handle(|tick: Tick| handle_tick(handled, tick)))
                .await;
        }
        sum
    })
}

#[test]
fn short_handled_children_share_the_outer_runner_budget() {
    let handled = AtomicUsize::new(0);
    drive_in_batches(short_children(&handled).run(), &handled);
}

#[test]
fn root_and_short_handled_children_share_one_budget() {
    let handled = AtomicUsize::new(0);
    let handled = &handled;
    let future = Program::new(|mut y: Yielder<'_, Effects![Tick]>| async move {
        let mut sum = 0;
        for i in (0..EFFECT_COUNT).step_by(2) {
            sum += y.yield_(Tick(i)).await;
            let child = Program::new(|mut y: Yielder<'_, Effects![Tick]>| async move {
                y.yield_(Tick(i + 1)).await
            });
            sum += y
                .invoke_handled(child.handle(|tick: Tick| handle_tick(handled, tick)))
                .await;
        }
        sum
    })
    .handle(|tick: Tick| handle_tick(handled, tick))
    .run();

    drive_in_batches(future, handled);
}

#[test]
fn short_handled_children_share_the_budget_through_unhandled_invocations() {
    let handled = AtomicUsize::new(0);
    let handled = &handled;
    let future = Program::new(|mut y: Yielder<'_, Effects![]>| async move {
        y.invoke(short_children(handled)).await
    })
    .run();

    drive_in_batches(future, handled);
}

#[test]
fn short_handled_children_complete_at_the_budget_boundary_without_an_extra_poll() {
    for effects in [0, 1, 128, 129] {
        let handled = AtomicUsize::new(0);
        let handled = &handled;
        let future = Program::new(|mut y: Yielder<'_, Effects![]>| async move {
            for i in 0..effects {
                let child = Program::new(|mut y: Yielder<'_, Effects![Tick]>| async move {
                    y.yield_(Tick(i)).await
                });
                assert_eq!(
                    y.invoke_handled(child.handle(|tick: Tick| handle_tick(handled, tick)))
                        .await,
                    i + 1
                );
            }
            effects
        })
        .run();
        let mut future = std::pin::pin!(future);
        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(wakes.clone());
        let mut cx = Context::from_waker(&waker);
        let first = future.as_mut().poll(&mut cx);

        if effects <= 128 {
            assert_eq!(first, Poll::Ready(Ok(effects)));
            assert_eq!(wakes.0.load(Ordering::Relaxed), 0);
        } else {
            assert!(first.is_pending());
            assert_eq!(handled.load(Ordering::Relaxed), 128);
            assert!(wakes.0.load(Ordering::Relaxed) > 0);
            assert_eq!(future.as_mut().poll(&mut cx), Poll::Ready(Ok(effects)));
        }
        assert_eq!(handled.load(Ordering::Relaxed), effects);
    }
}

#[test]
fn independent_nested_runners_restore_the_outer_budget_on_completion_and_panic() {
    for synchronous in [false, true] {
        let handled = AtomicUsize::new(0);
        let future = Program::new(ready_effects)
            .handle(|tick: Tick| {
                let panic = tick.0 == 64;
                let nested = std::panic::catch_unwind(|| {
                    let program = Program::new(|mut y: Yielder<'_, Effects![Tick]>| async move {
                        y.yield_(Tick(0)).await;
                        assert!(!panic, "nested runner panic");
                    })
                    .handle(|_: Tick| Control::resume(0));
                    if synchronous {
                        assert_eq!(program.run_sync(), Ok(()));
                    } else {
                        assert_eq!(
                            std::pin::pin!(program.run())
                                .poll(&mut Context::from_waker(Waker::noop())),
                            Poll::Ready(Ok(()))
                        );
                    }
                });
                assert_eq!(nested.is_err(), panic);
                handle_tick(&handled, tick)
            })
            .run();
        let mut future = std::pin::pin!(future);

        // Fresh budgets in nested roots must not refill the enclosing root's.
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert_eq!(handled.load(Ordering::Relaxed), 128);
    }
}

#[test]
fn a_sync_root_continues_after_withdrawing_a_local_reschedule() {
    let handled = AtomicUsize::new(0);
    let handled = &handled;
    let result = Program::new(|mut y: Yielder<'_, Effects![]>| async move {
        tokio::select! {
            biased;
            _ = y.invoke_handled(
                Program::new(ready_effects).handle(|tick: Tick| handle_tick(handled, tick))
            ) => panic!("the invocation must yield before completing"),
            () = std::future::ready(()) => (),
        }
        assert_eq!(handled.load(Ordering::Relaxed), 128);
        // Withdrawal clears the signal, but does not refund completed work.
        // This child must request a new reschedule before its handler runs.
        let child =
            Program::new(
                |mut y: Yielder<'_, Effects![Tick]>| async move { y.yield_(Tick(128)).await },
            );
        y.invoke_handled(child.handle(|tick: Tick| handle_tick(handled, tick)))
            .await
    })
    .run_sync();

    assert_eq!(result, Ok(129));
    assert_eq!(handled.load(Ordering::Relaxed), 129);
}

#[test]
fn locally_handled_effects_complete_under_a_sync_root() {
    let handled = AtomicUsize::new(0);
    let handled = &handled;
    let result = Program::new::<Effects![], _>(|mut y| async move {
        y.invoke_handled(
            Program::new(ready_effects).handle(|tick: Tick| handle_tick(handled, tick)),
        )
        .await
    })
    .run_sync();

    assert_eq!(result, Ok(EFFECT_COUNT * (EFFECT_COUNT + 1) / 2));
}

#[test]
fn small_async_programs_complete_without_a_budget_yield() {
    // Include completion exactly at the budget boundary.
    for effects in [0, 1, 128] {
        let future = Program::new::<Effects![Tick], _>(|mut y| async move {
            for i in 0..effects {
                assert_eq!(y.yield_(Tick(i)).await, i);
            }
            effects
        })
        .handle(async |Tick(i)| Control::resume(i))
        .run();
        let mut future = std::pin::pin!(future);
        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(wakes.clone());
        let mut cx = Context::from_waker(&waker);

        assert_eq!(future.as_mut().poll(&mut cx), Poll::Ready(Ok(effects)));
        assert_eq!(wakes.0.load(Ordering::Relaxed), 0);
    }
}

struct DropCount(Arc<AtomicUsize>);

impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn cancellation_after_a_budget_yield_drops_the_computation() {
    let handled = AtomicUsize::new(0);
    let cancel = AtomicBool::new(false);
    let dropped = Arc::new(AtomicUsize::new(0));
    let guard = DropCount(dropped.clone());
    let future = Program::new(|y| async move {
        let _guard = guard;
        ready_effects(y).await
    })
    .handle(async |tick: Tick| {
        if cancel.load(Ordering::Relaxed) {
            Control::cancel()
        } else {
            handle_tick(&handled, tick)
        }
    })
    .run();
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());

    assert!(future.as_mut().poll(&mut cx).is_pending());
    let before_cancel = handled.load(Ordering::Relaxed);
    assert_eq!(dropped.load(Ordering::Relaxed), 0);
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(future.as_mut().poll(&mut cx), Poll::Ready(Err(Cancelled)));
    assert_eq!(handled.load(Ordering::Relaxed), before_cancel);
    assert_eq!(dropped.load(Ordering::Relaxed), 1);
}

#[test]
fn dropping_a_budget_suspended_runner_drops_the_computation() {
    let handled = AtomicUsize::new(0);
    let dropped = Arc::new(AtomicUsize::new(0));
    let guard = DropCount(dropped.clone());
    let future = Program::new(|y| async move {
        let _guard = guard;
        ready_effects(y).await
    })
    .handle(async |tick: Tick| handle_tick(&handled, tick))
    .run();
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());

    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(dropped.load(Ordering::Relaxed), 0);
    drop(future);
    assert_eq!(dropped.load(Ordering::Relaxed), 1);
}

#[test]
fn synchronous_runners_complete_long_effect_loops() {
    let handled = AtomicUsize::new(0);
    let result = Program::new(ready_effects)
        .handle(|tick: Tick| handle_tick(&handled, tick))
        .run_sync();
    assert_eq!(result, Ok(EFFECT_COUNT * (EFFECT_COUNT + 1) / 2));
    assert_eq!(handled.load(Ordering::Relaxed), EFFECT_COUNT);

    let mut state = 0;
    let result = Program::new(ready_effects)
        .handle(|state: &mut usize, Tick(i): Tick| {
            assert_eq!(*state, i);
            *state += 1;
            Control::resume(i + 1)
        })
        .run_sync_stateful(&mut state);
    assert_eq!(result, Ok(EFFECT_COUNT * (EFFECT_COUNT + 1) / 2));
    assert_eq!(state, EFFECT_COUNT);
}
