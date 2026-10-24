+++
title = "Performance"
weight = 7
description = "Measured execution costs and concrete future storage tradeoffs."
+++

The current implementation at commit `f9b3925` was compared with v0.7.0 at commit `0e52000` on 2026-10-24. The candidate was measured with no tracked changes, and the recorded source and generated-fixture hashes match that commit. Both builds used the same macOS arm64 machine, Rust 1.99.1, release optimization, LTO, and one code generation unit.

## Execution costs

Each row measures a complete operation, including construction and execution. Async cases use a warmed Tokio runtime on the current thread with immediately ready handlers; the cooperative dispatch budget remains enabled.

| Operation | v0.7.0 | Current |
| --- | ---: | ---: |
| Construct and drop | 0.5 ns | 0.6 ns |
| One sync effect | 28.1 ns | 13.4 ns |
| One async effect | 91.4 ns | 77.7 ns |
| 1,000 sync effects | 9.6 µs | 6.9 µs |
| 1,000 async effects | 11.5 µs | 8.2 µs |
| 1,000 stateful sync effects | 9.5 µs | 6.8 µs |
| 1,000 stateful async effects | 12.0 µs | 8.1 µs |
| Five nested invocations | 203 ns | 104 ns |
| Ten nested invocations | 375 ns | 189 ns |
| Twenty nested invocations | 748 ns | 348 ns |

Nested cases perform one effect at the leaf and run synchronously. Depth counts invocation edges, so depth 20 has 21 active programs. Every generated level invokes the preceding level exactly once. This comparison uses its own generated chains; the Divan stress fixtures are measured separately below.

Each run owns a small frame that carries effects and resumes between the computation and its runner, and an invocation moves each effect from its child's frame into its parent's without a future per level. One synchronous effect takes 52% less time, the 1,000-effect loops 28–32% less, and nested invocations 49–53% less. Construction stays below a nanosecond; its 0.1 ns difference is within the timer's resolution for this case. Real handlers can have much larger costs than the dispatch itself. Do not apply these ratios to an application without measuring it.

## Stress nesting benchmarks

The Divan fixtures were measured on 2026-10-24 at commit `f9b3925`, and for v0.7.0 from an archived checkout, on macOS arm64, Rust 1.99.1, and the workspace's optimized bench profile with LTO. Each case has 1,000 samples of 1,000 iterations. The medians include program construction, synchronous execution with a no-op handler, and destruction.

This suite counts active programs: the leaf has depth 1, and depth 20 has 19 invocation edges. The leaf-only case yields once. The other case yields once in each program and returns the number of yields. Regression tests run these same fixtures with guards that check peak active depth and handler calls; timing runs use a no-op observer.

| Depth (active programs) | One leaf effect, v0.7.0 | Current | One effect per level, v0.7.0 | Current |
| --- | ---: | ---: | ---: | ---: |
| 1 | 28.32 ns | 9.75 ns | 28.74 ns | 8.58 ns |
| 2 | 59.78 ns | 14.33 ns | 68.74 ns | 23.08 ns |
| 3 | 90.36 ns | 19.58 ns | 120.9 ns | 52.78 ns |
| 5 | 178.6 ns | 34.91 ns | 256.9 ns | 108.2 ns |
| 10 | 371.1 ns | 70.41 ns | 790.2 ns | 354.6 ns |
| 15 | 581.8 ns | 120.6 ns | — | — |
| 20 | 798.9 ns | 158.5 ns | — | — |

These fixtures use a zero-sized effect and resume, so v0.7.0's allocation per program dominates them, and the difference is larger than in the execution comparison. Yielding at every level also increases total work: at depth `d`, effects cross `d × (d − 1) / 2` invocation edges in total, versus `d − 1` for the single leaf effect. These are complete workload timings, not an isolated cost per invocation.

To reproduce these cases, run:

```sh
cargo bench -p corophage --bench stress -- nesting_depth --sample-count 1000 --sample-size 1000 --color never
```

The raw Divan output for both revisions and the source hashes are recorded in `corophage/benches/results/nesting.txt` in the repository.

## Allocations and future size

Allocations were counted separately from timings, after runtime warmup.

| Case | v0.7.0 | Current |
| --- | ---: | ---: |
| Construct and drop | 0 allocations | 0 allocations |
| Flat run, 1 or 1,000 ordinary effects | 1 allocation | 0 allocations |
| Twenty nested invocations | 21 allocations | 0 allocations |
| Unhandled flat program | 160 bytes | 96 bytes |
| Flat async execution future | 448 bytes | 352 bytes |
| Depth-20 async execution future | 8,048 bytes | 3,792 bytes |

Future sizes exclude heap allocations. Concrete bodies and their frames are stored inside the surrounding future. Invocation stores each child once, so concrete storage grows linearly with the chain depth, by 176 bytes per level in this fixture. Use explicit boxing for recursive programs or to limit stored type size.

Explicit body boxing adds one allocation per body. A flat run using `local` required 1 allocation for one call and 1,000 for 1,000 calls: one boxed future per handler call. Ordinary handlers add no per-dispatch allocation. Both local lending adapters explicitly box their returned futures.

## Build and code size

The common measurement binary grew from 568,848 to 569,232 bytes, and its Mach-O `__TEXT` segment stayed at 400 KiB. One clean offline build, including dependencies, took 6.9 seconds for v0.7.0 and 5.6 seconds for the candidate, which no longer depends on `fauxgen`. These are single build observations, not stable estimates of application compile time or library footprint.

## Reproduce

From the repository root, run:

```sh
python3 scripts/execution/measure.py
```

The script archives the baseline, v0.7.0 by default, into a temporary directory, builds both revisions with fresh target directories and compiler caching disabled, and writes `corophage/benches/results/execution.json`. Explicit target directories override `CARGO_TARGET_DIR` and Cargo target-directory configuration, including for the candidate-only cases. Timings report the median, minimum, and maximum of seven samples after calibration. The script measures explicit boxing and local lending in a separate candidate binary so those extra cases do not affect the common-workload code-size comparison. It records the compiler, platform, source hashes, build times, and allocation counts.

The existing Divan suites remain available with `cargo bench`.
