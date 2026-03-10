+++
title = "Getting Started"
weight = 1
description = "Install corophage and run your first effectful program."
+++

## Installation

Add `corophage` to your `Cargo.toml`:

```toml
[dependencies]
corophage = "0.2.0"
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
declare_effect!(Log(String) -> ());
declare_effect!(FileRead(String) -> String);

// 2. Define the effect set
type MyEffects = Effects![Log, FileRead];

// 3. Write the computation
let result = Program::new(|y: Yielder<'_, MyEffects>| async move {
    y.yield_(Log("Starting...".into())).await;
    let config = y.yield_(FileRead("config.toml".into())).await;
    config
})
// 4. Attach handlers
.handle(|Log(msg)| {
    println!("{msg}");
    Control::resume(())
})
.handle(|FileRead(f)| {
    Control::resume(format!("contents of {f}"))
})
// 5. Run
.run_sync();

assert_eq!(result, Ok("contents of config.toml".to_string()));
```

## Benefits

- **Testability**: Swap in mock handlers for testing without touching the real world.
- **Modularity**: The core logic is completely decoupled from its execution context. Run the same logic with different handlers for different environments.
- **Clarity**: Business logic becomes a pure, high-level description of the steps involved, making it easier to read and reason about.

> corophage provides **single-shot** effect handlers: each handler can resume the computation at most once. This is a deliberate design choice that keeps the implementation efficient and compatible with Rust's ownership model.
