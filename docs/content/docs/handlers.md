+++
title = "Handlers"
weight = 4
description = "Write sync and async handlers that implement your effects."
+++

A **Handler** is a function that implements the logic for a specific effect. It receives the effect instance and returns a `Control<R>`, which tells the runner what to do next.

## Control flow

`Control<R>` has two variants:

- `Control::resume(value)` — resumes the computation, passing `value` back as the result of `yield_`. The type of `value` must match the effect's `Resume` type.
- `Control::cancel()` — aborts the entire computation immediately. The final result will be `Err(Cancelled)`.

## Sync handlers

A sync handler is a regular closure:

```rust
|Log(msg)| {
    println!("LOG: {msg}");
    Control::resume(())
}
```

## Async handlers

An async handler is an async closure:

```rust
async |FileRead(file)| {
    let content = tokio::fs::read_to_string(file).await.unwrap();
    Control::resume(content)
}
```

## Named function handlers

Programs can mix sync and async handlers in one `.run()` call, whether their body is concrete or boxed. `.run_sync()` accepts only synchronous handlers, even if an async handler would complete immediately.

Handlers can also be named functions. When using stateful handlers, the state is passed as the first argument:

```rust
struct AppState { verbose: bool }

fn log(s: &mut AppState, Log(msg): Log) -> Control<()> {
    if s.verbose {
        println!("LOG: {msg}");
    }
    Control::resume(())
}

fn file_read(_: &mut AppState, FileRead(file): FileRead) -> Control<String> {
    println!("Reading file: {file}");
    Control::resume("file content".to_string())
}
```

## Lending async closures

Ordinary async handlers return a future through `FnMut(...) -> Future`. When an async closure borrows its captures across an await, wrap it with `corophage::local`:

```rust
let mut messages = Vec::new();
let result = program
    .handle(corophage::local(async |Log(message)| {
        messages.push(message);
        tokio::task::yield_now().await;
        Control::resume(())
    }))
    .run()
    .await;
```

Repeated calls borrow the same closure and retain its mutations. The adapter boxes one non-`Send` future per call. Use `corophage::local_stateful` for a lending closure that also takes a `&mut S` state argument. A runner using either adapter must be awaited locally. Ordinary handlers do not box dispatch futures.

## Cancellation

Use `Control::cancel()` when an effect should abort the entire computation:

```rust
#[effect(Never)]
struct Cancel;

// The handler cancels the computation
|_: Cancel| Control::cancel()
```

The `Never` resume type indicates this effect can never resume — the handler must always cancel.

## Throwing errors

`Control::cancel()` does not record why the computation stopped. To abort with a typed error, yield the built-in `Throw<E>` effect and catch it when running:

```rust
#[effect(Option<String>)]
struct Lookup(u64);

#[derive(Debug)]
struct NotFound(u64);

#[effectful(Lookup, Throw<NotFound>)]
fn find_user(id: u64) -> String {
    match yield_!(Lookup(id)) {
        Some(user) => user,
        None => throw!(NotFound(id)),
    }
}

let result: Result<Result<String, NotFound>, Cancelled> = find_user(42)
    .handle(|_: Lookup| Control::resume(None))
    .catch()
    .run_sync();
```

`.catch()` handles `Throw<E>` and changes what the run returns: `Ok(Ok(value))` on completion, `Ok(Err(error))` when the program throws, and `Err(Cancelled)` when a handler cancels. Use `?` to propagate the cancellation and keep the error: `find_user(42).handle(...).catch().run_sync()?` has type `Result<String, NotFound>`. Both `.run()` and `.run_sync()` are available after `.catch()`.

Inside `#[effectful]` functions, `throw!(error)` can stand in for a value of any type. In a `Program::new` closure, use `y.throw(error).await`. In statement position, write `return y.throw(error).await` so the type can be inferred.

Attach the other handlers and bind state before calling `.catch()`. A program catches one error type; use an enum when several errors are possible. A throw from an invoked sub-program aborts the whole computation, and catching inside a program body is not supported yet. Without `.catch()`, `Throw<E>` is an ordinary effect that you can handle like any other.
