use corophage::prelude::*;

#[effect(usize)]
struct Ask;

fn main() {
    let _ = Program::new(|mut y: Yielder<'_, Effects![Ask]>| async move { y.yield_(Ask).await })
        .map(|value| value + 1)
        .run_sync();
}
