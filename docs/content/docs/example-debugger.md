+++
title = "Example: Stepwise Debugger"
weight = 9
description = "An interactive debugger that can rewind time, built on a single generic Pause effect."
+++

The [debugger example](https://github.com/romac/corophage/blob/main/corophage/examples/debugger.rs) is an interactive debugger for effectful computations, inspired by the [Stepwise library for Unison](https://share.unison-lang.org/@pchiusano/stepwise). It lets you step through pause points, inspect and replace values, and — the interesting part — rewind to any previous point.

Run it with:

```
cargo run --example debugger
```

## The effect

The entire debugger is built on a single generic effect:

```rust
/// Pause the computation with a label and a value.
/// The handler may inspect, replace, or pass through the value.
#[effect(T)]
struct Pause<T> { label: String, value: T }
```

A handler receives each `Pause<T>` and decides what `T` to resume with. All debugger behavior — stepping, going, silencing, rewinding — lives in the handler. The type parameter lets you attach the debugger to any effectful computation whose pause values implement `Display`, `FromStr`, and `PartialEq`.

## The program

```rust
#[effectful(Pause<i64>)]
fn example_program() -> i64 {
    let x     = yield_!(pause("x", 1 + 1));
    let inner = yield_!(pause("what's this?", 99 + 1));
    let y     = yield_!(pause("y", x + x + inner));
    x + y
}
```

The program has no idea a debugger is attached. Swap in a different handler and it behaves differently — log the values, always pass through, fuzz the outputs, whatever you like.

## Debugger state

The handler is stateful, threading a `DebuggerState<T>` through each pause:

```rust
struct DebuggerState<T> {
    /// Decisions from a prior run to replay automatically.
    replay: Vec<Decision<T>>,
    /// Decisions recorded during this run.
    decisions: Vec<Decision<T>>,
    /// Current mode: Step (interactive), Go (print, no stop), Silent.
    mode: Mode,
    /// Set to true when the user presses "b" to go back.
    went_back: bool,
}
```

## Replay: how "back" works

The "back" feature is the showstopper. When the user presses `b`:

1. The handler pops the last decision from `decisions` and sets `went_back = true`.
2. It calls `Control::cancel()` to halt the computation immediately.
3. The main loop detects `went_back`, saves the remaining decisions as the `replay` list, and re-runs the computation from scratch.
4. On the re-run, the handler auto-resumes through all replayed decisions without stopping, then hands control back to the user one step earlier.

This works because effectful computations are *deterministic given the same handler responses*. The handler just feeds back its own recorded decisions:

```rust
fn debugger_handler<T>(state: &mut DebuggerState<T>, effect: Pause<T>) -> Control<T>
where
    T: Clone + Display + FromStr + PartialEq + Send + Sync,
    <T as FromStr>::Err: Display,
{
    let index = state.decisions.len();

    // Replay phase: auto-resume with the previously recorded value.
    if index < state.replay.len() {
        let decision = state.replay[index].clone();
        state.decisions.push(decision.clone());
        return Control::resume(decision.resumed);
    }

    // ... interactive prompt follows
}
```

The "back" command in the interactive prompt:

```rust
"b" if can_back => {
    state.decisions.pop();  // drop the last decision
    state.went_back = true;
    return Control::cancel();
}
```

## The main loop

```rust
let mut replay: Vec<Decision<i64>> = Vec::new();

loop {
    let mut state = DebuggerState {
        replay: replay.clone(),
        decisions: Vec::new(),
        mode: Mode::Step,
        went_back: false,
    };

    let result = example_program()
        .handle(debugger_handler)
        .run_sync_stateful(&mut state);

    match result {
        Ok(value) => {
            println!("Result: {value}");
            break;
        }
        Err(_) if state.went_back => {
            replay = state.decisions; // replay one fewer decision next run
            println!("<< Rewinding...");
        }
        Err(e) => panic!("Unexpected cancellation: {e}"),
    }
}
```

Each re-run is a full execution of `example_program()` from scratch. The effect system guarantees that replaying the same handler decisions produces identical intermediate values, so the computation reaches the same state as before — one step earlier.

## What this shows

This example demonstrates a non-obvious capability of the effect system: because computations are deterministic functions of their handler responses, you can implement time-travel debugging with no special support from the runtime. The entire implementation is a single generic handler and a short loop. Making `Pause` generic means the same debugger infrastructure works for any value type that can be displayed and parsed.

See the [full source](https://github.com/romac/corophage/blob/main/corophage/examples/debugger.rs) for the complete interactive prompt, `replace` and `go` modes, and display formatting.
