use std::cell::Cell;

use corophage::prelude::*;

#[effect(Cell<usize>)]
struct Ask;

fn assert_send(_: &impl Send) {}

fn main() {
    // Cell is Send, but the shared token transport also requires Sync. Check
    // this before polling, even when the body never actually yields Ask.
    let program = Program::new(|_: Yielder<'_, Effects![Ask]>| async {});
    assert_send(&program);
}
