//! Debug Snapshot: exact state-change assertions for effectful computations.
//!
//! Inspired by [Point-Free's DebugSnapshot library for Swift][pf], this
//! example shows how algebraic effects enable a testing pattern where:
//!
//! 1. You perform an action on your model
//! 2. You assert only the *changes* to state
//! 3. Any unaccounted-for mutation is caught automatically
//!
//! In Swift, DebugSnapshot needs a macro to generate test-friendly snapshots
//! of `@Observable` classes. Here, the model uses Rust value semantics and all
//! model changes pass through `Get` and `Set`, so ordinary `Clone`, `Debug`,
//! and `PartialEq` implementations provide the required comparison.
//!
//! The assertion is exhaustive for changes to `NumberFactModel` made through
//! these handlers. It does not observe state or side effects outside the model.
//!
//! [pf]: https://www.pointfree.co/blog/posts/205-beta-preview-debugsnapshots
//!
//! Run with: `cargo run --example debug_snapshot`
//! Demonstrate a failure with:
//! `cargo run --example debug_snapshot -- --demonstrate-failure`

use corophage::prelude::*;

// ── State ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
struct NumberFactModel {
    count: i32,
    fact: Option<String>,
    is_loading: bool,
}

impl NumberFactModel {
    fn new() -> Self {
        Self {
            count: 0,
            fact: None,
            is_loading: false,
        }
    }
}

// ── Effects ────────────────────────────────────────────────────────────

/// Read the current model state.
#[effect(NumberFactModel)]
struct Get;

/// Replace the model state.
#[effect(())]
struct Set(NumberFactModel);

// ── Model actions ──────────────────────────────────────────────────────

#[effectful(Get, Set)]
fn increment_button_tapped() {
    let mut s = yield_!(Get);
    s.count += 1;
    s.fact = None;
    yield_!(Set(s));
}

#[effectful(Get, Set)]
fn decrement_button_tapped() {
    let mut s = yield_!(Get);
    s.count -= 1;
    yield_!(Set(s));
}

#[effectful(Get, Set)]
fn fact_button_tapped() {
    let mut s = yield_!(Get);
    s.is_loading = true;
    yield_!(Set(s));
}

#[effectful(Get, Set)]
fn fact_response_received(fact: String) {
    let mut s = yield_!(Get);
    s.fact = Some(fact);
    s.is_loading = false;
    yield_!(Set(s));
}

// ── Debug Snapshot: the `expect_changes` function ──────────────────────

/// Assert exactly how state changes after running an effectful action.
///
/// 1. Clones the current state as an "expected" baseline
/// 2. Applies `changes` to the baseline (your assertion of what should differ)
/// 3. Runs the action with Get/Set handlers that mutate `state`
/// 4. Compares actual vs expected
///
/// This is the core idea: if the action changes a field you didn't account
/// for in `changes`, the test fails. Exhaustive by default.
fn expect_changes(
    state: &mut NumberFactModel,
    action: Effectful<'_, Effects![Get, Set], ()>,
    changes: impl FnOnce(&mut NumberFactModel),
) {
    let mut expected = state.clone();
    changes(&mut expected);

    action
        .handle(|s: &mut NumberFactModel, _: Get| Control::resume(s.clone()))
        .handle(|s: &mut NumberFactModel, Set(new)| {
            *s = new;
            Control::resume(())
        })
        .run_sync_stateful(state)
        .expect("computation was cancelled");

    assert_eq!(
        &*state, &expected,
        "actual state did not match the expected changes"
    );
}

// ── Demo ───────────────────────────────────────────────────────────────

fn main() {
    println!("=== Debug Snapshot ===");
    println!();

    let mut model = NumberFactModel::new();

    // ✓ Increment: count goes from 0 to 1.
    expect_changes(&mut model, increment_button_tapped(), |s| {
        s.count = 1;
    });
    println!("  ✓ increment (count 0 → 1)");

    // ✓ Increment again: count goes from 1 to 2.
    expect_changes(&mut model, increment_button_tapped(), |s| {
        s.count = 2;
    });
    println!("  ✓ increment (count 1 → 2)");

    // ✓ Requesting a fact starts loading.
    expect_changes(&mut model, fact_button_tapped(), |s| {
        s.is_loading = true;
    });
    println!("  ✓ fact request starts loading");

    // ✓ Receiving a fact stores it and stops loading.
    expect_changes(
        &mut model,
        fact_response_received("2 is even".into()),
        |s| {
            s.fact = Some("2 is even".into());
            s.is_loading = false;
        },
    );
    println!("  ✓ fact response stores fact and stops loading");

    // ✓ Incrementing clears the fact. The assertion MUST account for this,
    //   otherwise `expect_changes` catches the discrepancy.
    expect_changes(&mut model, increment_button_tapped(), |s| {
        s.count = 3;
        s.fact = None; // Required! The action clears the fact.
    });
    println!("  ✓ increment clears fact (count 2 → 3)");

    // ✓ Decrement only touches count; fact and is_loading stay the same.
    expect_changes(&mut model, decrement_button_tapped(), |s| {
        s.count = 2;
    });
    println!("  ✓ decrement (count 3 → 2)");

    println!();
    println!("All state-change assertions passed.");

    if !std::env::args().any(|arg| arg == "--demonstrate-failure") {
        return;
    }

    println!();
    println!("Demonstrating a missing state-change assertion:");
    println!();

    // ✗ Forget to assert that incrementing clears the fact.
    //   We set up state with a fact, then increment without accounting
    //   for the fact being cleared.
    model.fact = Some("2 is even".into());

    expect_changes(&mut model, increment_button_tapped(), |s| {
        s.count = 3;
        // Oops! Forgot: s.fact = None
        // The action clears .fact, but we didn't assert that.
    });
}
