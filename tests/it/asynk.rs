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

    async fn cancel(_c: Cancel) -> CoControl<'static, CoEffs> {
        CoControl::cancel()
    }

    async fn log(Log(msg): Log<'_>) -> CoControl<'static, CoEffs> {
        println!("LOG: {msg}");
        CoControl::resume(())
    }

    async fn file_read(FileRead(file): FileRead) -> CoControl<'static, CoEffs> {
        println!("Reading file: {file}");
        CoControl::resume("file content".to_string())
    }

    let state = RefCell::new(State { x: 42 });

    let result = corophage::run(
        co(),
        &mut hlist![
            cancel,
            log,
            file_read,
            async |_g: GetState<u64>| CoControl::resume(state.borrow().x),
            async |SetState(x)| {
                state.borrow_mut().x = x;
                CoControl::resume(((), ()))
            },
        ],
    )
    .await;

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state.into_inner(), State { x: 84 });
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn run_with() {
    #[derive(Debug, PartialEq, Eq)]
    struct State {
        x: u64,
    }

    async fn cancel(_: &mut State, _c: Cancel) -> CoControl<'static, CoEffs> {
        CoControl::cancel()
    }

    async fn log(_: &mut State, Log(msg): Log<'_>) -> CoControl<'static, CoEffs> {
        println!("LOG: {msg}");
        CoControl::resume(())
    }

    async fn file_read(_: &mut State, FileRead(file): FileRead) -> CoControl<'static, CoEffs> {
        println!("Reading file: {file}");
        CoControl::resume("file content".to_string())
    }

    let mut state = State { x: 42 };

    let result = corophage::run_with(
        co(),
        &mut state,
        &mut hlist![
            cancel,
            log,
            file_read,
            async |s: &mut State, _g: GetState<u64>| CoControl::resume(s.x),
            async |s: &mut State, SetState(x)| {
                s.x = x;
                CoControl::resume(((), ()))
            },
        ],
    )
    .await;

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, State { x: 84 });
}
