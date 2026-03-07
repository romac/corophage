use corophage::prelude::*;

use crate::common::*;

#[test]
fn run_mut() {
    use std::cell::RefCell;

    #[derive(Debug, PartialEq, Eq)]
    struct State {
        x: u64,
    }

    fn cancel(_c: Cancel) -> CoControl<'static, CoEffs> {
        CoControl::cancel()
    }

    fn log(Log(msg): Log<'_>) -> CoControl<'static, CoEffs> {
        println!("LOG: {msg}");
        CoControl::resume(())
    }

    fn file_read(FileRead(file): FileRead) -> CoControl<'static, CoEffs> {
        println!("Reading file: {file}");
        CoControl::resume("file content".to_string())
    }

    let state = RefCell::new(State { x: 42 });

    let result = corophage::sync::run(
        co(),
        &mut frunk::hlist![
            cancel,
            log,
            file_read,
            |_g: GetState<u64>| CoControl::resume(state.borrow().x),
            |SetState(x)| {
                state.borrow_mut().x = x;
                CoControl::resume(((), ()))
            },
        ],
    );

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state.into_inner(), State { x: 84 });
}

#[test]
fn run_stateful() {
    #[derive(Debug, PartialEq, Eq)]
    struct State {
        x: u64,
    }

    fn cancel(_: &mut State, _c: Cancel) -> CoControl<'static, CoEffs> {
        CoControl::cancel()
    }

    fn log(_: &mut State, Log(msg): Log<'_>) -> CoControl<'static, CoEffs> {
        println!("LOG: {msg}");
        CoControl::resume(())
    }

    fn file_read(_: &mut State, FileRead(file): FileRead) -> CoControl<'static, CoEffs> {
        println!("Reading file: {file}");
        CoControl::resume("file content".to_string())
    }

    let mut state = State { x: 42 };

    let result = corophage::sync::run_stateful(
        co(),
        &mut state,
        &mut hlist![
            cancel,
            log,
            file_read,
            |s: &mut State, _g: GetState<u64>| CoControl::resume(s.x),
            |s: &mut State, SetState(x)| {
                s.x = x;
                CoControl::resume(((), ()))
            },
        ],
    );

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, State { x: 84 });
}
