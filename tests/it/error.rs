use corophage::prelude::*;

// Section A: Cancelled trait tests

#[test]
fn cancelled_display() {
    assert_eq!(format!("{}", Cancelled), "computation was cancelled");
}

#[test]
fn cancelled_is_error() {
    fn accepts_error(_: &dyn std::error::Error) {}
    accepts_error(&Cancelled);
}

#[test]
fn cancelled_error_source_is_none() {
    let err: &dyn std::error::Error = &Cancelled;
    assert!(err.source().is_none());
}

#[test]
fn cancelled_question_mark_propagation() {
    fn might_fail() -> Result<(), Box<dyn std::error::Error>> {
        Err(Cancelled)?;
        Ok(())
    }
    assert_eq!(
        might_fail().unwrap_err().to_string(),
        "computation was cancelled"
    );
}

#[test]
fn cancelled_copy_and_eq() {
    let a = Cancelled;
    let b = a; // Copy
    assert_eq!(a, b); // PartialEq
}

// Section B: Single-effect coroutines

struct Ask(pub &'static str);

impl Effect for Ask {
    type Resume = &'static str;
}

type AskEffects = Effects![Ask];

#[test]
fn sync_single_effect_resume() {
    let co: Co<'_, AskEffects, &'static str> =
        Co::new(|yielder| async move { yielder.yield_(Ask("q")).await });

    let result = sync::run(co, &mut hlist![|Ask(_)| CoControl::resume("42")]);
    assert_eq!(result, Ok("42"));
}

#[test]
fn sync_single_effect_cancel() {
    let co: Co<'_, AskEffects, &'static str> = Co::new(|yielder| async move {
        yielder.yield_(Ask("forbidden")).await;
        "unreachable"
    });

    let result = sync::run(co, &mut hlist![|_: Ask| CoControl::cancel()]);
    assert_eq!(result, Err(Cancelled));
}

#[test]
fn sync_single_effect_multiple_yields() {
    let co: Co<'_, AskEffects, (&'static str, &'static str, &'static str)> =
        Co::new(|yielder| async move {
            let a = yielder.yield_(Ask("q1")).await;
            let b = yielder.yield_(Ask("q2")).await;
            let c = yielder.yield_(Ask("q3")).await;
            (a, b, c)
        });

    let mut state: u32 = 0;
    let result = sync::run_with(
        co,
        &mut state,
        &mut hlist![|s: &mut u32, _: Ask| {
            *s += 1;
            CoControl::resume(match *s {
                1 => "one",
                2 => "two",
                _ => "three",
            })
        }],
    );

    assert_eq!(result, Ok(("one", "two", "three")));
    assert_eq!(state, 3);
}

#[test]
fn sync_handler_accumulates_effects() {
    let co: Co<'_, AskEffects, ()> = Co::new(|yielder| async move {
        yielder.yield_(Ask("q1")).await;
        yielder.yield_(Ask("q2")).await;
        yielder.yield_(Ask("q3")).await;
    });

    let mut log: Vec<&str> = vec![];
    let result = sync::run(
        co,
        &mut hlist![|Ask(q)| {
            log.push(q);
            CoControl::resume("_")
        }],
    );

    assert_eq!(result, Ok(()));
    assert_eq!(log, ["q1", "q2", "q3"]);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_single_effect_with_real_sleep() {
    use std::time::Duration;

    let co: Co<'_, AskEffects, &'static str> =
        Co::new(|yielder| async move { yielder.yield_(Ask("q")).await });

    let result = run(
        co,
        &mut hlist![async |_: Ask| {
            tokio::time::sleep(Duration::from_millis(1)).await;
            CoControl::resume("answered")
        }],
    )
    .await;

    assert_eq!(result, Ok("answered"));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_handler_accumulates_via_refcell() {
    use std::cell::RefCell;

    let log: RefCell<Vec<&str>> = RefCell::new(vec![]);

    let co: Co<'_, AskEffects, ()> = Co::new(|yielder| async move {
        yielder.yield_(Ask("a")).await;
        yielder.yield_(Ask("b")).await;
    });

    let result = run(
        co,
        &mut hlist![async |Ask(q): Ask| {
            log.borrow_mut().push(q);
            CoControl::resume("_")
        }],
    )
    .await;

    assert_eq!(result, Ok(()));
    assert_eq!(log.into_inner(), ["a", "b"]);
}
