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

An async handler is an async closure (requires Rust 1.85+):

```rust
async |FileRead(file)| {
    let content = tokio::fs::read_to_string(file).await.unwrap();
    Control::resume(content)
}
```

## Named function handlers

Handlers can also be named functions. When using stateful handlers, the state is passed as the first argument:

```rust
struct AppState { verbose: bool }

fn log(s: &mut AppState, Log(msg): Log<'_>) -> Control<()> {
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

## Cancellation

Use `Control::cancel()` when an effect should abort the entire computation:

```rust
#[effect(Never)]
struct Cancel;

// The handler cancels the computation
|_: Cancel| Control::cancel()
```

The `Never` resume type indicates this effect can never resume — the handler must always cancel.
