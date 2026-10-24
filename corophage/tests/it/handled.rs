//! Invoking sub-programs with handlers attached: local handlers run inside
//! the invocation, and only the remaining effects reach the outer handlers.

use std::cell::Cell;
use std::future::{Future, poll_fn, ready};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use corophage::prelude::*;

struct Ask(&'static str);
impl Effect for Ask {
    type Resume<'r> = usize;
    fn shorten_resume<'a: 'b, 'b>(value: usize) -> usize {
        value
    }
}

struct Log(String);
impl Effect for Log {
    type Resume<'r> = ();
    fn shorten_resume<'a: 'b, 'b>(value: ()) {
        value
    }
}

struct Stop;
impl Effect for Stop {
    type Resume<'r> = Never;
    fn shorten_resume<'a: 'b, 'b>(value: Never) -> Never {
        value
    }
}

#[derive(Debug, PartialEq)]
struct NotFound(&'static str);

/// Asks for two values and logs their sum.
fn child<'a>() -> Program<'a, Effects![Ask, Log], impl Future<Output = usize> + 'a> {
    Program::new(|mut y: Yielder<'_, Effects![Ask, Log]>| async move {
        let sum = y.yield_(Ask("a")).await + y.yield_(Ask("b")).await;
        y.yield_(Log(format!("sum {sum}"))).await;
        sum
    })
}

fn ask(Ask(key): Ask) -> Control<usize> {
    Control::resume(key.len() * 10)
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

/// Poll like an executor that re-polls a task only after a wake-up.
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

#[test]
fn local_and_forwarded_effects() {
    let logs = Cell::new(Vec::new());
    let result = Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
        // Ask is handled locally; Log reaches the outer handler.
        let sum = y.invoke_handled(child().handle(ask)).await;
        y.yield_(Log("done".into())).await;
        sum
    })
    .handle(|Log(line)| {
        let mut seen = logs.take();
        seen.push(line);
        logs.set(seen);
        Control::resume(())
    })
    .run_sync();

    assert_eq!(result, Ok(20));
    assert_eq!(logs.take(), ["sum 20", "done"]);
}

#[test]
fn a_fully_handled_child_needs_no_outer_handler() {
    let result = Program::new(|mut y: Yielder<'_, Effects![]>| async move {
        y.invoke_handled(child().handle(ask).handle(|_: Log| Control::resume(())))
            .await
    })
    .run_sync();

    assert_eq!(result, Ok(20));
}

#[test]
fn handled_children_nest_and_forward_through_each_other() {
    let result = Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
        let middle = Program::new(|mut y: Yielder<'_, Effects![Ask, Log]>| async move {
            // Log passes through this invocation's forwarding to the root.
            let inner = y
                .invoke_handled(child().handle(|_: Ask| Control::resume(1)))
                .await;
            inner + y.yield_(Ask("abc")).await
        });
        y.invoke_handled(middle.handle(ask)).await
    })
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Ok(32));
}

#[test]
fn local_state_is_borrowed_for_the_invocation() {
    let result = Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
        let mut asked = Vec::new();
        let sum = y
            .invoke_handled(child().with_state(&mut asked).handle(
                |asked: &mut Vec<&'static str>, Ask(key)| {
                    asked.push(key);
                    Control::resume(asked.len())
                },
            ))
            .await;
        (sum, asked)
    })
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Ok((3, vec!["a", "b"])));
}

#[test]
fn async_local_handlers_under_an_async_root() {
    let result = block_on(
        Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
            y.invoke_handled(child().handle(|effect| async move {
                ready_on_second_poll(()).await;
                ask(effect)
            }))
            .await
        })
        .handle(|_: Log| Control::resume(()))
        .run(),
    );

    assert_eq!(result, Ok(20));
}

#[test]
#[should_panic(expected = "So do async handlers attached to an invoked sub-program")]
fn a_suspending_async_local_handler_under_a_sync_root_panics() {
    let _ = Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
        y.invoke_handled(child().handle(|effect| async move {
            ready_on_second_poll(()).await;
            ask(effect)
        }))
        .await
    })
    .handle(|_: Log| Control::resume(()))
    .run_sync();
}

#[test]
fn a_local_cancellation_cancels_the_whole_computation() {
    let reached = Cell::new(false);
    let reached = &reached;
    let result = Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
        y.invoke_handled(child().handle(|_: Ask| Control::cancel()))
            .await;
        reached.set(true);
    })
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Err(Cancelled));
    assert!(!reached.get());
}

