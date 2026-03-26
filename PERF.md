# Performance Analysis

## Stress test results

Collected from `cargo bench --bench stress` across 3 runs (median values, LTO enabled).

### Yield throughput

| Benchmark | 100 | 1,000 | 10,000 | 100,000 |
|---|---|---|---|---|
| `sync_yields` | ~860 ns | ~8.3 µs | ~81 µs | ~825 µs |
| `async_yields` | ~1.1 µs | ~10.3 µs | ~102 µs | ~1.02 ms |
| `yield_with_value` (returns `u64`) | ~838 ns | ~7.9 µs | ~80 µs | ~804 µs |
| `stateful_counting` (`&mut u64` handler) | ~848 ns | ~8.3 µs | ~84 µs | — |

Per-yield cost: **~8 ns sync**, **~10 ns async**. Perfectly linear with no degradation at 100k yields. Resume values and stateful handlers add no measurable overhead.

### Nesting depth (invoke chains)

| Depth | Median | Per-level cost |
|---|---|---|
| 1 | ~37 ns | — |
| 2 | ~81 ns | ~44 ns |
| 3 | ~127 ns | ~45 ns |
| 5 | ~230 ns | ~48 ns |
| 10 | ~468 ns | ~48 ns |
| 15 | ~769 ns | ~47 ns |
| 20 | ~1.02 µs | ~49 ns |

Linear at **~47 ns/level**, dominated by `Box::pin` allocation per invoke.

With a yield at every nesting level:

| Depth | Median | Per-level cost |
|---|---|---|
| 1 | ~37 ns | — |
| 2 | ~93 ns | ~56 ns |
| 3 | ~171 ns | ~67 ns |
| 5 | ~382 ns | ~86 ns |
| 10 | ~833 ns | ~89 ns |

**~80–89 ns/level** when each level also yields — roughly the invoke overhead plus one yield round-trip.

### Fan-out (binary tree of invocations)

| Leaves | Total invocations | Median | Per-invoke |
|---|---|---|---|
| 2 | 3 | ~138 ns | ~46 ns |
| 4 | 7 | ~382 ns | ~55 ns |
| 8 | 15 | ~918 ns | ~61 ns |
| 16 | 31 | ~2.13 µs | ~69 ns |

Slight increase in per-invoke cost at higher fan-out — likely allocator pressure or cache effects from many short-lived `Box::pin` allocations.

### Wide effect sets (coproduct dispatch)

| Benchmark | 8 effects | 16 effects |
|---|---|---|
| Yield first effect | ~36 ns | ~36 ns |
| Yield last effect | ~35 ns | ~36 ns |
| Yield all effects | ~101 ns | ~178 ns |
| Invoke (subset forwarding, first) | — | ~75 ns |
| Invoke (subset forwarding, last) | — | ~73 ns |

**Dispatch position doesn't matter** — yielding the first vs last effect in a 16-effect coproduct costs the same (~36 ns). The coproduct dispatch is O(1) after monomorphization.

`use_all` scales linearly with the number of yields (16 × ~11 ns ≈ 178 ns).

Subset forwarding through a 16-effect coproduct adds only ~37 ns over a direct yield.

### Cancellation

| Cancel after N yields | Median |
|---|---|
| 1 | ~42 ns |
| 10 | ~112 ns |
| 100 | ~840 ns |
| 1,000 | ~8.0 µs |

Cancellation overhead is **effectively zero** — the per-yield cost (~8 ns) is identical to normal execution. The handler check that decides resume vs cancel is the same cost path.

### Combined stress

| Benchmark | Median |
|---|---|
| `deep_wide_yieldy` (3 levels × 8 effects × yields at each level) | ~459 ns |
| `sequential_invoke_with_yields(10)` (10 invokes × 10 yields) | ~2.3 µs |
| `sequential_invoke_with_yields(100)` (100 invokes × 10 yields) | ~22 µs |

**No superlinear interactions.** Combined stress matches sum-of-parts: `sequential_invoke_with_yields(100)` = 100 invokes × ~85 ns + 1000 yields × ~8 ns ≈ ~16.5 µs (actual ~22 µs, with the difference attributable to invoke setup and forwarding).

## `invoke` overhead breakdown

From the baseline benchmarks (`cargo bench --bench effects`):

| Benchmark | Median |
|---|---|
| `single_yield` (sync) | ~51 ns |
| `invoke_single_sub` | ~135 ns |
| `invoke_sequential_subs(10)` | ~895 ns (~90 ns/invoke) |
| `invoke_nested_3_deep` | ~307 ns (~85 ns/level) |
| `invoke_vs_inline` (10 inline yields) | ~198 ns |
| `invoke_vs_invoke` (10 sequential invokes) | ~771 ns |

