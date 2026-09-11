# Changelog

## Unreleased

### Added

- **`Program::map` transforms return values** — map a body before or after attaching handlers and state while preserving its effects, cancellation, and conditional `Send`. Mapping is lazy and adds no allocation of its own. Mapped boxed bodies use inferred execution with the existing `local` / `local_stateful` adapters for lending async handlers. The `higher_order` example builds a generic effectful `Option` map with borrowed resumes and skips its callback for `None`.

- **Public bounds for generic effect sets** — `Effects<'a>` and `Resumes<'r, Es>` are available from the crate root, and the existing `EmbedEffect` / `ProjectResume` traits are documented for generic invocation helpers. `Effects<'a>` is also available through the prelude alongside `Effects!`. Helpers can preserve unknown effect sets and forward borrowed resumes without private module paths.

- **Exact state-change assertion example** — `cargo run --example debug_snapshot` runs model actions with `Get` / `Set` handlers and checks that the final state matches all expected changes. Add `-- --demonstrate-failure` to show how an omitted change fails the assertion. The check covers the model state handled by the example.

### Fixed

- **The test suite supports disabled procedural macros** — macro-dependent examples, benchmarks, and tests now require the `macros` feature. Manual API tests remain enabled, and CI runs tests and Clippy with `--no-default-features` as well as with all features.

- **Execution measurements match committed sources** — reran the comparison from a clean commit, verified its source and generated-fixture hashes, and refreshed the README and performance guides from the new record.

- **Execution comparisons use isolated Cargo targets** — the measurement script now overrides `CARGO_TARGET_DIR` and Cargo target-directory configuration for both revisions and the candidate-only cases, keeping clean builds and executable lookup consistent.

- **Nesting benchmarks measure their stated depths** — the depth-15 and depth-20 fixtures now form single nested chains with one leaf effect. The depth-10 yielding fixture yields once at each of ten levels. Shared fixtures have regression checks for peak active depth and effect counts, and the website reports refreshed measurements.

- **Stateful async execution preserves `Send`** — `Program::new_send(...).run_stateful(&mut state)` now returns a `Send` future when the state and result are `Send`, handlers are `Send + Sync`, and each handler future is `Send`. Handlers can borrow the shared state across awaits. Local programs retain support for non-`Send` state and handlers.

- **Explicit effect lifetimes work in enclosing `impl` and `trait` scopes** — `#[effectful('a)]` can reuse an enclosing lifetime in methods, associated functions, and default trait methods without redeclaring it. Generated input lifetime names also avoid the selected effect lifetime.

- **Nested reference arguments keep independent lifetimes** — `#[effectful]` no longer equates elided reference lifetimes with the program lifetime. Sequential calls with arguments such as `&mut &str` and `&mut &mut T` can now reuse the outer mutable borrow after each program completes, while active programs retain their required borrows.

- **Placeholder lifetimes in borrowed wrapper arguments compile** — `#[effectful]` now accepts arguments such as `Cow<'_, str>` and nested borrowed wrappers without generating invalid lifetime bounds. Each placeholder remains independent, including in invariant wrappers such as `RefMut`.

- **Ready async handlers yield cooperatively** — async runners now yield to the executor after a bounded number of effect dispatches, so a long or infinite effect loop with immediately ready handlers gives other tasks and cancellation a chance to run. This applies to local, sendable, and stateful async execution.

### Changed

- **Concrete program bodies are now the default (breaking)** — `Program::new` and `#[effectful]` retain the body future's type and infer `Send` from the execution components. Use `Program<'a, Effs, impl Future<Output = R> + 'a>` for a concrete return signature, or add `.boxed()` / `.boxed_send()` when returning the retained `Effectful` alias. Recursive macro functions can use `#[effectful(..., boxed)]` or `#[effectful(..., boxed, send)]`. Effectful trait methods called through `dyn Trait` also need `boxed`, since the default opaque return type prevents dynamic dispatch. The `send` flag still checks an explicit body guarantee.

- **State setup and execution share one protocol** — bind state with `.with_state(&mut state)`, then use `.run()` or `.run_sync()`. Stateful and state-free handlers can share a list, as can sync and async handlers when using `.run()`. Mutable dispatch removes the blanket handler `Sync` bound from the new path. State is borrowed from setup until completion, cancellation, or drop. Existing terminal stateful methods and erased constructors remain available with their compatibility contracts.

