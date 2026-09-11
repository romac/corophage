//! Mapping changes only the terminal value, at any point in handler setup.

use std::cell::Cell;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use corophage::prelude::*;

struct Ask;
impl Effect for Ask {
    type Resume<'r> = usize;
    fn shorten_resume<'a: 'b, 'b>(value: usize) -> usize {
        value
    }
}

struct Log(usize);
impl Effect for Log {
    type Resume<'r> = ();
    fn shorten_resume<'a: 'b, 'b>(value: ()) {
        value
    }
}

#[test]
fn map_is_lazy_and_accepts_a_consumed_callback() {
    let body_calls = Cell::new(0);
    let map_calls = Cell::new(0);
    let program = Program::new(|_: Yielder<'_, Effects![]>| async {
        body_calls.set(body_calls.get() + 1);
        7
    });
    let label = String::from("result");
    let mapped = program.map(|value| {
        map_calls.set(map_calls.get() + 1);
        (label, value)
    });
    assert_eq!(body_calls.get(), 0);
    assert_eq!(map_calls.get(), 0);
    assert_eq!(mapped.run_sync(), Ok((String::from("result"), 7)));
    assert_eq!(body_calls.get(), 1);
    assert_eq!(map_calls.get(), 1);
}

#[test]
fn map_preserves_partial_handlers_and_bound_state() {
    let mut state = 3;
    let mut logs = Vec::new();
    let result = Program::new(|mut y: Yielder<'_, Effects![Ask, Log]>| async move {
        let value = y.yield_(Ask).await;
        y.yield_(Log(value)).await;
        value
    })
    .map(|value| value * 2)
    .with_state(&mut state)
    .handle(|s: &mut usize, _: Ask| {
        *s += 1;
        Control::resume(*s)
    })
    .map(|value| value.to_string())
    .handle(|Log(value)| {
        logs.push(value);
        Control::resume(())
    })
    .map(|value| (value, true))
    .run_sync();
    assert_eq!(result, Ok((String::from("8"), true)));
    assert_eq!(state, 4);
    assert_eq!(logs, [4]);
}

struct Text<'a>(&'a str);
impl Effect for Text<'_> {
    type Resume<'r> = &'r str;
    fn shorten_resume<'a: 'b, 'b>(value: &'a str) -> &'b str {
        value
    }
}

#[test]
fn mapped_child_remains_invocable_with_borrowed_resumes() {
    let text = String::from("borrowed");
    let text_ref = text.as_str();
    let result = Program::new(|mut y: Yielder<'_, Effects![Ask, Text<'_>]>| async move {
        let length = y.yield_(Ask).await;
        let child = Program::new(move |mut y: Yielder<'_, Effects![Text<'_>]>| async move {
            y.yield_(Text(text_ref)).await
        })
        .map(move |value| &value[..length]);
        y.invoke(child).await
    })
    .handle(|Text(value)| Control::resume(value))
    .handle(|_: Ask| Control::resume(3))
    .run_sync();
    assert_eq!(result, Ok("bor"));
}

fn ask() -> Program<'static, Effects![Ask], impl Future<Output = usize>> {
    Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask).await })
}

#[tokio::test]
async fn map_preserves_send_and_async_suspensions() {
    let label = String::from("answer");
    let future = ask()
        .handle(async |_: Ask| {
            tokio::task::yield_now().await;
            Control::resume(21)
        })
        .map(move |value| (label, value * 2))
        .run();
    assert_eq!(
        tokio::spawn(future).await.unwrap(),
        Ok((String::from("answer"), 42))
    );

    let mut state = 0;
    let future = ask()
        .with_state(&mut state)
        .handle(async |state: &mut usize, _: Ask| {
            tokio::task::yield_now().await;
            *state += 1;
            Control::resume(*state)
        })
        .map(|value| value * 2)
        .run();
    fn assert_send(_: &impl Send) {}
    assert_send(&future);
    assert_eq!(future.await, Ok(2));
    assert_eq!(state, 1);
}

