use std::marker::PhantomData;

use corophage::prelude::*;

declare_effect!(Cancel -> Never);
declare_effect!(Log<'a>(pub &'a str) -> ());
declare_effect!(FileRead(pub String) -> String);

#[derive(Default)]
pub struct GetState<S> {
    _marker: PhantomData<S>,
}

impl<S> Effect for GetState<S>
where
    S: Sync + Send,
{
    type Resume<'r> = S;
}

pub struct SetState<S>(pub S);

impl<S> Effect for SetState<S> {
    type Resume<'r> = ();
}

type Effs = Effects![Cancel, Log<'static>, FileRead, GetState<u64>, SetState<u64>];

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

#[derive(Debug, PartialEq, Eq)]
struct State {
    x: u64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut state = State { x: 42 };

    let result = Program::new(|y: Yielder<'_, Effs>| async move {
        println!("Logging...");
        let () = y.yield_(Log("Hello, world!")).await;

        println!("Reading file...");
        let text = y.yield_(FileRead("example.txt".to_string())).await;
        println!("Read file: {text}");

        let state = y.yield_(GetState::default()).await;
        println!("State: {state}");
        y.yield_(SetState(state * 2)).await;
        let state = y.yield_(GetState::default()).await;
        println!("State: {state}");

        println!("Cancelling...");
        y.yield_(Cancel).await;
        println!("Cancelled!");
    })
    .handle(cancel)
    .handle(log)
    .handle(file_read)
    .handle(async |s: &mut State, _g: GetState<u64>| Control::resume(s.x))
    .handle(async |s: &mut State, SetState(x)| {
        s.x = x;
        Control::resume(())
    })
    .run_stateful(&mut state)
    .await;

    assert_eq!(result, Err(Cancelled));
    assert_eq!(state, State { x: 84 });
}
