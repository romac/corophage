//! Stress-test benchmarks for corophage.
//!
//! Tests performance under extreme conditions:
//! - Deeply nested invoke chains
//! - High yield counts
//! - Wide effect sets (many effects registered)
//! - Combinations of the above

use corophage::prelude::*;

// =============================================================================
// Effects — we need many distinct ones for "wide effect set" benchmarks
// =============================================================================

#[effect(())]
struct E0;
#[effect(())]
struct E1;
#[effect(())]
struct E2;
#[effect(())]
struct E3;
#[effect(())]
struct E4;
#[effect(())]
struct E5;
#[effect(())]
struct E6;
#[effect(())]
struct E7;
#[effect(())]
struct E8;
#[effect(())]
struct E9;
#[effect(())]
struct E10;
#[effect(())]
struct E11;
#[effect(())]
struct E12;
#[effect(())]
struct E13;
#[effect(())]
struct E14;
#[effect(())]
struct E15;

#[effect(u64)]
struct Accumulate;

#[effect(())]
struct Noop;

// =============================================================================
// Deep nesting — chains of invoke N levels deep
// =============================================================================

#[effectful(Noop)]
fn nest_leaf() -> () {
    yield_!(Noop);
}

// Recursive nesting helper: each level invokes the next.
// We build fixed-depth chains since #[effectful] can't be recursive.

#[effectful(Noop)]
fn nest_2() -> () {
    invoke!(nest_leaf());
}

#[effectful(Noop)]
fn nest_3() -> () {
    invoke!(nest_2());
}

#[effectful(Noop)]
fn nest_4() -> () {
    invoke!(nest_3());
}

#[effectful(Noop)]
fn nest_5() -> () {
    invoke!(nest_4());
}

#[effectful(Noop)]
fn nest_6() -> () {
    invoke!(nest_5());
}

#[effectful(Noop)]
fn nest_7() -> () {
    invoke!(nest_6());
}

#[effectful(Noop)]
fn nest_8() -> () {
    invoke!(nest_7());
}

#[effectful(Noop)]
fn nest_9() -> () {
    invoke!(nest_8());
}

#[effectful(Noop)]
fn nest_10() -> () {
    invoke!(nest_9());
}

#[effectful(Noop)]
fn nest_15() -> () {
    invoke!(nest_10());
    invoke!(nest_5()); // 10 + 5 more levels (sequential, not nested)
}

#[effectful(Noop)]
fn nest_20() -> () {
    invoke!(nest_10());
    invoke!(nest_10());
}

// Deep nesting with yields at every level
#[effectful(Noop)]
fn nest_yield_leaf() -> u64 {
    yield_!(Noop);
    1
}

#[effectful(Noop)]
fn nest_yield_2() -> u64 {
    yield_!(Noop);
    1 + invoke!(nest_yield_leaf())
}

#[effectful(Noop)]
fn nest_yield_3() -> u64 {
    yield_!(Noop);
    1 + invoke!(nest_yield_2())
}

#[effectful(Noop)]
fn nest_yield_4() -> u64 {
    yield_!(Noop);
    1 + invoke!(nest_yield_3())
}

#[effectful(Noop)]
fn nest_yield_5() -> u64 {
    yield_!(Noop);
    1 + invoke!(nest_yield_4())
}

#[effectful(Noop)]
fn nest_yield_10() -> u64 {
    yield_!(Noop);
    let a = invoke!(nest_yield_5());
    yield_!(Noop);
    let b = invoke!(nest_yield_4());
    a + b + 2
}

// =============================================================================
// High yield counts
// =============================================================================

#[effectful(Noop)]
fn yield_n(n: usize) -> () {
    for _ in 0..n {
        yield_!(Noop);
    }
}

#[effectful(Accumulate)]
fn yield_accumulate(n: usize) -> u64 {
    let mut total = 0u64;
    for _ in 0..n {
        total += yield_!(Accumulate);
    }
    total
}