fn cancelling_child<'a>() -> Program<'a, Effects![Log], impl Future<Output = usize> + 'a> {
    Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
        y.invoke_handled(child().handle(|_: Ask| Control::cancel()))
            .await
    })
}

fn through_unhandled<'a>() -> Program<'a, Effects![Log], impl Future<Output = usize> + 'a> {
    Program::new(
        |mut y: Yielder<'_, Effects![Log]>| async move { y.invoke(cancelling_child()).await },
    )
}

#[test]
fn a_local_cancellation_passes_through_unhandled_invocations() {
    let sync = through_unhandled()
        .handle(|_: Log| Control::resume(()))
        .run_sync();
    let asynk = block_on(
        through_unhandled()
            .handle(|_: Log| Control::resume(()))
            .run(),
    );

    assert_eq!(sync, Err(Cancelled));
    assert_eq!(asynk, Err(Cancelled));
}

#[test]
fn select_withdraws_a_local_cancellation() {
    let cancelled = Cell::new(0);
    let cancelled = &cancelled;
    let result = Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
        // The invocation cancels in the poll that the other branch wins, so
        // the parent continues without it.
        let invoked = tokio::select! {
            biased;
            sum = y.invoke_handled(child().handle(|_: Ask| {
                cancelled.set(cancelled.get() + 1);
                Control::cancel()
            })) => Some(sum),
            () = ready(()) => None,
        };
        y.yield_(Log("after".into())).await;
        invoked
    })
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Ok(None));
    assert_eq!(cancelled.get(), 1);
}

fn find<'a>(
    key: &'static str,
) -> Program<'a, Effects![Ask, Throw<NotFound>], impl Future<Output = usize> + 'a> {
    Program::new(
        move |mut y: Yielder<'_, Effects![Ask, Throw<NotFound>]>| async move {
            match y.yield_(Ask(key)).await {
                0 => y.throw(NotFound(key)).await,
                found => found,
            }
        },
    )
}

#[test]
fn a_body_catches_an_invoked_throw() {
    let result = Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move {
        let missing = y.invoke_handled(find("").catch()).await;
        let found = y.invoke_handled(find("ab").catch()).await;
        (missing, found)
    })
    .handle(ask)
    .run_sync();

    assert_eq!(result, Ok((Err(NotFound("")), Ok(20))));
}

#[test]
fn another_local_handler_that_cancels_still_cancels_a_caught_child() {
    let result = Program::new(|mut y: Yielder<'_, Effects![]>| async move {
        y.invoke_handled(find("ab").handle(|_: Ask| Control::cancel()).catch())
            .await
    })
    .run_sync();

    assert_eq!(result, Err(Cancelled));
}

#[test]
fn handled_invocations_keep_the_body_send() {
    fn assert_send<T: Send>(value: T) -> T {
        value
    }

    let future = assert_send(
        Program::new(|mut y: Yielder<'_, Effects![Log]>| async move {
            let sum = y
                .invoke_handled(child().handle(|effect| async move {
                    ready_on_second_poll(()).await;
                    ask(effect)
                }))
                .await;
            let caught = y.invoke_handled(find("").handle(ask).catch()).await;
            (sum, caught)
        })
        .handle(|_: Log| async { Control::resume(()) })
        .run(),
    );

    assert_eq!(block_on(future), Ok((20, Err(NotFound("")))));
}

#[test]
fn many_local_effects_under_a_sync_root_complete() {
    let result = Program::new(|mut y: Yielder<'_, Effects![]>| async move {
        let counter = Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move {
            let mut sum = 0;
            for _ in 0..1_000 {
                sum += y.yield_(Ask("x")).await;
            }
            sum
        });
        y.invoke_handled(counter.handle(ask)).await
    })
    .run_sync();

    assert_eq!(result, Ok(10_000));
}

#[test]
fn a_stopped_child_with_never_resumes_cancels() {
    let result = Program::new(|mut y: Yielder<'_, Effects![]>| async move {
        let stop = Program::new(|mut y: Yielder<'_, Effects![Stop]>| async move {
            match y.yield_(Stop).await {}
        });
        y.invoke_handled(stop.handle(|_: Stop| Control::cancel()))
            .await
    })
    .run_sync();

    assert_eq!(result, Err::<(), _>(Cancelled));
}
