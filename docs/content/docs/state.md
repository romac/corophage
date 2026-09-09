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
#[effect(S)]
pub struct GetState<S> {
    _marker: PhantomData<S>,
}

#[effect(())]
pub struct SetState<S>(pub S);

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

## Send-able async execution

Programs created with `Program::new_send` or `#[effectful(..., send)]` can run with shared state inside `tokio::spawn`. The state and result must be `Send`, handlers must be `Send + Sync`, and each handler future must be `Send`. Handlers can retain their `&mut S` argument across awaits:

```rust
use corophage::prelude::*;

#[effect(u64)]
struct Counter;

async fn increment(state: &mut u64, _: Counter) -> Control<u64> {
    tokio::task::yield_now().await;
    *state += 1;
    Control::resume(*state)
}

let program = Program::new_send::<Effects![Counter], _>(|mut y| async move {
    let first = y.yield_(Counter).await;
    let second = y.yield_(Counter).await;
    first + second
})
.handle(increment);

let (result, state) = tokio::spawn(async move {
    let mut state = 0;
    let result = program.run_stateful(&mut state).await;
    (result, state)
})
.await
.unwrap();

assert_eq!(result, Ok(3));
assert_eq!(state, 2);
```

The state is owned by the spawned task, so its mutable borrow stays valid until execution finishes. Outside `tokio::spawn`, a sendable runner can also borrow state and effect data from the surrounding scope; it does not require `'static` data.

Sendable stateful handlers use the same restriction on captured data as sendable stateless handlers: a returned future can borrow the explicit state argument, but must not borrow from the closure's captures. Use an `async fn` as above, or clone captured data into an owned future. Use `Program::new` for local execution with lending captures, non-`Send` state, or non-`Send` handler futures.

## Alternatives

If your handlers don't need shared state, use `.run_sync()` / `.run()` instead. You can also use `RefCell` or other interior mutability patterns to share state without `run_stateful`.
