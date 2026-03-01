use corophage::prelude::*;

use crate::common::*;

fn assert_send<T: Send>(_: &T) {}

#[test]
fn co_is_send() {
    fn co() -> Co<'static, Effects![FileRead], String> {
        Co::new(|y| async move { y.yield_(FileRead("test".to_string())).await })
    }

    let co = co();
    assert_send(&co);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn co_can_be_spawned() {
    fn co() -> Co<'static, Effects![FileRead], String> {
        Co::new(|y| async move { y.yield_(FileRead("test".to_string())).await })
    }

    let handle = tokio::spawn(async move {
        let co = co();
        sync::run(
            co,
            &mut hlist![|FileRead(file)| {
                println!("Reading file: {file}");
                CoControl::resume("file content".to_string())
            }],
        )
    });

    let result = handle.await.unwrap();
    assert_eq!(result, Ok("file content".to_string()));
}
