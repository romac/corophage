//! Stepwise: an interactive debugger for effectful computations.
//!
//! Inspired by the [Stepwise debugging library for Unison][stepwise], this
//! example demonstrates how algebraic effects let you annotate a computation
//! with pause points, then interpret them with a fully interactive debugger
//! -- all without modifying the original program logic.
//!
//! The debugger supports:
//! - **resume** (Enter): continue to the next pause point
//! - **back** (b): rewind to the previous pause point by re-running the
//!   computation and replaying prior decisions
//! - **go** (g): run the rest of the computation without stopping
//! - **silent** (s): run the rest silently, suppressing all output
//! - **replace** (r): replace the paused value before resuming
//!
//! The "back" feature is the showstopper: it re-runs the entire effectful
//! computation from scratch, replaying recorded handler decisions, then stops
//! one step earlier. This works because effectful computations are
//! deterministic given the same handler responses -- a property that falls
//! naturally out of the effect system.
//!
//! [stepwise]: https://share.unison-lang.org/@pchiusano/stepwise
//!
//! Run with: `cargo run --example debugger`

use std::fmt::Display;
use std::io::{self, BufRead, Write};
use std::str::FromStr;

use corophage::prelude::*;

// ── Effect ─────────────────────────────────────────────────────────────────

/// Pause the computation with a label and a value.
///
/// The handler may inspect, replace, or simply pass through the value.
/// The computation resumes with whatever `T` the handler provides.
#[effect(T)]
struct Pause<T> {
    label: String,
    value: T,
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn pause<T>(label: &str, value: T) -> Pause<T> {
    Pause {
        label: label.into(),
        value,
    }
}

// ── Program ────────────────────────────────────────────────────────────────

/// A small arithmetic program annotated with pause points.
///
/// This mirrors the Unison example:
/// ```text
/// x = pause "x" (1 + 1)
/// y = pause "y" (x + x + pause "what's this?" (99 + 1))
/// x + y
/// ```
#[effectful(Pause<i64>)]
fn example_program() -> i64 {
    let x = yield_!(pause("x", 1 + 1));
    let inner = yield_!(pause("what's this?", 99 + 1));
    let y = yield_!(pause("y", x + x + inner));
    x + y
}

// ── Debugger state ─────────────────────────────────────────────────────────

/// What the debugger decided at a pause point.
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct Decision<T> {
    label: String,
    original: T,
    resumed: T,
}

/// Debugger mode.
#[derive(Clone, Copy)]
enum Mode {
    /// Stop at each pause point and prompt the user.
    Step,
    /// Run to completion, printing each pause point.
    Go,
    /// Run to completion silently.
    Silent,
}

/// Mutable state threaded through the handler.
struct DebuggerState<T> {
    /// Decisions from a prior run to replay automatically.
    replay: Vec<Decision<T>>,
    /// Decisions recorded during this run.
    decisions: Vec<Decision<T>>,
    /// Current debugger mode.
    mode: Mode,
    /// Set to `true` when the user requests "back".
    went_back: bool,
}

// ── Handler ────────────────────────────────────────────────────────────────

fn debugger_handler<T>(state: &mut DebuggerState<T>, effect: Pause<T>) -> Control<T>
where
    T: Clone + Display + FromStr + PartialEq + Send + Sync,
    <T as FromStr>::Err: Display,
{
    let index = state.decisions.len();

    // Replay phase: auto-resume with the previously recorded value.
    if index < state.replay.len() {
        let decision = state.replay[index].clone();
        if decision.resumed != decision.original {
            println!(
                "    \u{23E9} {}: {} -> {}",
                decision.label, decision.original, decision.resumed
            );
        } else {
            println!("    \u{23E9} {}: {}", decision.label, decision.resumed);
        }
        state.decisions.push(decision.clone());
        return Control::resume(decision.resumed);
    }

    // Silent mode: pass through without printing.
    if matches!(state.mode, Mode::Silent) {
        state.decisions.push(Decision {
            label: effect.label,
            original: effect.value.clone(),
            resumed: effect.value.clone(),
        });
        return Control::resume(effect.value);
    }

    // Print the pause point.
    println!("    \u{1F440} {}", effect.label);
    println!("    {}", effect.value);

    // Go mode: print but don't stop.
    if matches!(state.mode, Mode::Go) {
        println!();
        state.decisions.push(Decision {
            label: effect.label,
            original: effect.value.clone(),
            resumed: effect.value.clone(),
        });
        return Control::resume(effect.value);
    }

    // Step mode: interactive prompt.
    println!("    \u{23F8}  Debugger paused \u{1F41B}");

    let can_back = !state.decisions.is_empty();
    let prompt = if can_back {
        "    resume (Enter), (b)ack, (g)o, (s)ilent, (r)eplace: "
    } else {
        "    resume (Enter), (g)o, (s)ilent, (r)eplace: "
    };

    loop {
        print!("{prompt}");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().lock().read_line(&mut input).unwrap();
        let cmd = input.trim();

        match cmd {
            // Resume with the original value.
            "" => {
                println!();
                state.decisions.push(Decision {
                    label: effect.label,
                    original: effect.value.clone(),
                    resumed: effect.value.clone(),
                });
                return Control::resume(effect.value);
            }

            // Back: pop the last decision and cancel so we re-run.
            "b" if can_back => {
                state.decisions.pop();
                state.went_back = true;
                return Control::cancel();
            }

            // Go: switch to Go mode and resume.
            "g" => {
                println!();
                state.mode = Mode::Go;
                state.decisions.push(Decision {
                    label: effect.label,
                    original: effect.value.clone(),
                    resumed: effect.value.clone(),
                });
                return Control::resume(effect.value);
            }

            // Silent: switch to Silent mode and resume.
            "s" => {
                state.mode = Mode::Silent;
                state.decisions.push(Decision {
                    label: effect.label,
                    original: effect.value.clone(),
                    resumed: effect.value.clone(),
                });
                return Control::resume(effect.value);
            }

            // Replace: read a new value from stdin.
            "r" => {
                print!("    > ");
                io::stdout().flush().unwrap();

                let mut val_input = String::new();
                io::stdin().lock().read_line(&mut val_input).unwrap();

                match val_input.trim().parse::<T>() {
                    Ok(new_val) => {
                        println!();
                        state.decisions.push(Decision {
                            label: effect.label,
                            original: effect.value,
                            resumed: new_val.clone(),
                        });
                        return Control::resume(new_val);
                    }
                    Err(e) => {
                        println!("    Invalid value: {e}, try again.");
                    }
                }
            }

            _ => {
                println!("    Unknown command, try again.");
            }
        }
    }
}

// ── Main ───────────────────────────────────────────────────────────────────

fn main() {
    println!("=== Stepwise Debugger ===");
    println!();

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
                println!("    Result: {value}");
                break;
            }
            Err(_) if state.went_back => {
                replay = state.decisions;
                println!("    << Rewinding...");
                println!();
            }
            Err(e) => {
                panic!("Unexpected cancellation: {e}");
            }
        }
    }
}
