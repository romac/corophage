//! The default execution proofs also run without the proc-macro feature.

use std::cell::Cell;
use std::future::{Future, ready};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use corophage::prelude::*;

struct Add(usize);
impl Effect for Add {
    type Resume<'r> = usize;
    fn shorten_resume<'a: 'b, 'b>(value: usize) -> usize {
        value
    }
}

struct Text<'a>(&'a str);
impl Effect for Text<'_> {
    type Resume<'r> = &'r str;
    fn shorten_resume<'a: 'b, 'b>(value: &'a str) -> &'b str {
        value
    }
}

fn assert_send(_: &impl Send) {}

fn ticks(n: usize) -> Program<'static, Effects![Add], impl Future<Output = usize>> {
    Program::new(move |mut y: Yielder<'_, Effects![Add]>| async move {
        let mut sum = 0;
        for _ in 0..n {
            sum += y.yield_(Add(1)).await;
        }
        sum
    })
}

#[test]
fn empty_and_inline_long_loop() {
    assert_eq!(
        Program::new(|_: Yielder<'_, Effects![]>| async { 7 }).run_sync(),
        Ok(7)
    );
    assert_eq!(
        ticks(1000).handle(|Add(n)| Control::resume(n)).run_sync(),
        Ok(1000)
    );
}

#[tokio::test]
async fn borrowed_gat_resumes_state_and_reordered_mixed_handlers() {
    async fn append<'a>(state: &mut Vec<&'a str>, Text(s): Text<'a>) -> Control<&'a str> {
        tokio::task::yield_now().await;
        state.push(s);
        Control::resume(s)
    }
    let text = String::from("borrowed");
    let text_ref = text.as_str();
    let mut state = Vec::new();
    let program = Program::new(
        move |mut y: Yielder<'_, Effects![Add, Text<'_>]>| async move {
            let n = y.yield_(Add(2)).await;
            let value = y.yield_(Text(text_ref)).await;
            (n, value)
        },
    );
    let future = program
        .with_state(&mut state)
        .handle(append)
        .handle(|Add(n)| Control::resume(n))
        .run();
    assert_send(&future);
    assert_eq!(future.await, Ok((2, "borrowed")));
    assert_eq!(state, ["borrowed"]);
}

#[tokio::test]
async fn send_without_sync_handler_and_state_borrow_across_await() {
    let captured = Cell::new(0);
    let handler = move |Add(n)| {
        captured.set(captured.get() + n);
        ready(Control::resume(captured.get()))
    };
    let future = ticks(3).handle(handler).run();
    assert_send(&future);
    assert_eq!(tokio::spawn(future).await.unwrap(), Ok(6));

    let captured = Cell::new(0);
    let mut state = 0;
    let future = ticks(3)
        .with_state(&mut state)
        .handle(move |state: &mut usize, Add(n)| {
            captured.set(captured.get() + n);
            *state += captured.get();
            ready(Control::resume(*state))
        })
        .run();
    assert_send(&future);
    assert_eq!(future.await, Ok(10));
    assert_eq!(state, 6);

    let mut state = 0;
    let future = ticks(3)
        .with_state(&mut state)
        .handle(async |s: &mut usize, Add(n)| {
            tokio::task::yield_now().await;
            *s += n;
            Control::resume(*s)
        })
        .run();
    assert_send(&future);
    assert_eq!(future.await, Ok(6));
    assert_eq!(state, 3);
}

#[tokio::test]
async fn local_body_state_handlers_and_results() {
    let body_capture = Rc::new(1);
    let body = Program::new(move |mut y: Yielder<'_, Effects![Add]>| async move {
        y.yield_(Add(*body_capture)).await;
        body_capture
    });
    let mut state = Rc::new(Cell::new(0));
    let result = body
        .with_state(&mut state)
        .handle(async |s: &mut Rc<Cell<usize>>, Add(n)| {
            tokio::task::yield_now().await;
            s.set(s.get() + n);
            Control::resume(s.get())
        })
        .run()
        .await;
    assert_eq!(result, Ok(Rc::new(1)));
    assert_eq!(state.get(), 1);

    let future = ticks(2)
        .handle(|Add(n)| async move {
            let local = Rc::new(n);
            tokio::task::yield_now().await;
            Control::resume(*local)
        })
        .run();
    assert_eq!(future.await, Ok(2));

    // A sendable body can use local state and handler captures independently.
    let mut local_state = Rc::new(Cell::new(0));
    let capture = local_state.clone();
    let result = ticks(2)
        .with_state(&mut local_state)
        .handle(move |state: &mut Rc<Cell<usize>>, Add(n)| {
            capture.set(capture.get() + n);
            Control::resume(state.get())
        })
        .run()
        .await;
    assert_eq!(result, Ok(3));
    assert_eq!(local_state.get(), 2);
}

