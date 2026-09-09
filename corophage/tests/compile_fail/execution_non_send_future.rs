use corophage::prelude::*;

#[effect(usize)]
struct Ask;

fn program() -> Program<'static, Effects![Ask], impl std::future::Future<Output = usize>> {
    Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask).await })
}

fn assert_send(_: impl Send) {}

fn main() {
    let future = program().handle(|_: Ask| async {
        let local = std::rc::Rc::new(1);
        std::future::pending::<()>().await;
        Control::resume(*local)
    }).run();
    assert_send(future);
}
