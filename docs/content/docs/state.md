+++
title = "Shared State"
weight = 5
description = "Bind shared mutable state with with_state before execution."
+++

Bind shared mutable state with `.with_state(&mut state)`, then execute with `.run_sync()` or `.run().await`. Stateful handlers receive `&mut S` as their first argument. Handlers that do not use state can omit that argument.

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
    .with_state(&mut count)
    .handle(|s: &mut u64, _: Counter| {
        *s += 1;
        Control::resume(*s)
    })
    .run_sync();

assert_eq!(result, Ok(3));  // 1 + 2
assert_eq!(count, 2);       // handler was called twice
```

## Multiple effects with shared state

The program holds one state reference and reborrows it for each handler call. State-free handlers can share the same list:

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
    .with_state(&mut state)
    .handle(|Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(|s: &mut AppState, _: GetState<u64>| Control::resume(s.x))
    .handle(|s: &mut AppState, SetState(x)| {
        s.x = x;
        Control::resume(())
    })
    .run_sync();

assert_eq!(result, Ok(84));
assert_eq!(state.x, 84);
```

## Send-able async execution

`Program::new` and default `#[effectful]` functions retain their body's concrete type. The resulting runner is `Send` when all retained components permit it. Mutable handler access removes the blanket handler `Sync` bound. Handlers can retain their `&mut S` argument across awaits:

```rust
use corophage::prelude::*;

#[effect(u64)]
struct Counter;

async fn increment(state: &mut u64, _: Counter) -> Control<u64> {
    tokio::task::yield_now().await;
    *state += 1;
    Control::resume(*state)
}

let program = Program::new::<Effects![Counter], _>(|mut y| async move {
    let first = y.yield_(Counter).await;
    let second = y.yield_(Counter).await;
    first + second
})
.handle(increment);

let (result, state) = tokio::spawn(async move {
    let mut state = 0;
    let result = program.with_state(&mut state).run().await;
    (result, state)
})
.await
.unwrap();

assert_eq!(result, Ok(3));
assert_eq!(state, 2);
```

The state is owned by the spawned task, so its mutable borrow stays valid until execution finishes. Outside `tokio::spawn`, a sendable runner can also borrow state and effect data from the surrounding scope; it does not require `'static` data.

Ordinary async handlers can borrow the explicit state argument. For a closure that also borrows its captures across awaits, use `corophage::local_stateful(async |state: &mut S, effect| { ... })`. This adapter boxes one non-`Send` future per dispatch. The generic runner also accepts non-`Send` state and handler futures when awaited locally.

## State borrow duration

The mutable borrow starts at `.with_state(...)`. You cannot access the original state while the configured program remains live. Completion, cancellation, or dropping the configured program or its execution future releases that borrow. Handler calls are serial and receive fresh short reborrows; the state owner and handler captures need not have the same lifetime.

A handler cannot resume the computation with a reference borrowed from its temporary mutable state loan. A borrowed resume must instead refer to data that lives for the computation lifetime, such as external data carried by the effect.

The compatibility methods `.run_stateful(&mut state)` and `.run_sync_stateful(&mut state)` retain their original borrow timing: the state borrow starts at that terminal call. Erased sendable programs retain their old `Send + Sync` handler checks on `.run_stateful()`. Calling `.with_state()` selects the new conditional execution path, including for erased bodies.

Local erased `.run_stateful()` requires all handlers to be stateful async callables compatible with `AsyncFn`. Use `.with_state(...).run()` to mix handler kinds or use `local_stateful`. See the [compatibility API](@/docs/advanced.md#compatibility-api).

## Alternatives

If your handlers do not need shared state, omit `.with_state()`. Captured `RefCell` or other interior mutability remains an alternative when it fits the application.
