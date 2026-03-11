//! Benchmarks for corophage effect handler library.

use std::cell::RefCell;

use corophage::prelude::*;

// =============================================================================
// Minimal effects for benchmarking (no I/O)
// =============================================================================

#[effect(())]
struct Noop;

#[effect(u64)]
struct GetValue;

#[effect(String)]
struct Alloc;

struct IncrementResult;

#[effect(IncrementResult)]
struct Increment;

struct DecrementResult;

#[effect(DecrementResult)]
struct Decrement;

// =============================================================================
// Effectful computations
// =============================================================================

#[effectful(Noop)]
fn empty_program() -> () {}

#[effectful(Noop)]
fn single_yield_program() -> () {
    yield_!(Noop);
}

#[effectful(Noop)]
fn multi_yield_program(n: usize) -> () {
    for _ in 0..n {
        yield_!(Noop);
    }
}

#[effectful(Noop, GetValue, Alloc, Increment, Decrement)]
fn dispatch_first_program() -> () {
    yield_!(Noop);
}

#[effectful(Noop, GetValue, Alloc, Increment, Decrement)]
fn dispatch_middle_program() -> () {
    yield_!(Alloc);
}

#[effectful(Noop, GetValue, Alloc, Increment, Decrement)]
fn dispatch_last_program() -> () {
    yield_!(Decrement);
}

#[effectful(Noop, GetValue, Alloc)]
fn alloc_program() -> () {
    let _ = yield_!(Alloc);
}

#[effectful(Noop, GetValue, Alloc)]
fn stateful_program() -> u64 {
    let value = yield_!(GetValue);
    yield_!(Noop);
    value
}

// =============================================================================
// Sub-programs for composition benchmarks
// =============================================================================

#[effectful(Noop)]
fn sub_noop() -> () {
    yield_!(Noop);
}

#[effectful(Noop)]
fn sub_multi_noop(n: usize) -> () {
    for _ in 0..n {
        yield_!(Noop);
    }
}

#[effectful(Noop)]
fn invoke_single() -> () {
    invoke!(sub_noop());
}

#[effectful(Noop)]
fn invoke_sequential(n: usize) -> () {
    for _ in 0..n {
        invoke!(sub_noop());
    }
}

#[effectful(Noop)]
fn invoke_nested_inner() -> () {
    invoke!(sub_noop());
}

#[effectful(Noop)]
fn invoke_nested_middle() -> () {
    invoke!(invoke_nested_inner());
}

#[effectful(Noop)]
fn invoke_nested_outer() -> () {
    invoke!(invoke_nested_middle());
}

#[effectful(GetValue)]
fn sub_get_value() -> u64 {
    yield_!(GetValue)
}

#[effectful(Noop, GetValue, Alloc)]
fn invoke_subset_effects() -> u64 {
    yield_!(Noop);
    let value = invoke!(sub_get_value());
    yield_!(Noop);
    value
}

// =============================================================================
// Sync benchmarks
// =============================================================================

mod sync_benches {
    use super::*;

