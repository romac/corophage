use std::cell::RefCell;
use std::marker::PhantomData;

use frunk::hlist;

use corophage::*;

pub enum Never {}

pub struct Cancel;

impl Effect for Cancel {
    type Resume = Never;
}

pub struct Log<'a>(pub &'a str);

impl<'a> Effect for Log<'a> {
    type Resume = ();
}

pub struct FileRead(pub String);

impl Effect for FileRead {
    type Resume = String;
}

#[derive(Default)]
pub struct GetState<S> {
    _marker: PhantomData<S>,
}

impl<S> Effect for GetState<S> {
    type Resume = S;
}

pub struct SetState<S>(pub S);

impl<S> Effect for SetState<S> {
    type Resume = ((), ());
}

pub type CoEffs = Effects![Cancel, Log<'static>, FileRead, GetState<u64>, SetState<u64>];

pub fn co() -> Co<CoEffs, ()> {
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

#[test]
fn it_works() {
    #[derive(Debug, PartialEq, Eq)]
    struct State {
        x: u64,
    }

    let state = RefCell::new(State { x: 42 });

    fn cancel(_c: Cancel) -> CoControl<CoEffs> {
        CoControl::cancel()
    }

    fn log(Log(msg): Log<'_>) -> CoControl<CoEffs> {
        println!("LOG: {msg}");
        CoControl::resume(())
    }

    fn file_read(FileRead(file): FileRead) -> CoControl<CoEffs> {
        println!("Reading file: {file}");
        CoControl::resume("file content".to_string())
    }

    let result = corophage::run(
        co(),
        &mut hlist![
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
