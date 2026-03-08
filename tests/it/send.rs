use corophage::coroutine::CoSend;
use corophage::prelude::*;
use corophage::sync;

use crate::common::*;

fn assert_send<T: Send>(_: &T) {}

#[test]
fn co_send_is_send() {
    fn co() -> CoSend<'static, Effects![FileRead], String> {
        CoSend::new(|y| async move { y.yield_(FileRead("test".to_string())).await })
    }

    let co = co();
    assert_send(&co);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn co_send_can_be_spawned() {
    fn co() -> CoSend<'static, Effects![FileRead], String> {
        CoSend::new(|y| async move { y.yield_(FileRead("test".to_string())).await })
    }

    let handle = tokio::spawn(async move {
        sync::run(
            co(),
            &mut hlist![|FileRead(file)| {
                println!("Reading file: {file}");
                CoControl::<'static, Effects![FileRead]>::resume("file content".to_string())
            }],
        )
    });

    let result = handle.await.unwrap();
    assert_eq!(result, Ok("file content".to_string()));
}
