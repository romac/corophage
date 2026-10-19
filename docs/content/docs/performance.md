+++
title = "Performance"
weight = 7
description = "Measured execution costs and concrete future storage tradeoffs."
+++

The current implementation at commit `6cc6ff2` was compared with the boxed implementation at commit `fa68b5a` on 2026-10-19. The candidate was measured with no tracked changes, and the recorded source and generated-fixture hashes match that commit. Both builds used the same macOS arm64 machine, Rust 1.99.1, release optimization, LTO, and one code generation unit.

## Execution costs

Each row measures a complete operation, including construction and execution. Async cases use a warmed Tokio runtime on the current thread with immediately ready handlers; the cooperative dispatch budget remains enabled.

| Operation | Boxed baseline | Current |
| --- | ---: | ---: |
| Construct and drop | 12.9 ns | 0.7 ns |
| One sync effect | 44.9 ns | 28.1 ns |
| One async effect | 114.5 ns | 92.2 ns |
| 1,000 sync effects | 10.4 µs | 9.6 µs |
| 1,000 async effects | 16.2 µs | 11.6 µs |
| 1,000 stateful sync effects | 10.4 µs | 9.5 µs |
| 1,000 stateful async effects | 17.2 µs | 12.1 µs |
| Five nested invocations | 406 ns | 209 ns |
| Ten nested invocations | 734 ns | 376 ns |
| Twenty nested invocations | 1,838 ns | 745 ns |

Nested cases perform one effect at the leaf and run synchronously. Depth counts invocation edges, so depth 20 has 21 active programs. Every generated level invokes the preceding level exactly once. This comparison uses its own generated chains; corrected Divan stress fixtures are measured separately below.

The current implementation is faster in every row. Construction no longer allocates, nested invocations take 49–60% less time, and the 1,000-effect loops are 8–9% faster when synchronous and 29–30% faster when asynchronous. Synchronous runners poll the program directly in an inline loop; async runners use the cooperative loop. Real handlers can have much larger costs than the dispatch itself. Do not apply these ratios to an application without measuring it.

## Stress nesting benchmarks

The corrected Divan fixtures were measured on 2026-10-19 at commit `6cc6ff2` on macOS arm64, Rust 1.99.1, and the workspace's optimized bench profile with LTO. Each case has 1,000 samples of 1,000 iterations. The medians include program construction, synchronous execution with a no-op handler, and destruction.

This suite counts active programs: the leaf has depth 1, and depth 20 has 19 invocation edges. The leaf-only case yields once. The other case yields once in each program and returns the number of yields. Regression tests run these same fixtures with guards that check peak active depth and handler calls; timing runs use a no-op observer.

| Depth (active programs) | One leaf effect | One effect per level |
| --- | ---: | ---: |
| 1 | 28.08 ns | 28.7 ns |
| 2 | 59.95 ns | 68.58 ns |
| 3 | 94.58 ns | 121.7 ns |
| 5 | 176.8 ns | 262.2 ns |
| 10 | 376.5 ns | 795.4 ns |
| 15 | 584.7 ns | — |
| 20 | 799.5 ns | — |

The old depth-15 and depth-20 fixtures both peaked at 11 active programs and yielded twice. The old depth-10 yielding fixture peaked at 6 programs and yielded 11 times. Their timings cannot support conclusions about the labelled depths; this table replaces those results.

Yielding at every level also increases total work: at depth `d`, effects cross `d × (d − 1) / 2` invocation edges in total, versus `d − 1` for the single leaf effect. These are complete workload timings, not an isolated cost per invocation. They are separate measurements from the execution migration comparison above.

To reproduce these cases, run:

```sh
cargo bench -p corophage --bench stress -- nesting_depth --sample-count 1000 --sample-size 1000 --color never
```

The raw Divan output and source hashes are recorded in `corophage/benches/results/nesting.txt` in the repository.

## Allocations and future size

Allocations were counted separately from timings, after runtime warmup.

| Case | Boxed baseline | Current |
| --- | ---: | ---: |
| Construct and drop | 1 allocation | 0 allocations |
| Flat run, 1 or 1,000 ordinary effects | 2 allocations | 1 allocation |
| Twenty nested invocations | 42 allocations | 21 allocations |
| Unhandled flat program | 48 bytes | 160 bytes |
| Flat async execution future | 280 bytes | 448 bytes |
| Depth-20 async execution future | 280 bytes | 8,048 bytes |

Future sizes exclude heap allocations. The boxed baseline stores each body separately on the heap; concrete bodies are stored inside the surrounding future. Invocation stores each child once, so concrete storage grows linearly with the chain depth. Use explicit boxing for recursive programs or to limit stored type size.

Explicit body boxing adds one allocation per body. A flat run using `local` required 2 allocations for one call and 1,001 for 1,000 calls: one runtime allocation plus one boxed future per handler call. Ordinary handlers add no per-dispatch allocation. Both local lending adapters explicitly box their returned futures.

## Build and code size

The common measurement binary grew from 545,808 to 568,800 bytes (4.2%). Its Mach-O `__TEXT` segment grew from 384 to 400 KiB. One clean offline build, including dependencies, took 5.7 seconds for the baseline and 6.7 seconds for the candidate. These are single build observations, not stable estimates of application compile time or library footprint.

## Reproduce

From the repository root, run:

```sh
python3 scripts/execution/measure.py
```

The script archives the baseline into a temporary directory, builds both revisions with fresh target directories and compiler caching disabled, and writes `corophage/benches/results/execution.json`. Explicit target directories override `CARGO_TARGET_DIR` and Cargo target-directory configuration, including for the candidate-only cases. Timings report the median, minimum, and maximum of seven samples after calibration. The script measures explicit boxing and local lending in a separate candidate binary so those extra cases do not affect the common-workload code-size comparison. It records the compiler, platform, source hashes, build times, and allocation counts.

The existing Divan suites remain available with `cargo bench`. The old published performance tables predate the default switch and have been replaced by this reproducible comparison.
