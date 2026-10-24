//! Effect operations find their computation's frame while its runner polls
//! the body, independently of the polling context.

use std::cell::Cell;
use std::future::{Future, poll_fn, ready};
use std::pin::{Pin, pin};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use corophage::prelude::*;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

struct Add(usize);
impl Effect for Add {
    type Resume<'r> = usize;
    fn shorten_resume<'a: 'b, 'b>(value: usize) -> usize {
        value
    }
}

// Counts drops of the effect payload that the slot holds.
struct Tracked(Arc<AtomicUsize>);
impl Effect for Tracked {
    type Resume<'r> = ();
    fn shorten_resume<'a: 'b, 'b>(value: ()) {
        value
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn add(Add(n): Add) -> Control<usize> {
    Control::resume(n * 10)
}

/// Pending once, waking itself, then ready.
fn ready_on_second_poll<T>(value: T) -> impl Future<Output = T> {
    let mut value = Some(value);
    let mut polled = false;
    poll_fn(move |cx| {
        if std::mem::replace(&mut polled, true) {
            Poll::Ready(value.take().unwrap())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
}

/// Poll like an executor that re-polls a task only after a wake-up, and fail
/// instead of hanging when the future is pending without one.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Woken(AtomicBool);
    impl Wake for Woken {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let woken = Arc::new(Woken(AtomicBool::new(false)));
    let waker = Waker::from(woken.clone());
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut Context::from_waker(&waker)) {
            return output;
        }
        assert!(
            woken.0.swap(false, Ordering::Relaxed),
            "pending without a wake-up"
        );
    }
}

/// A child performing two effects next to ready children. `FuturesUnordered`
/// polls each child with its own waker and re-polls only woken children.
fn unordered() -> Program<'static, Effects![Add], impl Future<Output = Vec<usize>>> {
    Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        let mut children: FuturesUnordered<Pin<Box<dyn Future<Output = usize> + '_>>> =
            FuturesUnordered::new();
        children.push(Box::pin(async {
            y.yield_(Add(1)).await + y.yield_(Add(2)).await
        }));
        children.push(Box::pin(ready(5)));
        children.push(Box::pin(ready(7)));
        let mut results = Vec::new();
        while let Some(result) = children.next().await {
            results.push(result);
        }
        results.sort();
        results
    })
}

#[test]
fn futures_unordered_with_one_child() {
    let result = Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        let mut children = FuturesUnordered::new();
        children.push(y.yield_(Add(2)));
        children.next().await
    })
    .handle(add)
    .run_sync();
    assert_eq!(result, Ok(Some(20)));
}

#[test]
fn futures_unordered_with_several_children_sync() {
    assert_eq!(unordered().handle(add).run_sync(), Ok(vec![5, 7, 30]));
}

#[test]
fn futures_unordered_with_several_children_async() {
    let result = block_on(
        unordered()
            .handle(|effect| async move {
                ready_on_second_poll(()).await;
                add(effect)
            })
            .run(),
    );
    assert_eq!(result, Ok(vec![5, 7, 30]));
}

#[test]
fn join_and_select_poll_operations_in_the_body_context() {
    let result = Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        let (joined, other) = tokio::join!(y.yield_(Add(1)), ready(2));
        let selected = tokio::select! {
            biased;
            value = y.yield_(Add(3)) => value,
            () = std::future::pending() => unreachable!(),
        };
        (joined, other, selected)
    })
    .handle(add)
    .run_sync();
    assert_eq!(result, Ok((10, 2, 30)));
}

#[test]
fn operation_polled_again_before_its_effect_is_taken() {
    let result = Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        let mut operation = pin!(y.yield_(Add(4)));
        poll_fn(|cx| {
            assert!(operation.as_mut().poll(cx).is_pending());
            assert!(operation.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        operation.await
    })
    .handle(add)
    .run_sync();
    assert_eq!(result, Ok(40));
}

#[test]
fn select_withdraws_an_effect_that_was_not_handled() {
    let handled = Cell::new(Vec::new());
    let result = Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        // The effect branch yields first, then the ready branch wins in the
        // same poll. Dropping the operation withdraws its effect.
        let first = tokio::select! {
            biased;
            value = y.yield_(Add(1)) => value,
            value = ready(100) => value,
        };
        (first, y.yield_(Add(2)).await)
    })
    .handle(|Add(n)| {
        let mut seen = handled.take();
        seen.push(n);
        handled.set(seen);
        add(Add(n))
    })
    .run_sync();
    assert_eq!(result, Ok((100, 20)));
    assert_eq!(handled.take(), vec![2]);
}

