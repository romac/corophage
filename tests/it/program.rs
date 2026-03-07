use corophage::prelude::*;
use corophage::program::handle;

#[allow(unused)]
struct Ask(&'static str);

impl Effect for Ask {
    type Resume<'r> = &'static str;
}

struct Counter;

impl Effect for Counter {
    type Resume<'r> = u64;
}

#[test]
fn sync_builder_style() {
    type Effs = Effects![Counter, Ask];

    let co: Co<'_, Effs, (&'static str, u64)> = Co::new(|yielder| async move {
        let n = yielder.yield_(Counter).await;
        let answer = yielder.yield_(Ask("question")).await;
        (answer, n)
    });

    let result = Program::new(co)
        .handle(|_: Counter| CoControl::resume(42u64))
        .handle(|_: Ask| CoControl::resume("yes"))
        .run_sync();

    assert_eq!(result, Ok(("yes", 42)));
}

#[test]
fn sync_free_function_style() {
    type Effs = Effects![Counter, Ask];

    let co: Co<'_, Effs, (&'static str, u64)> = Co::new(|yielder| async move {
        let n = yielder.yield_(Counter).await;
        let answer = yielder.yield_(Ask("question")).await;
        (answer, n)
    });

    let p = Program::new(co);
    let p = handle(p, |_: Counter| CoControl::resume(42u64));
    let p = handle(p, |_: Ask| CoControl::resume("yes"));
    let result = p.run_sync();

    assert_eq!(result, Ok(("yes", 42)));
}

#[test]
fn sync_single_effect() {
    type Effs = Effects![Ask];

    let co: Co<'_, Effs, &'static str> =
        Co::new(|yielder| async move { yielder.yield_(Ask("hello")).await });

    let result = Program::new(co)
        .handle(|_: Ask| CoControl::resume("world"))
        .run_sync();

    assert_eq!(result, Ok("world"));
}

#[test]
fn sync_no_yields() {
    type Effs = Effects![Ask];

    let co: Co<'_, Effs, u64> = Co::new(|_yielder| async move { 99u64 });

    let result = Program::new(co)
        .handle(|_: Ask| CoControl::resume(""))
        .run_sync();

    assert_eq!(result, Ok(99));
}

#[test]
fn sync_cancel() {
    type Effs = Effects![Ask];

    let co: Co<'_, Effs, &'static str> =
        Co::new(|yielder| async move { yielder.yield_(Ask("q")).await });

    let result = Program::new(co)
        .handle(|_: Ask| CoControl::cancel())
        .run_sync();

    assert_eq!(result, Err(Cancelled));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_builder_style() {
    type Effs = Effects![Counter, Ask];

    let co: Co<'_, Effs, (&'static str, u64)> = Co::new(|yielder| async move {
        let n = yielder.yield_(Counter).await;
        let answer = yielder.yield_(Ask("question")).await;
        (answer, n)
    });

    let result = Program::new(co)
        .handle(async |_: Counter| CoControl::resume(42u64))
        .handle(async |_: Ask| CoControl::resume("yes"))
        .run()
        .await;

    assert_eq!(result, Ok(("yes", 42)));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_free_function_style() {
    type Effs = Effects![Counter, Ask];

    let co: Co<'_, Effs, (&'static str, u64)> = Co::new(|yielder| async move {
        let n = yielder.yield_(Counter).await;
        let answer = yielder.yield_(Ask("question")).await;
        (answer, n)
    });

    let p = Program::new(co);
    let p = handle(p, async |_: Counter| CoControl::resume(42u64));
    let p = handle(p, async |_: Ask| CoControl::resume("yes"));
    let result = p.run().await;

    assert_eq!(result, Ok(("yes", 42)));
}

#[test]
fn sync_run_with_state() {
    type Effs = Effects![Counter];

    let co: Co<'_, Effs, u64> = Co::new(|yielder| async move {
        let a = yielder.yield_(Counter).await;
        let b = yielder.yield_(Counter).await;
        a + b
    });

    let mut state: u64 = 0;
    let result = Program::new(co)
        .handle(|s: &mut u64, _: Counter| {
            *s += 1;
            CoControl::resume(*s)
        })
        .run_sync_with(&mut state);

    assert_eq!(result, Ok(3u64));
    assert_eq!(state, 2u64);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_run_with_state() {
    type Effs = Effects![Counter];

    let co: Co<'_, Effs, u64> = Co::new(|yielder| async move { yielder.yield_(Counter).await });

    let mut state: u64 = 5;
    let result = Program::new(co)
        .handle(async |s: &mut u64, _: Counter| {
            *s += 10;
            CoControl::resume(*s)
        })
        .run_with(&mut state)
        .await;

    assert_eq!(result, Ok(15u64));
    assert_eq!(state, 15u64);
}
