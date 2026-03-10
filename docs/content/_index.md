+++
title = "corophage"
template = "index.html"
+++

<section class="hero">
<div class="hero-inner">

# corophage

<p class="hero-tagline">Algebraic effect handlers for stable Rust.<br>Separate <em>what</em> your program does from <em>how</em> it gets done.</p>

<div class="hero-buttons">
<a href="/docs/" class="btn btn-primary">Get Started</a>
<a href="https://docs.rs/corophage" class="btn btn-secondary">API Docs</a>
</div>

<div class="hero-install">
<code>cargo add corophage</code>
</div>

</div>
</section>

<section class="features">
<div class="features-inner">
<div class="feature">

### Testable

Swap in mock handlers for testing without touching the real world. Your business logic stays pure and easy to verify.

</div>
<div class="feature">

### Composable

Attach handlers incrementally with the `Program` API. Partially-handled programs are first-class values you can pass around and extend.

</div>
<div class="feature">

### Stable Rust

No nightly required. Built on async coroutines via [fauxgen](https://github.com/jmkr/fauxgen) and heterogeneous lists via [frunk](https://github.com/lloydmeta/frunk).

</div>
<div class="feature">

### Fast

~10 ns per yield. Zero-cost dispatch — the compiler monomorphizes and inlines effect dispatch into flat branches.

</div>
</div>
</section>

<section class="example-section">
<div class="example-inner">

## Define effects, write logic, attach handlers

<div class="tabs">
<input type="radio" name="example-tabs" id="tab-sync" checked>
<label for="tab-sync">Sync</label>
<input type="radio" name="example-tabs" id="tab-async">
<label for="tab-async">Async</label>

<div class="tab-panel" id="panel-sync">
<div class="example-grid">
<div class="example-step">

#### 1. Define your effects

```rust
use corophage::prelude::*;

declare_effect!(Log<'a>(&'a str) -> ());
declare_effect!(Read(String) -> String);
declare_effect!(Cancel -> Never);

type Effs = Effects![Cancel, Log<'static>, Read];
```

</div>
<div class="example-step">

#### 2. Describe what to do

```rust
let program = Program::new(
    |y: Yielder<'_, Effs>| async move {
        y.yield_(Log("Starting...")).await;
        let data = y.yield_(Read("config.toml".into())).await;
        data.len()
    },
);
```

</div>
<div class="example-step">

#### 3. Decide how to do it

```rust
let program = program
    .handle(|_: Cancel| Control::cancel())
    .handle(|Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(|Read(path)| {
        Control::resume(std::fs::read_to_string(path).unwrap())
    });
```

</div>
<div class="example-step">

#### 4. Run it

```rust
let result = program.run_sync();

assert_eq!(result, Ok(42));
```

</div>
</div>
</div>

<div class="tab-panel" id="panel-async">
<div class="example-grid">
<div class="example-step">

#### 1. Define your effects

```rust
use corophage::prelude::*;

declare_effect!(Log<'a>(&'a str) -> ());
declare_effect!(Read(String) -> String);
declare_effect!(Cancel -> Never);

type Effs = Effects![Cancel, Log<'static>, Read];
```

</div>
<div class="example-step">

#### 2. Describe what to do

```rust
let program = Program::new(
    |y: Yielder<'_, Effs>| async move {
        y.yield_(Log("Starting...")).await;
        let data = y.yield_(Read("config.toml".into())).await;
        data.len()
    },
);
```

</div>
<div class="example-step">

#### 3. Decide how to do it

```rust
let program = program
    .handle(async |_: Cancel| Control::cancel())
    .handle(async |Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(async |Read(path)| {
        let data = tokio::fs::read_to_string(path).await.unwrap();
        Control::resume(data)
    });
```

</div>
<div class="example-step">

#### 4. Run it

```rust
let result = program.run().await;

assert_eq!(result, Ok(42));
```

</div>
</div>
</div>

</div>
</div>
</section>

<section class="highlight-section">
<div class="highlight-inner">

## Borrowed resume types with GATs

Handlers can resume computations with *borrowed* data — no cloning needed.

```rust
use corophage::prelude::*;
use std::collections::HashMap;

struct Lookup<'a> {
    map: &'a HashMap<String, String>,
    key: &'a str,
}

impl<'a> Effect for Lookup<'a> {
    // The handler resumes with a &str borrowed from the map
    type Resume<'r> = &'r str;
}

let map = HashMap::from([
    ("host".into(), "localhost".into()),
    ("port".into(), "5432".into()),
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

</div>
</section>

<section class="cta-section">
<div class="cta-inner">

## Ready to get started?

<div class="hero-buttons">
<a href="/docs/" class="btn btn-primary">Read the Guide</a>
<a href="https://github.com/romac/corophage" class="btn btn-secondary">View on GitHub</a>
</div>

</div>
</section>