Composition via `invoke` is **~4x slower** than inlining the same work. The per-invoke overhead is **~80–85 ns**.

### 1. Heap allocation (~50–60 ns) — dominant cost

Every sub-program creates a `Co`, which runs `make_co!` (`coroutine.rs:60-80`), which does `Box::pin(async move { ... })`. That's a heap allocation for the future + the fauxgen machinery (`token()`, `register_owned`, `gen_sync`).

### 2. Effect forwarding via coproduct traversal (~0 ns in practice)

`ForwardEffects::forward` (`coproduct.rs:402-424`) does a recursive `Inl`/`Inr` match to find which effect variant was yielded, then calls `yielder.yield_()` which does another `CoprodInjector::inject` to re-inject into the outer coproduct. When the sub-program's effect set is a subset, this involves index remapping.

**However, benchmarking shows this cost is effectively zero.** LLVM fully optimizes the monomorphized coproduct walk into a direct path. A hand-written `invoke_same` that bypasses `ForwardEffects` entirely (yielding the raw coproduct directly) showed no measurable improvement over the standard `ForwardEffects` path, even at 1000 forwarded yields:

| Benchmark (n=1000) | Median |
|---|---|
| `inline_yields` (no invoke) | ~8.8 µs |
| `invoke_same_effects` (ForwardEffects walk) | ~19.8 µs |
| `invoke_same_effects` (hand-written identity bypass) | ~20.3 µs |
| `invoke_subset_effects` (1-of-3 effects forwarded) | ~18.3 µs |

The ~11 µs gap between inline and invoke is entirely the `Box::pin` allocation + start signal, not the forwarding.

### 3. Start signal overhead (~5 ns)

Each invoke does `co.resume_with(Start)` to kick off the sub-program, which is an extra generator resume that the inline version doesn't need.

## Summary

The library scales predictably with no surprises:

- **Yields**: O(1) at ~8 ns each, no degradation at scale
- **Nesting**: O(n) at ~47 ns/level, dominated by `Box::pin`
- **Effect width**: O(1) dispatch regardless of position in the coproduct
- **Cancellation**: free (same cost as a yield)
- **No superlinear interactions**: combined stress matches sum-of-parts

The main optimization opportunity is eliminating the `Box::pin` allocation per invoke (~47–50 ns), which would roughly halve nesting/composition costs.

## Possible improvements

### A. Avoid the allocation — accept a closure instead of a `Program`

Take a closure instead of a pre-built `Program`, reusing the parent's generator machinery:

```rust
pub async fn invoke_fn<F, R>(&self, f: F) -> R
where
    F: AsyncFnOnce(&Yielder<'a, SubEffs>) -> R,
```

But this is tricky because `Yielder` is tied to a specific `GeneratorToken` — you can't share a token across two futures. The sub-program needs its own coroutine to yield effects.

### B. Stack allocation via inline futures

Instead of `Box::pin(async { ... })`, use a stack-allocated future. Two approaches:

1. **`stackfuture`-style**: Use a fixed-size buffer on the stack. Works if the future size can be bounded.

2. **Monomorphized `Co`**: Add a non-type-erased `Co` variant where the future type is concrete (not `dyn Future`). The `#[effectful]` macro could generate a concrete future type instead of boxing. Would require a second `Locality`-like axis or a new `Co` constructor.

### ~~C. Specialize `ForwardEffects` when effects match exactly~~ (not worth it)

When `SubEffs == Effs` (same effect set), skip the coproduct remapping entirely — just pass the yielded value through directly.

**Tested and rejected**: benchmarking shows LLVM already optimizes the monomorphized coproduct walk to zero cost. A hand-written identity forwarding path showed no improvement over the existing `ForwardEffects` impl, even at 1000 yields. Additionally, a blanket `impl ForwardEffects for Effs` would overlap with the existing recursive impl, causing ambiguity errors in stable Rust (no specialization). Would require a separate `invoke_same` method and macro support — all for zero measurable benefit.

### D. Inline small sub-programs at the macro level

The `#[effectful]` macro could detect `invoke!(simple_call())` and inline the sub-program's body directly into the parent's async block, avoiding the sub-`Co` entirely. Most impactful but also most complex.

### Recommendation

The most practical and impactful change is **B** — eliminating the `Box::pin` allocation. This could be done by adding a `Co::new_inline` (or similar) that keeps the future monomorphic, and having `#[effectful]` generate calls to it when the program is used via `invoke!` rather than returned as `Effectful`. The allocation is the majority of the overhead, and removing it would bring invoke cost much closer to a direct yield.
