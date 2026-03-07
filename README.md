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
// This function describes WHAT to do, not HOW.
pub fn my_logic() -> Co<'static, Effects![Log, FileRead, GetState, SetState], ()> {
    Co::new(|yielder| async move {
        yielder.yield_(Log("Starting...")).await;
        let config = yielder.yield_(FileRead("config.toml")).await;
        let state = yielder.yield_(GetState).await;
        // ...and so on
    })
}
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

### 2. Computations (`Co` and `CoSend`)

A **Computation** is a piece of logic that can yield effects. It is represented by the `Co<'a, E, T>` type, where `'a` is the lifetime bound for the effects, `E` is a list of possible effects, and `T` is the final return value.

The lifetime `'a` controls how long the effects (and any data they borrow) must live. Use `'static` when your effects own all their data, or a shorter lifetime when effects need to borrow from the local scope.

Both `Co` and `CoSend` are type aliases for `GenericCo<'a, Effs, Return, L>`, parameterized by a `Locality` marker type:
- **`Co`** (uses `Local`) - the default, not `Send`. Use this when your coroutine doesn't need to cross thread boundaries.
- **`CoSend`** (uses `Sendable`) - `Send`-able. Use this when you need to spawn the coroutine on a multi-threaded executor like `tokio::spawn`.

You create a computation with `Co::new` (or `CoSend::new`), which takes an `async` closure. This closure receives a `Yielder` argument, which you use to perform effects with `yielder.yield_(...)`.

