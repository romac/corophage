use corophage::CoSend;
use corophage::prelude::*;
use corophage::sync;

use crate::common::*;

fn assert_send<T: Send>(_: &T) {}

#[test]
fn co_send_is_send() {
    fn co() -> CoSend<'static, Effects![FileRead], String> {
        CoSend::new(|mut y| async move { y.yield_(FileRead("test".to_string())).await })
    }

    let co = co();
    assert_send(&co);
}

#[test]
fn sendable_program_run_is_send_and_completes() {
    use std::future::{Future, poll_fn};
    use std::task::{Context, Poll, Waker};

    let future = Program::new_send::<Effects![FileRead], _>(|mut y| async move {
        let mut yielded = false;
        poll_fn(move |cx| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
        y.yield_(FileRead("test".to_string())).await
    })
    .handle(async |FileRead(file)| {
        let mut yielded = false;
        poll_fn(move |cx| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
        Control::resume(format!("file content for {file}"))
    })
    .run();
    assert_send(&future);

    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert!(future.as_mut().poll(&mut context).is_pending());
    let Poll::Ready(result) = future.as_mut().poll(&mut context) else {
        panic!("sendable program unexpectedly returned pending");
    };
    assert_eq!(result, Ok("file content for test".to_string()));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn co_send_can_be_spawned() {
    fn co() -> CoSend<'static, Effects![FileRead], String> {
        CoSend::new(|mut y| async move { y.yield_(FileRead("test".to_string())).await })
    }

    let handle = tokio::spawn(async move {
        sync::run(
            co(),
            &mut hlist![|FileRead(file)| {
                println!("Reading file: {file}");
                Control::resume("file content".to_string())
            }],
        )
    });

    let result = handle.await.unwrap();
    assert_eq!(result, Ok("file content".to_string()));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn sendable_program_run_can_be_spawned() {
    let program = Program::new_send::<Effects![FileRead], _>(|mut y| async move {
        y.yield_(FileRead("test".to_string())).await
    })
    .handle(async |FileRead(file)| {
        tokio::task::yield_now().await;
        Control::resume(format!("file content for {file}"))
    });

    let future = program.run();
    assert_send(&future);

    let result = tokio::spawn(future).await.unwrap();
    assert_eq!(result, Ok("file content for test".to_string()));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn sendable_computation_body_can_await() {
    let future = Program::new_send::<Effects![FileRead], _>(|mut y| async move {
        tokio::task::yield_now().await;
        let contents = y.yield_(FileRead("test".to_string())).await;
        tokio::task::yield_now().await;
        contents
    })
    .handle(async |FileRead(file)| Control::resume(format!("file content for {file}")))
    .run();
    assert_send(&future);

    let result = tokio::spawn(future).await.unwrap();
    assert_eq!(result, Ok("file content for test".to_string()));
}

#[cfg(feature = "macros")]
#[effectful(FileRead, send)]
fn sendable_effectful_program() -> String {
    tokio::task::yield_now().await;
    yield_!(FileRead("macro".to_string()))
}

#[cfg(feature = "macros")]
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn sendable_effectful_run_can_be_spawned() {
    let future = sendable_effectful_program()
        .handle(async |FileRead(file)| Control::resume(format!("handled {file}")))
        .run();
    assert_send(&future);

    let result = tokio::spawn(future).await.unwrap();
    assert_eq!(result, Ok("handled macro".to_string()));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn borrowed_sendable_program_run_is_send() {
    let file = String::from("borrowed");
    let file = file.as_str();
    let program = Program::new_send::<Effects![FileRead], _>(move |mut y| async move {
        y.yield_(FileRead(file.to_string())).await
    })
    .handle(async |FileRead(file)| Control::resume(format!("handled {file}")));

    let future = program.run();
    assert_send(&future);
    assert_eq!(future.await, Ok("handled borrowed".to_string()));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn sendable_program_run_stateful_can_be_spawned() {
    async fn read(state: &mut usize, FileRead(file): FileRead) -> Control<String> {
        *state += 1;
        tokio::task::yield_now().await;
        Control::resume(format!("read {file} #{}", *state))
    }

    let program = Program::new_send::<Effects![Log<'static>, FileRead], _>(|mut y| async move {
        tokio::task::yield_now().await;
        let first = y.yield_(FileRead("first".to_string())).await;
        y.yield_(Log("between reads")).await;
        let second = y.yield_(FileRead("second".to_string())).await;
        (first, second)
    })
    // Attach in the opposite order to exercise both handler lookup branches.
    .handle(read)
    .handle(async |state: &mut usize, _: Log<'static>| {
        tokio::task::yield_now().await;
        *state += 10;
        Control::resume(())
    });

    let (result, state) = tokio::spawn(async move {
        let mut state = 0;
        let future = program.run_stateful(&mut state);
        assert_send(&future);
        let result = future.await;
        (result, state)
    })
    .await
    .unwrap();

    assert_eq!(
        result,
        Ok(("read first #1".to_string(), "read second #12".to_string()))
    );
    assert_eq!(state, 12);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn sendable_stateful_program_can_borrow_data() {
    async fn record<'a>(state: &mut Vec<&'a str>, Log(message): Log<'a>) -> Control<()> {
        tokio::task::yield_now().await;
        state.push(message);
        Control::resume(())
    }

    let message = String::from("borrowed");
    let message = message.as_str();
    let mut state = Vec::new();
    let future = Program::new_send::<Effects![Log<'_>], _>(move |mut y| async move {
        y.yield_(Log(message)).await;
        message
    })
    .handle(record)
    .run_stateful(&mut state);

    assert_send(&future);
    assert_eq!(future.await, Ok("borrowed"));
    assert_eq!(state, vec!["borrowed"]);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn sendable_stateful_program_can_cancel() {
    let program = Program::new_send::<Effects![Cancel, FileRead], _>(|mut y| async move {
        y.yield_(Cancel).await;
        panic!("cancelled computation must not resume");
    })
    .handle(async |state: &mut usize, _: Cancel| {
        *state += 1;
        tokio::task::yield_now().await;
        Control::cancel()
    })
    .handle(async |_: &mut usize, _: FileRead| -> Control<String> {
        panic!("handler must not run after cancellation");
    });

    let (result, state) = tokio::spawn(async move {
        let mut state = 0;
        let result: Result<(), _> = program.run_stateful(&mut state).await;
        (result, state)
    })
    .await
    .unwrap();

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, 1);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn sendable_stateful_program_without_effects_can_be_spawned() {
    let result = tokio::spawn(async move {
        let mut state = 7;
        let result = Program::new_send::<Effects![], _>(|_| async {
            tokio::task::yield_now().await;
            42
        })
        .run_stateful(&mut state)
        .await;
        (result, state)
    })
    .await
    .unwrap();

    assert_eq!(result, (Ok(42), 7));
}
