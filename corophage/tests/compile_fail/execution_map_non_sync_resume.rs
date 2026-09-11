use std::cell::Cell;

use corophage::prelude::*;

#[effect(Cell<usize>)]
struct Ask;

fn assert_send(_: impl Send) {}

fn main() {
    // Mapping must retain the transport constraint even when no effect is yielded.
    let program = Program::new(|_: Yielder<'_, Effects![Ask]>| async {}).map(|()| 1);
    assert_send(program);
}
