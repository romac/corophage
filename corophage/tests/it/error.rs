use corophage::coroutine::Co;
use corophage::prelude::*;
use corophage::{ProjectResume, asynk, sync};
use frunk_core::coproduct::CNil;
use frunk_core::hlist::{HCons, HNil};
use frunk_core::indices::{Here, There};

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

#[test]
#[should_panic(expected = "handler resumed at the wrong effect index")]
fn mismatched_public_indices_panic() {
    type DuplicateAskEffects = Effects![Ask, Ask];

    let co: Co<'_, DuplicateAskEffects, &'static str> =
        Co::new(
            |mut yielder| async move { yielder.yield_::<Ask, There<Here>>(Ask("second")).await },
        );

    let _ = sync::run::<_, _, _, _, HCons<(Here, Here, _), HCons<(Here, Here, _), HNil>>>(
        co,
        &mut hlist![|_: Ask| Control::resume("answer")],
    );
}

#[test]
fn empty_resume_projection_panics() {
    fn project() -> CNil {
        <u8 as ProjectResume<'static, CNil, HNil>>::project(1)
    }

    assert!(std::panic::catch_unwind(project).is_err());
}

struct Outer;
impl Effect for Outer {
    type Resume<'r> = u64;
    fn shorten_resume<'a: 'b, 'b>(resume: u64) -> u64 {
        resume
    }
}

struct Inner(#[allow(dead_code)] u64, #[allow(dead_code)] u64);
impl Effect for Inner {
    type Resume<'r> = ();
    fn shorten_resume<'a: 'b, 'b>(resume: ()) {
        resume
    }
}

// fauxgen identifies a generator by its token's address. Dropping the yielder
// must not free that address while the generator runs, or a token allocated
// later could pass the check and write a foreign value into this generator.
#[test]
#[should_panic(expected = "waker id does not match generator id")]
fn foreign_yielder_after_dropping_own_yielder_panics() {
    let _ = Program::new(|outer: Yielder<'_, Effects![Outer]>| async move {
        drop(outer);
        let mut inner = Program::new(|inner: Yielder<'_, Effects![Inner]>| async move { inner })
            .handle(|_: Inner| Control::resume(()))
            .run_sync()
            .unwrap();
        inner.yield_(Inner(1, 2)).await;
    })
    .handle(|_: Outer| Control::resume(0))
    .run_sync();
}

// Section B: Single-effect coroutines

struct Ask(pub &'static str);

impl Effect for Ask {
    type Resume<'r> = &'static str;

    fn shorten_resume<'a: 'b, 'b>(resume: &'static str) -> &'static str {
        resume
    }
}

type AskEffects = Effects![Ask];

#[test]
fn sync_single_effect_resume() {
    let co: Co<'_, AskEffects, &'static str> =
        Co::new(|mut yielder| async move { yielder.yield_(Ask("q")).await });

    let result = sync::run(co, &mut hlist![|Ask(_)| Control::resume("42")]);
    assert_eq!(result, Ok("42"));
}

#[test]
fn sync_single_effect_cancel() {
    let co: Co<'_, AskEffects, &'static str> = Co::new(|mut yielder| async move {
        yielder.yield_(Ask("forbidden")).await;
        "unreachable"
    });

    let result = sync::run(co, &mut hlist![|_: Ask| Control::cancel()]);
    assert_eq!(result, Err(Cancelled));
}

#[test]
fn sync_single_effect_multiple_yields() {
    let co: Co<'_, AskEffects, (&'static str, &'static str, &'static str)> =
        Co::new(|mut yielder| async move {
            let a = yielder.yield_(Ask("q1")).await;
            let b = yielder.yield_(Ask("q2")).await;
            let c = yielder.yield_(Ask("q3")).await;
            (a, b, c)
        });

    let mut state: u32 = 0;
    let result = sync::run_stateful(
        co,
        &mut state,
        &hlist![|s: &mut u32, _: Ask| {
            *s += 1;
            Control::resume(match *s {
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
    let co: Co<'_, AskEffects, ()> = Co::new(|mut yielder| async move {
        yielder.yield_(Ask("q1")).await;
        yielder.yield_(Ask("q2")).await;
        yielder.yield_(Ask("q3")).await;
    });

    let mut log: Vec<&str> = vec![];
    let result = sync::run(
        co,
        &mut hlist![|Ask(q)| {
            log.push(q);
            Control::resume("_")
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
        Co::new(|mut yielder| async move { yielder.yield_(Ask("q")).await });

    let result = asynk::run(
        co,
        &mut hlist![async |_: Ask| {
            tokio::time::sleep(Duration::from_millis(1)).await;
            Control::resume("answered")
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

    let co: Co<'_, AskEffects, ()> = Co::new(|mut yielder| async move {
        yielder.yield_(Ask("a")).await;
        yielder.yield_(Ask("b")).await;
    });

    let result = asynk::run(
        co,
        &mut hlist![async |Ask(q): Ask| {
            log.borrow_mut().push(q);
            Control::resume("_")
        }],
    )
    .await;

    assert_eq!(result, Ok(()));
    assert_eq!(log.into_inner(), ["a", "b"]);
}
