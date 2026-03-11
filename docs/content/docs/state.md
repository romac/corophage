+++
title = "Shared State"
weight = 5
description = "Share mutable state across handlers with run_stateful."
+++

Handlers can share mutable state via `run_sync_stateful` / `run_stateful`. The state is passed as a `&mut S` first argument to every handler.

## Basic example

```rust
use corophage::prelude::*;

#[effect(u64)]
struct Counter;

#[effectful(Counter)]
fn count_up() -> u64 {
    let a = yield_!(Counter);
    let b = yield_!(Counter);
    a + b
}

let mut count: u64 = 0;

let result = count_up()
    .handle(|s: &mut u64, _: Counter| {
        *s += 1;
        Control::resume(*s)
    })
    .run_sync_stateful(&mut count);

assert_eq!(result, Ok(3));  // 1 + 2
assert_eq!(count, 2);       // handler was called twice
```

## Multiple effects with shared state

All handlers in a stateful run share the same `&mut S`:

```rust
use corophage::prelude::*;
use std::marker::PhantomData;

#[effect(())]
pub struct Log<'a>(pub &'a str);

#[derive(Default)]
pub struct GetState<S> {
    _marker: PhantomData<S>,
}
impl<S: Send + Sync> Effect for GetState<S> {
    type Resume<'r> = S;
}

pub struct SetState<S>(pub S);
impl<S> Effect for SetState<S> {
    type Resume<'r> = ();
}

#[effectful(Log<'static>, GetState<u64>, SetState<u64>)]
fn my_program() -> u64 {
    yield_!(Log("starting"));
    let val = yield_!(GetState::default());
    yield_!(SetState(val * 2));
    yield_!(GetState::default())
}

#[derive(Debug)]
struct AppState { x: u64 }

let mut state = AppState { x: 42 };

let result = my_program()
    .handle(|_s: &mut AppState, Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(|s: &mut AppState, _: GetState<u64>| Control::resume(s.x))
    .handle(|s: &mut AppState, SetState(x)| {
        s.x = x;
        Control::resume(())
    })
    .run_sync_stateful(&mut state);

assert_eq!(result, Ok(84));
assert_eq!(state.x, 84);
```

## Alternatives

If your handlers don't need shared state, use `.run_sync()` / `.run()` instead. You can also use `RefCell` or other interior mutability patterns to share state without `run_stateful`.
