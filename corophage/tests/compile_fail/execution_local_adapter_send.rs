use corophage::prelude::*;

#[effect(usize)]
struct Ask;

fn program() -> Program<'static, Effects![Ask], impl std::future::Future<Output = usize>> {
    Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask).await })
}

fn assert_send(_: impl Send) {}

fn main() {
    let mut count = 0;
    let future = program().handle(corophage::local(async |_: Ask| {
        count += 1;
        Control::resume(count)
    })).run();
    assert_send(future);
}
