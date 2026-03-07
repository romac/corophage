# Corophage

[![Crates.io](https://img.shields.io/crates/v/corophage.svg)](https://crates.io/crates/corophage)
[![Docs.rs](https://docs.rs/corophage/badge.svg)](https://docs.rs/corophage)
![Stable Rust](https://img.shields.io/badge/rust-stable-orange)
[![Coverage](https://codecov.io/github/romac/corophage/graph/badge.svg?token=U8FVD3HT2X)](https://codecov.io/github/romac/corophage)

**An effect handler library for Rust**

`corophage` provides a way to separate the *description* of what your program should do from the *implementation* of how it gets done. This allows you to write clean, testable, and composable business logic.

## Usage

Add `corophage` to your `Cargo.toml`:

```toml
[dependencies]
corophage = "0.1.0"
```

## What are effect handlers?

Imagine you are writing a piece of business logic:

1.  Log a "starting" message.
2.  Read some configuration from a file.
3.  Get the current application state.
4.  Perform a calculation and update the state.
5.  If a condition is met, cancel the entire operation.

Traditionally, you might write a function that takes a logger, a file system handle, and a mutable reference to the state. This function would be tightly coupled to these specific implementations.

With effect handlers, your business logic function does none of these things directly. Instead, it *describes* the side effects it needs to perform by `yield`ing **effects**.

```rust,ignore
use corophage::prelude::*;

type MyEffects = Effects![Log, FileRead, GetState, SetState];

// This describes WHAT to do, not HOW.
let result = Program::new(|y: Yielder<'_, MyEffects>| async move {
    y.yield_(Log("Starting...")).await;
    let config = y.yield_(FileRead("config.toml")).await;
    let state = y.yield_(GetState).await;
    // ...and so on
})
.handle(|Log(msg)| { println!("{msg}"); CoControl::resume(()) })
.handle(|FileRead(f)| CoControl::resume(std::fs::read_to_string(f).unwrap()))
.handle(|_: GetState| CoControl::resume(42u64))
.handle(|SetState(x)| { /* ... */ CoControl::resume(()) })
.run_sync();
```

The responsibility of *implementing* these effects (e.g., actually printing to the console, reading from the disk, or managing state) is given to a set of **handlers**. You provide these handlers to a runner, which executes your logic and calls the appropriate handler whenever an effect is yielded.

This separation provides powerful benefits:
*   **Testability**: For tests, you can provide mock handlers that simulate logging, file I/O, or state management without touching the real world.
*   **Modularity**: The core logic is completely decoupled from its execution context. You can run the same logic with different handlers for different environments (e.g., production vs. testing, CLI vs. GUI).
*   **Clarity**: The business logic code becomes a pure, high-level description of the steps involved, making it easier to read and reason about.

> [!NOTE]
> `corophage` provides **single-shot** effect handlers: each handler can resume the computation at most once. This means handlers cannot duplicate or replay continuations. This is a deliberate design choice that keeps the implementation efficient and compatible with Rust's ownership model.

## Core concepts

`corophage` is built around a few key concepts: Effects, Computations, and Handlers.

### 1. Effects

An **Effect** is a struct that represents a request for a side effect. It's a message from your computation to the outside world. To define an effect, you implement the `Effect` trait.

The most important part of the `Effect` trait is the associated type `Resume<'r>` (a generic associated type), which defines the type of the value that the computation will receive back after the effect is handled.

```rust,ignore
use corophage::{Effect, Never};

// An effect to request logging a message.
// It doesn't need any data back, so we resume with `()`.
pub struct Log<'a>(pub &'a str);
impl<'a> Effect for Log<'a> {
    type Resume<'r> = ();
}

// An effect to request reading a file.
// It expects the file's contents back, so we resume with `String`.
pub struct FileRead(pub String);
impl Effect for FileRead {
    type Resume<'r> = String;
}

// An effect that cancels the computation.
// It will never resume, so we use the special `Never` type.
pub struct Cancel;
impl Effect for Cancel {
    type Resume<'r> = Never;
}
```

### 2. Programs

A **Program** combines a computation with its effect handlers. You create one with `Program::new`, which takes an async closure that receives a `Yielder` — the interface for performing effects.

When you `await` the result of `yielder.yield_(some_effect)`, the computation pauses, the effect is handled, and the `await` resolves to the value provided by the handler (which must match the effect's `Resume<'r>` type).

You attach handlers one at a time with `.handle()`, in the same order as the effects in `Effects![...]`. Once all effects are handled, you can run the program.

```rust,ignore
use corophage::prelude::*;

type Effs = Effects![Log<'static>, FileRead];

let result = Program::new(|y: Yielder<'_, Effs>| async move {
    y.yield_(Log("fetching...")).await;
    y.yield_(FileRead("data.txt".to_string())).await
})
.handle(|Log(msg)| { println!("{msg}"); CoControl::resume(()) })
.handle(|FileRead(path)| CoControl::resume(format!("contents of {path}")))
.run_sync();

assert_eq!(result, Ok("contents of data.txt".to_string()));
```

A free function `handle` is available in the `program` module as an alternative style:

```rust,ignore
use corophage::program::handle;

let p = Program::new(|y: Yielder<'_, Effs>| async move { /* ... */ });
let p = handle(p, |Log(msg)| { println!("{msg}"); CoControl::resume(()) });
let p = handle(p, |FileRead(path)| CoControl::resume(format!("contents of {path}")));
let result = p.run_sync();
```

> [!IMPORTANT]
> Handlers must be attached in the same order as the effects appear in the `Effects![...]` list. This is enforced by the type system — attaching handlers in the wrong order is a compile error.

### 3. Handlers

A **Handler** is a function (sync or async) that implements the logic for a specific effect. It receives the `Effect` instance and returns a `CoControl`, which tells the runner what to do next:

*   `CoControl::resume(value)`: Resumes the computation, passing `value` back as the result of the `yield_`. The type of `value` must match the effect's `Resume` type.
*   `CoControl::cancel()`: Aborts the entire computation immediately. The final result of the run will be `Err(Cancelled)`.

```rust,ignore
// Sync handler — a regular closure
|Log(msg)| {
    println!("LOG: {msg}");
    CoControl::resume(())
}

// Async handler — an async closure
async |FileRead(file)| {
    let content = tokio::fs::read_to_string(file).await.unwrap();
    CoControl::resume(content)
}
```

### 4. Shared state

Handlers can share mutable state via `run_sync_stateful` / `run_stateful`. The state is passed as a `&mut S` first argument to every handler:

```rust,ignore
let mut count: u64 = 0;

let result = Program::new(|y: Yielder<'_, Effects![Counter]>| async move {
    let a = y.yield_(Counter).await;
    let b = y.yield_(Counter).await;
    a + b
})
.handle(|s: &mut u64, _: Counter| {
    *s += 1;
    CoControl::resume(*s)
})
.run_sync_stateful(&mut count);

assert_eq!(result, Ok(3));
assert_eq!(count, 2);
```

> [!NOTE]
> If your handlers don't need shared state, use `.run_sync()` / `.run()` instead. You can also use `RefCell` or other interior mutability patterns to share state without `run_stateful`.

## Advanced: `Co`, `CoSend`, and the direct API

For cases where you need to pass a computation around before attaching handlers (e.g., returning it from a function, or storing it in a data structure), you can use `Co` and `CoSend` directly.

```rust,ignore
use corophage::{Co, CoSend, Effects, sync, CoControl};
use corophage::frunk::hlist;

// Co — the computation type (not Send)
let co: Co<'_, Effects![FileRead], String> = Co::new(|y| async move {
    y.yield_(FileRead("data.txt".to_string())).await
});

// Run directly with all handlers at once via hlist
let result = sync::run(co, &mut hlist![
    |FileRead(f)| CoControl::resume(format!("contents of {f}"))
]);

// Or wrap in a Program for incremental handling
let result = Program::from_co(co).handle(/* ... */).run_sync();
```

`CoSend` is the `Send`-able variant, for use with multi-threaded runtimes:

```rust,ignore
fn my_computation() -> CoSend<'static, Effects![FileRead], String> {
    CoSend::new(|y| async move {
        y.yield_(FileRead("test".to_string())).await
    })
}

// Can be spawned on tokio
tokio::spawn(async move {
    let result = Program::from_co(my_computation())
        .handle(async |FileRead(f)| CoControl::resume(format!("contents of {f}")))
        .run()
        .await;
});
```

The direct API (`sync::run`, `sync::run_stateful`, `run`, `run_stateful`) accepts a `Co`/`CoSend` and an `hlist!` of all handlers at once. This is useful for concise one-shot execution but requires providing all handlers together.

#### Borrowing non-`'static` data

Effects can borrow data from the local scope by using a non-`'static` lifetime:

```rust,ignore
use corophage::prelude::*;

struct Log<'a>(pub &'a str);
impl<'a> Effect for Log<'a> {
    type Resume<'r> = ();
}

let msg = String::from("hello from a local string");
let msg_ref = msg.as_str();

let result = Program::new(move |y: Yielder<'_, Effects![Log<'_>]>| async move {
    y.yield_(Log(msg_ref)).await;
})
.handle(|Log(m)| { println!("{m}"); CoControl::resume(()) })
.run_sync();

assert_eq!(result, Ok(()));
```

#### Borrowed resume types

Because `Effect::Resume<'r>` is a generic associated type (GAT), handlers can resume computations with *borrowed* data instead of requiring owned values.

```rust,ignore
use corophage::prelude::*;
use std::collections::HashMap;

struct Lookup<'a> {
    map: &'a HashMap<String, String>,
    key: &'a str,
}

impl<'a> Effect for Lookup<'a> {
    // The handler can resume with a &str borrowed from the map
    type Resume<'r> = &'r str;
}

let map = HashMap::from([
    ("host".to_string(), "localhost".to_string()),
    ("port".to_string(), "5432".to_string()),
]);

let result = Program::new({
    let map = &map;
    move |y: Yielder<'_, Effects![Lookup<'_>]>| async move {
        let host: &str = y.yield_(Lookup { map, key: "host" }).await;
        let port: &str = y.yield_(Lookup { map, key: "port" }).await;
        format!("{host}:{port}")
    }
})
.handle(|Lookup { map, key }| {
    let value = map.get(key).unwrap();
    CoControl::resume(value.as_str())
})
.run_sync();

assert_eq!(result, Ok("localhost:5432".to_string()));
```

## Performance

Benchmarks were run using [Divan](https://github.com/nvzqz/divan). Run them with `cargo bench`.

### Coroutine Overhead

| Benchmark | Median | Notes |
|-----------|--------|-------|
| `coroutine_creation` | ~7 ns | Just struct initialization |
| `empty_coroutine` | ~30 ns | Full lifecycle with no yields |
| `single_yield` | ~38 ns | One yield/resume cycle |

Coroutine creation is nearly free, and the baseline overhead for running a coroutine is ~30 ns.

### Yield Scaling (Sync vs Async)

| Yields | Sync | Async | Overhead |
|--------|------|-------|----------|
| 10 | 131 ns | 178 ns | +36% |
| 100 | 1.0 µs | 1.27 µs | +27% |
| 1000 | 9.5 µs | 11.1 µs | +17% |

Async adds ~30% overhead at small scales, but the gap narrows as yield count increases. Per-yield cost is approximately **9-10 ns** for sync and **11 ns** for async.

### Effect Dispatch Position

| Position | Median |
|----------|--------|
| First (index 0) | 49 ns |
| Middle (index 2) | 42 ns |
| Last (index 4) | 47 ns |

Dispatch position has negligible impact. The coproduct-based dispatch is effectively O(1).

### State Management

| Pattern | Median |
|---------|--------|
| Stateless (`run`) | 38 ns |
| Stateful (`run_stateful`) | 53 ns |
| RefCell pattern | 55 ns |

Stateful handlers add ~15 ns overhead. RefCell is nearly equivalent to `run_stateful`.

### Handler Complexity

| Handler | Median |
|---------|--------|
| Noop (returns `()`) | 42 ns |
| Allocating (returns `String`) | 83 ns |

Allocation dominates handler cost. Consider returning references or zero-copy types for performance-critical effects.


## Acknowledgments

`corophage` is heavily inspired by [`effing-mad`](https://github.com/rosefromthedead/effing-mad), a pioneering algebraic effects library for nightly Rust. 
`effing-mad` demonstrated that algebraic effects and effect handlers are viable in Rust by leveraging coroutines to let effectful functions suspend, pass control to their callers, and resume with results.
While `effing-mad` requires nightly Rust for its `#[coroutine]`-based approach, `corophage` supports stable Rust by leveraging async coroutines (via [`fauxgen`](https://github.com/jmkr/fauxgen)). Big thanks as well to [`frunk`](https://github.com/lloydmeta/frunk) for its coproduct and hlist implementation.

## License

Licensed under either of

 * Apache License, Version 2.0 (<http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license (<http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