#[tokio::test]
async fn inferred_stateful_execution_inside_spawn_and_explicit_erasure_boundary() {
    async fn add(state: &mut usize, Add(n): Add) -> Control<usize> {
        tokio::task::yield_now().await;
        *state += n;
        Control::resume(*state)
    }
    let (result, state) = tokio::spawn(async {
        let mut state = 0;
        let result = ticks(2).with_state(&mut state).handle(add).run().await;
        (result, state)
    })
    .await
    .unwrap();
    assert_eq!(result, Ok(3));
    assert_eq!(state, 2);

    async fn record(state: &mut usize, Text(text): Text<'static>) -> Control<&'static str> {
        tokio::task::yield_now().await;
        *state += text.len();
        Control::resume(text)
    }
    let program = Program::new(
        |mut y: Yielder<'_, Effects![Text<'static>, Add]>| async move {
            let count = y.yield_(Add(1)).await;
            let text = y.yield_(Text("recorded")).await;
            (count, text)
        },
    )
    // rustc can over-generalize effect lifetimes when deriving Send through an
    // enclosing async block. Keep an explicit sendable boundary there, including
    // for a borrowed GAT resume.
    .boxed_send()
    .handle(|Add(n)| Control::resume(n))
    .handle(record);
    let (result, state) = tokio::spawn(async move {
        let mut state = 0;
        let result = program.run_stateful(&mut state).await;
        (result, state)
    })
    .await
    .unwrap();
    assert_eq!(result, Ok((1, "recorded")));
    assert_eq!(state, 8);
}

#[tokio::test]
async fn low_level_send_wrappers_keep_borrowed_handler_lists() {
    fn co() -> corophage::CoSend<'static, Effects![Add], usize> {
        corophage::CoSend::new(|mut y| async move { y.yield_(Add(2)).await })
    }
    let mut handlers = hlist![async |Add(n)| {
        tokio::task::yield_now().await;
        Control::resume(n)
    }];
    let future = corophage::asynk::run_send(co(), &mut handlers);
    assert_send(&future);
    assert_eq!(future.await, Ok(2));

    async fn add(state: &mut usize, Add(n): Add) -> Control<usize> {
        tokio::task::yield_now().await;
        *state += n;
        Control::resume(*state)
    }
    let handlers = hlist![add];
    let mut state = 3;
    let future = corophage::asynk::run_stateful_send(co(), &mut state, &handlers);
    assert_send(&future);
    assert_eq!(future.await, Ok(5));
    assert_eq!(state, 5);
}

#[test]
fn explicit_erasure() {
    fn erased() -> Effectful<'static, Effects![Add], usize> {
        ticks(1).boxed()
    }
    fn erased_send() -> Effectful<'static, Effects![Add], usize, corophage::Sendable> {
        ticks(1).boxed_send()
    }
    assert_eq!(
        erased().handle(|Add(n)| Control::resume(n)).run_sync(),
        Ok(1)
    );
    assert_eq!(
        erased_send().handle(|Add(n)| Control::resume(n)).run_sync(),
        Ok(1)
    );
}

// Boxed local programs accept the same handlers as concrete programs.
#[tokio::test]
async fn erased_local_programs_accept_every_handler_kind() {
    fn erased(n: usize) -> Effectful<'static, Effects![Add], usize> {
        ticks(n).boxed()
    }

    assert_eq!(
        erased(2).handle(|Add(n)| Control::resume(n)).run().await,
        Ok(2)
    );
    assert_eq!(
        erased(2)
            .handle(async |Add(n)| Control::resume(n))
            .run()
            .await,
        Ok(2)
    );

    let mut seen = Vec::new();
    let result = erased(2)
        .handle(corophage::local(async |Add(n)| {
            seen.push(n);
            ready(()).await;
            Control::resume(n)
        }))
        .run()
        .await;
    assert_eq!(result, Ok(2));
    assert_eq!(seen, [1, 1]);

    let mut calls = 0;
    let result = erased(3)
        .handle(|calls: &mut usize, Add(n)| {
            *calls += 1;
            Control::resume(n)
        })
        .run_stateful(&mut calls)
        .await;
    assert_eq!(result, Ok(3));
    assert_eq!(calls, 3);

    let co =
        corophage::Co::new(
            |mut y: Yielder<'_, Effects![Add]>| async move { y.yield_(Add(4)).await },
        );
    assert_eq!(
        Program::from_co(co)
            .handle(|Add(n)| Control::resume(n))
            .run()
            .await,
        Ok(4)
    );
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

