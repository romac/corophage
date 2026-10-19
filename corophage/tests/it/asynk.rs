use corophage::prelude::*;

use crate::common::*;

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn run_mut() {
    use std::cell::RefCell;

    #[derive(Debug, PartialEq, Eq)]
    struct State {
        x: u64,
    }

    async fn cancel(_c: Cancel) -> Control<Never> {
        Control::cancel()
    }

    async fn log(Log(msg): Log<'_>) -> Control<()> {
        println!("LOG: {msg}");
        Control::resume(())
    }

    async fn file_read(FileRead(file): FileRead) -> Control<String> {
        println!("Reading file: {file}");
        Control::resume("file content".to_string())
    }

    let state = RefCell::new(State { x: 42 });

    let result = corophage::asynk::run(
        co(),
        &mut hlist![
            cancel,
            log,
            file_read,
            async |_g: GetState<u64>| Control::resume(state.borrow().x),
            async |SetState(x)| {
                state.borrow_mut().x = x;
                Control::resume(())
            },
        ],
    )
    .await;

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state.into_inner(), State { x: 84 });
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn run_stateful() {
    #[derive(Debug, PartialEq, Eq)]
    struct State {
        x: u64,
    }

    async fn cancel(_: &mut State, _c: Cancel) -> Control<Never> {
        Control::cancel()
    }

    async fn log(_: &mut State, Log(msg): Log<'_>) -> Control<()> {
        println!("LOG: {msg}");
        Control::resume(())
    }

    async fn file_read(_: &mut State, FileRead(file): FileRead) -> Control<String> {
        println!("Reading file: {file}");
        Control::resume("file content".to_string())
    }

    let mut state = State { x: 42 };

    let result = corophage::asynk::run_stateful(
        co(),
        &mut state,
        &hlist![
            cancel,
            log,
            file_read,
            async |s: &mut State, _g: GetState<u64>| Control::resume(s.x),
            async |s: &mut State, SetState(x)| {
                s.x = x;
                Control::resume(())
            },
        ],
    )
    .await;

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, State { x: 84 });
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn low_level_runners_accept_mixed_handlers() {
    let state = std::cell::Cell::new(42);

    let result = corophage::asynk::run(
        co(),
        &mut hlist![
            |_: Cancel| Control::cancel(),
            |Log(msg): Log<'_>| {
                println!("LOG: {msg}");
                Control::resume(())
            },
            async |FileRead(file): FileRead| Control::resume(format!("content of {file}")),
            |_: GetState<u64>| Control::resume(state.get()),
            |SetState(x)| {
                state.set(x);
                Control::resume(())
            },
        ],
    )
    .await;

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state.get(), 84);

    let mut state = 42;
    let result = corophage::asynk::run_stateful(
        co(),
        &mut state,
        &hlist![
            |_: &mut u64, _: Cancel| Control::cancel(),
            |_: &mut u64, Log(msg): Log<'_>| {
                println!("LOG: {msg}");
                Control::resume(())
            },
            async |_: &mut u64, FileRead(file): FileRead| {
                Control::resume(format!("content of {file}"))
            },
            |s: &mut u64, _: GetState<u64>| Control::resume(*s),
            |s: &mut u64, SetState(x)| {
                *s = x;
                Control::resume(())
            },
        ],
    )
    .await;

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, 84);
}
