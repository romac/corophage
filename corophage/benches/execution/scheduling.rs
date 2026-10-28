use divan::black_box;

use crate::fixtures::*;
use crate::runtime;

mod suspension {
    use super::*;

    #[divan::bench(args = [1, 1000])]
    fn ready_handler(b: divan::Bencher, n: usize) {
        let rt = runtime();
        b.bench(|| black_box(rt.block_on(ticks(black_box(n)).handle(tick).run())));
    }

    #[divan::bench(args = [1, 1000])]
    fn pending_handler(b: divan::Bencher, n: usize) {
        let rt = runtime();
        b.bench(|| {
            black_box(
                rt.block_on(
                    ticks(black_box(n))
                        .handle(|effect| async move {
                            suspend_once().await;
                            tick(effect)
                        })
                        .run(),
                ),
            )
        });
    }

    #[divan::bench(args = [1, 1000])]
    fn pending_body(b: divan::Bencher, n: usize) {
        let rt = runtime();
        b.bench(|| black_box(rt.block_on(suspending_body(black_box(n)).handle(tick).run())));
    }

    #[divan::bench(args = [1, 1000])]
    fn direct_sync(b: divan::Bencher, n: usize) {
        b.bench(|| {
            let mut sum = 0usize;
            for _ in 0..black_box(n) {
                sum += black_box(black_box(1usize));
            }
            black_box(sum)
        });
    }

    #[divan::bench(args = [1, 1000])]
    fn direct_async_ready(b: divan::Bencher, n: usize) {
        let rt = runtime();
        b.bench(|| {
            rt.block_on(async {
                let mut sum = 0usize;
                for _ in 0..black_box(n) {
                    sum += std::future::ready(black_box(black_box(1usize))).await;
                }
                black_box(sum)
            })
        });
    }

    #[divan::bench(args = [1, 1000])]
    fn direct_async_pending(b: divan::Bencher, n: usize) {
        let rt = runtime();
        b.bench(|| {
            rt.block_on(async {
                let mut sum = 0usize;
                for _ in 0..black_box(n) {
                    suspend_once().await;
                    sum += black_box(black_box(1usize));
                }
                black_box(sum)
            })
        });
    }
}

mod fairness {
    use super::*;

    // The current budget is 128. Keep these cases aligned with the library's
    // boundary regression test when changing that private implementation detail.
    #[divan::bench(args = [127, 128, 129, 255, 256, 257, 10000])]
    fn flat_throughput(b: divan::Bencher, n: usize) {
        let rt = runtime();
        b.bench(|| black_box(rt.block_on(ticks(black_box(n)).handle(tick).run())));
    }

    #[divan::bench(args = [127, 128, 129, 255, 256, 257, 10000])]
    fn short_handled_children(b: divan::Bencher, n: usize) {
        let rt = runtime();
        b.bench(|| black_box(rt.block_on(short_children(black_box(n)).run())));
    }

    // 0 = idle executor, 1 = flat load, 2 = short handled children. Creating
    // tasks and dropping the unfinished load are outside the timed region.
    // Each input owns a fresh LocalSet, so samples cannot leak background work.
    #[divan::bench(args = [0, 1, 2], sample_size = 1)]
    fn peer_first_poll_latency(b: divan::Bencher, load: usize) {
        let rt = runtime();
        b.with_inputs(|| PeerLatency::new(load))
            .bench_local_refs(|input| {
                black_box(rt.block_on(input.local.run_until(&mut input.peer)).unwrap())
            });
    }
}
