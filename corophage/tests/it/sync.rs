use corophage::prelude::*;

use crate::common::*;

#[test]
fn run_mut() {
    use std::cell::RefCell;

    #[derive(Debug, PartialEq, Eq)]
    struct State {
        x: u64,
    }

    fn cancel(_c: Cancel) -> Control<Never> {
        Control::cancel()
    }

    fn log(Log(msg): Log<'_>) -> Control<()> {
        println!("LOG: {msg}");
        Control::resume(())
    }

    fn file_read(FileRead(file): FileRead) -> Control<String> {
        println!("Reading file: {file}");
        Control::resume("file content".to_string())
    }

    let state = RefCell::new(State { x: 42 });

    let result = corophage::sync::run(
        co(),
        &mut frunk::hlist![
            cancel,
            log,
            file_read,
            |_g: GetState<u64>| Control::resume(state.borrow().x),
            |SetState(x)| {
                state.borrow_mut().x = x;
                Control::resume(())
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

    fn cancel(_: &mut State, _c: Cancel) -> Control<Never> {
        Control::cancel()
    }

    fn log(_: &mut State, Log(msg): Log<'_>) -> Control<()> {
        println!("LOG: {msg}");
        Control::resume(())
    }

    fn file_read(_: &mut State, FileRead(file): FileRead) -> Control<String> {
        println!("Reading file: {file}");
        Control::resume("file content".to_string())
    }

    let mut state = State { x: 42 };

    let result = corophage::sync::run_stateful(
        co(),
        &mut state,
        &mut hlist![
            cancel,
            log,
            file_read,
            |s: &mut State, _g: GetState<u64>| Control::resume(s.x),
            |s: &mut State, SetState(x)| {
                s.x = x;
                Control::resume(())
            },
        ],
    );

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, State { x: 84 });
}
