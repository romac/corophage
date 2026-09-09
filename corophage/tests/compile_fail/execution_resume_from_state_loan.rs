use corophage::prelude::*;

#[effect(&'r str)]
struct Ask;

fn borrow(state: &mut String, _: Ask) -> Control<&str> {
    Control::resume(state.as_str())
}

fn main() {
    let mut state = String::from("temporary loan");
    let _ = Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move {
        let first = y.yield_(Ask).await;
        y.yield_(Ask).await;
        first
    }).with_state(&mut state).handle(borrow).run_sync();
}
