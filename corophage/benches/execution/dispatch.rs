use corophage::prelude::*;
use divan::black_box;

use crate::fixtures::*;
use crate::runtime;

mod catching {
    use super::*;

    // 1000 is the successful path; 0 and 999 throw before the first/last tick.
    #[divan::bench(args = [0, 999, 1000])]
    fn single(bencher: divan::Bencher, throw_at: usize) {
        bencher.bench(|| {
            black_box(
                fallible(black_box(1000), black_box(throw_at))
                    .catch()
                    .handle(tick)
                    .run_sync(),
            )
        });
    }

    #[divan::bench(args = [0, 999, 1000])]
    fn nested_forwarding(bencher: divan::Bencher, throw_at: usize) {
        bencher.bench(|| {
            black_box(
                catch_layer(black_box(1000), black_box(throw_at))
                    .catch()
                    .handle(tick)
                    .run_sync(),
            )
        });
    }

    #[divan::bench(args = [0, 999, 1000])]
    fn direct_result(bencher: divan::Bencher, throw_at: usize) {
        bencher.bench(|| {
            let mut sum = 0;
            let throw_at = black_box(throw_at);
            for i in 0..black_box(1000) {
                if i == throw_at {
                    return Err(Inner);
                }
                sum += black_box(black_box(1usize));
            }
            Ok(sum)
        });
    }
}

mod invocation {
    use super::*;

    // Match concrete types at the call site; boxing to select a depth would
    // change the operation being measured.
    macro_rules! run {
        ($fixture:ident, $n:expr) => {
            $fixture(black_box($n), false)
                .handle(tick)
                .handle(|effect| local_tick(effect, false))
                .run_sync()
        };
    }

    #[divan::bench(args = [1, 4, 8])]
    fn unhandled(bencher: divan::Bencher, depth: usize) {
        bencher.bench(|| {
            black_box(match depth {
                1 => run!(plain_1, 100),
                4 => run!(plain_4, 100),
                8 => run!(plain_8, 100),
                _ => unreachable!(),
            })
        });
    }

    #[divan::bench(args = [1, 4, 8])]
    fn partially_handled(bencher: divan::Bencher, depth: usize) {
        bencher.bench(|| {
            black_box(match depth {
                1 => run!(handled_1, 100),
                4 => run!(handled_4, 100),
                8 => run!(handled_8, 100),
                _ => unreachable!(),
            })
        });
    }
}

mod payload_size {
    use super::*;

    // Arrays remain inline on both sides; no allocator obscures transport cost.
    #[divan::bench(consts = [8, 64, 1024, 4096])]
    fn flat<const N: usize>(bencher: divan::Bencher) {
        bencher.bench(|| {
            black_box(
                payload_0::<N>(black_box(100))
                    .handle(payload::<N>)
                    .run_sync(),
            )
        });
    }

    #[divan::bench(consts = [8, 64, 1024, 4096])]
    fn forwarded_four_levels<const N: usize>(bencher: divan::Bencher) {
        bencher.bench(|| {
            black_box(
                payload_4::<N>(black_box(100))
                    .handle(payload::<N>)
                    .run_sync(),
            )
        });
    }

    #[divan::bench(consts = [8, 64, 1024, 4096])]
    fn direct<const N: usize>(bencher: divan::Bencher) {
        bencher.bench(|| {
            let mut bytes = black_box([7u8; N]);
            for _ in 0..black_box(100) {
                bytes = black_box(black_box(bytes));
            }
            black_box(bytes)
        });
    }
}

mod body_storage {
    use super::*;

