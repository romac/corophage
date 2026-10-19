<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/static/logo/corophage-dark.svg">
    <img src="docs/static/logo/corophage.svg" alt="Corophage logo" width="240">
  </picture>
</p>

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
corophage = "0.6.2"
```

The default `macros` feature provides `#[effect]` and `#[effectful]`. Set `default-features = false` on the dependency to use manual `Effect` implementations and `Program::new`. The `Effects!` macro remains available.

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
let result = Program::new(|mut y: Yielder<'_, MyEffects>| async move {
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

The `#[effectful]` macro transforms your function to return a `Program` and lets you use `yield_!(effect)` to perform effects. Both the macro and `Program::new` keep the body future concrete by default. The resulting execution future is `Send` when its retained components permit it; no separate constructor is needed for ordinary sendable programs. Add the macro's `send` flag to check an explicit body guarantee.

Borrowed and generic arguments are captured for the lifetime of the returned program automatically, so ordinary signatures such as `fn length(value: &str)` do not need artificial lifetime annotations. Borrowed wrapper arguments such as `Cow<'_, str>` are also supported, including nested wrappers with independent placeholder lifetimes.

Nested reference arguments such as `&mut &str` keep independent lifetimes for the outer borrow and inner data. The outer mutable borrow can be reused after the program completes or is dropped.

An explicitly named effect lifetime, such as `#[effectful('a)]`, selects a lifetime declared on the function, enclosing `impl`, or enclosing `trait`. For example, a free function declares it with `fn read<'a>(...)`, while a method can reuse the `'a` from `impl<'a> Reader<'a>`. Omit the lifetime or use `'_` for automatic inference.

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

let result = Program::new(|mut y: Yielder<'_, Effs>| async move {
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

A `Yielder` supports one effect operation at a time. Its `yield_` and `invoke` methods require `&mut self`, so the borrow checker rejects overlapping operations such as passing two `yield_` futures to `join!`. Await each operation before starting another.

Computation bodies may also await ordinary futures when executed with `.run().await` or the low-level `asynk` runners. Synchronous runners handle effect suspension but cannot wait for unrelated futures; if one returns `Pending`, use an async runner instead.

Use `.run_sync()` with synchronous handlers or `.run().await` with a mixture of sync and async handlers. Async runners yield to the executor after a bounded number of effect dispatches, even when handlers are immediately ready. Each runner has its own budget (currently 128 dispatches; this value may change). This gives the executor opportunities to schedule other tasks and process cancellation. It does not guarantee task ordering or preempt a computation body or handler that does not return from its poll.

> [!NOTE]
> Handlers can be attached in any order, both with `Program::handle()` and in the `hlist!` passed to the low-level `sync::run`/`asynk::run` functions. Low-level handler lists may also contain extra handlers for effects the computation never yields; every effect the computation can yield must still have a compatible handler.

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

Ordinary async handlers return futures through `FnMut(...) -> Future`. If an async closure borrows its captures across awaits, use `corophage::local(async |effect| { ... })`, or `corophage::local_stateful(async |state: &mut S, effect| { ... })` when it also uses shared state. These explicit adapters box one non-`Send` future per call. Ordinary dispatch does not box handler futures.

### 4. Shared state

Bind shared mutable state with `.with_state(&mut state)`, then execute with `.run_sync()` or `.run().await`. Stateful handlers take `&mut S` as their first argument. State-free handlers can share the same list:

```rust,ignore
let mut count: u64 = 0;

let result = Program::new(|mut y: Yielder<'_, Effects![Counter]>| async move {
    let a = y.yield_(Counter).await;
    let b = y.yield_(Counter).await;
    a + b
})
.with_state(&mut count)
.handle(|s: &mut u64, _: Counter| {
    *s += 1;
    Control::resume(*s)
})
.run_sync();

assert_eq!(result, Ok(3));
assert_eq!(count, 2);
```

The state borrow begins at `.with_state()` and ends on completion, cancellation, or drop. Each call receives a fresh reborrow; a handler can keep it across awaits but cannot resume the computation with a reference borrowed from that temporary loan.

The generic execution path accepts local state and handlers even when the body is sendable. It has no blanket handler `Sync` bound. `tokio::spawn` checks that the whole future and result are `Send + 'static`; an actual shared borrow across an await can still require `Sync`. The terminal `.run_stateful(&mut state)` and `.run_sync_stateful(&mut state)` methods remain as compatibility wrappers, binding state at the terminal call.

> [!NOTE]
> If your handlers do not need shared state, omit `.with_state()`. Captured `RefCell` or other interior mutability remains available.

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
let result = Program::new(|mut y: Yielder<'_, Effects![Ask, Print, Log]>| async move {
    y.yield_(Log("Starting...")).await;
    y.invoke(greet()).await;
    y.yield_(Log("Done!")).await;
})
.handle(|_: Ask| Control::resume("world"))
.handle(|Print(msg)| { println!("{msg}"); Control::resume(()) })
.handle(|_: Log| Control::resume(()))
.run_sync();
```

Sub-programs can themselves invoke other sub-programs. Concrete future storage grows with nesting depth; use explicit boxing for recursive functions or to limit stored type size.

### Explicit return types and boxing

Manual functions can return `Program<'a, Effs, impl Future<Output = R> + 'a>`. To retain an erased `Effectful` signature, box the unhandled body explicitly:

```rust,ignore
fn erased() -> Effectful<'static, Effects![FileRead], String> {
    Program::new(|mut y: Yielder<'_, Effects![FileRead]>| async move {
        y.yield_(FileRead("data.txt".into())).await
    }).boxed()
}
```

Use `.boxed_send()` for `Effectful<'a, Effs, R, corophage::Sendable>`. `.boxed()` erases the body's `Send` capability. For macro functions, `#[effectful(..., boxed)]` selects local erasure and `#[effectful(..., boxed, send)]` selects sendable erasure. The boxed form permits recursive `invoke!` calls. This is a breaking change to default return types: existing manual functions returning `Effectful` need explicit boxing, and full `Program` signatures now take the body future as their third parameter.

