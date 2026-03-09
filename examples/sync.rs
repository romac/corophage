use std::marker::PhantomData;

use corophage::coroutine::Co;
use corophage::prelude::*;

pub struct Cancel;

impl Effect for Cancel {
    type Resume<'r> = Never;
}

pub struct Log<'a>(pub &'a str);

impl<'a> Effect for Log<'a> {
    type Resume<'r> = ();
}

pub struct FileRead(pub String);

impl Effect for FileRead {
    type Resume<'r> = String;
}

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

pub type CoEffs = Effects![Cancel, Log<'static>, FileRead, GetState<u64>, SetState<u64>];

pub fn co() -> Co<'static, CoEffs, ()> {
    Co::new(|yielder| async move {
        println!("Logging...");
        let () = yielder.yield_(Log("Hello, world!")).await;

        println!("Reading file...");
        let text = yielder.yield_(FileRead("example.txt".to_string())).await;
        println!("Read file: {text}");

        let state = yielder.yield_(GetState::default()).await;
        println!("State: {state}");
        yielder.yield_(SetState(state * 2)).await;
        let state = yielder.yield_(GetState::default()).await;
        println!("State: {state}");

        println!("Cancelling...");
        yielder.yield_(Cancel).await;
        println!("Cancelled!");
    })
}

fn main() {
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
