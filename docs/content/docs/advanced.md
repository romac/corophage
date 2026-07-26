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
