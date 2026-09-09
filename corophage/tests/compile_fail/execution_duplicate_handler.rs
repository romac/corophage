use corophage::prelude::*;

#[effect(usize)]
struct Ask;

fn program() -> Program<'static, Effects![Ask], impl std::future::Future<Output = usize>> {
    Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask).await })
}

fn assert_send(_: impl Send) {}

fn main() {
    let _ = program().handle(|_: Ask| Control::resume(1)).handle(|_: Ask| Control::resume(2));
}
