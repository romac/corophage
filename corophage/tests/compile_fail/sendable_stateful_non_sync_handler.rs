use std::cell::Cell;

use corophage::prelude::*;

#[effect(())]
struct Tick;

fn main() {
    let captured = Cell::new(0);
    let mut state = 0;
    let _future = Program::new_send::<Effects![Tick], _>(|mut y| async move {
        y.yield_(Tick).await;
    })
    // This future is Send, but the shared handler reference is not Send.
    .handle(move |state: &mut usize, _: Tick| {
        captured.set(captured.get() + 1);
        *state += 1;
        std::future::ready(Control::resume(()))
    })
    .run_stateful(&mut state);
}
