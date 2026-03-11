use corophage::prelude::*;

#[effect(u64)]
struct Get;

#[effect(())]
struct Put(u64);

#[effectful(Get, Put)]
fn increment() -> u64 {
    let x = yield_!(Get);
    yield_!(Put(x + 1));
    x + 1
}

#[effectful(Get, Put)]
fn increment_twice() -> u64 {
    invoke!(increment());
    invoke!(increment())
}

#[inline(never)]
fn run_composed() -> u64 {
    let mut state: u64 = 0;
    increment_twice()
        .handle(|s: &mut u64, _: Get| Control::resume(*s))
        .handle(|s: &mut u64, Put(x)| {
            *s = x;
            Control::resume(())
        })
        .run_sync_stateful(&mut state)
        .unwrap()
}

fn main() {
    let result = run_composed();
    println!("Result: {result}");
    assert_eq!(result, 2);
}
