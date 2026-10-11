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

- Transforms the return type to `Program<'_, Effects![Eff1, Eff2, ...], impl Future<Output = T> + '_>`
- Wraps the body in `Program::new`
- Enables `yield_!(effect)` syntax to perform effects

The body future keeps its concrete type by default. Its auto traits, including `Send`, remain available to the caller. See [explicit boxing](@/docs/advanced.md#explicit-boxing-and-recursion) for stored, erased, or recursive programs.

### Lifetime handling

Borrowed and generic arguments are captured for the lifetime of the returned program automatically. Elided references, `&self` receivers, generic parameters, and argument-position `impl Trait` do not need artificial lifetime annotations or outlives bounds.

```rust
#[effectful]
fn length(value: &str) -> usize {
    value.len()
}
```

Borrowed wrappers can use placeholder lifetimes, including in nested argument types. Each placeholder is inferred independently:

```rust
use std::borrow::Cow;

#[effectful]
fn cow_length(value: Cow<'_, str>) -> usize {
    value.len()
}

let text = String::from("hello");
assert_eq!(cow_length(Cow::Borrowed(&text)).run_sync(), Ok(5));
```

Nested references also keep independent lifetimes. For example, the outer borrow in `&mut &str` can end before the inner string data expires. This permits sequential calls after each program completes or is dropped:

```rust
#[effectful]
fn advance(value: &mut &str) -> usize {
    *value = &value[1..];
    value.len()
}

let text = String::from("abc");
let mut remaining = text.as_str();
assert_eq!(advance(&mut remaining).run_sync(), Ok(2));
assert_eq!(advance(&mut remaining).run_sync(), Ok(1));
assert_eq!(remaining, "c");
```

Generic parameters, argument-position `impl Trait`, and borrowed receivers work the same way:

```rust
#[effectful]
fn stringify<T: ToString>(value: T) -> String {
    value.to_string()
}

#[effectful]
fn stringify_impl(value: impl ToString) -> String {
    value.to_string()
}

struct Label(String);

impl Label {
    #[effectful]
    fn length(&self) -> usize {
        self.0.len()
    }
}
```

For borrowed effects, declare a lifetime on the effect itself. Here, `BorrowedLog` carries a string reference. The macro infers the effect lifetime when the function has exactly one lifetime parameter:

```rust
#[effect(())]
struct BorrowedLog<'a>(&'a str);

#[effectful(BorrowedLog<'a>)]
fn log_msg<'a>(msg: &'a str) -> () {
    yield_!(BorrowedLog(msg));
}
```

With multiple lifetime parameters, specify the effect lifetime explicitly as the first argument:

```rust
#[effectful('a, BorrowedLog<'a>)]
fn log_msg<'a, 'b>(msg: &'a str, _other: &'b str) -> () {
    yield_!(BorrowedLog(msg));
}
```

An explicit lifetime can also be the only attribute argument. Declare a named lifetime on the function, enclosing `impl`, or enclosing `trait`; the attribute selects that declaration. A named lifetime needs no trailing comma, `'static` remains a concrete lifetime, and `'_` requests the same inference used when the lifetime is omitted:

```rust
#[effectful('a)]
fn named<'a>(value: &'a str) -> usize {
    value.len()
}

#[effectful('static)]
fn static_value() -> &'static str {
    "static"
}

#[effectful('_)]
fn inferred(value: &str) -> usize {
    value.len()
}
```

Methods and associated functions can reuse a lifetime from their enclosing `impl`:

```rust
struct Reader<'a>(&'a str);

impl<'a> Reader<'a> {
    #[effectful('a)]
    fn text(&self) -> &'a str {
        self.0
    }
}

let text = String::from("hello");
let reader = Reader(&text);
assert_eq!(reader.text().run_sync(), Ok("hello"));
```

If existing code relied on the attribute to introduce a named lifetime, add a Rust declaration such as `fn read<'a>(...)`, or omit the attribute lifetime to use inference.

### `Send`-able programs

The default program can be `Send` when its body and resume transport are `Send`. Add `send` to check the body guarantee at the function definition:

```rust
#[effectful(Counter, send)]
fn my_send_program() -> u64 {
    yield_!(Counter)
}
```

### Spreading an effects type alias

If you define a reusable set of effects with `type MyEffs = Effects![...]`, you can spread it into the attribute with `...Alias` (the same syntax as frunk's `Coprod!(...Tail)`):

```rust
type MyEffs = Effects![Log, Counter];

#[effectful(...MyEffs)]
fn using_alias() -> u64 {
    yield_!(Log("hello".into()));
    yield_!(Counter)
}
```

You can also prepend extra inline effects before the spread:

```rust
#[effect(&'r str)]
struct GetConfig;

#[effectful(GetConfig, ...MyEffs)]
fn with_extra() -> String {
    let cfg = yield_!(GetConfig);
    yield_!(Log(cfg.to_string()));
    format!("{cfg}: {}", yield_!(Counter))
}
```

The spread must be the last effect argument, before `send` or `boxed` flags if present. This combines naturally with lifetimes and `send`:

```rust
type LogEffs<'a> = Effects![GetConfig, BorrowedLog<'a>];

#[effectful('a, Counter, ...LogEffs<'a>, send)]
fn complex<'a>(msg: &'a str) -> u64 {
    yield_!(BorrowedLog(msg));
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

let program = Program::new(|mut y: Yielder<'_, Effs>| async move {
    y.yield_(Log("hello".into())).await;
    let n = y.yield_(Counter).await;
    n * 2
});
```

When you `await` the result of `y.yield_(some_effect)`, the computation pauses, the effect is handled, and the `await` resolves to the value provided by the handler.

A `Yielder` supports one effect operation at a time. Its `yield_` and `invoke` methods require `&mut self`, so the borrow checker rejects overlapping operations such as passing two `yield_` futures to `join!`. Await each operation before starting another.

Computation bodies may also await ordinary futures. This works inside `#[effectful]` functions, manual `Program::new` closures, and invoked sub-programs when executed with `.run().await` or a low-level `asynk` runner:

```rust
#[effectful(Counter)]
fn delayed_count() -> u64 {
    tokio::task::yield_now().await;
    yield_!(Counter)
}

let result = delayed_count()
    .handle(async |_: Counter| Control::resume(42))
    .run()
    .await;
```

Synchronous runners handle effect suspension but cannot wait for unrelated futures. If an ordinary future returns `Pending`, use `.run().await` instead.

This includes a sub-program executed with `.run().await` inside a synchronously run body. Async runners yield to the executor after a bounded number of effects, so such a body panics once the sub-program performs enough effects. Use `invoke!` (or `Yielder::invoke`) to forward the sub-program's effects to the outer handlers, or `.run_sync()` to handle them locally.

## Attaching handlers

Handlers are attached one at a time with `.handle()`. Handlers can be attached in any order — the type system tracks which effects remain unhandled.

```rust
let result = my_program()
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
#[effectful(Counter, Log)]
fn my_program() -> u64 {
    yield_!(Log("start".into()));
    yield_!(Counter)
}

let result = my_program()
    .handle_all(hlist![
        |_: Counter| Control::resume(42u64),
        |Log(msg)| { println!("{msg}"); Control::resume(()) },
    ])
    .run_sync();
```

## Mapping return values

Use `.map(...)` to transform a program's return value without changing its effects. You can map before, between, or after attaching handlers and state:

```rust
let result = Program::new(|mut y: Yielder<'_, Effects![Counter]>| async move {
    y.yield_(Counter).await
})
.map(|count| format!("count: {count}"))
.handle(|_: Counter| Control::resume(42u64))
.run_sync();

assert_eq!(result, Ok(String::from("count: 42")));
```

The synchronous `FnOnce` callback runs once, after the body completes. Construction remains lazy. Cancelling or dropping an incomplete program skips the callback and releases its captures. Mapping does not add a handler or handle cancellation: `.run()` and `.run_sync()` still return `Result<MappedValue, Cancelled>`. If the body returns a `Result` itself, `.map(...)` receives that whole value.

Mapping retains attached handlers, the remaining effect set, and the existing state borrow. An unhandled mapped program can still be [invoked](#program-composition) by a parent. Mapping itself adds no allocation; the body, callback captures, handlers, state, and resume transport determine whether execution is `Send`.

Mapped programs use inferred execution, including when their original body was boxed. Use `local` or `local_stateful` for lending async handlers. A local boxed body remains non-`Send`. To return an `Effectful` alias, apply `.boxed()` or `.boxed_send()` after mapping the unhandled program.

## Effectful callbacks

A higher-order helper can invoke a callback's program while forwarding an unknown effect set. This example maps the `Some` branch of an `Option` and skips constructing the child entirely for `None`:

```rust
use corophage::prelude::*;
use corophage::{EmbedEffect, ProjectResume, Resumes};
use std::future::Future;

fn option_map_eff<'a, T: 'a, U: 'a, Es, F, Make, Indices>(
    value: Option<T>,
    make: Make,
) -> Program<'a, Es, impl Future<Output = Option<U>> + 'a>
where
    Es: Effects<'a> + EmbedEffect<Es, Indices>,
    Resumes<'a, Es>: ProjectResume<'a, Es, Indices>,
    F: Future<Output = U> + 'a,
    Make: FnOnce(T) -> Program<'a, Es, F> + 'a,
{
    Program::new(|mut y: Yielder<'a, Es>| async move {
        match value {
            Some(value) => Some(y.invoke(make(value)).await),
            None => None,
        }
    })
}
```

The callback and child run when the outer program executes. Attach the child's handlers to the returned program; its declared effects still require handlers even when the input happens to be `None`. See [generic effect sets](@/docs/effects.md#generic-effect-sets) for the forwarding bounds.

`cargo run --example higher_order` runs a complete borrowed lookup example, combines `option_map_eff` with `Program::map`, and asserts that `None` calls neither the callback nor its handler. The helper is example code that can be adapted downstream. It also works with `--no-default-features`.

## Running programs

Once all effects are handled, you can run the program:

- `.run_sync()` — execute inline with synchronous handlers and return `Result<R, Cancelled>`; a body that suspends on an ordinary future causes a panic.
- `.run().await` — execute asynchronously, with synchronous and asynchronous handlers in the same list.

These rules describe the default concrete programs. Local boxed `.run()` accepts only async callables; use `.with_state(...)` to select mixed dispatch or the explicit lending adapters. See the [compatibility API](@/docs/advanced.md#compatibility-api).

Attach shared state with `.with_state(&mut state)` before running. Handlers that use state take `&mut S` as their first argument; other handlers can omit it. The compatibility methods `.run_sync_stateful(&mut state)` and `.run_stateful(&mut state)` still bind state at their terminal call site.

Async runners yield to the executor after a bounded number of effect dispatches, even when handlers are immediately ready. Each runner has its own budget (currently 128 dispatches; this value may change). This applies to `.run().await`, `.run_stateful(&mut state).await`, and the low-level `asynk` runners. It gives the executor opportunities to schedule other tasks and process cancellation, but does not guarantee task ordering or preempt a computation body or handler that does not return from its poll.

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

Both execution methods require all effects to be handled. Missing or duplicate handlers cause a compile error.

## Program composition

Programs can invoke other programs via `invoke!()` (or `y.invoke()` with the manual API). The sub-program's effects must be a subset of the outer program's effects — each yielded effect is forwarded to the outer handler automatically.

```rust
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

```rust
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

Sub-programs can be nested — a sub-program can itself invoke other sub-programs. The only requirement is that the inner program's effects are a subset of the outer program's effects.

## `Send`-able programs

`Program::new` and default `#[effectful]` functions preserve conditional `Send`. The execution future can be passed to `tokio::spawn` when its body, handlers, handler futures, state, and resume transport permit it. Spawning also requires a `Send + 'static` result and a `'static` future:

```rust
#[effectful(Counter)]
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
let program = Program::new(|mut y: Yielder<'_, Effects![Counter]>| async move {
    y.yield_(Counter).await
});

tokio::spawn(async move {
    let result = program
        .handle(async |_: Counter| Control::resume(42u64))
        .run()
        .await;
});
```

For local execution, the same constructor accepts `Rc` captures, non-`Send` state, and non-`Send` handler futures. A sendable body can also use local handlers or state. There is no blanket handler `Sync` bound on this path; an actual shared borrow across an await can still require `Sync`.

Use `corophage::local(async |effect| { ... })` when an async closure borrows its captures across an await. This explicit adapter boxes one local future per call. Ordinary async handlers use `FnMut(...) -> Future` and need no dispatch allocation. See [Handlers](@/docs/handlers.md#lending-async-closures).

Use `.with_state(&mut state)` with either execution method. Each stateful handler can borrow the state across awaits. See [Shared State](@/docs/state.md#send-able-async-execution) for a spawn example and [Advanced Usage](@/docs/advanced.md#compatibility-api) for the retained boxed contracts.
