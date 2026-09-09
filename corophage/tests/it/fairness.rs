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
