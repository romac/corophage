use corophage::coroutine::Co;
use corophage::prelude::*;

#[effect(&'static str)]
struct Ask(&'static str);

fn main() {
    type Effs = Effects![Ask];

    let _: Co<'_, Effs, ()> = Co::new(|mut yielder| async move {
        let first = yielder.yield_(Ask("first"));
        let second = yielder.yield_(Ask("second"));
        let _ = (first.await, second.await);
    });
}
