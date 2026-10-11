+++
title = "Advanced Usage"
weight = 6
description = "Borrowed data, borrowed resume types, and other advanced patterns."
+++

## Borrowing non-`'static` data

Effects can borrow data from the local scope by using a non-`'static` lifetime:

```rust
use corophage::prelude::*;

#[effect(())]
struct Log<'a>(pub &'a str);

#[effectful(Log<'a>)]
fn log_it<'a>(msg: &'a str) -> () {
    yield_!(Log(msg));
}

let msg = String::from("hello from a local string");

let result = log_it(&msg)
    .handle(|Log(m)| { println!("{m}"); Control::resume(()) })
    .run_sync();

assert_eq!(result, Ok(()));
```

## Borrowed resume types

Because `Effect::Resume<'r>` is a generic associated type (GAT), handlers can resume computations with *borrowed* data instead of requiring owned values.

```rust
use corophage::prelude::*;
use std::collections::HashMap;

#[effect(&'r str)]
struct Lookup<'a> {
    map: &'a HashMap<String, String>,
    key: &'a str,
}

#[effectful(Lookup<'a>)]
fn lookup_config<'a>(map: &'a HashMap<String, String>) -> String {
    let host: &str = yield_!(Lookup { map, key: "host" });
    let port: &str = yield_!(Lookup { map, key: "port" });
    format!("{host}:{port}")
}

let map = HashMap::from([
    ("host".to_string(), "localhost".to_string()),
    ("port".to_string(), "5432".to_string()),
]);

let result = lookup_config(&map)
    .handle(|Lookup { map, key }| {
        let value = map.get(key).unwrap();
        Control::resume(value.as_str())
    })
    .run_sync();

assert_eq!(result, Ok("localhost:5432".to_string()));
```

## Low-level `Co` and `CoSend` API

`Program` is the primary API, but `Co` and `CoSend` are useful when you need to pass or store a computation before attaching handlers. They are available from the crate root rather than the prelude:

```rust
use corophage::{Co, sync};
use corophage::prelude::*;

#[effect(String)]
struct FileRead(&'static str);

let co: Co<'_, Effects![FileRead], String> = Co::new(|mut y| async move {
    y.yield_(FileRead("config.toml")).await
});

let result = sync::run(co, &mut hlist![
    |FileRead(path)| Control::resume(format!("contents of {path}")),
]);

assert_eq!(result, Ok("contents of config.toml".to_string()));
```

Use `CoSend` for a sendable computation. Once its handlers are attached, the future returned by `Program::run` can be passed directly to `tokio::spawn` when the result and all handler futures are `Send`:

```rust
use corophage::CoSend;

fn read_file() -> CoSend<'static, Effects![FileRead], String> {
    CoSend::new(|mut y| async move {
        y.yield_(FileRead("config.toml")).await
    })
}

let future = Program::from_co(read_file())
    .handle(async |FileRead(path)| {
        Control::resume(format!("contents of {path}"))
    })
    .run();

let result = tokio::spawn(future).await.unwrap();
assert_eq!(result, Ok("contents of config.toml".to_string()));
```

The low-level `sync::run` and `asynk::run` functions take all handlers in an `hlist!` at once. Handlers may appear in any order, and the list may include extra handlers, but every effect the computation can yield must have a compatible handler.

## Explicit boxing and recursion

`Program::new` and default `#[effectful]` functions keep the body future concrete. A manual function can express that return type with `impl Future`:

```rust
use std::future::Future;

fn read() -> Program<'static, Effects![FileRead], impl Future<Output = String>> {
    Program::new(|mut y: Yielder<'_, Effects![FileRead]>| async move {
        y.yield_(FileRead("config.toml")).await
    })
}
```

For an existing erased signature, box the unhandled program explicitly:

```rust
fn erased() -> Effectful<'static, Effects![FileRead], String> {
    read().boxed()
}

fn erased_send() -> Effectful<'static, Effects![FileRead], String, corophage::Sendable> {
    read().boxed_send()
}
```

`.boxed()` erases `Send` even when the original body was sendable. `.boxed_send()` retains that guarantee and checks the body and resume transport. Each conversion allocates once for the body. `Effectful<'a, Effs, R, L>` remains the erased alias, including for callers generic over `L: Locality`. The concrete `Program` type now takes its body future as its third parameter; update explicit full `Program` signatures accordingly.

