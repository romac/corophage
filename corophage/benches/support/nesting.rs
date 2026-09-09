//! Nested fixtures shared by the stress benchmark and its depth regression tests.
//!
//! Depth counts active programs, including the leaf (depth 1).
//! Benchmarks pass `|| ()`; tests return a guard that observes active bodies.

use corophage::prelude::*;

#[effect(())]
pub struct Noop;

#[effectful(Noop)]
pub fn nest_leaf<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    yield_!(Noop);
}

// Fixed-depth chains retain concrete body futures. Recursive programs can use
// #[effectful(Noop, boxed)], which adds a body allocation at each level.

#[effectful(Noop)]
pub fn nest_2<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_leaf(enter));
}

#[effectful(Noop)]
pub fn nest_3<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_2(enter));
}

#[effectful(Noop)]
pub fn nest_4<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_3(enter));
}

#[effectful(Noop)]
pub fn nest_5<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_4(enter));
}

#[effectful(Noop)]
pub fn nest_6<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_5(enter));
}

#[effectful(Noop)]
pub fn nest_7<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_6(enter));
}

#[effectful(Noop)]
pub fn nest_8<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_7(enter));
}

#[effectful(Noop)]
pub fn nest_9<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_8(enter));
}

#[effectful(Noop)]
pub fn nest_10<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_9(enter));
}

#[effectful(Noop)]
pub fn nest_11<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_10(enter));
}

#[effectful(Noop)]
pub fn nest_12<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_11(enter));
}

#[effectful(Noop)]
pub fn nest_13<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_12(enter));
}

#[effectful(Noop)]
pub fn nest_14<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_13(enter));
}

#[effectful(Noop)]
pub fn nest_15<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_14(enter));
}

#[effectful(Noop)]
pub fn nest_16<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_15(enter));
}

#[effectful(Noop)]
pub fn nest_17<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_16(enter));
}

#[effectful(Noop)]
pub fn nest_18<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_17(enter));
}

#[effectful(Noop)]
pub fn nest_19<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_18(enter));
}

#[effectful(Noop)]
pub fn nest_20<G>(enter: impl Fn() -> G + Copy) -> () {
    let _level = enter();
    invoke!(nest_19(enter));
}

// Deep nesting with yields at every level
#[effectful(Noop)]
pub fn nest_yield_leaf<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1
}

#[effectful(Noop)]
pub fn nest_yield_2<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_leaf(enter))
}

#[effectful(Noop)]
pub fn nest_yield_3<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_2(enter))
}

#[effectful(Noop)]
pub fn nest_yield_4<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_3(enter))
}

#[effectful(Noop)]
pub fn nest_yield_5<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_4(enter))
}

#[effectful(Noop)]
pub fn nest_yield_6<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_5(enter))
}

#[effectful(Noop)]
pub fn nest_yield_7<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_6(enter))
}

#[effectful(Noop)]
pub fn nest_yield_8<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_7(enter))
}

#[effectful(Noop)]
pub fn nest_yield_9<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_8(enter))
}

#[effectful(Noop)]
pub fn nest_yield_10<G>(enter: impl Fn() -> G + Copy) -> u64 {
    let _level = enter();
    yield_!(Noop);
    1 + invoke!(nest_yield_9(enter))
}
