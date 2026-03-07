use corophage::prelude::*;

struct Log<'a>(pub &'a str);

impl<'a> Effect for Log<'a> {
    type Resume<'r> = ();
}

/// Test that effects can borrow non-'static data.
/// The key insight: `msg_ref` is a `&str` with a local lifetime,
/// captured by the closure and moved into the async block.
/// Previously this was impossible because `Co::new` required `'static`.
#[test]
fn sync_non_static_log() {
    type Effs<'a> = Effects![Log<'a>];

    let msg = String::from("hello from a local string");
    let msg_ref = msg.as_str();

    let co: Co<'_, Effs<'_>, ()> = Co::new(move |y| async move {
        y.yield_(Log(msg_ref)).await;
    });

    let mut logged: Vec<String> = vec![];
    let result = sync::run(
        co,
        &mut hlist![|Log(m)| {
            logged.push(m.to_string());
            CoControl::resume(())
        }],
    );

    assert_eq!(result, Ok(()));
    assert_eq!(logged, vec!["hello from a local string"]);
}

/// Test non-'static effects with run_stateful (stateful handler).
#[test]
fn sync_non_static_log_with_state() {
    type Effs<'a> = Effects![Log<'a>];

    let msg = String::from("stateful hello");
    let msg_ref = msg.as_str();

    let co: Co<'_, Effs<'_>, ()> = Co::new(move |y| async move {
        y.yield_(Log(msg_ref)).await;
        y.yield_(Log(msg_ref)).await;
    });

    let mut count: u32 = 0;
    let result = sync::run_stateful(
        co,
        &mut count,
        &mut hlist![|s: &mut u32, Log(_)| {
            *s += 1;
            CoControl::resume(())
        }],
    );

    assert_eq!(result, Ok(()));
    assert_eq!(count, 2);
}

/// Test non-'static effects with async handler.
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_non_static_log() {
    type Effs<'a> = Effects![Log<'a>];

    let msg = String::from("async hello");
    let msg_ref = msg.as_str();

    let co: Co<'_, Effs<'_>, ()> = Co::new(move |y| async move {
        y.yield_(Log(msg_ref)).await;
    });

    let mut logged: Vec<String> = vec![];
    let result = run(
        co,
        &mut hlist![async |Log(m)| {
            logged.push(m.to_string());
            CoControl::resume(())
        }],
    )
    .await;

    assert_eq!(result, Ok(()));
    assert_eq!(logged, vec!["async hello"]);
}
