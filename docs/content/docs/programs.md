+++
title = "Programs"
weight = 3
description = "Build programs with the Program API for incremental handler attachment."
+++

A **Program** combines a computation with its effect handlers. This is the primary API for using corophage.

## Creating a program

Use `Program::new` to create a program from an async closure that receives a `Yielder`:

```rust
use corophage::prelude::*;

declare_effect!(Log(String) -> ());
declare_effect!(Counter -> u64);

type Effs = Effects![Log, Counter];

let program = Program::new(|y: Yielder<'_, Effs>| async move {
    y.yield_(Log("hello".into())).await;
    let n = y.yield_(Counter).await;
    n * 2
});
```

When you `await` the result of `y.yield_(some_effect)`, the computation pauses, the effect is handled, and the `await` resolves to the value provided by the handler.

## Attaching handlers

Handlers are attached one at a time with `.handle()`. Handlers can be attached in any order — the type system tracks which effects remain unhandled.

```rust
let result = program
    .handle(|Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(|_: Counter| Control::resume(42u64))
    .run_sync();

assert_eq!(result, Ok(84));
```

You can also attach multiple handlers at once with `.handle_all()`:

```rust
let result = Program::new(|y: Yielder<'_, Effects![Counter, Log]>| async move {
    y.yield_(Log("start".into())).await;
    y.yield_(Counter).await
})
.handle_all(hlist![
    |_: Counter| Control::resume(42u64),
    |Log(msg)| { println!("{msg}"); Control::resume(()) },
])
.run_sync();
```

## Running programs

Once all effects are handled, you can run the program:

- `.run_sync()` — execute synchronously, returns `Result<R, Cancelled>`
- `.run().await` — execute asynchronously
- `.run_sync_stateful(&mut state)` — synchronous with shared mutable state
- `.run_stateful(&mut state).await` — async with shared mutable state

## Partially-handled programs

A partially-handled program is a first-class value you can pass around, store, or extend later:

```rust
fn add_logging<Effs>(program: Program</* ... */>) -> Program</* ... */> {
    program.handle(|Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
}
```

The compiler enforces at the type level that you can only call `.run_sync()` when all effects have been handled — attempting to run a partially-handled program is a compile error.

## `Send`-able programs

For use with multi-threaded runtimes like tokio, use `Program::new_send`:

```rust
let program = Program::new_send(|y: Yielder<'_, Effs>| async move {
    y.yield_(Counter).await
});

// Can be spawned on tokio
tokio::spawn(async move {
    let result = program
        .handle(async |_: Counter| Control::resume(42u64))
        .run()
        .await;
});
```