#[tokio::test]
async fn map_accepts_boxed_bodies_and_local_captures() {
    let mut calls = Vec::new();
    let local = Rc::new(String::from("local"));
    let result = ask()
        .boxed()
        .handle(corophage::local(async |_: Ask| {
            calls.push(1);
            tokio::task::yield_now().await;
            Control::resume(calls.len())
        }))
        .map(move |value| (local, value))
        .run()
        .await;
    assert_eq!(result, Ok((Rc::new(String::from("local")), 1)));
    assert_eq!(calls, [1]);

    let future = ask()
        .boxed_send()
        .map(|value| value + 1)
        .handle(|_: Ask| Control::resume(1))
        .run();
    assert_eq!(tokio::spawn(future).await.unwrap(), Ok(2));

    // Mapping also remains compatible with an erased return signature.
    let erased: Effectful<'static, Effects![Ask], usize> = ask().map(|n| n + 1).boxed();
    assert_eq!(erased.handle(|_: Ask| Control::resume(2)).run_sync(), Ok(3));
}

struct DropCount(Rc<Cell<usize>>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn cancellation_skips_mapping_and_drops_captures() {
    let drops = Rc::new(Cell::new(0));
    let body_guard = DropCount(drops.clone());
    let map_guard = DropCount(drops.clone());
    let mut state = 0;
    let result = Program::new(move |mut y: Yielder<'_, Effects![Ask]>| async move {
        let _guard = body_guard;
        y.yield_(Ask).await
    })
    .with_state(&mut state)
    .map(move |_| {
        drop(map_guard);
        panic!("cancellation must not run the mapper");
    })
    .handle(|state: &mut usize, _: Ask| {
        *state += 1;
        Control::cancel()
    })
    .run_sync();
    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, 1);
    assert_eq!(drops.get(), 2);
}

#[test]
fn dropping_mapped_execution_releases_captures_and_state() {
    // Drop before polling, during an ordinary await, and during a handler await.
    for suspension in 0..3 {
        let drops = Rc::new(Cell::new(0));
        let body_guard = DropCount(drops.clone());
        let map_guard = DropCount(drops.clone());
        let mut state = 0;
        let future = Program::new(move |mut y: Yielder<'_, Effects![Ask]>| async move {
            let _guard = body_guard;
            if suspension == 1 {
                std::future::pending::<()>().await;
            }
            y.yield_(Ask).await
        })
        .with_state(&mut state)
        .handle(async |state: &mut usize, _: Ask| {
            *state += 1;
            std::future::pending::<()>().await;
            Control::resume(1)
        })
        .map(move |_| {
            drop(map_guard);
            panic!("dropping an incomplete program must not run the mapper");
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
        assert_eq!(drops.get(), 0);
        drop(future);
        assert_eq!(drops.get(), 2);
        assert_eq!(state, usize::from(suspension == 2));
    }
}

#[derive(Default)]
struct PinnedBody {
    address: Cell<usize>,
    _pin: PhantomPinned,
}

impl Future for PinnedBody {
    type Output = usize;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<usize> {
        let this = self.as_ref().get_ref();
        let address = std::ptr::from_ref(this) as usize;
        if this.address.get() == 0 {
            this.address.set(address);
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            assert_eq!(this.address.get(), address);
            Poll::Ready(7)
        }
    }
}

impl Drop for PinnedBody {
    fn drop(&mut self) {
        if self.address.get() != 0 {
            assert_eq!(self.address.get(), std::ptr::from_ref(self) as usize);
        }
    }
}

#[test]
fn map_keeps_a_suspended_body_pinned_through_completion_and_drop() {
    let future = Program::new(|_: Yielder<'_, Effects![]>| PinnedBody::default())
        .map(|value| value + 1)
        .run();
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(future.as_mut().poll(&mut cx), Poll::Pending);
    // Ownership of the pinned outer future can move while the child stays put.
    let mut moved = (future,);
    assert_eq!(moved.0.as_mut().poll(&mut cx), Poll::Ready(Ok(8)));
}

#[test]
fn mapping_keeps_the_ready_effect_budget() {
    let handled = Cell::new(0);
    let mapped = Cell::new(false);
    let future = Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move {
        for _ in 0..1000 {
            y.yield_(Ask).await;
        }
        1000
    })
    .map(|value| {
        mapped.set(true);
        value + 1
    })
    .handle(|_: Ask| {
        handled.set(handled.get() + 1);
        Control::resume(1)
    })
    .run();
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(future.as_mut().poll(&mut cx), Poll::Pending);
    assert_eq!(handled.get(), 128);
    assert!(!mapped.get());
    for _ in 0..8 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
            assert_eq!(result, Ok(1001));
            assert_eq!(handled.get(), 1000);
            assert!(mapped.get());
            return;
        }
    }
    panic!("mapped program did not finish within its dispatch budget");
}
