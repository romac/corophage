use corophage::coroutine::Co;
use corophage::prelude::*;
use corophage::{asynk, sync};

#[allow(dead_code)]
struct Ask(pub &'static str);

impl Effect for Ask {
    type Resume<'r> = &'static str;
}

impl CovariantResume for Ask {
    fn shorten_resume<'a: 'b, 'b>(resume: &'static str) -> &'static str {
        resume
    }
}

struct Counter;

impl Effect for Counter {
    type Resume<'r> = u64;
}

impl CovariantResume for Counter {
    fn shorten_resume<'a: 'b, 'b>(resume: u64) -> u64 {
        resume
    }
}

#[derive(Debug, PartialEq)]
struct Report {
    count: u64,
    label: &'static str,
}

#[test]
fn sync_ok_unit_return() {
    type Effs = Effects![Ask];

    let co: Co<'_, Effs, ()> = Co::new(|yielder| async move {
        let _: &'static str = yielder.yield_(Ask("q")).await;
    });

    let result = sync::run(co, &mut hlist![|_: Ask| Control::resume("_")]);
    assert_eq!(result, Ok(()));
}

#[test]
fn sync_ok_value_return() {
    type Effs = Effects![Ask];

    let co: Co<'_, Effs, &'static str> =
        Co::new(|yielder| async move { yielder.yield_(Ask("the question")).await });

    let result = sync::run(co, &mut hlist![|Ask(_)| Control::resume("42")]);
    assert_eq!(result, Ok("42"));
}

#[test]
fn sync_ok_no_yields() {
    type Effs = Effects![Ask];

    let co: Co<'_, Effs, u64> = Co::new(|_yielder| async move { 99u64 });

    let result = sync::run(co, &mut hlist![|_: Ask| Control::resume("")]);
    assert_eq!(result, Ok(99u64));
}

#[test]
fn sync_ok_multiple_yields() {
    type Effs = Effects![Counter];

    let co: Co<'_, Effs, u64> = Co::new(|yielder| async move {
        let a = yielder.yield_(Counter).await;
        let b = yielder.yield_(Counter).await;
        a + b
    });

    let mut state: u64 = 0;
    let result = sync::run_stateful(
        co,
        &mut state,
        &hlist![|s: &mut u64, _: Counter| {
            *s += 1;
            Control::resume(*s)
        }],
    );

    assert_eq!(result, Ok(3u64));
    assert_eq!(state, 2u64);
}

#[test]
fn sync_ok_struct_return() {
    type Effs = Effects![Counter, Ask];

    let co: Co<'_, Effs, Report> = Co::new(|yielder| async move {
        let n = yielder.yield_(Counter).await;
        let label = yielder.yield_(Ask("tag")).await;
        Report { count: n, label }
    });

    let result = sync::run(
        co,
        &mut hlist![|_: Counter| Control::resume(7u64), |_: Ask| {
            Control::resume("hello")
        },],
    );
    assert_eq!(
        result,
        Ok(Report {
            count: 7,
            label: "hello"
        })
    );
}

#[test]
fn sync_run_stateful_ok() {
    type Effs = Effects![Counter];

    let co: Co<'_, Effs, u64> = Co::new(|yielder| async move { yielder.yield_(Counter).await });

    let mut state: u64 = 10;
    let result = sync::run_stateful(
        co,
        &mut state,
        &hlist![|s: &mut u64, _: Counter| Control::resume(*s)],
    );

    assert_eq!(result, Ok(10u64));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_ok_value_return() {
    type Effs = Effects![Ask];

    let co: Co<'_, Effs, &'static str> =
        Co::new(|yielder| async move { yielder.yield_(Ask("async")).await });

    let result = asynk::run(co, &mut hlist![async |_: Ask| Control::resume("done")]).await;
    assert_eq!(result, Ok("done"));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_run_stateful_ok() {
    type Effs = Effects![Counter];

    let co: Co<'_, Effs, u64> = Co::new(|yielder| async move { yielder.yield_(Counter).await });

    let mut state: u64 = 5;
    let result = asynk::run_stateful(
        co,
        &mut state,
        &hlist![async |s: &mut u64, _: Counter| {
            *s += 10;
            Control::resume(*s)
        }],
    )
    .await;

    assert_eq!(result, Ok(15u64));
    assert_eq!(state, 15u64);
}