// =============================================================================
// Wide effect sets — programs declaring many effects but using few
// =============================================================================

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7)]
fn wide_8_use_first() -> () {
    yield_!(E0);
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7)]
fn wide_8_use_last() -> () {
    yield_!(E7);
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7)]
fn wide_8_use_all() -> () {
    yield_!(E0);
    yield_!(E1);
    yield_!(E2);
    yield_!(E3);
    yield_!(E4);
    yield_!(E5);
    yield_!(E6);
    yield_!(E7);
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7, E8, E9, E10, E11, E12, E13, E14, E15)]
fn wide_16_use_first() -> () {
    yield_!(E0);
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7, E8, E9, E10, E11, E12, E13, E14, E15)]
fn wide_16_use_last() -> () {
    yield_!(E15);
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7, E8, E9, E10, E11, E12, E13, E14, E15)]
fn wide_16_use_all() -> () {
    yield_!(E0);
    yield_!(E1);
    yield_!(E2);
    yield_!(E3);
    yield_!(E4);
    yield_!(E5);
    yield_!(E6);
    yield_!(E7);
    yield_!(E8);
    yield_!(E9);
    yield_!(E10);
    yield_!(E11);
    yield_!(E12);
    yield_!(E13);
    yield_!(E14);
    yield_!(E15);
}

// =============================================================================
// Wide effects + invoke (subset forwarding stress)
// =============================================================================

#[effectful(E0)]
fn wide_sub_e0() -> () {
    yield_!(E0);
}

#[effectful(E15)]
fn wide_sub_e15() -> () {
    yield_!(E15);
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7, E8, E9, E10, E11, E12, E13, E14, E15)]
fn wide_16_invoke_first() -> () {
    invoke!(wide_sub_e0());
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7, E8, E9, E10, E11, E12, E13, E14, E15)]
fn wide_16_invoke_last() -> () {
    invoke!(wide_sub_e15());
}

// =============================================================================
// Combined stress: deep nesting + many yields + wide effects
// =============================================================================

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7)]
fn combined_leaf() -> u64 {
    yield_!(E0);
    yield_!(E3);
    yield_!(E7);
    3
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7)]
fn combined_depth_2() -> u64 {
    yield_!(E1);
    yield_!(E4);
    let r = invoke!(combined_leaf());
    yield_!(E6);
    r + 3
}

#[effectful(E0, E1, E2, E3, E4, E5, E6, E7)]
fn combined_depth_3() -> u64 {
    yield_!(E2);
    yield_!(E5);
    let r = invoke!(combined_depth_2());
    yield_!(E7);
    r + 3
}

// Fan-out: invoke multiple sub-programs at each level
#[effectful(Noop)]
fn fan_out_leaf() -> u64 {
    yield_!(Noop);
    1
}

#[effectful(Noop)]
fn fan_out_2() -> u64 {
    let a = invoke!(fan_out_leaf());
    let b = invoke!(fan_out_leaf());
    a + b
}

#[effectful(Noop)]
fn fan_out_4() -> u64 {
    let a = invoke!(fan_out_2());
    let b = invoke!(fan_out_2());
    a + b
}

#[effectful(Noop)]
fn fan_out_8() -> u64 {
    let a = invoke!(fan_out_4());
    let b = invoke!(fan_out_4());
    a + b
}

#[effectful(Noop)]
fn fan_out_16() -> u64 {
    let a = invoke!(fan_out_8());
    let b = invoke!(fan_out_8());
    a + b
}

// =============================================================================
// Cancellation stress
// =============================================================================

#[effectful(Accumulate)]
fn cancellable_program(n: usize) -> u64 {
    let mut total = 0u64;
    for _ in 0..n {
        total += yield_!(Accumulate);
    }
    total
}

// =============================================================================
// Benchmarks: Deep nesting
// =============================================================================

mod deep_nesting {
    use super::*;

