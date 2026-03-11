+++
title = "Programs"
weight = 3
description = "Build programs with the Program API for incremental handler attachment."
+++

A **Program** combines a computation with its effect handlers. This is the primary API for using corophage.

## Creating a program with `#[effectful]`

The simplest way to create a program is with the `#[effectful]` attribute macro:

```rust
use corophage::prelude::*;

#[effect(())]
struct Log(String);

#[effect(u64)]
struct Counter;

#[effectful(Log, Counter)]
fn my_program() -> u64 {
    yield_!(Log("hello".into()));
    let n = yield_!(Counter);
    n * 2
}
```

The `#[effectful(Eff1, Eff2, ...)]` macro:
- Transforms the return type to `Eff<'_, Effects![Eff1, Eff2, ...], T>`
- Wraps the body in `Program::new`
- Enables `yield_!(effect)` syntax to perform effects

### Lifetime handling

If your effects borrow data, the macro infers the lifetime automatically when the function has exactly one lifetime parameter:

```rust
#[effectful(Log<'a>)]
fn log_msg<'a>(msg: &'a str) -> () {
    yield_!(Log(msg));
}
```

With multiple lifetime parameters, specify the effect lifetime explicitly as the first argument:

```rust
#[effectful('a, Log<'a>)]
fn log_msg<'a, 'b>(msg: &'a str, _other: &'b str) -> () {
    yield_!(Log(msg));
}
```

### `Send`-able programs

Add `send` to the attribute to create a `Send`-able program (for use with `tokio::spawn`):

```rust
#[effectful(Counter, send)]
fn my_send_program() -> u64 {
    yield_!(Counter)
}
```

## Creating a program with `Program::new`

You can also create programs manually with `Program::new`, which takes an async closure that receives a `Yielder`:

```rust
use corophage::prelude::*;

#[effect(())]
struct Log(String);

#[effect(u64)]
struct Counter;

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

For use with multi-threaded runtimes like tokio, use `#[effectful(..., send)]` or `Program::new_send`:

```rust
#[effectful(Counter, send)]
fn my_program() -> u64 {
    yield_!(Counter)
}

// Can be spawned on tokio
tokio::spawn(async move {
    let result = my_program()
        .handle(async |_: Counter| Control::resume(42u64))
        .run()
        .await;
});
```

Or with the manual API:

```rust
let program = Program::new_send(|y: Yielder<'_, Effs>| async move {
    y.yield_(Counter).await
});

tokio::spawn(async move {
    let result = program
        .handle(async |_: Counter| Control::resume(42u64))
        .run()
        .await;
});
```
