+++
title = "corophage"
template = "index.html"
+++

<section class="hero">
<div class="hero-inner">

<div class="hero-logo">
<img src="logo/corophage-2b.svg" alt="corophage logo">
</div>

# corophage

<p class="hero-tagline">Algebraic effect handlers for stable Rust.<br>Separate <em>what</em> your program does from <em>how</em> it gets done.</p>

<div class="hero-buttons">
<a href="docs/" class="btn btn-primary">Get Started</a>
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

No nightly required. Built on async coroutines via [fauxgen](https://github.com/jmkr/fauxgen) and hlists/coproducts via [frunk](https://github.com/lloydmeta/frunk).

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

<p class="section-intro">Your program describes <em>what</em> to do by yielding effects.</p>

<div class="example-shared">
<div class="example-step">

#### Define your effects

Each effect is a struct annotated with `#[effect(ResumeType)]`.
The resume type defines what the handler sends back.

```rust
use corophage::prelude::*;

#[effect(())]
struct Log<'a>(&'a str);

#[effect(String)]
struct Read(String);

#[effect(Never)]
struct Cancel;

type Effs = Effects![Cancel, Log<'static>, Read];
```

</div>
<div class="example-step">

#### Describe what to do

Use `#[effectful]` to write effectful functions with `yield_!()`.
Your program doesn't know or care how effects are handled.

```rust
#[effectful(Cancel, Log<'static>, Read)]
fn program() -> usize {
    yield_!(Log("Starting..."));
    let data = yield_!(Read("config.toml".into()));
    data.len()
}
```

</div>
</div>

<p class="section-transition">Now decide <em>how</em> to handle each effect.</p>

<div class="tabs">
<input type="radio" name="example-tabs" id="tab-sync" checked>
<label for="tab-sync">Sync</label>
<input type="radio" name="example-tabs" id="tab-async">
<label for="tab-async">Async</label>
<input type="radio" name="example-tabs" id="tab-testing">
<label for="tab-testing">Mocks</label>

<div class="tab-panel" id="panel-sync">
<p class="tab-description">Run with plain closures as handlers.</p>

```rust
let result = program()
    .handle(|_: Cancel| Control::cancel())
    .handle(|Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(|Read(path)| {
        Control::resume(std::fs::read_to_string(path).unwrap())
    })
    .run_sync();

assert_eq!(result, Ok(42));
```

</div>

<div class="tab-panel" id="panel-async">
<p class="tab-description">Use async closures and <code>.await</code> real I/O.</p>

```rust
let result = program()
    .handle(async |_: Cancel| Control::cancel())
    .handle(async |Log(msg)| {
        println!("{msg}");
        Control::resume(())
    })
    .handle(async |Read(path)| {
        let data = tokio::fs::read_to_string(path).await.unwrap();
        Control::resume(data)
    })
    .run().await;

assert_eq!(result, Ok(42));
```

</div>

<div class="tab-panel" id="panel-testing">
<p class="tab-description">Swap in mock handlers, test without side effects.</p>

```rust
let result = program()
    .handle(|_: Cancel| Control::cancel())
    .handle(|Log(_)| Control::resume(())) // silent
    .handle(|Read(_)| {
        // Fake data instead of reading from disk
        Control::resume("mock content!".into())
    })
    .run_sync();

// No filesystem access, no stdout output
assert_eq!(result, Ok(13));
```

</div>

</div>
</div>

<p class="section-transition">The effects and logic stay the same — only the handlers change.</p>

</section>

<section class="highlight-section">
<div class="highlight-inner">

## More features

<div class="tabs">
<input type="radio" name="highlight-tabs" id="tab-stateful" checked>
<label for="tab-stateful">Shared state</label>
<input type="radio" name="highlight-tabs" id="tab-borrow">
<label for="tab-borrow">Borrowed resume types</label>
<input type="radio" name="highlight-tabs" id="tab-borrowed-effects">
<label for="tab-borrowed-effects">Borrowed effects</label>

<div class="tab-panel" id="panel-stateful">

Handlers can share mutable state. The state is passed as an argument to every handler.

```rust
use corophage::prelude::*;

#[effect(u64)]
struct Counter;

#[effectful(Counter)]
fn count_up() -> u64 {
    let a = yield_!(Counter);
    let b = yield_!(Counter);
    a + b
}

let mut count: u64 = 0;

let result = count_up()
    .handle(|s: &mut u64, _: Counter| {
        *s += 1;
        Control::resume(*s)
    })
    .run_sync_stateful(&mut count);

assert_eq!(result, Ok(3));  // 1 + 2
assert_eq!(count, 2);       // handler was called twice
```

</div>

<div class="tab-panel" id="panel-borrow">

Handlers can resume computations with *borrowed* data — no cloning needed.  
Because _`Effect::Resume<'r>`_ is a GAT, handlers can return references instead of owned values.

```rust
use corophage::prelude::*;
use std::collections::HashMap;

#[effect(&'r str)]
struct Lookup<'a> {
    map: &'a HashMap<String, String>,
    key: &'a str,
}

let map = HashMap::from([
    ("host".into(), "localhost".into()),
    ("port".into(), "5432".into()),
]);

// Borrowed effects need Program::new for explicit capture
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

<div class="tab-panel" id="panel-borrowed-effects">

Effects can borrow data from the local scope by using a non-_`'static`_ lifetime.

```rust
use corophage::prelude::*;

#[effect(())]
struct Log<'a>(pub &'a str);

let msg = String::from("hello from a local string");
let msg_ref = msg.as_str();

// Borrowed effects need Program::new for explicit capture
let result = Program::new(move |y: Yielder<'_, Effects![Log<'_>]>| async move {
    y.yield_(Log(msg_ref)).await;
})
.handle(|Log(m)| { println!("{m}"); Control::resume(()) })
.run_sync();

assert_eq!(result, Ok(()));
```

</div>

</div>
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
