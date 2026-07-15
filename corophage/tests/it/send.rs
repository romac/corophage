use corophage::coroutine::CoSend;
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

#[effectful(FileRead, send)]
fn sendable_effectful_program() -> String {
    yield_!(FileRead("macro".to_string()))
}

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
