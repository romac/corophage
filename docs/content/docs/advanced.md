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

#[effect(&'r str)]
struct Lookup<'a> {
    map: &'a HashMap<String, String>,
    key: &'a str,
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
