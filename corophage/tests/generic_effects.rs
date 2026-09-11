//! These helpers use only public imports and also compile without procedural macros.

use std::future::Future;

use corophage::prelude::*;
use corophage::{EmbedEffect, ProjectResume, Resumes};

fn identity<'a, Es: corophage::Effects<'a>, F>(program: Program<'a, Es, F>) -> Program<'a, Es, F> {
    program
}

// The callback's effect set stays unknown until an application supplies a program.
fn invoke_callback<'a, Es, F, Make, Indices>(
    make: Make,
) -> Program<'a, Es, impl Future<Output = F::Output> + 'a>
where
    Es: Effects<'a> + EmbedEffect<Es, Indices>,
    Resumes<'a, Es>: ProjectResume<'a, Es, Indices>,
    F: Future + 'a,
    Make: FnOnce() -> Program<'a, Es, F> + 'a,
{
    Program::new(|mut y: Yielder<'a, Es>| async move { y.invoke(make()).await })
}

struct Ask;
impl Effect for Ask {
    type Resume<'r> = usize;
    fn shorten_resume<'a: 'b, 'b>(value: usize) -> usize {
        value
    }
}

struct Log(usize);
impl Effect for Log {
    type Resume<'r> = ();
    fn shorten_resume<'a: 'b, 'b>(value: ()) {
        value
    }
}

fn child() -> Program<'static, Effects![Ask, Log], impl Future<Output = usize>> {
    Program::new(|mut y: Yielder<'_, Effects![Ask, Log]>| async move {
        let value = y.yield_(Ask).await;
        y.yield_(Log(value)).await;
        value * 2
    })
}

#[test]
fn generic_helpers_infer_effect_sets_and_forward_resumes() {
    let mut logs = Vec::new();
    let result = invoke_callback(|| identity(child()))
        .handle(|Log(value)| {
            logs.push(value);
            Control::resume(())
        })
        .handle(|_: Ask| Control::resume(4))
        .run_sync();
    assert_eq!(result, Ok(8));
    assert_eq!(logs, [4]);

    let empty =
        invoke_callback(|| identity(Program::new(|_: Yielder<'_, Effects![]>| async { 7 })));
    assert_eq!(empty.run_sync(), Ok(7));
}

struct Read<'a>(&'a str);
impl Effect for Read<'_> {
    type Resume<'r> = &'r str;
    fn shorten_resume<'a: 'b, 'b>(value: &'a str) -> &'b str {
        value
    }
}

#[test]
fn same_helpers_accept_borrowed_effects_and_gat_resumes() {
    let text = String::from("borrowed");
    let text_ref = text.as_str();
    let result = invoke_callback(|| {
        identity(Program::new(
            |mut y: Yielder<'_, Effects![Read<'_>]>| async move { y.yield_(Read(text_ref)).await },
        ))
    })
    .handle(|Read(text)| Control::resume(text))
    .run_sync();
    assert_eq!(result, Ok(text.as_str()));
}

#[test]
fn forwarding_preserves_send_for_owned_components() {
    fn assert_send(_: impl Send) {}
    let future = invoke_callback(child)
        .handle(|_: Ask| Control::resume(4))
        .handle(|_: Log| Control::resume(()))
        .run();
    assert_send(future);
}
