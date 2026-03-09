# Changelog

## Unreleased

### Added

- **`CoControl::resume_for`** — disambiguates effects that share the same `Resume` type. When multiple effects have identical resume types (e.g., both resume with `()`), `CoControl::resume()` cannot infer the correct coproduct index. `resume_for::<E, _>(value)` resolves the index from the effect type instead:

  ```rust
  // Before: ambiguous when Foo and Bar both have Resume = ()
  CoControl::resume(())  // error: type annotations needed

  // After: specify which effect to resume for
  CoControl::resume_for::<Foo, _>(())  // unambiguous
  ```

  The existing `CoControl::resume()` continues to work when resume types are distinct.

- **`InjectResume` trait** — maps effect types to their position in the resume coproduct, backing `resume_for`. Automatically implemented for all effect coproducts.

## v0.2.0 (2026-03-08)

### Added

- **`Program` type** — a builder-style API for assembling and running effect-handled computations. This is now the recommended way to use the library for most users.

  The key feature is **incremental handler attachment**: handlers are added one at a time via `.handle()`, which means a partially-handled program is a first-class value you can pass around, store, or extend later. The compiler tracks which effects are still unhandled and only permits running the computation once all effects have a handler.

  ```rust
  // Define a computation once...
  let program = Program::new(|yielder: Yielder<'_, Effs>| async move {
      let n = yielder.yield_(Counter).await;
      let answer = yielder.yield_(Ask("question")).await;
      (answer, n)
  });

  // ...attach handlers incrementally, e.g. in different modules or call sites...
  let program = program.handle(|_: Counter| CoControl::resume(42u64));
  let program = program.handle(|_: Ask| CoControl::resume("yes"));

  // ...and run only when all effects are handled.
  let result = program.run_sync();
  ```

  Available constructors and methods:

  - `Program::new(f)` — creates a program from an async closure
  - `Program::new_send(f)` — creates a `Send`-able program (for use with `tokio::spawn`)
  - `Program::from_co(co)` — wraps an existing `Co`/`CoSend` coroutine
  - `.handle(handler)` — attaches the next handler (in effect declaration order); type-checked at compile time
  - `.run_sync()` — executes synchronously, returns `Result<R, Cancelled>`
  - `.run_sync_stateful(&mut state)` — executes synchronously with shared mutable state
  - `.run()` — executes asynchronously, returns `Result<R, Cancelled>`
  - `.run_stateful(&mut state)` — executes asynchronously with shared mutable state

- **`handle` free function** — functional alternative to `.handle()`, useful when incrementally building up a program across call sites without method chaining:

  ```rust
  let p = handle(p, |_: Counter| CoControl::resume(42u64));
  let p = handle(p, |_: Ask| CoControl::resume("yes"));
  p.run_sync()
  ```

- All public items are now documented.

### Changed

- **`run_with` renamed to `run_stateful`** everywhere — affects `sync::run_stateful` and `asynk::run_stateful` (previously `sync::run_with` / `run_with` at crate root).

- **`asynk` module is now public** — async runner functions `asynk::run` and `asynk::run_stateful` are now accessed via `corophage::asynk::*` instead of being re-exported at the crate root.

- **Prelude narrowed** — `use corophage::prelude::*` now re-exports only `Effect`, `Effects!`, `CoControl`, `Cancelled`, `Never`, `Program`, `Yielder`, `frunk`, and `hlist`. `Co`, `CoSend`, `sync`, and `asynk` are no longer in the prelude but remain public.

## v0.1.0 (2026-03-02)

Initial release.
