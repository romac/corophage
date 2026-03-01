//! Benchmarks for corophage effect handler library.

use std::cell::RefCell;

use corophage::prelude::*;

// =============================================================================
// Minimal effects for benchmarking (no I/O)
// =============================================================================

struct Noop;
impl Effect for Noop {
    type Resume<'r> = ();
}

struct GetValue;
impl Effect for GetValue {
    type Resume<'r> = u64;
}

struct Alloc;
impl Effect for Alloc {
    type Resume<'r> = String;
}

struct IncrementResult;

struct Increment;
impl Effect for Increment {
    type Resume<'r> = IncrementResult;
}

struct DecrementResult;

struct Decrement;
impl Effect for Decrement {
    type Resume<'r> = DecrementResult;
}

// =============================================================================
// Effect type aliases for varying handler counts
// =============================================================================

type OneEffect = Effects![Noop];
type ThreeEffects = Effects![Noop, GetValue, Alloc];
type FiveEffects = Effects![Noop, GetValue, Alloc, Increment, Decrement];

// =============================================================================
// Coroutine factories
// =============================================================================

fn empty_co() -> Co<'static, OneEffect, ()> {
    Co::new(|_y| async move {})
}

fn single_yield_co() -> Co<'static, OneEffect, ()> {
    Co::new(|y| async move {
        y.yield_(Noop).await;
    })
}

fn multi_yield_co(n: usize) -> Co<'static, OneEffect, ()> {
    Co::new(move |y| async move {
        for _ in 0..n {
            y.yield_(Noop).await;
        }
    })
}

fn dispatch_first_co() -> Co<'static, FiveEffects, ()> {
    Co::new(|y| async move {
        y.yield_(Noop).await;
    })
}

fn dispatch_middle_co() -> Co<'static, FiveEffects, ()> {
    Co::new(|y| async move {
        y.yield_(Alloc).await;
    })
}

fn dispatch_last_co() -> Co<'static, FiveEffects, ()> {
    Co::new(|y| async move {
        y.yield_(Decrement).await;
    })
}

fn alloc_co() -> Co<'static, ThreeEffects, ()> {
    Co::new(|y| async move {
        let _ = y.yield_(Alloc).await;
    })
}

fn stateful_co() -> Co<'static, ThreeEffects, u64> {
    Co::new(|y| async move {
        let value = y.yield_(GetValue).await;
        y.yield_(Noop).await;
        value
    })
}

// =============================================================================
// Sync benchmarks
// =============================================================================

mod sync_benches {
    use super::*;

    #[divan::bench]
    fn coroutine_creation(bencher: divan::Bencher) {
        bencher.bench(|| divan::black_box(single_yield_co()));
    }

