+++
title = "Performance"
weight = 7
description = "Benchmarks and performance characteristics."
+++

Benchmarks were run using [Divan](https://github.com/nvzqz/divan). Run them with `cargo bench`.

## Coroutine overhead

| Benchmark | Median | Notes |
|-----------|--------|-------|
| `coroutine_creation` | ~7 ns | Just struct initialization |
| `empty_coroutine` | ~30 ns | Full lifecycle with no yields |
| `single_yield` | ~38 ns | One yield/resume cycle |

Coroutine creation is nearly free, and the baseline overhead for running a coroutine is ~30 ns.

## Yield scaling (sync vs async)

| Yields | Sync | Async | Overhead |
|--------|------|-------|----------|
| 10 | 131 ns | 178 ns | +36% |
| 100 | 1.0 µs | 1.27 µs | +27% |
| 1000 | 9.5 µs | 11.1 µs | +17% |

Async adds ~30% overhead at small scales, but the gap narrows as yield count increases. Per-yield cost is approximately **9–10 ns** for sync and **11 ns** for async.

## Effect dispatch position

| Position | Median |
|----------|--------|
| First (index 0) | 49 ns |
| Middle (index 2) | 42 ns |
| Last (index 4) | 47 ns |

Dispatch position has negligible impact. While the source-level dispatch uses recursive trait impls over nested `Coproduct::Inl`/`Inr` variants, the compiler monomorphizes and inlines the entire chain into a flat discriminant-based branch — the same code LLVM would emit for a plain `match` on a flat enum. The result is effectively O(1).

## State management

| Pattern | Median |
|---------|--------|
| Stateless (`run`) | 38 ns |
| Stateful (`run_stateful`) | 53 ns |
| RefCell pattern | 55 ns |

Stateful handlers add ~15 ns overhead. RefCell is nearly equivalent to `run_stateful`.

## Handler complexity

| Handler | Median |
|---------|--------|
| Noop (returns `()`) | 42 ns |
| Allocating (returns `String`) | 83 ns |

Allocation dominates handler cost. Consider returning references or zero-copy types for performance-critical effects.
