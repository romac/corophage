use corophage::prelude::*;

// An explicit lifetime selects a Rust declaration. Add `<'a>` to this function
// or omit the attribute lifetime to request inference.
#[effectful('a)]
fn length(value: &str) -> usize {
    value.len()
}

fn main() {}