    #[divan::bench]
    fn empty_coroutine(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = empty_co();
            let mut handler =
                hlist![|_: Noop| -> CoControl<'static, OneEffect> { CoControl::resume(()) }];
            divan::black_box(corophage::sync::run(co, &mut handler))
        });
    }

    #[divan::bench]
    fn single_yield(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = single_yield_co();
            let mut handler =
                hlist![|_: Noop| -> CoControl<'static, OneEffect> { CoControl::resume(()) }];
            divan::black_box(corophage::sync::run(co, &mut handler))
        });
    }

    #[divan::bench(args = [10, 100, 1000])]
    fn yield_scaling(bencher: divan::Bencher, n: usize) {
        bencher.bench(|| {
            let co = multi_yield_co(n);
            let mut handler =
                hlist![|_: Noop| -> CoControl<'static, OneEffect> { CoControl::resume(()) }];
            divan::black_box(corophage::sync::run(co, &mut handler))
        });
    }

    #[divan::bench]
    fn stateless_handler(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = single_yield_co();
            let mut handler =
                hlist![|_: Noop| -> CoControl<'static, OneEffect> { CoControl::resume(()) }];
            divan::black_box(corophage::sync::run(co, &mut handler))
        });
    }

    #[divan::bench]
    fn stateful_handler(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = stateful_co();
            let mut state = 42u64;
            let mut handler = hlist![
                |_s: &mut u64, _: Noop| -> CoControl<'static, ThreeEffects> {
                    CoControl::resume(())
                },
                |s: &mut u64, _: GetValue| -> CoControl<'static, ThreeEffects> {
                    CoControl::resume(*s)
                },
                |_s: &mut u64, _: Alloc| -> CoControl<'static, ThreeEffects> {
                    CoControl::resume(String::new())
                },
            ];
            divan::black_box(corophage::sync::run_with(co, &mut state, &mut handler))
        });
    }

    #[divan::bench]
    fn refcell_pattern(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = stateful_co();
            let state = RefCell::new(42u64);
            let mut handler = hlist![
                |_: Noop| -> CoControl<'static, ThreeEffects> { CoControl::resume(()) },
                |_: GetValue| -> CoControl<'static, ThreeEffects> {
                    CoControl::resume(*state.borrow())
                },
                |_: Alloc| -> CoControl<'static, ThreeEffects> { CoControl::resume(String::new()) },
            ];
            divan::black_box(corophage::sync::run(co, &mut handler))
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
                let co = single_yield_co();
                let mut handler = hlist![async |_: Noop| -> CoControl<'static, OneEffect> {
                    CoControl::resume(())
                }];
                divan::black_box(corophage::run(co, &mut handler).await)
            })
        });
    }

    #[divan::bench(args = [10, 100, 1000])]
    fn yield_scaling(bencher: divan::Bencher, n: usize) {
        let rt = runtime();
        bencher.bench(|| {
            rt.block_on(async {
                let co = multi_yield_co(n);
                let mut handler = hlist![async |_: Noop| -> CoControl<'static, OneEffect> {
                    CoControl::resume(())
                }];
                divan::black_box(corophage::run(co, &mut handler).await)
            })
        });
    }

    #[divan::bench]
    fn stateful_handler(bencher: divan::Bencher) {
        let rt = runtime();
        bencher.bench(|| {
            rt.block_on(async {
                let co = stateful_co();
                let mut state = 42u64;
                let mut handler = hlist![
                    async |_s: &mut u64, _: Noop| -> CoControl<'static, ThreeEffects> {
                        CoControl::resume(())
                    },
                    async |s: &mut u64, _: GetValue| -> CoControl<'static, ThreeEffects> {
                        CoControl::resume(*s)
                    },
                    async |_s: &mut u64, _: Alloc| -> CoControl<'static, ThreeEffects> {
                        CoControl::resume(String::new())
                    },
                ];
                divan::black_box(corophage::run_with(co, &mut state, &mut handler).await)
            })
        });
    }
}

// =============================================================================
// Effect dispatch position benchmarks
// =============================================================================

mod dispatch_benches {
    use super::*;

    fn five_effect_handler() -> frunk::HList![
        impl FnMut(Noop) -> CoControl<'static, FiveEffects>,
        impl FnMut(GetValue) -> CoControl<'static, FiveEffects>,
        impl FnMut(Alloc) -> CoControl<'static, FiveEffects>,
        impl FnMut(Increment) -> CoControl<'static, FiveEffects>,
        impl FnMut(Decrement) -> CoControl<'static, FiveEffects>,
    ] {
        hlist![
            |_: Noop| -> CoControl<'static, FiveEffects> { CoControl::resume(()) },
            |_: GetValue| -> CoControl<'static, FiveEffects> { CoControl::resume(42) },
            |_: Alloc| -> CoControl<'static, FiveEffects> { CoControl::resume(String::new()) },
            |_: Increment| -> CoControl<'static, FiveEffects> {
                CoControl::resume(IncrementResult)
            },
            |_: Decrement| -> CoControl<'static, FiveEffects> {
                CoControl::resume(DecrementResult)
            },
        ]
    }

    #[divan::bench]
    fn dispatch_first_effect(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = dispatch_first_co();
            let mut handler = five_effect_handler();
            divan::black_box(corophage::sync::run(co, &mut handler))
        });
    }

    #[divan::bench]
    fn dispatch_middle_effect(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = dispatch_middle_co();
            let mut handler = five_effect_handler();
            divan::black_box(corophage::sync::run(co, &mut handler))
        });
    }

    #[divan::bench]
    fn dispatch_last_effect(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = dispatch_last_co();
            let mut handler = five_effect_handler();
            divan::black_box(corophage::sync::run(co, &mut handler))
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
            let co = single_yield_co();
            let mut handler =
                hlist![|_: Noop| -> CoControl<'static, OneEffect> { CoControl::resume(()) }];
            divan::black_box(corophage::sync::run(co, &mut handler))
        });
    }

    #[divan::bench]
    fn allocating_handler(bencher: divan::Bencher) {
        bencher.bench(|| {
            let co = alloc_co();
            let mut handler = hlist![
                |_: Noop| -> CoControl<'static, ThreeEffects> { CoControl::resume(()) },
                |_: GetValue| -> CoControl<'static, ThreeEffects> { CoControl::resume(42) },
                |_: Alloc| -> CoControl<'static, ThreeEffects> {
                    CoControl::resume("allocated string".to_string())
                },
            ];
            divan::black_box(corophage::sync::run(co, &mut handler))
        });
    }
}

fn main() {
    divan::main();
}