- **Lending async closures use explicit local adapters on the new path (breaking)** — wrap a closure that borrows its captures across awaits with `corophage::local(...)` or `corophage::local_stateful(...)`. Each adapter boxes one non-`Send` future per call. Ordinary handlers have no per-dispatch allocation. Erased local async runners retain direct lending closure support and require async callables. Use `.with_state(...).run()` to select mixed dispatch or the new adapters for an erased body.

- **Execution internals consolidated** — sync and sendable dispatch now share the concrete mutable protocol and loop. Four old dispatch families and the unchecked `SendFuture` wrapper were removed; two local async compatibility families remain. Invocation stores each pinned child once to keep concrete nested future storage linear in the invocation depth. Rust 1.93 remains supported.

- **Named `#[effectful]` lifetimes require a Rust declaration** — declare the lifetime on the function, enclosing `impl`, or enclosing `trait`. Code that relied on `#[effectful('a)]` to introduce `'a` must add a declaration such as `fn read<'a>(...)`. Omitted lifetimes and `'_` continue to use automatic inference.

- **MSRV raised to Rust 1.93** — corophage now requires Rust 1.93 or later. The newer compiler can prove that recursive async handler dispatch futures are `Send`, removing an internal unchecked `Send` assertion and its associated wrapper plumbing.

## v0.5.0 (2026-07-16)

### Fixed

- **Explicit `#[effectful]` lifetimes parse correctly** — a sole named lifetime no longer requires a trailing comma, `'static` remains concrete instead of becoming an invalid generic parameter, and `'_` uses normal lifetime inference.

- **`Co` and `CoSend` are now available from the crate root** — the documented `corophage::{Co, CoSend}` imports now compile without routing through the `coroutine` module.

- **Proc-macro expansion is now hygienic** — `#[effect]` and `#[effectful]` honor Cargo dependency renames and no longer collide with user parameters named `__y` or effect lifetimes named `'r`.

- **Overlapping `Yielder` operations can no longer cause undefined behavior** — `yield_` and `invoke` now require exclusive `&mut self` access, so safe Rust rejects starting a second operation before awaiting the first. Manual `Program::new` and `Co::new` closures must bind their `Yielder` parameter as mutable. Incorrect explicit dispatch indices also produce a checked panic in release builds.

- **Invalid resume projections no longer cause undefined behavior** — directly invoking the hidden `ProjectResume` helper with an unmatched resume now produces a checked panic in all build profiles.

- **Async `Sendable` programs can now be spawned** — the future returned by `Program::run` for `Program::new_send` and `#[effectful(..., send)]` computations is now `Send` when the result and handler futures are `Send`, making it compatible with `tokio::spawn`.

- **`#[effectful]` now accepts ordinary borrowed and generic arguments** — elided references, `&self` receivers, generic parameters, and argument-position `impl Trait` are tied to the generated program lifetime automatically, without artificial lifetime annotations or outlives bounds.

- **Async computation bodies can now await ordinary futures** — `Program::run`, `asynk::run`, and invoked subprograms now drive non-effect suspension points instead of panicking. Synchronous runners continue to support effect suspension and now report a clear error directing callers to an async runner when a computation awaits a pending non-effect future.

## v0.4.1 (2026-05-18)

### Fixed

- **Public macros no longer require downstream crates to depend on `frunk_core` directly** — `Effects!`, empty effect sets, spread-based `#[effectful]` signatures, and internal runner macro expansions now route through `corophage`'s hidden `frunk_core` re-export.

### Added

- **Expanded examples and website docs** — added saga-style order processing, stepwise debugger, and choreographic programming examples, plus website pages for the saga and debugger examples.

## v0.4.0 (2026-03-28)

### Fixed

