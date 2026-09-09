use corophage::prelude::*;

#[effect(usize)]
struct Ask;

fn program() -> Program<'static, Effects![Ask], impl std::future::Future<Output = usize>> {
    Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask).await })
}

fn assert_send(_: impl Send) {}

fn main() {
    let mut state = 1;
    let configured = program().with_state(&mut state);
    state += 1;
    drop(configured);
}
