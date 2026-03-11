# Changelog

## Unreleased

### Added

- **`#[effect(ResumeType)]` proc macro** — derive an `Effect` impl by annotating a struct. Supports lifetimes, generics, and GAT resume types via `'r`.

  ```rust
  #[effect(bool)]
  pub struct Ask(i32);

  #[effect(&'r str)]
  pub struct GetConfig;

  #[effect(())]
  pub struct Log<'a>(pub &'a str);
  ```

- **`#[effectful(Eff1, Eff2, ...)]` proc macro** — mark a function as an effectful computation. The macro transforms the return type to `Eff<...>`, wraps the body in `Program::new`, and enables `yield_!(expr)` syntax inside the function.

  ```rust
  #[effectful(Ask, Log<'a>)]
  fn my_prog<'a>(msg: &'a str) -> bool {
      yield_!(Log(msg));
      yield_!(Ask(42))
  }
  ```

  Supports a `send` flag for `Send`-able programs (`#[effectful(Ask, send)]`), automatic lifetime inference, and explicit lifetime annotation as the first argument.

- **`yield_!()` fallback macro** — emits a clear compile error when used outside an `#[effectful]` function.

- **`corophage-macros` crate** — new proc-macro crate, added as a workspace member. Re-exported from `corophage` behind the `macros` feature (enabled by default).

- **`macros` feature flag** — controls whether the proc macros are available. Enabled by default; disable with `default-features = false` to opt out.

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

- **`HandlersToEffects` impls for stateful handlers** — `Fn(&mut S, E) -> Control<E::Resume<'a>>` and `AsyncFn(&mut S, E) -> Control<E::Resume<'a>>` closures are now recognized by `HandlersToEffects`, enabling stateful handlers to work with `handle()` and `handle_all()`.

### Changed

- **`Program::handle()` is now order-independent** — `.handle()` now uses `CoproductSubsetter` (like `.handle_all()`) to remove the handled effect from the remaining set, so handlers can be attached in any order. Handlers passed to the low-level `sync::run`/`asynk::run` functions must still match the `Effects![...]` order.
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
