use corophage::prelude::*;

#[allow(dead_code)]
struct Ask(&'static str);

impl Effect for Ask {
    type Resume<'r> = &'static str;
}

struct Print(String);

impl Effect for Print {
    type Resume<'r> = ();
}

#[allow(dead_code)]
struct Log(&'static str);

impl Effect for Log {
    type Resume<'r> = ();
}

fn greet<'a>() -> Effectful<'a, Effects![Ask, Print], ()> {
    Program::new(|y: Yielder<'_, Effects![Ask, Print]>| async move {
        let name: &str = y.yield_(Ask("name?")).await;
        y.yield_(Print(format!("Hello, {name}!"))).await;
    })
}

#[test]
fn sync_invoke_subprogram() {
    type Effs = Effects![Ask, Print, Log];

    let mut log = Vec::new();
    let result = Program::new(|y: Yielder<'_, Effs>| async move {
        y.yield_(Log("Starting...")).await;
        y.invoke(greet()).await;
        y.yield_(Log("Done!")).await;
    })
    .handle(|_: Ask| Control::resume("world"))
    .handle({
        let log = &mut log;
        move |p: Print| {
            log.push(p.0);
            Control::resume(())
        }
    })
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Ok(()));
    assert_eq!(log, vec!["Hello, world!"]);
}

#[test]
fn sync_invoke_subprogram_returns_value() {
    struct Add(i32, i32);

    impl Effect for Add {
        type Resume<'r> = i32;
    }

    fn compute<'a>() -> Effectful<'a, Effects![Add], i32> {
        Program::new(|y: Yielder<'_, Effects![Add]>| async move { y.yield_(Add(1, 2)).await })
    }

    type Effs = Effects![Add, Log];

    let result = Program::new(|y: Yielder<'_, Effs>| async move {
        y.yield_(Log("computing...")).await;
        let val = y.invoke(compute()).await;
        val * 10
    })
    .handle(|a: Add| Control::resume(a.0 + a.1))
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Ok(30));
}

#[test]
fn sync_invoke_subprogram_subset_effects() {
    // Sub-program uses only Ask, outer uses Ask + Print + Log
    fn ask_name<'a>() -> Effectful<'a, Effects![Ask], &'static str> {
        Program::new(|y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask("name?")).await })
    }

    type Effs = Effects![Ask, Print, Log];

    let result = Program::new(|y: Yielder<'_, Effs>| async move {
        y.yield_(Log("asking...")).await;
        let name = y.invoke(ask_name()).await;
        y.yield_(Print(format!("Got: {name}"))).await;
        name
    })
    .handle(|_: Ask| Control::resume("Alice"))
    .handle(|_: Print| Control::resume(()))
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Ok("Alice"));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_invoke_subprogram() {
    type Effs = Effects![Ask, Print, Log];

    let result = Program::new(|y: Yielder<'_, Effs>| async move {
        y.yield_(Log("Starting...")).await;
        y.invoke(greet()).await;
        y.yield_(Log("Done!")).await;
    })
    .handle(async |_: Ask| Control::resume("world"))
    .handle(async |_: Print| Control::resume(()))
    .handle(async |_: Log| Control::resume(()))
    .run()
    .await;

    assert_eq!(result, Ok(()));
}

#[test]
fn sync_invoke_no_effects_subprogram() {
    fn pure_computation<'a>() -> Effectful<'a, Effects![], i32> {
        Program::new(|_: Yielder<'_, Effects![]>| async move { 42 })
    }

    type Effs = Effects![Log];

    let result = Program::new(|y: Yielder<'_, Effs>| async move {
        y.yield_(Log("before")).await;
        let val = y.invoke(pure_computation()).await;
        y.yield_(Log("after")).await;
        val
    })
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Ok(42));
}

#[test]
fn sync_invoke_nested() {
    // Invoke a sub-program that itself invokes another sub-program
    fn inner<'a>() -> Effectful<'a, Effects![Ask], &'static str> {
        Program::new(|y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask("inner")).await })
    }

    fn middle<'a>() -> Effectful<'a, Effects![Ask, Print], &'static str> {
        Program::new(|y: Yielder<'_, Effects![Ask, Print]>| async move {
            let name = y.invoke(inner()).await;
            y.yield_(Print(format!("middle: {name}"))).await;
            name
        })
    }

    type Effs = Effects![Ask, Print, Log];

    let mut prints = Vec::new();
    let result = Program::new(|y: Yielder<'_, Effs>| async move {
        y.yield_(Log("start")).await;
        let name = y.invoke(middle()).await;
        y.yield_(Log("end")).await;
        name
    })
    .handle(|_: Ask| Control::resume("deep"))
    .handle({
        let prints = &mut prints;
        move |p: Print| {
            prints.push(p.0);
            Control::resume(())
        }
    })
    .handle(|_: Log| Control::resume(()))
    .run_sync();

    assert_eq!(result, Ok("deep"));
    assert_eq!(prints, vec!["middle: deep"]);
}
