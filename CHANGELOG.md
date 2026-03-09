# Changelog

## Unreleased

### Added

- **`Control<R>`** — new return type for effect handlers, parameterized by the resume type `R` instead of the full effect set. Handlers now return `Control::resume(value)` or `Control::cancel()`, making them reusable across different effect sets.

  ```rust
  // Before: handler was coupled to the full effect set
  |_: Counter| CoControl::resume(42u64)
  |_: Ask| CoControl::<'static, Effects![Counter, Ask]>::cancel()

  // After: handler only knows its own resume type
  |_: Counter| Control::resume(42u64)
  |_: Ask| Control::<&str>::cancel()
  ```

- **`Program::handle_all`** — attach multiple handlers at once from an HList. Handlers can cover any subset of the remaining effects, in any order.

  ```rust
  let handlers = hlist![
      |_: Counter| Control::resume(42u64),
      |_: Ask| Control::resume("yes"),
  ];

  Program::new(|y: Yielder<'_, Effects![Other, Counter, Ask]>| async move { ... })
      .handle_all(handlers)
      .handle(|_: Other| Control::resume(()))
      .run_sync()
  ```

### Changed

- **Handlers can be in any order** — effect dispatch now uses type-based handler lookup (`FindHandler`) instead of positional matching. Handlers attached via `.handle()`, `.handle_all()`, or passed to `sync::run`/`asynk::run` no longer need to be in the same order as the `Effects![...]` list.
- **`CoControl` is now internal** — replaced by `Control<R>` in user-facing code. `CoControl` is still used internally by the runner loop but is no longer exported.
- **Prelude updated** — `CoControl` removed from prelude, `Control` added.

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