#[test]
fn cooperative_budget_and_exact_boundary_completion() {
    for count in [0, 1, 128, 129, 1000] {
        let handled = Cell::new(0);
        let future = ticks(count)
            .handle(|Add(n)| {
                handled.set(handled.get() + 1);
                Control::resume(n)
            })
            .run();
        let mut future = std::pin::pin!(future);
        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(wakes.clone());
        let mut cx = Context::from_waker(&waker);
        let mut polls = 0;
        loop {
            let before = handled.get();
            polls += 1;
            let result = future.as_mut().poll(&mut cx);
            assert!(handled.get() - before <= 128);
            match result {
                Poll::Ready(result) => {
                    assert_eq!(result, Ok(count));
                    assert_eq!(handled.get(), count);
                    assert_eq!(polls, count.saturating_sub(1) / 128 + 1);
                    break;
                }
                Poll::Pending => {
                    assert_eq!(wakes.0.load(Ordering::Relaxed), polls);
                    assert!(handled.get() > before);
                }
            }
        }
    }
}

struct DropCount(Arc<AtomicUsize>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn drop_before_execution_and_during_each_suspension_releases_state() {
    for suspension in [0, 1, 2, 3] {
        let dropped = Arc::new(AtomicUsize::new(0));
        let guard = DropCount(dropped.clone());
        let mut state = 0;
        let future = Program::new(move |mut y: Yielder<'_, Effects![Add]>| async move {
            let _guard = guard;
            if suspension == 1 {
                std::future::pending::<()>().await;
            }
            for _ in 0..1000 {
                y.yield_(Add(1)).await;
            }
        })
        .with_state(&mut state)
        .handle(async |state: &mut usize, Add(n)| {
            *state += n;
            if suspension == 2 {
                std::future::pending::<()>().await;
            }
            Control::resume(n)
        })
        .run();
        let mut future = Box::pin(future);
        if suspension != 0 {
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        drop(future);
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert_eq!(
            state,
            match suspension {
                0 | 1 => 0,
                2 => 1,
                _ => 128,
            }
        );
        state += 1;
        assert!(state > 0);
    }
}

#[test]
fn cancellation_and_handler_panic_drop_without_resuming() {
    for panics in [false, true] {
        let dropped = Arc::new(AtomicUsize::new(0));
        let guard = DropCount(dropped.clone());
        let resumed = Cell::new(false);
        let mut state = 0;
        let resumed_ref = &resumed;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
                let _guard = guard;
                y.yield_(Add(1)).await;
                resumed_ref.set(true);
            })
            .with_state(&mut state)
            .handle(|state: &mut usize, _: Add| {
                *state += 1;
                assert!(!panics, "handler panic");
                Control::cancel()
            })
            .run_sync()
        }));
        if panics {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), Err(Cancelled));
        }
        assert!(!resumed.get());
        assert_eq!(state, 1);
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn send_runner_can_migrate_during_body_handler_and_budget_suspension() {
    // A pinned allocation stays put while ownership crosses actual OS threads.
    for suspension in [1, 2, 3] {
        let future = Program::new(move |mut y: Yielder<'_, Effects![Add]>| async move {
            if suspension == 1 {
                tokio::task::yield_now().await;
            }
            for _ in 0..129 {
                y.yield_(Add(1)).await;
            }
            129
        })
        .handle(move |Add(n)| async move {
            if suspension == 2 {
                tokio::task::yield_now().await;
            }
            Control::resume(n)
        })
        .run();
        assert_send(&future);
        let mut future = Box::pin(future);
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    for _ in 0..260 {
                        if let Poll::Ready(value) = future
                            .as_mut()
                            .poll(&mut Context::from_waker(Waker::noop()))
                        {
                            assert_eq!(value, Ok(129));
                            return;
                        }
                    }
                    panic!("runner failed to complete after thread migration");
                })
                .join()
                .unwrap()
        });
    }
}