    #[divan::bench(args = [1, 2, 3, 5, 10, 15, 20])]
    fn nesting_depth(bencher: divan::Bencher, depth: usize) {
        bencher.bench(|| {
            let program = match depth {
                1 => nest_leaf(),
                2 => nest_2(),
                3 => nest_3(),
                5 => nest_5(),
                10 => nest_10(),
                15 => nest_15(),
                20 => nest_20(),
                _ => unreachable!(),
            };
            divan::black_box(program.handle(|_: Noop| Control::resume(())).run_sync())
        });
    }

    #[divan::bench(args = [1, 2, 3, 5, 10])]
    fn nesting_depth_with_yields(bencher: divan::Bencher, depth: usize) {
        bencher.bench(|| {
            let program = match depth {
                1 => nest_yield_leaf(),
                2 => nest_yield_2(),
                3 => nest_yield_3(),
                5 => nest_yield_5(),
                10 => nest_yield_10(),
                _ => unreachable!(),
            };
            divan::black_box(program.handle(|_: Noop| Control::resume(())).run_sync())
        });
    }

    #[divan::bench(args = [2, 4, 8, 16])]
    fn fan_out(bencher: divan::Bencher, leaves: usize) {
        bencher.bench(|| {
            let program = match leaves {
                2 => fan_out_2(),
                4 => fan_out_4(),
                8 => fan_out_8(),
                16 => fan_out_16(),
                _ => unreachable!(),
            };
            divan::black_box(program.handle(|_: Noop| Control::resume(())).run_sync())
        });
    }
}

// =============================================================================
// Benchmarks: High yield counts
// =============================================================================

mod many_yields {
    use super::*;

    #[divan::bench(args = [100, 1_000, 10_000, 100_000])]
    fn sync_yields(bencher: divan::Bencher, n: usize) {
        bencher.bench(|| {
            divan::black_box(yield_n(n).handle(|_: Noop| Control::resume(())).run_sync())
        });
    }

    #[divan::bench(args = [100, 1_000, 10_000, 100_000])]
    fn async_yields(bencher: divan::Bencher, n: usize) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        bencher.bench(|| {
            rt.block_on(async {
                divan::black_box(yield_n(n).handle(async |_: Noop| Control::resume(())).run()).await
            })
        });
    }

    #[divan::bench(args = [100, 1_000, 10_000, 100_000])]
    fn yield_with_value(bencher: divan::Bencher, n: usize) {
        bencher.bench(|| {
            divan::black_box(
                yield_accumulate(n)
                    .handle(|_: Accumulate| Control::resume(1u64))
                    .run_sync(),
            )
        });
    }

    #[divan::bench(args = [100, 1_000, 10_000])]
    fn stateful_counting(bencher: divan::Bencher, n: usize) {
        bencher.bench(|| {
            let mut counter = 0u64;
            divan::black_box(
                yield_accumulate(n)
                    .handle(|s: &mut u64, _: Accumulate| {
                        *s += 1;
                        Control::resume(*s)
                    })
                    .run_sync_stateful(&mut counter),
            )
        });
    }
}

// =============================================================================
// Benchmarks: Wide effect sets
// =============================================================================

mod wide_effects {
    use super::*;

    fn handle_8(program: Effectful<'_, Effects![E0, E1, E2, E3, E4, E5, E6, E7], ()>) -> () {
        program
            .handle(|_: E0| Control::resume(()))
            .handle(|_: E1| Control::resume(()))
            .handle(|_: E2| Control::resume(()))
            .handle(|_: E3| Control::resume(()))
            .handle(|_: E4| Control::resume(()))
            .handle(|_: E5| Control::resume(()))
            .handle(|_: E6| Control::resume(()))
            .handle(|_: E7| Control::resume(()))
            .run_sync()
            .unwrap()
    }

