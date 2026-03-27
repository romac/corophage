+++
title = "Getting Started"
weight = 1
description = "Install corophage and run your first effectful program."
+++

## Installation

Add `corophage` to your `Cargo.toml`:

```toml
[dependencies]
corophage = "0.3.2"
```

corophage requires **Rust 1.85** or later (stable).

## What are effect handlers?

Imagine you are writing a piece of business logic:

1. Log a "starting" message.
2. Read some configuration from a file.
3. Get the current application state.
4. Perform a calculation and update the state.
5. If a condition is met, cancel the entire operation.

Traditionally, you'd write a function that takes a logger, a file system handle, and a mutable reference to the state. This function would be tightly coupled to these specific implementations.

With effect handlers, your business logic function does none of these things directly. Instead, it *describes* the side effects it needs to perform by `yield`ing **effects**.

The responsibility of *implementing* these effects is given to a set of **handlers**. You provide these handlers to a runner, which executes your logic and calls the appropriate handler whenever an effect is yielded.

## Your first program

```rust
use corophage::prelude::*;

// 1. Declare effects
#[effect(())]
struct Log(String);

#[effect(String)]
struct FileRead(String);

// 2. Write the computation
#[effectful(Log, FileRead)]
fn my_program() -> String {
    yield_!(Log("Starting...".into()));
    let config = yield_!(FileRead("config.toml".into()));
    config
}

// 3. Attach handlers and run
let result = my_program()
    .handle(|Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(|FileRead(f)| {
        Control::resume(format!("contents of {f}"))
    })
    .run_sync();

assert_eq!(result, Ok("contents of config.toml".to_string()));
```

Let's walk through each step.

### Step 1: Declare effects

```rust
#[effect(())]
struct Log(String);

#[effect(String)]
struct FileRead(String);
```

An effect is a plain struct annotated with `#[effect(ResumeType)]`. The struct's fields carry the request data — `Log` carries the message to log, `FileRead` carries the path to read.

The attribute argument defines what the handler sends back. `Log` resumes with `()` because logging doesn't produce a value. `FileRead` resumes with a `String` — the file's contents.

### Step 2: Write the computation

```rust
#[effectful(Log, FileRead)]
fn my_program() -> String {
    yield_!(Log("Starting...".into()));
    let config = yield_!(FileRead("config.toml".into()));
    config
}
```

The `#[effectful(Eff1, Eff2, ...)]` macro marks a function as an effectful computation. Inside the function body, use `yield_!(effect)` to perform an effect — this pauses execution, hands the effect to its handler, and resumes with the handler's return value.

The computation doesn't know *how* logging or file reading work. It just describes what it needs.

### Step 3: Attach handlers and run

```rust
let result = my_program()
    .handle(|Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(|FileRead(f)| {
        Control::resume(format!("contents of {f}"))
    })
    .run_sync();
```

Calling `my_program()` returns a `Program` — a description of the computation, not the result. Each `.handle()` call attaches a handler for one effect. Handlers can be attached in any order.

Once all effects are handled, `.run_sync()` executes the computation and returns a `Result<R, Cancelled>`. If you try to call `.run_sync()` before all effects are handled, you'll get a compile error.

### Manual alternative

You can also create programs directly with `Program::new` instead of `#[effectful]`:

```rust
type MyEffects = Effects![Log, FileRead];

let program = Program::new(|y: Yielder<'_, MyEffects>| async move {
    y.yield_(Log("Starting...".into())).await;
    let config = y.yield_(FileRead("config.toml".into())).await;
    config
});
```

This is useful when you need more control over the closure (e.g., capturing specific variables by reference).

## Benefits

- **Testability**: Swap in mock handlers for testing without touching the real world.
- **Modularity**: The core logic is completely decoupled from its execution context. Run the same logic with different handlers for different environments.
- **Clarity**: Business logic becomes a pure, high-level description of the steps involved, making it easier to read and reason about.

> corophage provides **single-shot** effect handlers: each handler can resume the computation at most once. This is a deliberate design choice that keeps the implementation efficient and compatible with Rust's ownership model.
