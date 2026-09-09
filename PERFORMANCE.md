# Performance

Refreshed on 2026-09-09 with a new execution comparison from committed sources and the recorded corrected stress nesting measurements. Both records use macOS arm64 and Rust 1.97.1. The [website performance guide](docs/content/docs/performance.md) presents the same measurements.

The concrete default removes the body allocation and improves construction and nested execution in these fixtures. The 1,000-effect synchronous loops take about 33% longer than the boxed baseline at `fa68b5a`, and concrete futures occupy more inline storage. The measurements cover ready handlers and do not establish application throughput or I/O latency.

## Execution comparison

The [execution record](corophage/benches/results/execution.json) compares the concrete implementation at commit `6d073a9` with the boxed implementation at commit `fa68b5a268e66fc1e57e579f7fb3699cabb55745`. The candidate was measured with no tracked changes, and its source and generated-fixture hashes match that commit. The baseline already includes changes after v0.5.0, including cooperative scheduling. This comparison measures the execution migration.

Both revisions use the same dependency versions and machine, fresh release builds with LTO and one code generation unit, and no compiler cache. Each timing is the median of seven samples after warmup and calibration. Operations include construction, execution, and destruction. Async cases also include `block_on` on a warmed current-thread Tokio runtime and use immediately ready handlers. The cooperative dispatch budget remains enabled.

| Complete operation | Boxed baseline | Concrete default |
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

Here, nesting depth counts invocation edges: depth 20 has 21 active programs and one effect at the leaf. The script generates each level as a single invocation of the preceding level.

The 1,000-effect rows include loop work, resume transport, and dispatch. Dividing a row by 1,000 gives an average for that workload; it does not isolate dispatch cost. Handlers that perform I/O or substantial computation need application measurements before these ratios can be used to predict a benefit.

## Allocations and future storage

Allocations are counted separately from the timed samples, after runtime warmup.

| Complete operation | Boxed baseline | Concrete default |
| --- | ---: | ---: |
| Construct and drop | 1 allocation | 0 allocations |
| Flat run with 1 or 1,000 ordinary effects | 2 allocations | 1 allocation |
| Twenty nested invocations | 42 allocations | 21 allocations |

`Program::new` and default `#[effectful]` functions retain the concrete body future. Invocation stores each pinned child once. The measured flat run still needs one runtime allocation; ordinary dispatch adds no allocation per handler call.

Explicit body erasure with `.boxed()`, `.boxed_send()`, or the erased constructors adds a body allocation. The separate local lending measurements use 2 allocations for one `local` call and 1,001 for 1,000 calls: one runtime allocation plus one boxed future per call. `local_stateful` also boxes its handler future on each call.

| Stored value | Boxed baseline | Concrete default |
| --- | ---: | ---: |
| Unhandled flat program | 48 bytes | 184 bytes |
| Flat execution future | 312 bytes | 528 bytes |
| Depth-1 execution future | 312 bytes | 880 bytes |
| Depth-5 execution future | 312 bytes | 2,608 bytes |
| Depth-10 execution future | 312 bytes | 4,768 bytes |
| Depth-20 execution future | 312 bytes | 9,088 bytes |

These values exclude heap storage. The boxed baseline keeps its body futures on the heap, while concrete child futures are stored inside their parents. The concrete sizes grow linearly with depth in this fixture. Use explicit boxing for recursive programs or to bound each body's stored type; each active recursive call still consumes memory.

## Corrected stress nesting benchmarks

The [Divan record](corophage/benches/results/nesting.txt) measures the corrected [shared nesting fixtures](corophage/benches/support/nesting.rs) with the concrete default. Each case uses 1,000 samples of 1,000 iterations and the workspace's optimized bench profile with LTO. Timings include construction, synchronous execution with a no-op handler, and destruction.

This suite counts active programs: the leaf has depth 1, and depth 20 has 19 invocation edges. It uses a different workload and depth convention from the execution comparison above. The leaf-only case yields once; the other case yields once in every active program.

| Depth (active programs) | One leaf effect | One effect per level |
| --- | ---: | ---: |
| 1 | 34.33 ns | 31.46 ns |
| 2 | 66.13 ns | 80.58 ns |
| 3 | 103.2 ns | 136.7 ns |
| 5 | 199.9 ns | 294.7 ns |
| 10 | 443.2 ns | 936.1 ns |
| 15 | 726.2 ns | — |
| 20 | 969.9 ns | — |

The [regression tests](corophage/tests/benchmark_nesting.rs) exercise the same fixtures with guards that check peak active depth and effect counts. Timing runs use a no-op observer.

The old depth-15 and depth-20 fixtures both peaked at 11 active programs and yielded twice. The old depth-10 yielding fixture peaked at 6 programs and yielded 11 times. This table replaces those incorrectly labelled results.

Yielding at every level also increases forwarding work. At depth `d`, those effects cross `d × (d − 1) / 2` invocation edges in total, compared with `d − 1` for the single leaf effect. These workload timings do not isolate a fixed cost per invocation or prove that forwarding is free.

## Build and binary size

The execution comparison records one clean offline build per revision, including dependencies. Candidate-only boxing and lending cases are built in a separate binary and excluded from this comparison.

| Measurement | Boxed baseline | Concrete default |
| --- | ---: | ---: |
| Common measurement binary | 541,696 bytes | 582,512 bytes |
| Mach-O `__TEXT` segment | 384 KiB | 416 KiB |
| Clean build time | 6.70 s | 7.75 s |

The common binary grew by 7.5%. Build times are single observations, so they do not establish a general compile-time or application-size penalty.

## Reproduce

From the repository root, run the execution comparison:

```sh
python3 scripts/execution/measure.py
```

The script archives the baseline into a temporary directory, builds both revisions with fresh target directories and compiler caching disabled, and writes `corophage/benches/results/execution.json`. Explicit target directories override `CARGO_TARGET_DIR` and Cargo target-directory configuration, including for the candidate-only cases. The record includes compiler and platform details, source and fixture hashes, timing ranges, allocation counts, build times, and binary sizes.

Run the corrected stress nesting cases with:

```sh
cargo bench -p corophage --bench stress -- nesting_depth --sample-count 1000 --sample-size 1000 --color never
```

The recorded output and fixture hashes are in `corophage/benches/results/nesting.txt`. The remaining Divan suites are available through `cargo bench`. Earlier tables for effect width, fan-out, and cancellation predate the concrete default and are omitted from this report.