When you `await` the result of `yielder.yield_(some_effect)`, the computation pauses, the effect is handled by its corresponding handler, and the `await` resolves to the value provided by the handler (which must match the effect's `Resume<'r>` type).

```rust,ignore
use corophage::{Co, Effects};

// A type alias for all the effects our computation can perform.
// The `Effects!` macro creates a type-level list of effects.
pub type MyEffects = Effects![Cancel, Log<'static>, FileRead, GetState<u64>, SetState<u64>];

// This function defines a computation.
pub fn co() -> Co<'static, MyEffects, ()> {
    Co::new(|yielder| async move {
        // Yield a Log effect and wait for it to be handled.
        // The await resolves to `()`, as defined in `Log::Resume`.
        let () = yielder.yield_(Log("Hello, world!")).await;

        // Yield a FileRead effect.
        // The await resolves to a `String`.
        let text = yielder.yield_(FileRead("example.txt".to_string())).await;
        println!("Read file: {text}");

        // ... and so on for other effects.

        // This effect will never resume.
        yielder.yield_(Cancel).await;

        // This line is never reached.
        println!("Cancelled!");
    })
}
```

#### Borrowing non-`'static` data

Effects can borrow data from the local scope by using a non-`'static` lifetime:

```rust,ignore
use corophage::{Co, Effects, sync, CoControl};
use corophage::frunk::hlist;

struct Log<'a>(pub &'a str);
impl<'a> Effect for Log<'a> {
    type Resume<'r> = ();
}

let msg = String::from("hello from a local string");
let msg_ref = msg.as_str(); // a non-'static reference

// The lifetime on `Co` ties the computation to `msg_ref`'s lifetime.
let co: Co<'_, Effects![Log<'_>], ()> = Co::new(move |y| async move {
    y.yield_(Log(msg_ref)).await;
});

let result = sync::run(
    co,
    &mut hlist![|Log(m)| {
        println!("{m}");
        CoControl::resume(())
    }],
);
assert_eq!(result, Ok(()));
```

#### Borrowed resume types

Because `Effect::Resume<'r>` is a generic associated type (GAT), handlers can resume computations with *borrowed* data instead of requiring owned values. This is useful when the handler has access to data that the computation only needs to read temporarily.

```rust,ignore
use corophage::prelude::*;
use std::collections::HashMap;

// An effect that looks up a key in a map.
// Thanks to the GAT, the handler can resume with a `&str`
// borrowed from the map, avoiding a clone.
struct Lookup<'a> {
    map: &'a HashMap<String, String>,
    key: &'a str,
}

impl<'a> Effect for Lookup<'a> {
    type Resume<'r> = &'r str;
}

let map = HashMap::from([
    ("host".to_string(), "localhost".to_string()),
    ("port".to_string(), "5432".to_string()),
]);

let co: Co<'_, Effects![Lookup<'_>], String> = Co::new({
    let map = &map;
    move |y| async move {
        let host: &str = y.yield_(Lookup { map, key: "host" }).await;
        let port: &str = y.yield_(Lookup { map, key: "port" }).await;
        format!("{host}:{port}")
    }
});

let result = sync::run(
    co,
    &mut hlist![|Lookup { map, key }| {
        let value = map.get(key).unwrap();
        CoControl::resume(value.as_str()) // resumes with a borrowed &str
    }],
);

assert_eq!(result, Ok("localhost:5432".to_string()));
```

#### `Send`-able coroutines with `CoSend`

By default, `Co` is not `Send`, which means it cannot be moved across threads. When you need to spawn a coroutine on a multi-threaded async runtime (e.g., `tokio::spawn`), use `CoSend` instead:

```rust,ignore
use corophage::{CoSend, Effects, sync, CoControl};
use corophage::frunk::hlist;

fn co() -> CoSend<'static, Effects![FileRead], String> {
    CoSend::new(|y| async move {
        y.yield_(FileRead("test".to_string())).await
    })
}

// CoSend is Send, so it can be spawned on a multi-threaded executor.
let handle = tokio::spawn(async move {
    sync::run(
        co(),
        &mut hlist![|FileRead(file)| {
            println!("Reading file: {file}");
            CoControl::<'static, Effects![FileRead]>::resume("file content".to_string())
        }],
    )
});
```

Both `Co` and `CoSend` work with the same `run`/`run_stateful` functions - the runner is generic over the `Locality` parameter.

### 3. Handlers

A **Handler** is an `async` function that implements the logic for a specific effect. It receives a mutable reference to a shared `State` and the `Effect` instance that was yielded.

The handler must return a `CoControl`, which tells the runner what to do next:
*   `CoControl::resume(value)`: Resumes the computation, passing `value` back as the result of the `yield_`. The type of `value` must match the effect's `Resume` type.
*   `CoControl::cancel()`: Aborts the entire computation immediately. The final result of the run will be `Err(Cancelled)`.

```rust,ignore
use corophage::{CoControl, Cancelled};

// A handler for the `Log` effect.
async fn log(_: &mut State, Log(msg): Log<'_>) -> CoControl<'static, MyEffects> {
    println!("LOG: {msg}");
    CoControl::resume(()) // Resume the computation with `()`.
}

// A handler for the `FileRead` effect.
async fn file_read(_: &mut State, FileRead(file): FileRead) -> CoControl<'static, MyEffects> {
    println!("Reading file: {file}");
    // In a real app, you'd read the file here.
    // tokio::fs::read_to_string(file).await...
    CoControl::resume("file content".to_string()) // Resume with the content.
}

// A handler for the `Cancel` effect.
async fn cancel(_: &mut State, _c: Cancel) -> CoControl<'static, MyEffects> {
    CoControl::cancel() // Abort the computation.
}
```

### 4. State

As seen above, handlers can be stateful. The `run_stateful` function takes a mutable reference to a state object of your choosing. This same state object is passed as the first argument to every handler, allowing them to share and modify state.

> [!NOTE]
> If your handlers do not need shared state, you can instead run them using the `run` function, which does not require a state parameter.

The example uses `GetState` and `SetState` effects to explicitly manage state from within the computation itself.

```rust,ignore
// The shared state for our handlers.
#[derive(Debug, PartialEq, Eq)]
struct State {
    x: u64,
}

// Handler for GetState<u64>
async fn handle_get_state(s: &mut State, _g: GetState<u64>) -> CoControl<'static, MyEffects> {
    CoControl::resume(s.x)
}

// Handler for SetState<u64>
async fn handle_set_state(s: &mut State, SetState(x): SetState<u64>) -> CoControl<'static, MyEffects> {
    s.x = x;
    CoControl::resume(((), ())) // Resume with a dummy value.
}
```

## Incremental handler attachment with `Program`

Sometimes you want to attach handlers to a computation incrementally — for example, when different parts of your application are responsible for handling different effects, or when you want to partially apply a fixed set of handlers and re-use them across multiple computations.

`Program` wraps a `Co` and accumulates handlers one at a time. The type system tracks which effects still need a handler, and `.run()`/`.run_sync()` are only available once all effects are handled.

```rust,ignore
use corophage::{Co, Effects, Program, CoControl, Cancelled};

type Effs = Effects![Log<'static>, FileRead];

let co: Co<'_, Effs, String> = Co::new(|y| async move {
    y.yield_(Log("fetching...")).await;
    y.yield_(FileRead("data.txt".to_string())).await
});

// Attach handlers one at a time, in effect declaration order.
let result = Program::new(co)
    .handle(|Log(msg)| {
        println!("{msg}");
        CoControl::resume(())
    })
    .handle(|FileRead(path)| CoControl::resume(format!("contents of {path}")))
    .run_sync();

assert_eq!(result, Ok("contents of data.txt".to_string()));
```

A free function `handle` is available in the `program` module as an alternative style:

```rust,ignore
use corophage::program::handle;

let p = Program::new(co);
let p = handle(p, |Log(msg)| { println!("{msg}"); CoControl::resume(()) });
let p = handle(p, |FileRead(path)| CoControl::resume(format!("contents of {path}")));
let result = p.run_sync();
```

> [!IMPORTANT]
> As with `run`/`run_sync`, handlers must be attached in the same order as the effects appear in the `Effects![...]` list. This is enforced by the type system — attaching handlers in the wrong order is a compile error.

## Putting it all together

To run a computation, you use `corophage::run_stateful`. You need three things:
1.  The `Co` computation to run.
2.  An initial `State`.
3.  A list of handlers, provided in a heterogeneous list (`hlist`).

> [!IMPORTANT]
> The order of handlers in the `hlist` must exactly match the order of effects in your `Effects!` macro.**

```rust,ignore
use corophage::frunk::hlist;
use corophage::{run_stateful, Cancelled};

// 1. Define the computation (see `co()` function above).

// 2. Define and initialize the state.
#[derive(Debug, PartialEq, Eq)]
struct State {
    x: u64,
}

let mut state = State { x: 42 };

// 3. Define handlers (see handler functions above).

// 4. Run the computation.
let result = run_stateful(
    co(),
    &mut state,
    // The hlist of handlers. Order must match `MyEffects`.
    // MyEffects = Effects![Cancel, Log, FileRead, GetState<u64>, SetState<u64>]
    &mut hlist![
        cancel, // Handler for Cancel
        log,    // Handler for Log
        file_read, // Handler for FileRead
        // You can also use async closures as handlers.
        async |s: &mut State, _g: GetState<u64>| CoControl::resume(s.x),
        async |s: &mut State, SetState(x)| {
            s.x = x;
            CoControl::resume(((), ()))
        },
    ],
)
.await;

// The computation was cancelled by the `Cancel` effect.
assert_eq!(result, Err(Cancelled));

// The state was modified by the handlers before cancellation.
assert_eq!(state, State { x: 84 });
```

### Execution flow

1.  `run_stateful` starts the `co()` computation.
2.  `yielder.yield_(Log(...))` is called. The computation pauses.
3.  `run_stateful` finds the `log` handler (2nd in the list) and executes it. "LOG: Hello, world!" is printed. The handler returns `CoControl::resume(())`.
4.  The computation resumes.
5.  `yielder.yield_(FileRead(...))` is called. The computation pauses.
6.  `run_stateful` finds the `file_read` handler (3rd in the list) and executes it. It returns `CoControl::resume("file content")`.
7.  The computation resumes. `text` is `"file content"`.
8.  `yielder.yield_(GetState)` is called. The 4th handler runs, returning `CoControl::resume(42)`.
9.  The computation resumes. `state` is `42`.
10. `yielder.yield_(SetState(84))` is called. The 5th handler runs, changing `state.x` to `84` and resuming.
11. `yielder.yield_(GetState)` is called again. The 4th handler runs, now returning `CoControl::resume(84)`.
12. `yielder.yield_(Cancel)` is called. The computation pauses.
13. `run_stateful` finds the `cancel` handler (1st in the list) and executes it. It returns `CoControl::cancel()`.
14. The entire execution is aborted. `run_stateful` returns `Err(Cancelled)`.

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