    #[divan::bench]
    fn coroutine_creation(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(single_yield_program()));
    }

    #[divan::bench]
    fn empty_coroutine(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                empty_program()
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn single_yield(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                single_yield_program()
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench(args = [10, 100, 1000])]
    fn yield_scaling(bencher: divan::Bencher, n: usize) {
        bencher.bench(|| {
            divan::black_box(
                multi_yield_program(n)
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn stateless_handler(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                single_yield_program()
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn stateful_handler(bencher: divan::Bencher) {
        bencher.bench(|| {
            let mut state = 42u64;
            divan::black_box(
                stateful_program()
                    .handle(|_s: &mut u64, _: Noop| Control::resume(()))
                    .handle(|s: &mut u64, _: GetValue| Control::resume(*s))
                    .handle(|_s: &mut u64, _: Alloc| Control::resume(String::new()))
                    .run_sync_stateful(&mut state),
            )
        });
    }

    #[divan::bench]
    fn refcell_pattern(bencher: divan::Bencher) {
        bencher.bench(|| {
            let state = RefCell::new(42u64);
            divan::black_box(
                stateful_program()
                    .handle(|_: Noop| Control::resume(()))
                    .handle(|_: GetValue| Control::resume(*state.borrow()))
                    .handle(|_: Alloc| Control::resume(String::new()))
                    .run_sync(),
            )
        });
    }
}

// =============================================================================
// Async benchmarks
// =============================================================================

mod async_benches {
    use super::*;
    use tokio::runtime;

    fn runtime() -> runtime::Runtime {
        runtime::Builder::new_current_thread().build().unwrap()
    }

    #[divan::bench]
    fn single_yield(bencher: divan::Bencher) {
        let rt = runtime();
        bencher.bench(|| {
            rt.block_on(async {
                divan::black_box(
                    single_yield_program()
                        .handle(async |_: Noop| Control::resume(()))
                        .run(),
                )
                .await
            })
        });
    }

    #[divan::bench(args = [10, 100, 1000])]
    fn yield_scaling(bencher: divan::Bencher, n: usize) {
        let rt = runtime();
        bencher.bench(|| {
            rt.block_on(async {
                divan::black_box(
                    multi_yield_program(n)
                        .handle(async |_: Noop| Control::resume(()))
                        .run(),
                )
                .await
            })
        });
    }

    #[divan::bench]
    fn stateful_handler(bencher: divan::Bencher) {
        let rt = runtime();
        bencher.bench(|| {
            rt.block_on(async {
                let mut state = 42u64;
                divan::black_box(
                    stateful_program()
                        .handle(async |_s: &mut u64, _: Noop| Control::resume(()))
                        .handle(async |s: &mut u64, _: GetValue| Control::resume(*s))
                        .handle(async |_s: &mut u64, _: Alloc| Control::resume(String::new()))
                        .run_stateful(&mut state),
                )
                .await
            })
        });
    }
}

// =============================================================================
// Effect dispatch position benchmarks
// =============================================================================

mod dispatch_benches {
    use super::*;

    #[divan::bench]
    fn dispatch_first_effect(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                dispatch_first_program()
                    .handle(|_: Noop| Control::resume(()))
                    .handle(|_: GetValue| Control::resume(42))
                    .handle(|_: Alloc| Control::resume(String::new()))
                    .handle(|_: Increment| Control::resume(IncrementResult))
                    .handle(|_: Decrement| Control::resume(DecrementResult))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn dispatch_middle_effect(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                dispatch_middle_program()
                    .handle(|_: Noop| Control::resume(()))
                    .handle(|_: GetValue| Control::resume(42))
                    .handle(|_: Alloc| Control::resume(String::new()))
                    .handle(|_: Increment| Control::resume(IncrementResult))
                    .handle(|_: Decrement| Control::resume(DecrementResult))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn dispatch_last_effect(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                dispatch_last_program()
                    .handle(|_: Noop| Control::resume(()))
                    .handle(|_: GetValue| Control::resume(42))
                    .handle(|_: Alloc| Control::resume(String::new()))
                    .handle(|_: Increment| Control::resume(IncrementResult))
                    .handle(|_: Decrement| Control::resume(DecrementResult))
                    .run_sync(),
            )
        });
    }
}

// =============================================================================
// Handler complexity benchmarks
// =============================================================================

mod handler_complexity_benches {
    use super::*;

    #[divan::bench]
    fn noop_handler(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                single_yield_program()
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn allocating_handler(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                alloc_program()
                    .handle(|_: Noop| Control::resume(()))
                    .handle(|_: GetValue| Control::resume(42))
                    .handle(|_: Alloc| Control::resume("allocated string".to_string()))
                    .run_sync(),
            )
        });
    }
}

// =============================================================================
// Program composition benchmarks
// =============================================================================

mod composition_benches {
    use super::*;

    #[divan::bench]
    fn invoke_single_sub(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                invoke_single()
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench(args = [1, 10, 100])]
    fn invoke_sequential_subs(bencher: divan::Bencher, n: usize) {
        bencher.bench(|| {
            divan::black_box(
                invoke_sequential(n)
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn invoke_nested_3_deep(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                invoke_nested_outer()
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn invoke_subset(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                invoke_subset_effects()
                    .handle(|_: Noop| Control::resume(()))
                    .handle(|_: GetValue| Control::resume(42))
                    .handle(|_: Alloc| Control::resume(String::new()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn invoke_vs_inline(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                sub_multi_noop(10)
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    #[divan::bench]
    fn invoke_vs_invoke(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                invoke_sequential(10)
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }
}

fn main() {
    divan::main();
}
