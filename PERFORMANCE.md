# Performance

Refreshed on 2026-10-19 with a new execution comparison and stress nesting measurements, both from committed sources at `6cc6ff2`. Both records use macOS arm64 and Rust 1.99.1. The [website performance guide](docs/content/docs/performance.md) presents the same measurements.

The current implementation removes the body allocation and is faster than the boxed baseline at `fa68b5a` in every measured operation. Synchronous runners use an inline loop that polls the program directly. Concrete futures occupy more inline storage. The measurements cover ready handlers and do not establish application throughput or I/O latency.

## Execution comparison

The [execution record](corophage/benches/results/execution.json) compares the current implementation at commit `6cc6ff2597f68379bb0a17b400bac02ab6b80376` with the boxed implementation at commit `fa68b5a268e66fc1e57e579f7fb3699cabb55745`. The candidate was measured with no tracked changes, and its source and generated-fixture hashes match that commit. The baseline already includes changes after v0.5.0, including cooperative scheduling. This comparison measures the execution migration and the later inline synchronous loop.

Both revisions use the same dependency versions and machine, fresh release builds with LTO and one code generation unit, and no compiler cache. Each timing is the median of seven samples after warmup and calibration. Operations include construction, execution, and destruction. Async cases also include `block_on` on a warmed current-thread Tokio runtime and use immediately ready handlers. The cooperative dispatch budget remains enabled.

| Complete operation | Boxed baseline | Current |
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

Here, nesting depth counts invocation edges: depth 20 has 21 active programs and one effect at the leaf. Nested cases run synchronously. The script generates each level as a single invocation of the preceding level.

The 1,000-effect rows include loop work, resume transport, and dispatch. Dividing a row by 1,000 gives an average for that workload; it does not isolate dispatch cost. Handlers that perform I/O or substantial computation need application measurements before these ratios can be used to predict a benefit.

## Allocations and future storage

Allocations are counted separately from the timed samples, after runtime warmup.

| Complete operation | Boxed baseline | Current |
| --- | ---: | ---: |
| Construct and drop | 1 allocation | 0 allocations |
| Flat run with 1 or 1,000 ordinary effects | 2 allocations | 1 allocation |
| Twenty nested invocations | 42 allocations | 21 allocations |

`Program::new` and default `#[effectful]` functions retain the concrete body future. Invocation stores each pinned child once. The measured flat run still needs one runtime allocation; ordinary dispatch adds no allocation per handler call.

Explicit body erasure with `.boxed()`, `.boxed_send()`, or the erased constructors adds a body allocation. The separate local lending measurements use 2 allocations for one `local` call and 1,001 for 1,000 calls: one runtime allocation plus one boxed future per call. `local_stateful` also boxes its handler future on each call.

| Stored value | Boxed baseline | Current |
| --- | ---: | ---: |
| Unhandled flat program | 48 bytes | 160 bytes |
| Flat async execution future | 280 bytes | 448 bytes |
| Depth-1 async execution future | 280 bytes | 752 bytes |
| Depth-5 async execution future | 280 bytes | 2,288 bytes |
| Depth-10 async execution future | 280 bytes | 4,208 bytes |
| Depth-20 async execution future | 280 bytes | 8,048 bytes |

These values exclude heap storage. The boxed baseline keeps its body futures on the heap, while concrete child futures are stored inside their parents. The concrete sizes grow linearly with depth in this fixture. Use explicit boxing for recursive programs or to bound each body's stored type; each active recursive call still consumes memory.

## Corrected stress nesting benchmarks

The [Divan record](corophage/benches/results/nesting.txt) measures the corrected [shared nesting fixtures](corophage/benches/support/nesting.rs) at commit `6cc6ff2`. Each case uses 1,000 samples of 1,000 iterations and the workspace's optimized bench profile with LTO. Timings include construction, synchronous execution with a no-op handler, and destruction.

This suite counts active programs: the leaf has depth 1, and depth 20 has 19 invocation edges. It uses a different workload and depth convention from the execution comparison above. The leaf-only case yields once; the other case yields once in every active program.

| Depth (active programs) | One leaf effect | One effect per level |
| --- | ---: | ---: |
| 1 | 28.08 ns | 28.7 ns |
| 2 | 59.95 ns | 68.58 ns |
| 3 | 94.58 ns | 121.7 ns |
| 5 | 176.8 ns | 262.2 ns |
| 10 | 376.5 ns | 795.4 ns |
| 15 | 584.7 ns | — |
| 20 | 799.5 ns | — |

The [regression tests](corophage/tests/benchmark_nesting.rs) exercise the same fixtures with guards that check peak active depth and effect counts. Timing runs use a no-op observer.

The old depth-15 and depth-20 fixtures both peaked at 11 active programs and yielded twice. The old depth-10 yielding fixture peaked at 6 programs and yielded 11 times. This table replaces those incorrectly labelled results.

Yielding at every level also increases forwarding work. At depth `d`, those effects cross `d × (d − 1) / 2` invocation edges in total, compared with `d − 1` for the single leaf effect. These workload timings do not isolate a fixed cost per invocation or prove that forwarding is free.

## Build and binary size

The execution comparison records one clean offline build per revision, including dependencies. Candidate-only boxing and lending cases are built in a separate binary and excluded from this comparison.

| Measurement | Boxed baseline | Current |
| --- | ---: | ---: |
| Common measurement binary | 545,808 bytes | 568,800 bytes |
| Mach-O `__TEXT` segment | 384 KiB | 400 KiB |
| Clean build time | 5.68 s | 6.69 s |

The common binary grew by 4.2%. Build times are single observations, so they do not establish a general compile-time or application-size penalty.

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
