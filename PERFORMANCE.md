# Performance

Refreshed on 2026-10-25 with a new execution comparison and stress nesting measurements, both from committed sources at `bcfb6f2` and taken on AC power. Both records use macOS arm64 and Rust 1.99.1. The [website performance guide](docs/content/docs/performance.md) presents the same measurements.

The frame transport replaces `fauxgen`, removes the per-program allocation, and forwards invoked effects between frames without a future per level. It is faster than v0.7.0 in every timed operation except construction, which stays below one nanosecond. The measurements cover ready handlers and do not establish application throughput or I/O latency.

## Execution comparison

The [execution record](corophage/benches/results/execution.json) compares the current implementation at commit `bcfb6f2c6ac4c159cc3bd1ccab12c141230fedd9` with v0.7.0 at commit `0e520004bb9462a94341d41a5609455e3ba8beca`. The candidate was measured with no tracked changes, and its source and generated-fixture hashes match that commit.

Both revisions use the same dependency versions and machine, fresh release builds with LTO and one code generation unit, and no compiler cache. Each timing is the median of seven samples after warmup and calibration. Operations include construction, execution, and destruction. Async cases also include `block_on` on a warmed current-thread Tokio runtime and use immediately ready handlers. The cooperative dispatch budget remains enabled.

| Complete operation | v0.7.0 | Current |
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

Here, nesting depth counts invocation edges: depth 20 has 21 active programs and one effect at the leaf. Nested cases run synchronously. The script generates each level as a single invocation of the preceding level.

The 1,000-effect rows include loop work, resume transport, and dispatch. Dividing a row by 1,000 gives an average for that workload; it does not isolate dispatch cost. Each revision is built once, in a fresh temporary directory. Across two runs on AC power on 2026-10-25, the v0.7.0 single-effect rows varied between builds, from 27.9 to 35.0 ns for one sync effect and from 90.5 to 100.8 ns for one async effect; the table shows the recorded run. The current rows varied by less than 2%. For v0.7.0, the 1,000-effect rows varied by less than 3% and the nested rows by up to 11%. Handlers that perform I/O or substantial computation need application measurements before these ratios can be used to predict a benefit.

## Allocations and future storage

Allocations are counted separately from the timed samples, after runtime warmup.

| Complete operation | v0.7.0 | Current |
| --- | ---: | ---: |
| Construct and drop | 0 allocations | 0 allocations |
| Flat run with 1 or 1,000 ordinary effects | 1 allocation | 0 allocations |
| Twenty nested invocations | 21 allocations | 0 allocations |

`Program::new` and default `#[effectful]` functions retain the concrete body future, and each run keeps its frame inside the run future. Invocation stores each pinned child and its frame once. Ordinary dispatch adds no allocation per handler call.

Explicit body erasure with `.boxed()`, `.boxed_send()`, or the erased constructors adds a body allocation. The separate local lending measurements use 1 allocation for one `local` call and 1,000 for 1,000 calls: one boxed future per call. `local_stateful` also boxes its handler future on each call.

| Stored value | v0.7.0 | Current |
| --- | ---: | ---: |
| Unhandled flat program | 160 bytes | 96 bytes |
| Flat async execution future | 448 bytes | 232 bytes |
| Depth-1 async execution future | 752 bytes | 280 bytes |
| Depth-5 async execution future | 2,288 bytes | 632 bytes |
| Depth-10 async execution future | 4,208 bytes | 1,072 bytes |
| Depth-20 async execution future | 8,048 bytes | 1,952 bytes |

These values exclude heap storage. Concrete child futures are stored inside their parents, so the sizes grow linearly with depth: each level adds 88 bytes in this fixture, down from 384. Use explicit boxing for recursive programs or to bound each body's stored type; each active recursive call still consumes memory.

## Stress nesting benchmarks

The [Divan record](corophage/benches/results/nesting.txt) measures the [shared nesting fixtures](corophage/benches/support/nesting.rs) at commit `bcfb6f2`, and v0.7.0 from an archived checkout on the same machine and toolchain. Each case uses 1,000 samples of 1,000 iterations and the workspace's optimized bench profile with LTO. Timings include construction, synchronous execution with a no-op handler, and destruction.

This suite counts active programs: the leaf has depth 1, and depth 20 has 19 invocation edges. It uses a different workload and depth convention from the execution comparison above. The leaf-only case yields once; the other case yields once in every active program.

| Depth (active programs) | One leaf effect, v0.7.0 | Current | One effect per level, v0.7.0 | Current |
| --- | ---: | ---: | ---: | ---: |
| 1 | 28.57 ns | 6.83 ns | 28.28 ns | 5.99 ns |
| 2 | 61.03 ns | 12.57 ns | 67.32 ns | 16.61 ns |
| 3 | 91.15 ns | 18.11 ns | 118.3 ns | 38.15 ns |
| 5 | 178.6 ns | 34.53 ns | 257.3 ns | 96.74 ns |
| 10 | 378.7 ns | 94.28 ns | 773.1 ns | 327.8 ns |
| 15 | 564 ns | 146.2 ns | — | — |
| 20 | 779.6 ns | 198.7 ns | — | — |

The fixtures use a zero-sized effect and resume, so v0.7.0's per-program allocation dominates these timings, and the gain is larger than in the execution comparison. The [regression tests](corophage/tests/benchmark_nesting.rs) exercise the same fixtures with guards that check peak active depth and effect counts. Timing runs use a no-op observer.

Yielding at every level also increases forwarding work. At depth `d`, those effects cross `d × (d − 1) / 2` invocation edges in total, compared with `d − 1` for the single leaf effect. These workload timings do not isolate a fixed cost per invocation or prove that forwarding is free.

## Build and binary size

The execution comparison records one clean offline build per revision, including dependencies. Candidate-only boxing and lending cases are built in a separate binary and excluded from this comparison.

| Measurement | v0.7.0 | Current |
| --- | ---: | ---: |
| Common measurement binary | 568,848 bytes | 563,328 bytes |
| Mach-O `__TEXT` segment | 400 KiB | 400 KiB |
| Clean build time | 6.83 s | 6.19 s |

The common binary is 5,520 bytes smaller. The clean build no longer compiles `fauxgen` and its build dependencies. Build times are single observations, so they do not establish a general compile-time or application-size change.

## Reproduce

From the repository root, run the execution comparison:

```sh
python3 scripts/execution/measure.py
```

The script archives the baseline, v0.7.0 by default, into a temporary directory, builds both revisions with fresh target directories and compiler caching disabled, and writes `corophage/benches/results/execution.json`. Explicit target directories override `CARGO_TARGET_DIR` and Cargo target-directory configuration, including for the candidate-only cases. The record includes compiler and platform details, source and fixture hashes, timing ranges, allocation counts, build times, and binary sizes.

Run the stress nesting cases with:

```sh
cargo bench -p corophage --bench stress -- nesting_depth --sample-count 1000 --sample-size 1000 --color never
```

The recorded output and fixture hashes are in `corophage/benches/results/nesting.txt`. The remaining Divan suites are available through `cargo bench`. Earlier tables for effect width, fan-out, and cancellation predate the concrete default and are omitted from this report.
