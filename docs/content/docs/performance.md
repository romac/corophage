+++
title = "Performance"
weight = 7
description = "Measured execution costs and concrete future storage tradeoffs."
+++

The default concrete implementation at commit `6d073a9` was compared with the boxed implementation at commit `fa68b5a` on 2026-09-09. The candidate was measured with no tracked changes, and the recorded source and generated-fixture hashes match that commit. Both builds used the same macOS arm64 machine, Rust 1.97.1, release optimization, LTO, and one code generation unit.

## Execution costs

Each row measures a complete operation, including construction and execution. Async cases use a warmed Tokio runtime on the current thread with immediately ready handlers; the cooperative dispatch budget remains enabled.

| Operation | Boxed baseline | Concrete default |
| --- | ---: | ---: |
| Construct and drop | 16.1 ns | 0.7 ns |
| One sync effect | 50.4 ns | 35.3 ns |
| One async effect | 125.0 ns | 92.2 ns |
| 1,000 sync effects | 10.5 µs | 14.1 µs |
| 1,000 async effects | 17.6 µs | 16.6 µs |
| 1,000 stateful sync effects | 10.5 µs | 14.0 µs |
| 1,000 stateful async effects | 17.7 µs | 14.6 µs |
| Five nested invocations | 436 ns | 246 ns |
| Ten nested invocations | 863 ns | 471 ns |
| Twenty nested invocations | 2,076 ns | 932 ns |

Nested cases perform one effect at the leaf. Depth counts invocation edges, so depth 20 has 21 active programs. Every generated level invokes the preceding level exactly once. This comparison uses its own generated chains; corrected Divan stress fixtures are measured separately below.

Construction and nesting improve, while the 1,000-effect synchronous loops take about 33% longer than the boxed baseline at `fa68b5a` in this fixture. Real handlers can have much larger costs than the dispatch itself. Do not apply these ratios to an application without measuring it.

## Stress nesting benchmarks

The corrected Divan fixtures were measured on 2026-09-09 with the concrete default on macOS arm64, Rust 1.97.1, and the workspace's optimized bench profile with LTO. Each case has 1,000 samples of 1,000 iterations. The medians include program construction, synchronous execution with a no-op handler, and destruction.

This suite counts active programs: the leaf has depth 1, and depth 20 has 19 invocation edges. The leaf-only case yields once. The other case yields once in each program and returns the number of yields. Regression tests run these same fixtures with guards that check peak active depth and handler calls; timing runs use a no-op observer.

| Depth (active programs) | One leaf effect | One effect per level |
| --- | ---: | ---: |
| 1 | 34.33 ns | 31.46 ns |
| 2 | 66.13 ns | 80.58 ns |
| 3 | 103.2 ns | 136.7 ns |
| 5 | 199.9 ns | 294.7 ns |
| 10 | 443.2 ns | 936.1 ns |
| 15 | 726.2 ns | — |
| 20 | 969.9 ns | — |

The old depth-15 and depth-20 fixtures both peaked at 11 active programs and yielded twice. The old depth-10 yielding fixture peaked at 6 programs and yielded 11 times. Their timings cannot support conclusions about the labelled depths; this table replaces those results.

Yielding at every level also increases total work: at depth `d`, effects cross `d × (d − 1) / 2` invocation edges in total, versus `d − 1` for the single leaf effect. These are complete workload timings, not an isolated cost per invocation. They are separate measurements from the execution migration comparison above.

To reproduce these cases, run:

```sh
cargo bench -p corophage --bench stress -- nesting_depth --sample-count 1000 --sample-size 1000 --color never
```

The raw Divan output and source hashes are recorded in `corophage/benches/results/nesting.txt` in the repository.

## Allocations and future size

Allocations were counted separately from timings, after runtime warmup.

| Case | Boxed baseline | Concrete default |
| --- | ---: | ---: |
| Construct and drop | 1 allocation | 0 allocations |
| Flat run, 1 or 1,000 ordinary effects | 2 allocations | 1 allocation |
| Twenty nested invocations | 42 allocations | 21 allocations |
| Unhandled flat program | 48 bytes | 184 bytes |
| Flat execution future | 312 bytes | 528 bytes |
| Depth-20 execution future | 312 bytes | 9,088 bytes |

Future sizes exclude heap allocations. The boxed baseline stores each body separately on the heap; concrete bodies are stored inside the surrounding future. Invocation stores each child once, so concrete storage grows linearly with the chain depth. Use explicit boxing for recursive programs or to limit stored type size.

Explicit body boxing adds one allocation per body. A flat run using `local` required 2 allocations for one call and 1,001 for 1,000 calls: one runtime allocation plus one boxed future per handler call. Ordinary handlers add no per-dispatch allocation. Both local lending adapters explicitly box their returned futures.

## Build and code size

The common measurement binary grew from 541,696 to 582,512 bytes (7.5%). Its Mach-O `__TEXT` segment grew from 384 to 416 KiB. One clean offline build, including dependencies, took 6.7 seconds for the baseline and 7.8 seconds for the candidate. These are single build observations, not stable estimates of application compile time or library footprint.

## Reproduce

From the repository root, run:

```sh
python3 scripts/execution/measure.py
```

The script archives the baseline into a temporary directory, builds both revisions with fresh target directories and compiler caching disabled, and writes `corophage/benches/results/execution.json`. Explicit target directories override `CARGO_TARGET_DIR` and Cargo target-directory configuration, including for the candidate-only cases. Timings report the median, minimum, and maximum of seven samples after calibration. The script measures explicit boxing and local lending in a separate candidate binary so those extra cases do not affect the common-workload code-size comparison. It records the compiler, platform, source hashes, build times, and allocation counts.

The existing Divan suites remain available with `cargo bench`. The old published performance tables predate the default switch and have been replaced by this reproducible comparison.
