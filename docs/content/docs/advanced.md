+++
title = "Advanced Usage"
weight = 6
description = "Co, CoSend, borrowed data, and the direct API."
+++

## `Co` and `CoSend`

For cases where you need to pass a computation around before attaching handlers, you can use `Co` and `CoSend` directly.

```rust
use corophage::{Co, CoSend, sync, Control};
use corophage::prelude::*;

declare_effect!(FileRead(pub String) -> String);

// Co — the computation type (not Send)
let co: Co<'_, Effects![FileRead], String> = Co::new(|y| async move {
    y.yield_(FileRead("data.txt".to_string())).await
});

// Run directly with all handlers at once via hlist
let result = sync::run(co, &mut hlist![
    |FileRead(f)| Control::resume(format!("contents of {f}"))
]);

assert_eq!(result, Ok("contents of data.txt".to_string()));
```

`CoSend` is the `Send`-able variant for multi-threaded runtimes:

```rust
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

## Borrowing non-`'static` data

Effects can borrow data from the local scope by using a non-`'static` lifetime:

```rust
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
.handle(|Log(m)| { println!("{m}"); Control::resume(()) })
.run_sync();

assert_eq!(result, Ok(()));
```

## Borrowed resume types

Because `Effect::Resume<'r>` is a generic associated type (GAT), handlers can resume computations with *borrowed* data instead of requiring owned values.

```rust
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
    Control::resume(value.as_str())
})
.run_sync();

assert_eq!(result, Ok("localhost:5432".to_string()));
```

## Direct API

The direct API (`sync::run`, `sync::run_stateful`, `asynk::run`, `asynk::run_stateful`) accepts a `Co`/`CoSend` and an `hlist!` of all handlers at once. This is useful for concise one-shot execution but requires providing all handlers together in the correct `Effects![...]` order.
