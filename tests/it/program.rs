use corophage::handle;
use corophage::prelude::*;

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

    let result = Program::new(|yielder: Yielder<'_, Effs>| async move {
        let n = yielder.yield_(Counter).await;
        let answer = yielder.yield_(Ask("question")).await;
        (answer, n)
    })
    .handle(|_: Counter| Control::resume(42u64))
    .handle(|_: Ask| Control::resume("yes"))
    .run_sync();

    assert_eq!(result, Ok(("yes", 42)));
}

#[test]
fn sync_free_function_style() {
    type Effs = Effects![Counter, Ask];

    let p = Program::new(|yielder: Yielder<'_, Effs>| async move {
        let n = yielder.yield_(Counter).await;
        let answer = yielder.yield_(Ask("question")).await;
        (answer, n)
    });

    let p = handle(p, |_: Counter| Control::resume(42u64));
    let p = handle(p, |_: Ask| Control::resume("yes"));
    let result = p.run_sync();

    assert_eq!(result, Ok(("yes", 42)));
}

#[test]
fn sync_single_effect() {
    type Effs = Effects![Ask];

    let result =
        Program::new(
            |yielder: Yielder<'_, Effs>| async move { yielder.yield_(Ask("hello")).await },
        )
        .handle(|_: Ask| Control::resume("world"))
        .run_sync();

    assert_eq!(result, Ok("world"));
}

#[test]
fn sync_no_yields() {
    type Effs = Effects![Ask];

    let result = Program::new(|_: Yielder<'_, Effs>| async move { 99u64 })
        .handle(|_: Ask| Control::resume(""))
        .run_sync();

    assert_eq!(result, Ok(99));
}

#[test]
fn sync_cancel() {
    type Effs = Effects![Ask];

    let result =
        Program::new(|yielder: Yielder<'_, Effs>| async move { yielder.yield_(Ask("q")).await })
            .handle(|_: Ask| Control::<&'static str>::cancel())
            .run_sync();

    assert_eq!(result, Err(Cancelled));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_builder_style() {
    type Effs = Effects![Counter, Ask];

    let result = Program::new(|yielder: Yielder<'_, Effs>| async move {
        let n = yielder.yield_(Counter).await;
        let answer = yielder.yield_(Ask("question")).await;
        (answer, n)
    })
    .handle(async |_: Counter| Control::resume(42u64))
    .handle(async |_: Ask| Control::resume("yes"))
    .run()
    .await;

    assert_eq!(result, Ok(("yes", 42)));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_free_function_style() {
    type Effs = Effects![Counter, Ask];

    let p = Program::new(|yielder: Yielder<'_, Effs>| async move {
        let n = yielder.yield_(Counter).await;
        let answer = yielder.yield_(Ask("question")).await;
        (answer, n)
    });

    let p = handle(p, async |_: Counter| Control::resume(42u64));
    let p = handle(p, async |_: Ask| Control::resume("yes"));
    let result = p.run().await;

    assert_eq!(result, Ok(("yes", 42)));
}

#[test]
fn sync_run_stateful_state() {
    type Effs = Effects![Counter];

    let mut state: u64 = 0;
    let result = Program::new(|yielder: Yielder<'_, Effs>| async move {
        let a = yielder.yield_(Counter).await;
        let b = yielder.yield_(Counter).await;
        a + b
    })
    .handle(|s: &mut u64, _: Counter| {
        *s += 1;
        Control::resume(*s)
    })
    .run_sync_stateful(&mut state);

    assert_eq!(result, Ok(3u64));
    assert_eq!(state, 2u64);
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_run_stateful_state() {
    type Effs = Effects![Counter];

    let mut state: u64 = 5;
    let result =
        Program::new(|yielder: Yielder<'_, Effs>| async move { yielder.yield_(Counter).await })
            .handle(async |s: &mut u64, _: Counter| {
                *s += 10;
                Control::resume(*s)
            })
            .run_stateful(&mut state)
            .await;

    assert_eq!(result, Ok(15u64));
    assert_eq!(state, 15u64);
}

#[test]
fn from_co() {
    use corophage::coroutine::Co;

    type Effs = Effects![Ask];

    let co: Co<'_, Effs, &'static str> =
        Co::new(|yielder| async move { yielder.yield_(Ask("hello")).await });

    let result = Program::from_co(co)
        .handle(|_: Ask| Control::resume("world"))
        .run_sync();

    assert_eq!(result, Ok("world"));
}
