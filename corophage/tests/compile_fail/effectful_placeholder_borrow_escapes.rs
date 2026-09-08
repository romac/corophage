use std::borrow::Cow;

use corophage::prelude::*;

#[effectful]
fn length(value: Cow<'_, str>) -> usize {
    value.len()
}

fn main() {
    let program;
    {
        let value = String::from("borrowed");
        program = length(Cow::Borrowed(&value));
    }
    // The captured borrow must remain valid until the program runs or is dropped.
    program.run_sync().unwrap();
}