    fn handle_16(
        program: Effectful<
            '_,
            Effects![
                E0, E1, E2, E3, E4, E5, E6, E7, E8, E9, E10, E11, E12, E13, E14, E15
            ],
            (),
        >,
    ) -> () {
        program
            .handle(|_: E0| Control::resume(()))
            .handle(|_: E1| Control::resume(()))
            .handle(|_: E2| Control::resume(()))
            .handle(|_: E3| Control::resume(()))
            .handle(|_: E4| Control::resume(()))
            .handle(|_: E5| Control::resume(()))
            .handle(|_: E6| Control::resume(()))
            .handle(|_: E7| Control::resume(()))
            .handle(|_: E8| Control::resume(()))
            .handle(|_: E9| Control::resume(()))
            .handle(|_: E10| Control::resume(()))
            .handle(|_: E11| Control::resume(()))
            .handle(|_: E12| Control::resume(()))
            .handle(|_: E13| Control::resume(()))
            .handle(|_: E14| Control::resume(()))
            .handle(|_: E15| Control::resume(()))
            .run_sync()
            .unwrap()
    }

    #[divan::bench]
    fn eight_effects_use_first(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(handle_8(wide_8_use_first())));
    }

    #[divan::bench]
    fn eight_effects_use_last(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(handle_8(wide_8_use_last())));
    }

    #[divan::bench]
    fn eight_effects_use_all(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(handle_8(wide_8_use_all())));
    }

    #[divan::bench]
    fn sixteen_effects_use_first(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(handle_16(wide_16_use_first())));
    }

    #[divan::bench]
    fn sixteen_effects_use_last(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(handle_16(wide_16_use_last())));
    }

    #[divan::bench]
    fn sixteen_effects_use_all(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(handle_16(wide_16_use_all())));
    }

    // Subset forwarding through wide effect set
    #[divan::bench]
    fn sixteen_invoke_first(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(handle_16(wide_16_invoke_first())));
    }

    #[divan::bench]
    fn sixteen_invoke_last(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(handle_16(wide_16_invoke_last())));
    }
}

// =============================================================================
// Benchmarks: Combined stress
// =============================================================================

mod combined {
    use super::*;

    #[divan::bench]
    fn deep_wide_yieldy(bencher: divan::Bencher) {
        bencher.bench(|| {
            divan::black_box(
                combined_depth_3()
                    .handle(|_: E0| Control::resume(()))
                    .handle(|_: E1| Control::resume(()))
                    .handle(|_: E2| Control::resume(()))
                    .handle(|_: E3| Control::resume(()))
                    .handle(|_: E4| Control::resume(()))
                    .handle(|_: E5| Control::resume(()))
                    .handle(|_: E6| Control::resume(()))
                    .handle(|_: E7| Control::resume(()))
                    .run_sync(),
            )
        });
    }

    // Many sequential invokes, each doing multiple yields
    #[divan::bench(args = [10, 100])]
    fn sequential_invoke_with_yields(bencher: divan::Bencher, n: usize) {
        #[effectful(Noop)]
        fn sub_multi() -> () {
            for _ in 0..10 {
                yield_!(Noop);
            }
        }

        #[effectful(Noop)]
        fn invoke_n_multi(n: usize) -> () {
            for _ in 0..n {
                invoke!(sub_multi());
            }
        }

        bencher.bench(|| {
            divan::black_box(
                invoke_n_multi(n)
                    .handle(|_: Noop| Control::resume(()))
                    .run_sync(),
            )
        });
    }
}

// =============================================================================
// Benchmarks: Cancellation
// =============================================================================

mod cancellation {
    use super::*;

    #[divan::bench(args = [1, 10, 100, 1000])]
    fn cancel_after_n(bencher: divan::Bencher, n: usize) {
        bencher.bench(|| {
            let mut count = 0usize;
            divan::black_box(
                cancellable_program(n * 2)
                    .handle(|c: &mut usize, _: Accumulate| {
                        *c += 1;
                        if *c >= n {
                            Control::cancel()
                        } else {
                            Control::resume(1u64)
                        }
                    })
                    .run_sync_stateful(&mut count),
            )
        });
    }
}

fn main() {
    divan::main();
}
