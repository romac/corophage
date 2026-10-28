+++
title = "Performance"
weight = 7
description = "Measured execution costs and concrete future storage tradeoffs."
+++

The current implementation at commit `bcfb6f2` was compared with v0.7.0 at commit `0e52000` on 2026-10-25, on AC power. The candidate was measured with no tracked changes, and the recorded source and generated-fixture hashes match that commit. Both builds used the same macOS arm64 machine, Rust 1.99.1, release optimization, LTO, and one code generation unit.

## Execution costs

Each row measures a complete operation, including construction and execution. Async cases use a warmed Tokio runtime on the current thread with immediately ready handlers; the cooperative dispatch budget remains enabled.

| Operation | v0.7.0 | Current |
| --- | ---: | ---: |
| Construct and drop | 0.5 ns | 0.5 ns |
| One sync effect | 35.0 ns | 15.6 ns |
| One async effect | 100.8 ns | 74.0 ns |
| 1,000 sync effects | 9.6 µs | 7.0 µs |
| 1,000 async effects | 11.6 µs | 8.6 µs |
| 1,000 stateful sync effects | 9.5 µs | 7.0 µs |
| 1,000 stateful async effects | 12.0 µs | 8.8 µs |
| Five nested invocations | 215 ns | 73 ns |
| Ten nested invocations | 371 ns | 131 ns |
| Twenty nested invocations | 738 ns | 274 ns |

Nested cases perform one effect at the leaf and run synchronously. Depth counts invocation edges, so depth 20 has 21 active programs. Every generated level invokes the preceding level exactly once. This comparison uses its own generated chains; the Divan stress fixtures are measured separately below.

Each run owns a small frame that carries effects and resumes between the computation and its runner, and an invocation moves each effect from its child's frame into its parent's without a future per level. One synchronous effect takes 44–56% less time, the 1,000-effect loops 25–27% less, and nested invocations 62–66% less. Construction stays below a nanosecond. Each revision is built once, in a fresh temporary directory. Across two runs on AC power on 2026-10-25, the v0.7.0 single-effect rows varied between builds, from 27.9 to 35.0 ns for one sync effect and from 90.5 to 100.8 ns for one async effect; the table shows the recorded run. The current rows varied by less than 2%. For v0.7.0, the 1,000-effect rows varied by less than 3% and the nested rows by up to 11%. Real handlers can have much larger costs than the dispatch itself. Do not apply these ratios to an application without measuring it.

## Stress nesting benchmarks

The Divan fixtures were measured on 2026-10-25 at commit `bcfb6f2`, and for v0.7.0 from an archived checkout right after it, on macOS arm64, Rust 1.99.1, and the workspace's optimized bench profile with LTO. Each case has 1,000 samples of 1,000 iterations. The medians include program construction, synchronous execution with a no-op handler, and destruction.

This suite counts active programs: the leaf has depth 1, and depth 20 has 19 invocation edges. The leaf-only case yields once. The other case yields once in each program and returns the number of yields. Regression tests run these same fixtures with guards that check peak active depth and handler calls; timing runs use a no-op observer.

| Depth (active programs) | One leaf effect, v0.7.0 | Current | One effect per level, v0.7.0 | Current |
| --- | ---: | ---: | ---: | ---: |
| 1 | 28.57 ns | 6.83 ns | 28.28 ns | 5.99 ns |
| 2 | 61.03 ns | 12.57 ns | 67.32 ns | 16.61 ns |
| 3 | 91.15 ns | 18.11 ns | 118.3 ns | 38.15 ns |
| 5 | 178.6 ns | 34.53 ns | 257.3 ns | 96.74 ns |
| 10 | 378.7 ns | 94.28 ns | 773.1 ns | 327.8 ns |
| 15 | 564 ns | 146.2 ns | — | — |
| 20 | 779.6 ns | 198.7 ns | — | — |

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
| Flat async execution future | 448 bytes | 232 bytes |
| Depth-20 async execution future | 8,048 bytes | 1,952 bytes |

Future sizes exclude heap allocations. Concrete bodies and their frames are stored inside the surrounding future. Invocation stores each child once, so concrete storage grows linearly with the chain depth, by 88 bytes per level in this fixture. Use explicit boxing for recursive programs or to limit stored type size.

Explicit body boxing adds one allocation per body. A flat run using `local` required 1 allocation for one call and 1,000 for 1,000 calls: one boxed future per handler call. Ordinary handlers add no per-dispatch allocation. Both local lending adapters explicitly box their returned futures.

## Build and code size

The common measurement binary shrank from 568,848 to 563,328 bytes, and its Mach-O `__TEXT` segment stayed at 400 KiB. One clean offline build, including dependencies, took 6.8 seconds for v0.7.0 and 6.2 seconds for the candidate, which no longer depends on `fauxgen`. These are single build observations, not stable estimates of application compile time or library footprint.

## Reproduce

From the repository root, run:

```sh
python3 scripts/execution/measure.py
```

The script archives the baseline, v0.7.0 by default, into a temporary directory, builds both revisions with fresh target directories and compiler caching disabled, and writes `corophage/benches/results/execution.json`. Explicit target directories override `CARGO_TARGET_DIR` and Cargo target-directory configuration, including for the candidate-only cases. Timings report the median, minimum, and maximum of seven samples after calibration. The script measures explicit boxing and local lending in a separate candidate binary so those extra cases do not affect the common-workload code-size comparison. It records the compiler, platform, source hashes, build times, and allocation counts.

The Divan suites remain available with `cargo bench`. In addition to the basic `effects` and scaling-focused `stress` suites, `cargo bench -p corophage --bench execution` covers catch/throw paths, partially handled invocation, real async suspension, cooperative fairness, inline payload sizes, nested cancellation, suspended cleanup, matched concrete/boxed bodies, and lending handlers with borrowed resumes. It includes direct-Rust baselines. These cases extend coverage; the historical measurements above do not include them.

The [benchmark guide](https://github.com/romac/corophage/blob/main/corophage/benches/README.md) describes each workload, depth conventions, and timing boundaries. In particular, the competing-task case measures initial service latency, and the suspended-drop cases exclude setup from their timings. Run `cargo bench -p corophage --bench execution -- --test` for a smoke pass without collecting timings.