#[test]
#[should_panic(expected = "computation suspended on a non-effect future")]
fn sync_body_suspension_panics_without_spinning() {
    let _ = Program::new(|_: Yielder<'_, Effects![]>| std::future::pending::<()>()).run_sync();
}

// The async runner yields to the executor after its dispatch budget, which a
// synchronous outer runner cannot wait for. Nested `.run_sync()` never yields.
#[test]
#[should_panic(expected = "use `invoke` or `.run_sync()` for sub-programs")]
fn nested_async_runner_in_sync_body_panics_after_budget() {
    let _ = Program::new(|_: Yielder<'_, Effects![]>| async {
        ticks(129).handle(|Add(n)| Control::resume(n)).run().await
    })
    .run_sync();
}

#[test]
fn nested_sync_runner_in_sync_body_completes() {
    let result = Program::new(|_: Yielder<'_, Effects![]>| async {
        ticks(129).handle(|Add(n)| Control::resume(n)).run_sync()
    })
    .run_sync();
    assert_eq!(result, Ok(Ok(129)));
}

#[test]
fn configured_program_drop_releases_state_before_execution() {
    let mut state = 1;
    let configured = ticks(1).with_state(&mut state);
    drop(configured);
    state += 1;
    assert_eq!(state, 2);
}

#[tokio::test]
async fn local_lending_closure_reuses_mutable_captures_and_borrowed_effects() {
    let mut captured = Vec::new();
    let text = String::from("local");
    let text_ref = text.as_str();
    let program = Program::new(|mut y: Yielder<'_, Effects![Text<'_>]>| async move {
        let first = y.yield_(Text(text_ref)).await;
        let second = y.yield_(Text(text_ref)).await;
        (first, second)
    });
    let result = program
        .handle(corophage::local(async |Text(s): Text<'_>| {
            captured.push(s);
            tokio::task::yield_now().await;
            captured.push(s);
            Control::resume(s)
        }))
        .run()
        .await;
    assert_eq!(result, Ok(("local", "local")));
    assert_eq!(captured, ["local"; 4]);
}

#[tokio::test]
async fn local_lending_stateful_closure_keeps_independent_input_lifetimes() {
    let mut captures = Vec::new();
    let text = String::from("state");
    let text_ref = text.as_str();
    let mut state = vec![text_ref];
    let program = Program::new(|mut y: Yielder<'_, Effects![Text<'_>]>| async move {
        y.yield_(Text(text_ref)).await;
        y.yield_(Text(text_ref)).await
    });
    let result = program
        .with_state(&mut state)
        .handle(corophage::local_stateful(
            async |state: &mut Vec<&str>, Text(s): Text<'_>| {
                captures.push(state.len());
                tokio::task::yield_now().await;
                state.clear();
                captures.push(state.len());
                Control::resume(s)
            },
        ))
        .run()
        .await;
    assert_eq!(result, Ok("state"));
    assert!(state.is_empty());
    assert_eq!(captures, [1, 0, 0, 0]);
}

fn borrowed_child<'a>(
    state: &'a mut usize,
) -> Program<'a, Effects![Add], impl Future<Output = usize> + 'a> {
    Program::new(move |mut y: Yielder<'_, Effects![Add]>| async move {
        *state += y.yield_(Add(1)).await;
        *state
    })
}

#[tokio::test]
async fn sequential_nested_invocations_keep_short_borrows_and_send() {
    let mut state = 0;
    let state_ref = &mut state;
    let outer = Program::new(|mut y: Yielder<'_, Effects![Add]>| async move {
        let a = y.invoke(borrowed_child(state_ref)).await;
        *state_ref += 1;
        let b = y.invoke(borrowed_child(state_ref)).await;
        a + b
    });
    let future = outer.handle(|Add(n)| ready(Control::resume(n))).run();
    assert_send(&future);
    assert_eq!(future.await, Ok(4));
    assert_eq!(state, 3);
}

#[test]
fn generic_locality_erased_signatures_still_work() {
    fn wrap<'a, L: corophage::Locality>(
        co: corophage::coroutine::GenericCo<'a, Effects![Add], usize, L>,
    ) -> Effectful<'a, Effects![Add], usize, L> {
        Program::from_co(co)
    }
    assert_eq!(
        wrap(corophage::Co::new(
            |mut y| async move { y.yield_(Add(2)).await }
        ))
        .handle(|Add(n)| Control::resume(n))
        .run_sync(),
        Ok(2)
    );
}

#[cfg(feature = "macros")]
mod macros {
    use super::*;

