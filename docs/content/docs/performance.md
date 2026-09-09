+++
title = "Performance"
weight = 7
description = "Measured execution costs and concrete future storage tradeoffs."
+++

The default concrete implementation was compared with the boxed implementation at commit `fa68b5a` on 2026-09-09. Both builds used the same macOS arm64 machine, Rust 1.97.1, release optimization, LTO, and one code generation unit.

## Execution costs

Each row measures a complete operation, including construction and execution. Async cases use a warmed Tokio runtime on the current thread with immediately ready handlers; the cooperative dispatch budget remains enabled.

| Operation | Boxed baseline | Concrete default |
| --- | ---: | ---: |
| Construct and drop | 18.0 ns | 0.7 ns |
| One sync effect | 49.4 ns | 35.2 ns |
| One async effect | 120.6 ns | 90.7 ns |
| 1,000 sync effects | 10.5 µs | 12.5 µs |
| 1,000 async effects | 17.1 µs | 16.2 µs |
| 1,000 stateful sync effects | 10.2 µs | 13.4 µs |
| 1,000 stateful async effects | 17.4 µs | 14.5 µs |
| Five nested invocations | 428 ns | 241 ns |
| Ten nested invocations | 822 ns | 453 ns |
| Twenty nested invocations | 1,992 ns | 906 ns |

Nested cases perform one effect at the leaf. Depth counts invocation edges, so depth 20 has 21 active programs. Every generated level invokes the preceding level exactly once. The historical `stress` benchmark labels for depths 15 and 20 combine sequential calls and are not used for this comparison.

Construction and nesting improve, while long synchronous effect loops are about 20–32% slower in this fixture. Real handlers can have much larger costs than the dispatch itself. Do not apply these ratios to an application without measuring it.

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

The common measurement binary grew from 541,728 to 582,512 bytes (7.5%). Its Mach-O `__TEXT` segment grew from 384 to 416 KiB. One clean offline build, including dependencies, took 6.0 seconds for the baseline and 7.3 seconds for the candidate. These are single build observations, not stable estimates of application compile time or library footprint.

## Reproduce

From the repository root, run:

```sh
python3 scripts/execution/measure.py
```

The script archives the baseline into a temporary directory, builds both revisions with fresh target directories and compiler caching disabled, and writes `corophage/benches/results/execution.json`. Timings report the median, minimum, and maximum of seven samples after calibration. The script measures explicit boxing and local lending in a separate candidate binary so those extra cases do not affect the common-workload code-size comparison. It records the compiler, platform, source hashes, build times, and allocation counts.

The existing Divan suites remain available with `cargo bench`. The old published performance tables predate the default switch and have been replaced by this reproducible comparison.
