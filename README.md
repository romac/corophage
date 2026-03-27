# Corophage

[![Crates.io](https://img.shields.io/crates/v/corophage.svg)](https://crates.io/crates/corophage)
[![Docs.rs](https://docs.rs/corophage/badge.svg)](https://docs.rs/corophage)
![Stable Rust](https://img.shields.io/badge/rust-stable-orange)
[![Coverage](https://codecov.io/github/romac/corophage/graph/badge.svg?token=U8FVD3HT2X)](https://codecov.io/github/romac/corophage)

**Algebraic effects for stable Rust**

`corophage` provides a way to separate the *description* of what your program should do from the *implementation* of how it gets done. This allows you to write clean, testable, and composable business logic.

## Usage

Add `corophage` to your `Cargo.toml`:

```toml
[dependencies]
corophage = "0.3.2"
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
.handle(|Log(msg)| { println!("{msg}"); Control::resume(()) })
.handle(|FileRead(f)| Control::resume(std::fs::read_to_string(f).unwrap()))
.handle(|_: GetState| Control::resume(42u64))
.handle(|SetState(x)| { /* ... */ Control::resume(()) })
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
    fn shorten_resume<'long: 'short, 'short>(resume: ()) -> () { resume }
}

// An effect to request reading a file.
// It expects the file's contents back, so we resume with `String`.
pub struct FileRead(pub String);
impl Effect for FileRead {
    type Resume<'r> = String;
    fn shorten_resume<'long: 'short, 'short>(resume: String) -> String { resume }
}

// An effect that cancels the computation.
// It will never resume, so we use the special `Never` type.
pub struct Cancel;
impl Effect for Cancel {
    type Resume<'r> = Never;
    fn shorten_resume<'long: 'short, 'short>(resume: Never) -> Never { resume }
}
```

Each manual `Effect` impl must provide `shorten_resume`, a one-liner that witnesses the covariance of `Resume<'r>` in `'r`. This is required for [program composition](#5-program-composition) to work correctly. The `#[effect]` macro generates it automatically.

You can also use the `#[effect]` attribute macro to derive the `Effect` impl:

```rust,ignore
use corophage::prelude::*;

#[effect(())]
pub struct Log<'a>(pub &'a str);

#[effect(String)]
pub struct FileRead(pub String);

#[effect(Never)]
pub struct Cancel;

// The resume type may reference the GAT lifetime `'r`:
#[effect(&'r str)]
pub struct Lookup(pub String);

// Generics work too:
#[effect(T)]
pub struct Identity<T: Debug + Send + Sync>(pub T);
```

### 2. Programs

A **Program** combines a computation with its effect handlers. The simplest way to create one is with the `#[effectful]` attribute macro:

```rust,ignore
use corophage::prelude::*;

#[effectful(Log<'static>, FileRead)]
fn fetch_data() -> String {
    yield_!(Log("fetching..."));
    yield_!(FileRead("data.txt".to_string()))
}

let result = fetch_data()
    .handle(|Log(msg)| { println!("{msg}"); Control::resume(()) })
    .handle(|FileRead(path)| Control::resume(format!("contents of {path}")))
    .run_sync();

assert_eq!(result, Ok("contents of data.txt".to_string()));
```

The `#[effectful]` macro transforms your function to return a `Program` and lets you use `yield_!(effect)` to perform effects.

If you have a pre-defined effects type alias, you can spread it into the attribute with `...Alias` (same syntax as frunk's `Coprod!(...Tail)`):

```rust,ignore
type IoEffects = Effects![Log<'static>, FileRead];

#[effectful(...IoEffects)]
fn fetch_data() -> String {
    yield_!(Log("fetching..."));
    yield_!(FileRead("data.txt".to_string()))
}

// Extra inline effects can precede the spread:
#[effectful(Cancel, ...IoEffects)]
fn fetch_or_cancel() -> String {
    yield_!(Log("fetching..."));
    yield_!(FileRead("data.txt".to_string()))
}
```

You can also create programs manually with `Program::new`:

```rust,ignore
use corophage::prelude::*;

type Effs = Effects![Log<'static>, FileRead];

let result = Program::new(|y: Yielder<'_, Effs>| async move {
    y.yield_(Log("fetching...")).await;
    y.yield_(FileRead("data.txt".to_string())).await
})
.handle(|Log(msg)| { println!("{msg}"); Control::resume(()) })
.handle(|FileRead(path)| Control::resume(format!("contents of {path}")))
.run_sync();

assert_eq!(result, Ok("contents of data.txt".to_string()));
```

The `Effects!` macro also supports the `...Alias` spread syntax to compose effect sets:

```rust,ignore
type IoEffects = Effects![Log<'static>, FileRead];
type AllEffects = Effects![Cancel, ...IoEffects];
// Equivalent to: Effects![Cancel, Log<'static>, FileRead]
```

When you call `yield_!` (or `y.yield_(...).await` in the manual style), the computation pauses, the effect is handled, and execution resumes with the value provided by the handler.

> [!NOTE]
> Handlers can be attached in any order when using `Program::handle()`. The type system tracks which effects are still unhandled regardless of attachment order. However, handlers passed as an `hlist!` to the low-level `sync::run`/`asynk::run` functions must match the `Effects![...]` declaration order.

### 3. Handlers

A **Handler** is a function (sync or async) that implements the logic for a specific effect. It receives the `Effect` instance and returns a `Control<R>` (where `R` is the effect's `Resume` type), which tells the runner what to do next:

*   `Control::resume(value)`: Resumes the computation, passing `value` back as the result of the `yield_`. The type of `value` must match the effect's `Resume` type.
*   `Control::cancel()`: Aborts the entire computation immediately. The final result of the run will be `Err(Cancelled)`.

```rust,ignore
// Sync handler — a regular closure
|Log(msg)| {
    println!("LOG: {msg}");
    Control::resume(())
}

// Async handler — an async closure
async |FileRead(file)| {
    let content = tokio::fs::read_to_string(file).await.unwrap();
    Control::resume(content)
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
    Control::resume(*s)
})
.run_sync_stateful(&mut count);

assert_eq!(result, Ok(3));
assert_eq!(count, 2);
```

> [!NOTE]
> If your handlers don't need shared state, use `.run_sync()` / `.run()` instead. You can also use `RefCell` or other interior mutability patterns to share state without `run_stateful`.

### 5. Program composition

Programs can invoke other programs. The sub-program's effects must be a subset of the outer program's effects — each yielded effect is forwarded to the outer handler automatically.

```rust,ignore
use corophage::prelude::*;

#[effect(&'static str)]
struct Ask(&'static str);

#[effect(())]
struct Print(String);

#[effect(())]
struct Log(&'static str);

#[effectful(Ask, Print)]
fn greet() {
    let name: &str = yield_!(Ask("name?"));
    yield_!(Print(format!("Hello, {name}!")));
}

#[effectful(Ask, Print, Log)]
fn main_program() {
    yield_!(Log("Starting..."));
    invoke!(greet());
    yield_!(Log("Done!"));
}

let result = main_program()
    .handle(|_: Ask| Control::resume("world"))
    .handle(|Print(msg)| { println!("{msg}"); Control::resume(()) })
    .handle(|_: Log| Control::resume(()))
    .run_sync();

assert_eq!(result, Ok(()));
```

With the manual `Program::new` API, use `y.invoke(program).await`:

```rust,ignore
let result = Program::new(|y: Yielder<'_, Effects![Ask, Print, Log]>| async move {
    y.yield_(Log("Starting...")).await;
    y.invoke(greet()).await;
    y.yield_(Log("Done!")).await;
})
.handle(|_: Ask| Control::resume("world"))
.handle(|Print(msg)| { println!("{msg}"); Control::resume(()) })
.handle(|_: Log| Control::resume(()))
.run_sync();
```

Sub-programs can be nested arbitrarily — a sub-program can itself invoke other sub-programs.

## Advanced: `Co`, `CoSend`, and the direct API

For cases where you need to pass a computation around before attaching handlers (e.g., returning it from a function, or storing it in a data structure), you can use `Co` and `CoSend` directly.

```rust,ignore
use corophage::{Co, CoSend, sync, Control};
use corophage::prelude::*;

// Co — the computation type (not Send)
let co: Co<'_, Effects![FileRead], String> = Co::new(|y| async move {
    y.yield_(FileRead("data.txt".to_string())).await
});

// Run directly with all handlers at once via hlist
let result = sync::run(co, &mut hlist![
    |FileRead(f)| Control::resume(format!("contents of {f}"))
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
        .handle(async |FileRead(f)| Control::resume(format!("contents of {f}")))
        .run()
        .await;
});
```

The direct API (`sync::run`, `sync::run_stateful`, `asynk::run`, `asynk::run_stateful`) accepts a `Co`/`CoSend` and an `hlist!` of all handlers at once. This is useful for concise one-shot execution but requires providing all handlers together.

#### Borrowing non-`'static` data

Effects can borrow data from the local scope by using a non-`'static` lifetime:

```rust,ignore
use corophage::prelude::*;

struct Log<'a>(pub &'a str);
impl<'a> Effect for Log<'a> {
    type Resume<'r> = ();
    fn shorten_resume<'long: 'short, 'short>(resume: ()) -> () { resume }
}

let msg = String::from("hello from a local string");
let msg_ref = msg.as_str();

let result = Program::new(move |y: Yielder<'_, Effects![Log<'_>]>| async move {
    y.yield_(Log(msg_ref)).await;
})
.handle(|Log(m)| { println!("{m}"); Control::resume(()) })
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
    fn shorten_resume<'long: 'short, 'short>(resume: &'long str) -> &'short str { resume }
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
    Control::resume(value.as_str())
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

Dispatch position has negligible impact. While the source-level dispatch uses recursive trait impls over nested `Coproduct::Inl`/`Inr` variants, the compiler monomorphizes and inlines the entire chain into a flat discriminant-based branch — the same code LLVM would emit for a plain `match` on a flat enum. The result is effectively O(1).

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
