# Performance Analysis: `invoke` overhead

## Benchmark results (baseline)

| Benchmark | Median |
|---|---|
| `single_yield` (sync) | ~51 ns |
| `invoke_single_sub` | ~135 ns |
| `invoke_sequential_subs(10)` | ~895 ns (~90 ns/invoke) |
| `invoke_nested_3_deep` | ~307 ns (~85 ns/level) |
| `invoke_vs_inline` (10 inline yields) | ~198 ns |
| `invoke_vs_invoke` (10 sequential invokes) | ~771 ns |

Composition via `invoke` is **~4x slower** than inlining the same work. The per-invoke overhead is **~80-85 ns**.

## Where the overhead comes from

### 1. Heap allocation (~50-60 ns) — dominant cost

Every sub-program creates a `Co`, which runs `make_co!` (`coroutine.rs:60-80`), which does `Box::pin(async move { ... })`. That's a heap allocation for the future + the fauxgen machinery (`token()`, `register_owned`, `gen_sync`).

### 2. Effect forwarding via coproduct traversal (~15-20 ns)

`ForwardEffects::forward` (`coproduct.rs:417-422`) does a recursive `Inl`/`Inr` match to find which effect variant was yielded, then calls `yielder.yield_()` which does another `CoprodInjector::inject` to re-inject into the outer coproduct. When the sub-program's effect set is a subset, this involves index remapping. Each forwarded yield pays this injection/extraction tax on top of the normal yield cost.

### 3. Start signal overhead (~5 ns)

Each invoke does `co.resume_with(Start)` to kick off the sub-program, which is an extra generator resume that the inline version doesn't need.

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

### C. Specialize `ForwardEffects` when effects match exactly

When `SubEffs == Effs` (same effect set), skip the coproduct remapping entirely — just pass the yielded value through directly.

### D. Inline small sub-programs at the macro level

The `#[effectful]` macro could detect `invoke!(simple_call())` and inline the sub-program's body directly into the parent's async block, avoiding the sub-`Co` entirely. Most impactful but also most complex.

## Recommendation

The most practical and impactful change is **B** — eliminating the `Box::pin` allocation. This could be done by adding a `Co::new_inline` (or similar) that keeps the future monomorphic, and having `#[effectful]` generate calls to it when the program is used via `invoke!` rather than returned as `Effectful`. The allocation is the majority of the overhead, and removing it would bring invoke cost much closer to a direct yield.
