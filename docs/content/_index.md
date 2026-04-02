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

<p class="hero-tagline"><em class="emphasis">Algebraic effects</em> for <em class="stable">stable</em> Rust.<br>Separate <em class="what">what</em> your program does from <em class="how">how</em> it gets done.</p>

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

Test your business logic by swapping in mock handlers. Same code, different effects, no real I/O involved.

</div>
<div class="feature">

### Composable

Build complex programs from simple building blocks. Effects propagate to the caller, no extra wiring needed.

</div>
<div class="feature">

### Practical

Works on stable Rust, no nightly required. Built on async coroutines via [fauxgen](https://github.com/Phantomical/fauxgen) and hlists/coproducts via [frunk](https://github.com/lloydmeta/frunk).

</div>
<div class="feature">

### Fast

~10 ns per yield. Zero-cost dispatch, the compiler monomorphizes and inlines effect dispatch into flat branches.

</div>
</div>
</section>

<section class="example-section">
<div class="example-inner">

## Define effects, write logic, attach handlers

<p class="section-intro">Your program describes <em class="what">what</em> to do by yielding effects.</p>

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
struct FileRead(String);

#[effect(Never)]
struct Cancel;

type Effs = Effects![Cancel, Log<'static>, FileRead];
```

</div>
<div class="example-step">

#### Describe what to do

Use `#[effectful]` to write effectful functions with `yield_!()`.
Your program doesn't know or care how effects are handled.

```rust
#[effectful(Cancel, Log<'static>, FileRead)]
fn program() -> usize {
    yield_!(Log("Starting..."));
    let data = yield_!(FileRead("config.toml".into()));
    data.len()
}
```

</div>
</div>

<p class="section-transition">Now decide <em class="how">how</em> to handle each effect.</p>

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
    .handle(|FileRead(path)| {
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
    .handle(async |FileRead(path)| {
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
    .handle(|FileRead(_)| {
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

<p class="section-transition">The effects and logic stay the same, only the handlers change.</p>

</section>

<section class="highlight-section">
<div class="highlight-inner">

## More features

<div class="tabs">
<input type="radio" name="highlight-tabs" id="tab-composition" checked>
<label for="tab-composition">Program composition</label>
<input type="radio" name="highlight-tabs" id="tab-stateful">
<label for="tab-stateful">Shared state</label>
<input type="radio" name="highlight-tabs" id="tab-borrow">
<label for="tab-borrow">Borrowed resumes</label>
<input type="radio" name="highlight-tabs" id="tab-borrowed-effects">
<label for="tab-borrowed-effects">Borrowed effects</label>

<div class="tab-panel" id="panel-composition">

Invoke sub-programs from within a program.  
Effects are forwarded automatically, the sub-program's effects just need to be a subset of the outer program's.

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

</div>

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

Handlers can resume computations with *borrowed* data, no cloning needed.  
Because _`Effect::Resume<'r>`_ is a GAT, handlers can return references instead of owned values.

```rust
use corophage::prelude::*;
use std::collections::HashMap;

#[effect(&'r str)]
struct Lookup<'a> {
    map: &'a HashMap<String, String>,
    key: &'a str,
}

// Pass borrowed data as function parameters.
// For inline use or fine-grained capture control,
// use Program::new directly instead.
#[effectful(Lookup<'a>)]
fn lookup<'a>(map: &'a HashMap<String, String>) -> String {
    let host: &str = yield_!(Lookup { map, key: "host" });
    let port: &str = yield_!(Lookup { map, key: "port" });
    format!("{host}:{port}")
}

let map = HashMap::from([
    ("host".into(), "localhost".into()),
    ("port".into(), "5432".into()),
]);

let result = lookup(&map)
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

// Pass borrowed data as function parameters.
// For inline use or fine-grained capture control,
// use Program::new directly instead.
#[effectful(Log<'a>)]
fn greet<'a>(msg: &'a str) {
    yield_!(Log(msg));
}

let msg = String::from("hello from a local string");

let result = greet(&msg)
    .handle(|Log(m)| { println!("{m}"); Control::resume(()) })
    .run_sync();

assert_eq!(result, Ok(()));
```

</div>

</div>
</div>
</section>

<section class="showcase-section">
<div class="showcase-inner">

## Real-world examples

<p>See how corophage handles production-style problems.</p>

<div class="showcase-grid">
<div class="showcase-card">

### Order processing saga

A multi-step async workflow where each step is an effect. When a step fails, `Control::cancel()` halts the computation — the caller reads accumulated state and runs compensating rollbacks in reverse order.

```rust
let result = process_order(order)
    .handle(handle_reserve)
    .handle(handle_payment)
    .handle(handle_confirmation)
    .handle(handle_shipping)
    .run_stateful(&mut state).await;

match result {
    Ok(summary)   => println!("done: {summary}"),
    Err(_)        => state.compensate(),
}
```

<div class="showcase-card-links">
<a href="/docs/example-saga/" class="btn btn-secondary">Read more</a>
<a href="https://github.com/romac/corophage/blob/main/corophage/examples/saga.rs">View source</a>
</div>

</div>
<div class="showcase-card">

### Stepwise debugger

An interactive debugger — with rewind — built on a single `Pause` effect. The "back" command cancels the computation and replays it from scratch, stopping one step earlier. This works because effectful computations are deterministic given the same handler responses.

```rust
loop {
    let result = example_program()
        .handle(debugger_handler)
        .run_sync_stateful(&mut state);

    if state.went_back {
        replay = state.decisions; // re-run one step earlier
    } else {
        break;
    }
}
```

<div class="showcase-card-links">
<a href="/docs/example-debugger/" class="btn btn-secondary">Read more</a>
<a href="https://github.com/romac/corophage/blob/main/corophage/examples/debugger.rs">View source</a>
</div>

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

<section class="acknowledgments-section">
<div class="acknowledgments-inner">

## Acknowledgments

`corophage` is heavily inspired by [`effing-mad`](https://github.com/rosefromthedead/effing-mad), a pioneering algebraic effects library for nightly Rust.
`effing-mad` demonstrated that algebraic effects and effect handlers are viable in Rust by leveraging coroutines to let effectful functions suspend, pass control to their callers, and resume with results.
While `effing-mad` requires nightly Rust for its `#[coroutine]`-based approach, `corophage` supports stable Rust by leveraging async coroutines via [`fauxgen`](https://github.com/Phantomical/fauxgen). Big thanks as well to [`frunk`](https://github.com/lloydmeta/frunk) for its coproduct and hlist implementation.

</div>
</section>