#[test]
fn select_discards_a_resume_after_its_handler_ran() {
    let handled = Cell::new(Vec::new());
    let result = Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        // The other branch is polled first and wins on the poll after the
        // handler resumed the operation, so that resume is discarded.
        let first = tokio::select! {
            biased;
            value = ready_on_second_poll(100) => value,
            value = y.yield_(Add(1)) => value,
        };
        (first, y.yield_(Add(2)).await)
    })
    .handle(|Add(n)| {
        let mut seen = handled.take();
        seen.push(n);
        handled.set(seen);
        add(Add(n))
    })
    .run_sync();
    assert_eq!(result, Ok((100, 20)));
    assert_eq!(handled.take(), vec![1, 2]);
}

#[test]
fn withdrawn_effects_are_dropped_once() {
    let drops = Arc::new(AtomicUsize::new(0));
    let effect = Tracked(drops.clone());
    let result = Program::new(|mut y: Yielder<'_, Effects![Tracked]>| async move {
        tokio::select! {
            biased;
            () = y.yield_(effect) => false,
            () = ready(()) => true,
        }
    })
    .handle(|_: Tracked| -> Control<()> { unreachable!("the effect was withdrawn") })
    .run_sync();
    assert_eq!(result, Ok(true));
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn a_forgotten_effect_is_dropped_with_the_frame() {
    let drops = Arc::new(AtomicUsize::new(0));
    let effect = Tracked(drops.clone());
    let result = Program::new(|mut y: Yielder<'_, Effects![Tracked]>| async move {
        let mut operation = y.yield_(effect);
        let poll = poll_fn(|cx| Poll::Ready(Pin::new(&mut operation).poll(cx))).await;
        std::mem::forget(operation);
        poll.is_pending()
    })
    .handle(|_: Tracked| -> Control<()> { unreachable!("the body completed first") })
    .run_sync();
    assert_eq!(result, Ok(true));
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
#[should_panic(expected = "another effect operation of this computation is in flight")]
fn an_operation_after_a_forgotten_one_panics() {
    let _ = Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        let mut operation = y.yield_(Add(1));
        let _ = poll_fn(|cx| Poll::Ready(Pin::new(&mut operation).poll(cx))).await;
        std::mem::forget(operation);
        y.yield_(Add(2)).await
    })
    .handle(add)
    .run_sync();
}

fn escaped_yielder() -> Yielder<'static, Effects![Add]> {
    Program::new(|y: Yielder<'static, Effects![Add]>| async move { y })
        .handle(add)
        .run_sync()
        .unwrap()
}

#[test]
#[should_panic(expected = "effect operation polled outside its computation")]
fn an_escaped_yielder_panics() {
    let mut y = escaped_yielder();
    let _ = pin!(y.yield_(Add(1))).poll(&mut Context::from_waker(Waker::noop()));
}

#[test]
fn an_operation_on_another_thread_panics() {
    let result = Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        let mut operation = y.yield_(Add(1));
        std::thread::scope(|scope| {
            scope
                .spawn(|| Pin::new(&mut operation).poll(&mut Context::from_waker(Waker::noop())))
                .join()
                .is_err()
        })
    })
    .handle(add)
    .run_sync();
    assert_eq!(result, Ok(true));
}

#[test]
fn a_yielder_inside_a_nested_async_runner_reaches_its_own_frame() {
    let result = Program::new(|mut outer: Yielder<'_, Effects![Add]>| async move {
        Program::new(|_: Yielder<'_, Effects![]>| async move { outer.yield_(Add(1)).await + 1 })
            .run()
            .await
    })
    .handle(add)
    .run_sync();
    assert_eq!(result, Ok(Ok(11)));
}

// The nested runner sees its own frame empty while the outer effect is
// pending, as when its body awaits an ordinary future.
#[test]
#[should_panic(expected = "computation suspended on a non-effect future")]
fn a_yielder_inside_a_nested_sync_runner_panics() {
    let _ = Program::new(|mut outer: Yielder<'_, Effects![Add]>| async move {
        Program::new(|_: Yielder<'_, Effects![]>| async move { outer.yield_(Add(1)).await })
            .run_sync()
    })
    .handle(add)
    .run_sync();
}

#[test]
fn a_panic_during_a_poll_unregisters_the_frame() {
    let panicked = std::panic::catch_unwind(|| {
        Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
            y.yield_(Add(1)).await;
            panic!("body panic");
        })
        .handle(add)
        .run_sync()
    });
    assert!(panicked.is_err());

    // A lookup that matches no frame walks the whole list, so it would reach
    // a frame left registered by the panic.
    let result = Program::new(|_: Yielder<'_, Effects![]>| async move {
        let mut foreign = escaped_yielder();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = pin!(foreign.yield_(Add(1))).poll(&mut Context::from_waker(Waker::noop()));
        }))
        .is_err()
    })
    .run_sync();
    assert_eq!(result, Ok(true));
}
