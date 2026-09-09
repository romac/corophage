use corophage::prelude::*;

#[effect(usize)]
struct Ask;

fn program() -> Program<'static, Effects![Ask], impl std::future::Future<Output = usize>> {
    Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask).await })
}

fn assert_send(_: impl Send) {}

fn main() {
    let capture = std::rc::Rc::new(1);
    let future = program().handle(move |_: Ask| Control::resume(*capture)).run();
    assert_send(future);
}
