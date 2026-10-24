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

`.boxed()` erases `Send` even when the original body was sendable. `.boxed_send()` retains that guarantee and checks the body. Each conversion allocates once for the body. `Effectful<'a, Effs, R, L>` remains the erased alias, including for callers generic over `L: Locality`. The concrete `Program` type now takes its body future as its third parameter; update explicit full `Program` signatures accordingly.

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

Local erased programs accept the same handlers as concrete programs. Sync and async handlers can be mixed, and async closures that borrow their captures across an await use `corophage::local` or `corophage::local_stateful`. The low-level `asynk::run` and `asynk::run_stateful` functions follow the same rules:

```rust
let mut paths = Vec::new();
let result = erased()
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

Before 0.7, local erased `.run()` and `.run_stateful()` accepted only async closures, including closures that borrow their captures, and rejected synchronous handlers and the adapters. To migrate, wrap closures that borrow their captures across an await with `corophage::local` or `corophage::local_stateful`.

Erased sendable programs retain explicit `Send` guarantees on `.run()` and `.run_stateful()`. The latter retains its `Send + Sync` handler bound. `.with_state()` instead selects the new mutable protocol and conditional `Send`, while keeping the already erased body.

Rust can report that an implementation is not general enough (for example of `Effect` or `FnOnce`) when a program with lifetime-parameterized effects or GAT resumes is awaited inside another async block that must be `Send`, such as `tokio::spawn(async { ... })`. This affects both stateless and stateful runners, and can occur even if the run future itself passes a `Send` assertion. Passing the run future to `tokio::spawn` directly, without an enclosing async block, avoids the problem. Otherwise, box the unhandled body with `.boxed_send()` and use the compatibility `.run()` or `.run_stateful(&mut state)` method. Both have an explicit `Send` return guarantee.

The same limitation applies to invoking a sub-program with handlers attached inside a body that must be `Send`, when any of the sub-program's effects has a lifetime parameter. Unhandled invocations are not affected. To keep such a body `Send`, box the invocation where it is created, while its lifetimes are still known:

```rust,ignore
let invocation: Pin<Box<dyn Future<Output = usize> + Send + '_>> =
    Box::pin(y.invoke_handled(child(text).handle(count)));
invocation.await
```

## Combinators and cancellation

Each run owns a frame that holds the pending effect and its resume value. While the runner polls the computation, the frame is registered on the current thread, and effect operations find it there. An operation therefore works inside any combinator that is polled within the computation's own poll, such as `select!`, `join!`, or `FuturesUnordered`, and inside boxed futures. Polling an operation anywhere else panics: after its computation has finished, from another task, or on another thread.

With an explicit yielder, `y.yield_(effect)` and `y.invoke(program)` are futures that a combinator can poll. Inside an `#[effectful]` function, wrap the awaiting macro in an async block instead, as in `async { yield_!(effect) }`.

Only one operation of a computation can be in flight at a time, because `yield_!` and `invoke!` borrow the yielder mutably.

Dropping an operation before it completes withdraws its effect:

- If the handler has not taken the effect yet, the effect is never handled.
- If the handler already resumed, the resume value is discarded. The handler's side effects remain.

This is weaker than cancel-safety. The runner does not poll the computation while a handler runs, so a competing branch cannot interrupt a handler. It can only win in the poll that yielded the effect, which withdraws it, or in the poll after the handler resumed, which discards the resume. Under `tokio::select!`'s random branch order, the second case is likely. Use `biased;` and list the effect branch first to rule it out:

```rust,ignore
let reply = tokio::select! {
    biased;
    reply = y.yield_(Ask) => Some(reply),
    () = shutdown.notified() => None,
};
```

For the same reason, `tokio::time::timeout` around an effect operation has no effect: the operation is polled again only after its handler finishes, and it completes first. Dropping an `invoke!` operation withdraws the effect it forwards and drops the sub-program.
