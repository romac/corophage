use std::future::Future;
use std::future::pending;
use std::task::Context;
use std::task::Waker;

use corophage::prelude::*;
use divan::black_box;

use crate::fixtures::*;
use crate::runtime;

mod cancellation {
    use super::*;

    // A local cancellation at the leaf crosses all handled invocation layers;
    // the unhandled comparison cancels the same leaf effect at the root.
    macro_rules! run {
        ($fixture:ident, $async:expr, $rt:expr) => {{
            let p = $fixture(black_box(100), true)
                .handle(tick)
                .handle(|effect| local_tick(effect, true));
            if $async {
                $rt.block_on(p.run())
            } else {
                p.run_sync()
            }
        }};
    }

    #[divan::bench(args = [1, 4, 8])]
    fn unhandled_sync(b: divan::Bencher, depth: usize) {
        let rt = runtime();
        b.bench(|| {
            black_box(match depth {
                1 => run!(plain_1, false, rt),
                4 => run!(plain_4, false, rt),
                8 => run!(plain_8, false, rt),
                _ => unreachable!(),
            })
        });
    }

    #[cfg(not(comparison_v070))]
    #[divan::bench(args = [1, 4, 8])]
    fn handled_sync(b: divan::Bencher, depth: usize) {
        let rt = runtime();
        b.bench(|| {
            black_box(match depth {
                1 => run!(handled_1, false, rt),
                4 => run!(handled_4, false, rt),
                8 => run!(handled_8, false, rt),
                _ => unreachable!(),
            })
        });
    }

    #[divan::bench(args = [1, 4, 8])]
    fn unhandled_async(b: divan::Bencher, depth: usize) {
        let rt = runtime();
        b.bench(|| {
            black_box(match depth {
                1 => run!(plain_1, true, rt),
                4 => run!(plain_4, true, rt),
                8 => run!(plain_8, true, rt),
                _ => unreachable!(),
            })
        });
    }

    #[cfg(not(comparison_v070))]
    #[divan::bench(args = [1, 4, 8])]
    fn handled_async(b: divan::Bencher, depth: usize) {
        let rt = runtime();
        b.bench(|| {
            black_box(match depth {
                1 => run!(handled_1, true, rt),
                4 => run!(handled_4, true, rt),
                8 => run!(handled_8, true, rt),
                _ => unreachable!(),
            })
        });
    }
}

mod suspended_drop {
    use super::*;

    macro_rules! drop_bench {
        ($name:ident, $fixture:ident, $pending_handler:expr) => {
            #[divan::bench(args = [64, 4096], sample_size = 1)]
            fn $name(b: divan::Bencher, bytes: usize) {
                b.with_inputs(|| {
                    let owned = || Owned {
                        bytes: vec![7; bytes],
                        guard: (),
                    };
                    let mut future = Box::pin(
                        $fixture(owned(), owned())
                            .handle(|effect| async move {
                                if $pending_handler {
                                    pending::<()>().await;
                                }
                                Control::resume(effect)
                            })
                            .run(),
                    );
                    // Setup includes allocation and the first poll. Timing covers
                    // dropping suspended frames, the handler, and both payloads.
                    assert!(
                        future
                            .as_mut()
                            .poll(&mut Context::from_waker(Waker::noop()))
                            .is_pending()
                    );
                    future
                })
                .bench_local_values(drop);
            }
        };
    }

    drop_bench!(flat_pending_handler, held_0, true);
    drop_bench!(nested_pending_handler, held_4, true);
    drop_bench!(flat_pending_body, held_0, false);
    drop_bench!(nested_pending_body, held_4, false);
}