Use the `boxed` macro flag for recursive functions or to limit concrete type growth. Combine it with `send` when the erased body must be sendable:

```rust
#[effectful(FileRead, boxed, send)]
fn recursive(depth: usize) -> String {
    if depth == 0 {
        yield_!(FileRead("config.toml"))
    } else {
        invoke!(recursive(depth - 1))
    }
}
```

Concrete nested futures grow with the invocation depth. Boxing bounds each body's stored type, but each active recursive call still consumes memory.

### Trait objects

Default `#[effectful]` trait methods return a type containing `impl Future`, which prevents the trait from being used as `dyn Trait`. Add `boxed` to effectful methods that need dynamic dispatch. Use the same erased return type in any overriding implementation. The trait must also satisfy Rust's other requirements for dynamic dispatch.

```rust
trait FileSource {
    fn path(&self) -> &'static str;

    #[effectful(FileRead, boxed)]
    fn read(&self) -> String {
        yield_!(FileRead(self.path()))
    }
}

struct Config;

impl FileSource for Config {
    fn path(&self) -> &'static str {
        "config.toml"
    }
}

let config = Config;
let source: &dyn FileSource = &config;
let result = source.read()
    .handle(|FileRead(path)| Control::resume(format!("contents of {path}")))
    .run_sync();

assert_eq!(result, Ok("contents of config.toml".to_string()));
```

## Compatibility API

`Co::new`, `CoSend::new`, `Program::new_send`, and `Program::from_co` remain available for erased signatures. Local erased programs also include values returned by `.boxed()` and `#[effectful(..., boxed)]`.

Local erased `.run()` requires every handler to be an async callable and accepts lending `AsyncFnMut` closures directly. Its `.run_stateful(&mut state)` counterpart requires stateful async callables compatible with `AsyncFn`, so their captures are borrowed immutably. These two methods do not accept synchronous handlers or the `local` / `local_stateful` adapters. Their low-level counterparts, `asynk::run` and `asynk::run_stateful`, retain the same restrictions. Local erased `.run_sync()` continues to accept synchronous handlers.

To use mixed handlers or the explicit lending adapters with a local boxed body, call `.with_state(...)` before `.run()`. This selects the new dispatch path while retaining the boxed body. If no handler needs shared state, bind a unit value:

```rust
let mut state = ();
let mut paths = Vec::new();
let result = erased()
    .with_state(&mut state)
    .handle(corophage::local(async |FileRead(path)| {
        paths.push(path);
        tokio::task::yield_now().await;
        Control::resume(format!("contents of {path}"))
    }))
    .run()
    .await;

assert_eq!(result, Ok("contents of config.toml".to_string()));
assert_eq!(paths, ["config.toml"]);
```

Erased sendable programs retain explicit `Send` guarantees on `.run()` and `.run_stateful()`. The latter retains its `Send + Sync` handler bound. `.with_state()` instead selects the new mutable protocol and conditional `Send`, while keeping the already erased body.

Rust can report `FnOnce is not general enough` when a stateful runner with lifetime-parameterized effects or GAT resumes is awaited inside another async block that must be `Send`. This can occur even if the runner itself passes a `Send` assertion. At that boundary, box the unhandled body with `.boxed_send()` and use the compatibility `.run_stateful(&mut state)` method. That combination has an explicit `Send` return guarantee and is covered by the execution tests.

## Polling context restriction

`fauxgen` transports effects through the polling context. Effect operations must be polled through that context; placing them inside a combinator that substitutes its own context can break this transport. Ordinary awaited futures remain supported, but boxing the body or using an async runner does not remove this restriction. Keep `yield_!` and `invoke!` in the directly awaited computation flow, and await each operation before starting another.

Effect operations are also not cancel-safe. The first poll of a `yield_!` or `invoke!` operation hands its effect to the runner. If the operation is then dropped before completing, for example by losing a `select!` race, the abandoned effect may still be handled at the computation's next suspension, with its resume value discarded, or it may be silently dropped. Do not race effect operations against other futures; await each one to completion.
