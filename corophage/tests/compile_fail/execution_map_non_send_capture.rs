use std::rc::Rc;

use corophage::prelude::*;

fn assert_send(_: impl Send) {}

fn main() {
    let capture = Rc::new(1);
    let program = Program::new(|_: Yielder<'_, Effects![]>| async { 2 })
        .map(move |value| value + *capture);
    assert_send(program.run());
}
