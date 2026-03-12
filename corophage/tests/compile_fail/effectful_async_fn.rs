use corophage::prelude::*;

#[effect(bool)]
struct Ask(i32);

#[effectful(Ask)]
async fn bad(x: i32) -> bool {
    yield_!(Ask(x))
}

fn main() {}
