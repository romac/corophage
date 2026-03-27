use corophage::prelude::*;

#[effect(bool)]
struct Ask(i32);

#[effect(())]
struct Log(String);

type AskEff = Effects![Ask];

#[effectful(...AskEff, Log)]
fn bad() -> bool {
    yield_!(Ask(42))
}

fn main() {}