Effectful trait methods called through `dyn Trait` also need `boxed`: the default return type contains `impl Future`, which prevents dynamic dispatch. See the [trait-object example](https://corophage.rs/docs/advanced/#trait-objects).

## Advanced: `Co`, `CoSend`, and the direct API

`Co` and `CoSend` retain their erased constructors and can be wrapped with `Program::from_co`. `Program::new_send` remains an erased sendable constructor for compatibility.

```rust,ignore
use corophage::{Co, CoSend, sync, Control};
use corophage::prelude::*;

// Co — the computation type (not Send)
let co: Co<'_, Effects![FileRead], String> = Co::new(|mut y| async move {
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
    CoSend::new(|mut y| async move {
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

Async handlers attached to a `CoSend` program must return `Send` futures. Handlers that capture state across an await should clone it into a non-lending future, for example `move |effect| { let state = Arc::clone(&state); async move { ... } }`. Local programs accept such handlers through `corophage::local(async |effect| { ... })`.

Local erased programs, including those from `.boxed()` and `#[effectful(..., boxed)]`, accept the same handlers as concrete programs. Before 0.7, their `.run()` and `.run_stateful()` accepted only async closures; wrap closures that borrow their captures across an await with `corophage::local` or `corophage::local_stateful`.

Erased sendable programs retain explicit `Send` terminal contracts, including the `Send + Sync` handler bound on `.run_stateful()`. `.with_state()` selects the new conditional protocol for an erased body. A compiler lifetime limitation can affect programs with lifetime-parameterized effects awaited inside a `Send` async block; `.boxed_send()` followed by the compatibility `.run()` or `.run_stateful()` provides an explicit boundary for these cases. See the [advanced documentation](https://corophage.rs/docs/advanced/#compatibility-api).

The direct API (`sync::run`, `sync::run_stateful`, `asynk::run`, `asynk::run_stateful`) accepts a `Co`/`CoSend` and an `hlist!` of all handlers at once. This is useful for concise one-shot execution but requires providing all handlers together.

`fauxgen` transports effects through the polling context. Effect operations inside combinators that replace that context can break the transport. Keep `yield_!` and `invoke!` in the directly awaited computation flow; boxing does not remove this restriction.

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

let result = Program::new(move |mut y: Yielder<'_, Effects![Log<'_>]>| async move {
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
    move |mut y: Yielder<'_, Effects![Lookup<'_>]>| async move {
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

## Examples

- [Order processing saga](https://github.com/romac/corophage/blob/main/corophage/examples/saga.rs): run an async workflow and compensate completed steps after cancellation.
- [Stepwise debugger](https://github.com/romac/corophage/blob/main/corophage/examples/debugger.rs): pause a computation and replay it to an earlier step.
- [Exact state-change assertions](https://github.com/romac/corophage/blob/main/corophage/examples/debug_snapshot.rs): specify the expected changes to a model and reject unaccounted-for changes. This checks the final model state handled by the example's `Get` and `Set` effects.

Run the state-change example with `cargo run --example debug_snapshot`. Add `-- --demonstrate-failure` to show the assertion failure caused by an omitted state change.

## Performance

The current implementation at commit `6cc6ff2` was measured against the boxed implementation at commit `fa68b5a` on the same macOS arm64 machine with Rust 1.99.1, release optimization, and LTO. These are medians for complete operations, including construction and execution:

| Operation | Boxed baseline | Current |
| --- | ---: | ---: |
| Construct and drop | 12.9 ns | 0.7 ns |
| 1,000 sync effects | 10.4 µs | 9.6 µs |
| 1,000 async effects | 16.2 µs | 11.6 µs |
| 1,000 stateful sync effects | 10.4 µs | 9.5 µs |
| 1,000 stateful async effects | 17.2 µs | 12.1 µs |
| 20 nested invocations, one leaf effect | 1.84 µs | 0.74 µs |

Construction performs no allocation. A flat computation with 1 or 1,000 ordinary handler calls used one runtime allocation, down from two; ordinary dispatch adds no per-call allocation. Explicit body boxing adds one allocation, and each `local` or `local_stateful` handler call boxes its future.

Concrete storage is larger: the measured flat async execution future grew from 280 to 448 bytes, and the depth-20 future occupies 8,048 bytes. These sizes exclude heap storage used by boxed bodies. The common measurement binary grew by 4.2%, and one clean build took 6.7 seconds versus 5.7 seconds. These results describe this fixture and toolchain, not a general performance guarantee.

Run `python3 scripts/execution/measure.py` from the repository root to reproduce the isolated comparison. The [performance documentation](https://corophage.rs/docs/performance/) gives the method, tradeoffs, and refreshed Divan nesting results. Run the Divan suites with `cargo bench`; their nesting fixtures are shared with regression tests that check peak active depth and effect counts. The stress suite counts active programs (leaf depth 1), while the comparison above counts invocation edges (leaf depth 0).

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
