use corophage::prelude::*;

struct Trigger;

impl Effect for Trigger {
    type Resume = Never;
}

struct Log(pub &'static str);

impl Effect for Log {
    type Resume = ();
}

struct Fetch(pub &'static str);

impl Effect for Fetch {
    type Resume = String;
}

#[test]
fn sync_early_cancel_no_side_effects() {
    type Effs = Effects![Trigger, Log];

    let co: Co<'_, Effs, ()> = Co::new(|yielder| async move {
        let _ = yielder.yield_(Trigger).await;
        yielder.yield_(Log("should not appear")).await;
    });

    let mut state: Vec<&str> = vec![];
    let result = sync::run_with(
        co,
        &mut state,
        &mut hlist![
            |_s: &mut Vec<&str>, _: Trigger| CoControl::cancel(),
            |s: &mut Vec<&str>, Log(m)| {
                s.push(m);
                CoControl::resume(())
            },
        ],
    );

    assert_eq!(result, Err(Cancelled));
    assert!(state.is_empty());
}

#[test]
fn sync_non_cancel_handler_cancels() {
    type Effs = Effects![Fetch];

    let co: Co<'_, Effs, &'static str> = Co::new(|yielder| async move {
        let _ = yielder.yield_(Fetch("restricted")).await;
        "unreachable"
    });

    let result = sync::run(
        co,
        &mut hlist![|Fetch(path)| {
            if path == "restricted" {
                CoControl::cancel()
            } else {
                CoControl::resume("ok".to_string())
            }
        }],
    );

    assert_eq!(result, Err(Cancelled));
}

#[test]
fn sync_cancel_mid_pipeline() {
    type Effs = Effects![Trigger, Log, Fetch];

    let co: Co<'_, Effs, ()> = Co::new(|yielder| async move {
        yielder.yield_(Log("step1")).await;
        let _ = yielder.yield_(Fetch("f")).await;
        let _ = yielder.yield_(Trigger).await;
        yielder.yield_(Log("step4")).await;
    });

    let mut state: Vec<String> = vec![];
    let result = sync::run_with(
        co,
        &mut state,
        &mut hlist![
            |_s: &mut Vec<String>, _: Trigger| CoControl::cancel(),
            |s: &mut Vec<String>, Log(m)| {
                s.push(m.to_string());
                CoControl::resume(())
            },
            |_s: &mut Vec<String>, Fetch(p)| CoControl::resume(format!("data:{p}")),
        ],
    );

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, vec!["step1".to_string()]);
}

#[test]
fn sync_cancel_preserves_state_before_cancel() {
    type Effs = Effects![Fetch, Trigger];

    let co: Co<'_, Effs, ()> = Co::new(|yielder| async move {
        let _ = yielder.yield_(Fetch("a")).await;
        let _ = yielder.yield_(Fetch("b")).await;
        let _ = yielder.yield_(Trigger).await;
        let _ = yielder.yield_(Fetch("c")).await;
    });

    let mut state: u64 = 0;
    let result = sync::run_with(
        co,
        &mut state,
        &mut hlist![
            |s: &mut u64, _: Fetch| {
                *s += 1;
                CoControl::resume(String::new())
            },
            |_s: &mut u64, _: Trigger| CoControl::cancel(),
        ],
    );

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, 2u64);
}

#[test]
fn sync_refcell_cancel_no_borrow_leak() {
    use std::cell::RefCell;

    type Effs = Effects![Fetch, Trigger];

    let state: RefCell<Vec<String>> = RefCell::new(vec![]);

    let co: Co<'_, Effs, ()> = Co::new(|yielder| async move {
        let _ = yielder.yield_(Fetch("x")).await;
        let _ = yielder.yield_(Trigger).await;
    });

    let result = sync::run(
        co,
        &mut hlist![
            |Fetch(p)| {
                state.borrow_mut().push(p.to_string());
                CoControl::resume(String::new())
            },
            |_: Trigger| CoControl::cancel(),
        ],
    );

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state.into_inner(), vec!["x".to_string()]);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_non_cancel_handler_cancels() {
    type Effs = Effects![Fetch];

    let co: Co<'_, Effs, &'static str> = Co::new(|yielder| async move {
        let _ = yielder.yield_(Fetch("anything")).await;
        "unreachable"
    });

    let result = run(co, &mut hlist![async |_: Fetch| CoControl::cancel()]).await;
    assert_eq!(result, Err(Cancelled));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_early_cancel() {
    type Effs = Effects![Trigger, Log];

    let co: Co<'_, Effs, ()> = Co::new(|yielder| async move {
        let _ = yielder.yield_(Trigger).await;
        yielder.yield_(Log("never")).await;
    });

    let result = run(
        co,
        &mut hlist![async |_: Trigger| CoControl::cancel(), async |_: Log| {
            CoControl::resume(())
        },],
    )
    .await;

    assert_eq!(result, Err(Cancelled));
}
