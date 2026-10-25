# Changelog

## Unreleased

### Fixed

- Reduced catch overhead by inlining effect transport polling.
- Corrected cancellation documentation: timeouts can withdraw effects before dispatch and interrupt suspended handlers inside invoked sub-programs.
- Local handler dispatches share their root runner's budget across successive and nested invocations. A loop of short handled children can no longer bypass cooperative scheduling by starting a fresh budget for each child. Independently run programs retain their own budgets.

### Added

- A sub-program with handlers attached can be invoked. Its handlers run inside the invocation, and only its remaining effects must belong to the invoking program, whose handlers receive them. `invoke!` selects `Yielder::invoke` or the new `Yielder::invoke_handled`. Fully handled and caught sub-programs can be invoked too, so `invoke!(child.catch())` catches inside a body. Local handlers can be async; under `.run_sync()`, one that suspends panics. A local cancellation cancels the whole computation, and local dispatches share the outer runner's scheduling budget, also under `.run_sync()`.
- Effect operations work inside combinators that poll them with their own context, such as `FuturesUnordered`, as well as `select!` and `join!`. A yielder moved into a nested async runner within its own computation also works. Both previously panicked.
- Dropping a `yield_` or `invoke` operation before it completes withdraws its effect. An effect that the handler has not taken yet is never handled; a resume that the handler already provided is discarded. Previously, an abandoned effect could be handled at a later suspension or dropped silently. The advanced guide describes how this interacts with `select!`.

### Changed

- **Breaking:** `Program::catch` returns an ordinary program whose result is `Result<T, E>`, instead of a `Catch` wrapper. The caught body takes its own throws and completes with the error, so a throw no longer cancels the run, and handlers, state, and `.map()` can follow `.catch()`. The run still returns `Ok(Ok(value))`, `Ok(Err(error))`, or `Err(Cancelled)`. The `Catch` type and its `run` and `run_sync` methods are removed, and `catch` gains a type parameter for the position of `Throw<E>`, so explicit turbofish calls need one more `_`.
- Effects travel through a small frame owned by each run, and `fauxgen` is no longer a dependency. A yielder finds its run's frame by a unique id while the runner polls the computation, so the transport no longer relies on another crate's private API or on allocation addresses.
- Runs no longer allocate, and `invoke` moves each effect of a sub-program into its parent's frame without a future per level. In our measurements against 0.7.0, one synchronous effect takes 44–56% less time, 1,000 effects take 25–27% less, and 5 to 20 nested invocations take 62–66% less. Boxed bodies are faster too. The performance guide has been re-measured against 0.7.0. The run future of a 20-level invocation chain shrinks from 8,048 to 1,952 bytes.
- Resume values no longer need to be `Sync`. `CoSend::new`, `.boxed_send()`, and `Program::new_send` drop their resume bounds, and the sendable `.run()`, `.run_stateful()`, `asynk::run_send`, and `asynk::run_stateful_send` require resumes to be `Send` only. An unrun program places no requirement on its resumes.
- Polling an effect operation outside its computation panics with "effect operation polled outside its computation".

## v0.7.0 (2026-10-23)

### Added

- The `Throw<E>` effect aborts a computation with a typed error. Throw with `throw!(error)` inside `#[effectful]` functions or `Yielder::throw` elsewhere. `Program::catch` handles it: the caught run returns `Ok(Err(error))` for a throw, `Ok(Ok(value))` on completion, and `Err(Cancelled)` for cancellation, so `?` propagates cancellation and keeps the error. Programs that do not throw are unaffected.

### Changed

- **Breaking:** Local boxed programs, from `.boxed()`, `#[effectful(..., boxed)]`, or `Program::from_co` with a `Co`, accept the same handlers as concrete programs. Their `.run()` and `.run_stateful()` now accept synchronous handlers, async handlers, and the `local`/`local_stateful` adapters in any mix, without first calling `.with_state(&mut ())`. Async closures that borrow their captures across an await must be wrapped with `corophage::local` or `corophage::local_stateful`, as they already were for concrete programs. The low-level `asynk::run` and `asynk::run_stateful` follow the same rules.
- The effects guide documents that a resume borrowing a value of the effect's type parameter, as in `#[effect(&'r T)]`, needs that parameter to be `'static`. Lifting this requirement is blocked by a compiler limitation.

## v0.6.2 (2026-10-19)

### Changed

- Synchronous execution (`.run_sync()`, `sync::run`, and `sync::run_stateful`) polls the program directly instead of driving the async execution loop. In our measurements this is about 7–12% faster for flat programs and 13–22% faster for 20 nested invocations, with the same allocations.
- The performance documentation has been re-measured. The previously reported slowdown of synchronous loops relative to boxed bodies no longer applies; every measured operation is now faster.

## v0.6.1 (2026-10-11)

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
- The documentation notes that `yield_` and `invoke` are not cancel-safe, and that the `Send` generality limitation applies to any program with lifetime-parameterized effects, not only stateful runners.

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
