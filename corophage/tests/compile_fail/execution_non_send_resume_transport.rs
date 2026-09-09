use std::rc::Rc;

use corophage::prelude::*;

#[effect(Rc<usize>)]
struct Ask;

fn assert_send(_: &impl Send) {}

fn main() {
    let future = Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move {
        drop(y.yield_(Ask).await);
    })
    .handle(|_: Ask| Control::resume(Rc::new(1)))
    .run();
    assert_send(&future);
}