    // Each row constructs exactly the same concrete workload, then optionally
    // erases only its outer body. Nested child bodies stay concrete in all rows.
    macro_rules! storage_benches {
        ($module:ident, $fixture:ident) => {
            mod $module {
                use super::*;

                #[divan::bench(args = [1, 1000])]
                fn concrete_sync(b: divan::Bencher, n: usize) {
                    b.bench(|| black_box($fixture(black_box(n)).handle(tick).run_sync()));
                }
                #[divan::bench(args = [1, 1000])]
                fn boxed_sync(b: divan::Bencher, n: usize) {
                    b.bench(|| black_box($fixture(black_box(n)).boxed().handle(tick).run_sync()));
                }
                #[divan::bench(args = [1, 1000])]
                fn boxed_send_sync(b: divan::Bencher, n: usize) {
                    b.bench(|| {
                        black_box($fixture(black_box(n)).boxed_send().handle(tick).run_sync())
                    });
                }
                #[divan::bench(args = [1, 1000])]
                fn concrete_async(b: divan::Bencher, n: usize) {
                    let rt = runtime();
                    b.bench(|| black_box(rt.block_on($fixture(black_box(n)).handle(tick).run())));
                }
                #[divan::bench(args = [1, 1000])]
                fn boxed_async(b: divan::Bencher, n: usize) {
                    let rt = runtime();
                    b.bench(|| {
                        black_box(rt.block_on($fixture(black_box(n)).boxed().handle(tick).run()))
                    });
                }
                #[divan::bench(args = [1, 1000])]
                fn boxed_send_async(b: divan::Bencher, n: usize) {
                    let rt = runtime();
                    b.bench(|| {
                        black_box(
                            rt.block_on($fixture(black_box(n)).boxed_send().handle(tick).run()),
                        )
                    });
                }
            }
        };
    }

    #[effectful(Tick)]
    fn nested_1(n: usize) -> usize {
        invoke!(ticks(n))
    }
    #[effectful(Tick)]
    fn nested_2(n: usize) -> usize {
        invoke!(nested_1(n))
    }
    #[effectful(Tick)]
    fn nested_3(n: usize) -> usize {
        invoke!(nested_2(n))
    }
    #[effectful(Tick)]
    fn nested_4(n: usize) -> usize {
        invoke!(nested_3(n))
    }

    storage_benches!(flat, ticks);
    storage_benches!(nested, nested_4);
}

mod lending {
    use super::*;

    #[divan::bench(args = [1, 1000])]
    fn borrowed_sync(b: divan::Bencher, n: usize) {
        let bytes = vec![7u8; 64];
        b.bench(|| {
            black_box(
                borrowed(&bytes, black_box(n))
                    .handle(|Borrow(bytes)| Control::resume(bytes))
                    .run_sync(),
            )
        });
    }

    #[divan::bench(args = [1, 1000])]
    fn ordinary_suspending(b: divan::Bencher, n: usize) {
        let rt = runtime();
        let bytes = vec![7u8; 64];
        b.bench(|| {
            black_box(
                rt.block_on(
                    borrowed(&bytes, black_box(n))
                        .handle(|Borrow(bytes)| async move {
                            suspend_once().await;
                            Control::resume(bytes)
                        })
                        .run(),
                ),
            )
        });
    }

    #[divan::bench(args = [1, 1000])]
    fn local_suspending(b: divan::Bencher, n: usize) {
        let rt = runtime();
        let bytes = vec![7u8; 64];
        b.bench(|| {
            let mut captured = 0usize;
            let result = rt.block_on(
                borrowed(&bytes, black_box(n))
                    .handle(corophage::local(async |Borrow(bytes)| {
                        // Keep an actual mutable capture loan alive across Pending.
                        let loan = &mut captured;
                        *loan += 1;
                        suspend_once().await;
                        *loan += 1;
                        Control::resume(bytes)
                    }))
                    .run(),
            );
            black_box((result, captured))
        });
    }

    #[divan::bench(args = [1, 1000])]
    fn local_stateful_suspending(b: divan::Bencher, n: usize) {
        let rt = runtime();
        let bytes = vec![7u8; 64];
        b.bench(|| {
            let mut state = 0usize;
            let mut captured = 0usize;
            let result = rt.block_on(
                borrowed(&bytes, black_box(n))
                    .with_state(&mut state)
                    .handle(corophage::local_stateful(
                        async |state: &mut usize, Borrow(bytes)| {
                            let loan = &mut captured;
                            *loan += 1;
                            *state += 1;
                            suspend_once().await;
                            *loan += 1;
                            *state += 1;
                            Control::resume(bytes)
                        },
                    ))
                    .run(),
            );
            black_box((result, state, captured))
        });
    }
}
