// Tests for: #[effectful] + invoke! + mutable references
//
// Verifies that invoke! (and Yielder::invoke) allow sequential mutable borrows
// by accepting sub-programs with shorter lifetimes.

use corophage::prelude::*;

#[effect(())]
struct DoWork;

struct State {
    count: u32,
}

impl State {
    fn do_something(&mut self) {
        self.count += 1;
    }
}

// ============================================================
// Test 1: Direct yields work fine with &mut
// ============================================================
fn process_direct<'a>(state: &'a mut State) -> Effectful<'a, Effects![DoWork], ()> {
    Program::new(move |y: Yielder<'_, Effects![DoWork]>| async move {
        state.do_something();
        y.yield_(DoWork).await;
        state.do_something();
        y.yield_(DoWork).await;
        state.do_something();
    })
}

#[test]
fn test_direct_yields_work() {
    let mut state = State { count: 0 };
    let result = process_direct(&mut state)
        .handle(|_: DoWork| Control::resume(()))
        .run_sync();

    assert_eq!(result, Ok(()));
    assert_eq!(state.count, 3);
}

// ============================================================
// Test 2: invoke! with mutable references (the originally broken pattern)
// ============================================================

#[effectful(DoWork)]
fn sub_fn<'a>(state: &'a mut State) -> () {
    state.do_something();
    yield_!(DoWork);
}

// This is the pattern from the issue report.
// Previously failed with: "cannot borrow value as mutable more than once at a time"
// Now works because Yielder::invoke accepts Effectful<'b, ...> where 'b < 'a.
#[effectful(DoWork)]
fn process_with_invoke<'a>(state: &'a mut State) -> () {
    invoke!(sub_fn(state));
    state.do_something();
    invoke!(sub_fn(state));
}

#[test]
fn test_invoke_with_mut_ref() {
    let mut state = State { count: 0 };
    let result = process_with_invoke(&mut state)
        .handle(|_: DoWork| Control::resume(()))
        .run_sync();

    assert_eq!(result, Ok(()));
    assert_eq!(state.count, 3); // sub_fn + do_something + sub_fn
}

// ============================================================
// Test 3: Manual Program::new + y.invoke with mutable references
// ============================================================

fn sub_fn_manual<'a>(state: &'a mut State) -> Effectful<'a, Effects![DoWork], ()> {
    Program::new(move |y: Yielder<'_, Effects![DoWork]>| async move {
        state.do_something();
        y.yield_(DoWork).await;
    })
}

fn process_manual<'a>(state: &'a mut State) -> Effectful<'a, Effects![DoWork], ()> {
    Program::new(move |y: Yielder<'_, Effects![DoWork]>| async move {
        y.invoke(sub_fn_manual(state)).await;
        state.do_something();
        y.invoke(sub_fn_manual(state)).await;
    })
}

#[test]
fn test_manual_invoke_with_mut_ref() {
    let mut state = State { count: 0 };
    let result = process_manual(&mut state)
        .handle(|_: DoWork| Control::resume(()))
        .run_sync();

    assert_eq!(result, Ok(()));
    assert_eq!(state.count, 3);
}

// ============================================================
// Test 4: invoke! with ? operator and mutable references
// ============================================================

#[effect(())]
struct Log;

#[effectful(DoWork, Log)]
fn sub_with_log<'a>(state: &'a mut State) -> Result<(), &'static str> {
    state.do_something();
    yield_!(DoWork);
    yield_!(Log);
    Ok(())
}

#[effectful(DoWork, Log)]
fn process_with_question_mark<'a>(state: &'a mut State) -> Result<(), &'static str> {
    invoke!(sub_with_log(state))?;
    state.do_something();
    invoke!(sub_with_log(state))?;
    Ok(())
}

#[test]
fn test_invoke_with_question_mark_and_mut_ref() {
    let mut state = State { count: 0 };
    let result = process_with_question_mark(&mut state)
        .handle(|_: DoWork| Control::resume(()))
        .handle(|_: Log| Control::resume(()))
        .run_sync();

    assert_eq!(result, Ok(Ok(())));
    assert_eq!(state.count, 3);
}

// ============================================================
// Test 5: Inline sub-function approach (always worked)
// ============================================================

async fn sub_fn_inline(y: &Yielder<'_, Effects![DoWork]>, state: &mut State) {
    state.do_something();
    y.yield_(DoWork).await;
}

fn process_inline<'a>(state: &'a mut State) -> Effectful<'a, Effects![DoWork], ()> {
    Program::new(move |y: Yielder<'_, Effects![DoWork]>| async move {
        sub_fn_inline(&y, state).await;
        state.do_something();
        sub_fn_inline(&y, state).await;
    })
}

#[test]
fn test_inline_sub_fn_works() {
    let mut state = State { count: 0 };
    let result = process_inline(&mut state)
        .handle(|_: DoWork| Control::resume(()))
        .run_sync();

    assert_eq!(result, Ok(()));
    assert_eq!(state.count, 3);
}