    #[effectful('a, Add, send)]
    fn child<'a>(state: &'a mut usize) -> usize {
        *state += yield_!(Add(1));
        *state
    }

    #[effectful(Add, send)]
    fn parent(state: &mut usize) -> usize {
        let a = invoke!(child(state));
        *state += 1;
        a + invoke!(child(state))
    }

    #[tokio::test]
    async fn opaque_macro_signatures_and_nested_lifetimes() {
        let mut state = 0;
        let future = parent(&mut state).handle(|Add(n)| Control::resume(n)).run();
        assert_send(&future);
        assert_eq!(future.await, Ok(4));
        assert_eq!(state, 3);
    }

    #[effectful(Add, boxed, send)]
    fn recursive(depth: usize, drops: Arc<AtomicUsize>) -> usize {
        let _guard = DropCount(drops.clone());
        if depth == 0 {
            yield_!(Add(1))
        } else {
            1 + invoke!(recursive(depth - 1, drops))
        }
    }

    #[test]
    fn boxed_recursion_and_nested_cancellation_drop_every_body() {
        for cancel in [false, true] {
            let drops = Arc::new(AtomicUsize::new(0));
            let result = recursive(20, drops.clone())
                .handle(|Add(n)| {
                    if cancel {
                        Control::cancel()
                    } else {
                        Control::resume(n)
                    }
                })
                .run_sync();
            assert_eq!(result, if cancel { Err(Cancelled) } else { Ok(21) });
            assert_eq!(drops.load(Ordering::Relaxed), 21);
        }
    }

    #[effectful(Add)]
    fn nested_0() -> usize {
        yield_!(Add(1))
    }

    macro_rules! nested {
        ($name:ident, $child:ident) => {
            #[effectful(Add)]
            fn $name() -> usize {
                invoke!($child())
            }
        };
    }
    nested!(nested_1, nested_0);
    nested!(nested_2, nested_1);
    nested!(nested_3, nested_2);
    nested!(nested_4, nested_3);
    nested!(nested_5, nested_4);
    nested!(nested_6, nested_5);
    nested!(nested_7, nested_6);
    nested!(nested_8, nested_7);
    nested!(nested_9, nested_8);
    nested!(nested_10, nested_9);
    nested!(nested_11, nested_10);
    nested!(nested_12, nested_11);
    nested!(nested_13, nested_12);
    nested!(nested_14, nested_13);
    nested!(nested_15, nested_14);
    nested!(nested_16, nested_15);
    nested!(nested_17, nested_16);
    nested!(nested_18, nested_17);
    nested!(nested_19, nested_18);
    nested!(nested_20, nested_19);

    #[test]
    fn twenty_actual_nested_invocations_have_bounded_future_storage() {
        fn future_bytes<F: Future>(_: impl FnOnce() -> F) -> usize {
            std::mem::size_of::<F>()
        }
        // The first concrete implementation doubled the child storage at every
        // invoke and overflowed the stack at depth 20. This allows normal layout
        // variation while rejecting that exponential growth without constructing it.
        assert!(future_bytes(|| nested_20().handle(|Add(n)| Control::resume(n)).run()) < 64 * 1024);
        assert_eq!(
            nested_20().handle(|Add(n)| Control::resume(n)).run_sync(),
            Ok(1)
        );
    }
}

struct LocalResume;
impl Effect for LocalResume {
    type Resume<'r> = Rc<usize>;
    fn shorten_resume<'a: 'b, 'b>(value: Rc<usize>) -> Rc<usize> {
        value
    }
}

#[tokio::test]
async fn local_resume_storage_and_result_need_not_be_send() {
    let future = Program::new(|mut y: Yielder<'_, Effects![LocalResume]>| async move {
        let a = y.yield_(LocalResume).await;
        let b = y.yield_(LocalResume).await;
        Rc::new(*a + *b)
    })
    .handle(|_: LocalResume| Control::resume(Rc::new(3)))
    .run();
    assert_eq!(future.await, Ok(Rc::new(6)));
}

#[test]
fn dropping_pending_handler_destroys_its_future() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let future = ticks(1)
        .handle(|_: Add| {
            let guard = DropCount(dropped.clone());
            async move {
                let _guard = guard;
                std::future::pending::<()>().await;
                Control::resume(1)
            }
        })
        .run();
    let mut future = Box::pin(future);
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(dropped.load(Ordering::Relaxed), 0);
    drop(future);
    assert_eq!(dropped.load(Ordering::Relaxed), 1);
}