- **`invoke!` inside `#[effectful(..., send)]` functions** — previously failed to compile due to a compiler limitation ([rust-lang/rust#100013](https://github.com/rust-lang/rust/issues/100013)) where the compiler could not prove that `ForwardEffects::forward`'s return type was `Send`. Internally replaced with synchronous coproduct conversions (`EmbedEffect`/`ProjectResume`) that the compiler handles correctly.

- **Sequential `invoke!` calls with mutable borrows** — `invoke!` previously required sub-programs to borrow for the entire outer computation's lifetime, preventing patterns like `invoke!(sub(&mut state)); state.mutate(); invoke!(sub(&mut state));`. Sub-programs can now borrow from shorter-lived data than the outer program.

- **`invoke!` with overlapping resume types** — fixed an index inference bug where multiple effects sharing the same resume type (e.g., both resuming with `()`) caused incorrect dispatch.

### Changed

- **Manual `Effect` impls require `shorten_resume`** — the `Effect` trait now includes a `shorten_resume` method that witnesses the covariance of `Resume<'r>` in `'r`. This is needed by `invoke` to safely shorten resume lifetimes when composing programs. The body is always just `resume`. The `#[effect]` macro generates it automatically, so this only affects hand-written `impl Effect` blocks.

- **`ForwardEffects` trait removed** — replaced internally by `EmbedEffect` and `ProjectResume`. This trait was not part of the public API.

## v0.3.2 (2026-03-27)

### Added

- **`Eff` added to the prelude** — the deprecated `Eff` type alias (renamed to `Effectful` in v0.3.0) is now re-exported from `corophage::prelude` for convenience during migration.

- **`...Spread` syntax in `#[effectful]` and `Effects!`** — reference a pre-defined effects type alias with `...Alias`, using the same spread syntax as frunk's `Coprod!(...Tail)`. Works in both `#[effectful(Extra, ...MyEffs)]` and `Effects![Extra, ...MyEffs]`. Inline effects can precede the spread, and `send`/lifetime arguments compose naturally: `#[effectful('a, Extra, ...MyEffs<'a>, send)]`.

## v0.3.1 (2026-03-26)


### Changed

- **`Eff` type alias renamed to `Effectful`** — the `Eff<'a, Effs, R, L>` type alias for an unhandled `Program` has been renamed to `Effectful<'a, Effs, R, L>` for clarity. The `#[effectful]` macro now generates `Effectful<...>` as its return type.
- **`unsafe { unreachable_unchecked() }` replaced with debug-checked macro** — all internal uses of `core::hint::unreachable_unchecked()` are now wrapped in a `debug_unreachable!` macro that panics with a descriptive message in debug builds, while preserving the optimization hint in release builds. This makes invariant violations easier to diagnose during development.

### Removed

- **`declare_effect!` macro removed** — use the `#[effect(ResumeType)]` attribute macro instead, which supports all the same features (lifetimes, generics, named fields, GAT resume types) plus visibility modifiers, multiple trait bounds, `#[derive(...)]`, and arbitrary attributes.

## v0.3.0 (2026-03-11)

### Added

- **Program composition via `Yielder::invoke`** — invoke a sub-program from within another program, forwarding the inner program's effects through the outer yielder. The sub-program's effects must be a subset of the outer program's effects. This enables reusable effectful computations that can be composed together.

  ```rust
  #[effectful(Ask, Print)]
  fn greet() {
      let name: &str = yield_!(Ask("name?"));
      yield_!(Print(format!("Hello, {name}!")));
  }

  #[effectful(Ask, Print, Log)]
  fn main_program() {
      yield_!(Log("Starting..."));
      invoke!(greet());
      yield_!(Log("Done!"));
  }
  ```

- **`invoke!()` macro** — call a sub-program inside an `#[effectful]` function. Expands to `__y.invoke(expr).await`. Outside `#[effectful]`, emits a compile error. For the manual `Program::new` API, use `y.invoke(program).await` directly.

- **`ForwardEffects` trait** — internal trait used by `Yielder::invoke` to forward each effect variant from a sub-program's coproduct through the outer yielder.

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

- **Removed `Send + Sync` bounds from `Effect::Resume<'r>`** — the `Effect` trait no longer requires resume types to be `Send + Sync`. This allows effects with non-`Send` resume types (e.g., `Rc<str>`) when used with non-`Send` coroutines (`Co`/`Program::new`). The bounds are still enforced for `Send`-able coroutines (`CoSend`/`Program::new_send`) via the existing `for<'r> Resumes<'r, ...>: Send + Sync` constraint. The `#[effect]` proc macro and `declare_effect!` macro no longer add `Send + Sync` bounds on generic type parameters used in the resume type.
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
