use std::marker::PhantomData;

use corophage::prelude::*;

#[effect(Never)]
struct Cancel;

#[effect(())]
pub struct Log<'a>(pub &'a str);

#[effect(String)]
pub struct FileRead(pub String);

#[derive(Default)]
pub struct GetState<S> {
    _marker: PhantomData<S>,
}

impl<S> Effect for GetState<S>
where
    S: Send + Sync,
{
    type Resume<'r> = S;
}

pub struct SetState<S>(pub S);

impl<S> Effect for SetState<S> {
    type Resume<'r> = ();
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

#[derive(Debug, PartialEq, Eq)]
struct State {
    x: u64,
}

#[effectful(Cancel, Log<'static>, FileRead, GetState<u64>, SetState<u64>)]
fn my_program() -> () {
    println!("Logging...");
    let () = yield_!(Log("Hello, world!"));

    println!("Reading file...");
    let text = yield_!(FileRead("example.txt".to_string()));
    println!("Read file: {text}");

    let state = yield_!(GetState::default());
    println!("State: {state}");
    yield_!(SetState(state * 2));
    let state = yield_!(GetState::default());
    println!("State: {state}");

    println!("Cancelling...");
    yield_!(Cancel);
    println!("Cancelled!");
}

fn main() {
    let mut state = State { x: 42 };

    let result = my_program()
        .handle(cancel)
        .handle(log)
        .handle(file_read)
        .handle(|s: &mut State, _g: GetState<u64>| Control::resume(s.x))
        .handle(|s: &mut State, SetState(x)| {
            s.x = x;
            Control::resume(())
        })
        .run_sync_stateful(&mut state);

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, State { x: 84 });
}
