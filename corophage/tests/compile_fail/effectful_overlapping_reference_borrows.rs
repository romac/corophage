use corophage::prelude::*;

#[effectful]
fn advance(value: &mut &str) {
    *value = &value[1..];
}

fn main() {
    let mut remaining = "hello";
    let first = advance(&mut remaining);
    // Independent reference lifetimes must still prevent overlapping programs
    // from holding mutable access to the same variable.
    let second = advance(&mut remaining);
    first.run_sync().unwrap();
    second.run_sync().unwrap();
}
