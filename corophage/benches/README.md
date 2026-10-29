# Benchmarks

Run all Divan suites with `cargo bench -p corophage`, or select `--bench effects`, `--bench stress`, or `--bench execution`. The `effects` suite covers basic dispatch, handlers, and composition. The `stress` suite covers long loops, wide effect sets, nesting, fan-out, and flat cancellation.

The `execution` suite covers additional execution paths:

| Group | Workload |
| --- | --- |
| `dispatch::catching` | One catch and nested catches around an invoked child, with success or a throw before effect 0 or 999 of 1,000. Includes a direct `Result` baseline. Ordinary effects cross the catch layers. |
| `dispatch::invocation` | Matched unhandled and partially handled chains with 1, 4, or 8 invocation edges. Both perform 100 forwarded effects and one locally handleable effect per active program. |
| `dispatch::payload_size` | 100 round trips of inline effect/resume arrays of 8, 64, 1,024, or 4,096 bytes, flat or through four invocation edges, plus a direct baseline. No payload allocation. |
| `dispatch::body_storage` | The same flat or four-edge nested workload with a concrete, boxed, or Send-boxed outer body, using sync and async runners at 1 or 1,000 effects. Child bodies remain concrete. |
| `dispatch::lending` | Non-static borrowed resumes, ordinary suspending handlers, and `local`/`local_stateful` handlers that keep mutable capture/state loans across suspension. The lending cases include counter updates as well as adapter costs. |
| `scheduling::suspension` | Ready handlers, handlers that suspend, and bodies that suspend, plus direct synchronous, ready-async, and suspending-async Rust baselines. Each explicit suspension returns `Pending` once and wakes the task once. |
| `scheduling::fairness` | Flat loops and short handled children around the current 128-dispatch budget boundaries: 127/128/129 and 255/256/257, plus 10,000 effects. Also measures service latency for a competing task. |
| `lifecycle::cancellation` | Leaf cancellation through 1, 4, or 8 unhandled or handled invocation edges, using sync and async roots. |
| `lifecycle::suspended_drop` | Drop a flat or four-edge nested program while its handler or body is pending, with two live 64- or 4,096-byte heap payloads. |

Depth in this suite counts invocation edges: depth 4 has five active programs. This differs from the stress suite's active-program depth convention.

## Timing boundaries

Except for the two cases below, each timing includes construction, execution, and cleanup of the program. Async timings include `Runtime::block_on` on a reused current-thread Tokio runtime; runtime construction is excluded. Direct baselines perform the same scalar/array operations and use corresponding optimization barriers, but omit effect dispatch and its cooperative scheduling. They are workload comparisons, not a subtraction that isolates a universal per-effect cost.

`peer_first_poll_latency` creates a fresh Tokio `LocalSet` and queues a load task before a probe task. The timer covers entering the runtime and driving the local executor until the probe runs. The arguments select an idle executor (`0`), a flat million-effect load (`1`), or a million short handled children (`2`). Task creation and unfinished-load destruction are outside the timer. This measures initial service latency, including executor overhead; it does not measure steady-state tail latency or multithreaded scheduling. A regression test verifies that the probe observes progress before the load finishes.

`suspended_drop` allocates both payloads, boxes the outer runner for stable storage, and polls it into `Pending` before timing. The timer covers only destruction, including the runner box, suspended frames/handler, and both payloads. Regression tests verify that both payloads remain live at the start of cleanup and are dropped exactly once. Both input-based cases use one operation per sample to avoid accumulating queued tasks or allocations across a batch.

## Validation and measurement

Run fixture checks and a one-iteration smoke pass with:

```sh
cargo nextest run -p corophage --test benchmark_execution
cargo bench -p corophage --bench execution -- --test
```

For a short timing pass over every new case:

```sh
cargo bench -p corophage --bench execution -- --sample-count 10 --sample-size 1 --color never
```

For focused measurements, use a filter, for example:

```sh
cargo bench -p corophage --bench execution -- dispatch::catching
cargo bench -p corophage --bench execution -- scheduling::fairness
```

Smoke runs establish that fixtures execute; their timings are not performance conclusions. For comparisons, retain the revision, source state, compiler, profile, and host conditions, and repeat runs in alternating order.

## Automated comparison with v0.7.0

```sh
python3 scripts/execution/measure.py
```

The script requires Python 3.9 or newer and the repository's Cargo/nextest tooling. It archives v0.7.0 and builds the current core probe and execution suite against each revision in separate fresh targets, with the compiler wrapper disabled. The same suite sources and starting dependency lockfile are used on both sides, and shared resolved dependency versions must match. Divan is pinned to 0.1.21 because the script parses its timing output. The script runs the shared fixture tests with nextest and smoke-tests every registered case before measuring. No files in the archived revision are patched.

All 108 current suite cases are recorded. Of these, 88 run on both revisions. The other 20 are recorded as unsupported on v0.7.0, with individual names and reasons: nested invocable catches, partially handled invocation, cancellation through handled invocations, short handled children, and the competing-task case using handled children. The v0.7.0 compatibility configuration excludes only those cases; it does not emulate them with a different workload. Simple catch attaches its ordinary handler before `.catch()`, which both APIs support.

By default, each revision runs twice, in baseline/candidate then candidate/baseline order. Each suite case uses 100 Divan samples; Divan chooses its sample size except for latency and suspended-drop cases, which retain one operation per sample. Use `--runs` (at least 2) and `--sample-count` to increase repetition. `--baseline` selects another revision; the old terminal `Catch` API selects the v0.7.0 compatibility configuration. Other revisions must support the current benchmark API.

The JSON at `corophage/benches/results/execution.json` uses schema version 2. It retains the original core probe's `revisions`, first-run `measurements`, and `candidate_extras`, and adds repeated `runs`, `run_order`, and `comparisons`. `execution_suite` contains inventories, fixture-check output, per-run measurements and raw Divan output, unsupported-case reasons, and comparisons. Ratios are candidate time divided by baseline time, so values below 1 mean less elapsed time; a zero baseline produces a null ratio. Comparisons use the median of each revision's run medians. Candidate-only cases have measurements but no invented baseline or ratio.

Core timings still use seven calibrated samples and count allocations separately. The expanded Divan suite records timing only. Core and suite build times and binary sizes are recorded separately; the suite binaries contain different numbers of cases, so their sizes are not equivalent-workload comparisons. Source, fixture, harness, and resolved-lock hashes, compiler/platform details, and host load are recorded. The script refuses to write results if measured source files change during the run.

For a shorter validation run without replacing saved results:

```sh
python3 -m unittest discover -s scripts/execution -p 'test_*.py'
python3 scripts/execution/measure.py --sample-count 10 --output /tmp/corophage-comparison.json
```

The historical performance tables are not refreshed automatically by this command.
