# Changelog

## Unreleased

### Added

- A bare `#[effect]` is shorthand for `#[effect(())]`.

### Fixed

- Dropping a `Yielder` before the computation finishes no longer allows undefined behavior. Previously, a yielder from another program could then be accepted by the running computation, delivering an effect of the wrong type to its handlers. Such misuse now panics.
- `#[effectful]` accepts type parameters that appear only in the effects or return type, such as `fn get<T>() -> T` with `#[effectful(Get<T>)]`.
- `#[effectful]` accepts placeholder lifetimes in trait-object arguments, such as `Box<dyn Display + '_>`.
- `#[effectful]` no longer triggers Clippy's `multiple_bound_locations` lint on functions with bounded type parameters.
- `#[effectful]` no longer triggers rustc's `unused_braces` warning on single-line function bodies.

### Changed

- When a synchronously run program suspends on an ordinary future, the panic message now covers nested `.run().await` calls and suggests `invoke` or `.run_sync()` instead. The programs guide documents this case.
- `fauxgen` is pinned to version 0.1.7, because corophage relies on its unstable internal API.

## v0.6.0 (2026-09-19)

### Added

- `Program::map` transforms a program's return value without changing its effects or cancellation behavior.
- `Effects` and `Resumes` are exported from the crate root for helpers that work with generic effect sets. `Effects` is also in the prelude.
- The `higher_order` example shows an effectful `Option` map. The `debug_snapshot` example checks expected state changes.
- `.with_state(&mut state)` binds state before calling `.run()` or `.run_sync()`. Stateful and stateless handlers can be mixed, as can sync and async handlers with `.run()`.

### Fixed

- `Program::new_send(...).run_stateful(&mut state)` returns a `Send` future when the state, result, and handler futures are `Send`, and handlers are `Send + Sync`.
- `#[effectful('a)]` accepts lifetimes declared on an enclosing `impl` or `trait`.
- `#[effectful]` no longer keeps nested mutable references borrowed after a program completes.
- `#[effectful]` accepts placeholder lifetimes in arguments such as `Cow<'_, str>` and `RefMut<'_, T>`.
- Async effect loops with immediately ready handlers no longer starve other tasks or prevent cancellation.
- Corrected nesting benchmark depths and updated the published performance results.

### Changed

- **Breaking:** `Program::new` and `#[effectful]` return concrete program types by default. Add `.boxed()` or `.boxed_send()` when returning `Effectful`. Recursive functions and methods called through `dyn Trait` need `#[effectful(..., boxed)]`, optionally with `send`.
- **Breaking:** Async handlers that borrow their captures across awaits need `local(...)` or `local_stateful(...)` with concrete programs. These adapters produce non-`Send` futures. Existing erased local runners still accept lending closures directly.
- **Breaking:** Named `#[effectful]` lifetimes must be declared on the function, enclosing `impl`, or enclosing `trait`. Omitted lifetimes and `'_` are still inferred.
- Minimum supported Rust version raised to 1.93.

## v0.5.0 (2026-07-16)

### Fixed

- `#[effectful]` accepts a lone named lifetime without a trailing comma, handles `'static` correctly, and infers `'_`.
- `Co` and `CoSend` can be imported from the crate root.
- `#[effect]` and `#[effectful]` support renamed Cargo dependencies and avoid collisions with user names.
- Overlapping `Yielder` operations no longer cause undefined behavior. `yield_` and `invoke` now require `&mut self`; manual `Program::new` and `Co::new` closures must bind the yielder as mutable.
- Invalid explicit dispatch indices and resume projections panic instead of causing undefined behavior.
- Async programs created with `Program::new_send` or `#[effectful(..., send)]` can use `tokio::spawn` when their result and handler futures are `Send`.
- `#[effectful]` supports elided references, `&self`, generic arguments, and argument-position `impl Trait` without extra lifetime annotations.
- Async programs and invoked subprograms can await ordinary futures. Sync runners report an error if such a future suspends.

## v0.4.1 (2026-05-18)

### Fixed

- Public macros no longer require a direct dependency on `frunk_core`.

### Added

- Saga, debugger, and choreographic programming examples, with website guides for the saga and debugger.

## v0.4.0 (2026-03-28)

### Fixed

- `invoke!` works inside `#[effectful(..., send)]` functions.
- Invoked subprograms can borrow data for less than the outer program's lifetime, allowing sequential calls with mutable borrows.
- `invoke!` dispatches correctly when multiple effects share a resume type.

### Changed

- Manual `Effect` implementations must define `shorten_resume` with a body that returns `resume`. `#[effect]` generates it automatically.

## v0.3.2 (2026-03-27)

### Added

- The deprecated `Eff` alias is re-exported from the prelude to ease migration to `Effectful`.
- `#[effectful]` and `Effects!` accept effect aliases using `...Alias`, including after inline effects.

## v0.3.1 (2026-03-26)

### Changed

- Renamed the `Eff` type alias to `Effectful`.

### Removed

- Removed `declare_effect!`. Use `#[effect(ResumeType)]` instead.

## v0.3.0 (2026-03-11)

### Added

- `Yielder::invoke` and `invoke!` compose programs. The invoked program's effects must be a subset of the caller's effects.
- `#[effect(ResumeType)]` derives `Effect` for structs, including generic effects and borrowed resume types.
- `#[effectful(Eff1, Eff2, ...)]` declares effectful functions with `yield_!` and `invoke!`. It supports lifetime inference, explicit lifetimes, and a `send` flag.
- Procedural macros are enabled by default. Set `default-features = false` to disable them.
- `Control<R>` lets handlers return `Control::resume(value)` or `Control::cancel()` without depending on the full effect set.
- `Program::handle_all` attaches multiple handlers from an HList in any order.
- `.handle()` and `.handle_all()` accept stateful sync and async handlers.

### Changed

- Local programs accept non-`Send` resume types such as `Rc<str>`. Sendable programs still require resume types to be `Send + Sync`.
- `Program::handle()` accepts handlers in any order. Low-level runners still require effect declaration order.
- `Control<R>` replaces the public `CoControl` type, including in the prelude.

## v0.2.0 (2026-03-08)

### Added

- `Program` builds computations by attaching handlers with `.handle()`. Programs can run only once all effects are handled.
- `Program::new`, `Program::new_send`, and `Program::from_co` create local programs, sendable programs, or programs from existing coroutines.
- `.run()` and `.run_sync()` execute programs; `.run_stateful()` and `.run_sync_stateful()` also take shared mutable state. All return `Result<R, Cancelled>`.
- The `handle` free function provides an alternative to `.handle()`.
- Documentation for all public items.

### Changed

- Renamed `run_with` to `run_stateful`.
- Async runners are accessed through `corophage::asynk` instead of the crate root.
- `Co`, `CoSend`, `sync`, and `asynk` are no longer in the prelude. Import them explicitly.

## v0.1.0 (2026-03-02)

Initial release.
