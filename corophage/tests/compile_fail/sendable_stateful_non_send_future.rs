use std::rc::Rc;

use corophage::prelude::*;

#[effect(())]
struct Tick;

async fn handle(state: &mut usize, _: Tick) -> Control<()> {
    // The handler itself is Send + Sync, but its suspended future is not Send.
    let value = Rc::new(1);
    std::future::pending::<()>().await;
    *state += *value;
    Control::resume(())
}

fn main() {
    let mut state = 0;
    let _future = Program::new_send::<Effects![Tick], _>(|mut y| async move {
        y.yield_(Tick).await;
    })
    .handle(handle)
    .run_stateful(&mut state);
}
