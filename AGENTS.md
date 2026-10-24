# corophage

Algebraic effects for stable Rust, built on async/await with an internal frame transport, and `frunk_core` (heterogeneous lists/coproducts).

## Architecture

- `corophage/src/effect.rs` — `Effect` trait (with GAT `Resume<'r>`), `MapResume` (with GAT `Output<'r>`), `Resumes<'r, E>`, `Effects`
- `corophage/src/frame.rs` — the frame transport: `Frame` (id, effect/resume slot), the thread-local registry, `FrameRef` (a typed frame id), `Operation` (one withdrawable effect or signal on a frame), `RawYield`, `Interceptor`, and the `Generator` adapter that polls a body with its frame registered. `Signal::Cancel` and `Signal::Reschedule` share the slot with effects. Its module docs state invariants I1–I5
- `corophage/src/coroutine.rs` — concrete `Coroutine<'a, Effs, F>`, erased `GenericCo<'a, Effs, Return, L>` with `Co`/`CoSend` aliases, the intercepting body wrapper used by `catch`, the `yield_` future, and `Yielder<'a, Effs>`
- `corophage/src/invoke.rs` — invocation: `unhandled` (the `Direct` future, which moves effects and signals between frames) and `handled` (a local dispatch loop whose `Forward` handler serves the remaining effects and whose `Relay` scheduler sends signals to the parent), plus the hidden `Program::__invoke` methods through which `invoke!` selects one
- `corophage/src/control.rs` — `Control<R>` (handler resume/cancel) and `Cancelled`
- `corophage/src/catch.rs` — the `Throw<E>` effect and `Program::catch`, which wraps the body so that it takes its own throws from its frame (`Coroutine::intercept`) and completes with `Err`. The placeholder `CatchHandler` only fills the dispatch entry for `Throw<E>` and is never called
- `corophage/src/coproduct/dispatch.rs` — one mutable dispatch protocol with named branch/injection futures, synchronous metadata, and explicit `local`/`local_stateful` lending adapters
- `corophage/src/coproduct.rs` — handler-kind inference and effect/resume conversion
- `corophage/src/execution.rs` — cooperative async loop (`drive_with`, parameterized by a `Scheduler`: a root yields to the executor, a handled invocation relays signals) and inline synchronous loop sharing one dispatch protocol, one state owner, owned or lent handler lists (`HandlerOwner`), short handler/state loans, and checked `Send` compatibility wrapper
- `corophage/src/locality.rs` — `Locality` sealed trait, `Local` and `Sendable` marker types for controlling `Send`-ness of `Co`, and the execution mode of erased programs (`Local` uses inferred execution, `Sendable` keeps explicit `Send` methods)
- `corophage/src/macros.rs` — the `Effects!` public macro and the `yield_!`/`invoke!`/`throw!` fallback macros
- `corophage/src/lib.rs` — low-level sync/async runners and primary exports including `Locality`/`Local`/`Sendable` and the local lending adapters
- `corophage/src/program.rs` — `Program<'a, Effs, F>` retains a concrete body by default; `.with_state()` binds state and `.run()`/`.run_sync()` execute. `Effectful<'a, Effs, R, L>` is the explicit erased alias; `.boxed()`/`.boxed_send()`, `new_send`, and `from_co` preserve erased signatures and terminal contracts.
- `corophage/src/prelude.rs` — re-exports core types (`Effect`, `Effectful`, `Effects!`, `Control`, `Cancelled`, `Never`, `Program`, `Throw`, `Yielder`, `frunk`, `hlist`); `Co`/`CoSend`/`sync`/`asynk` are public but not in prelude
- `corophage-macros/` — `corophage-macros` proc-macro crate providing `#[effect]` and `#[effectful]` attribute macros

## Key Design Constraints

- **Effects are parameterized by lifetime `'a`**: The bound is `Effects<'a>: MapResume + Send + Sync + 'a`, propagated through `Co<'a, Effs, Return>`. Effects can borrow non-`'static` data by using a shorter lifetime.
- **`Effect::Resume<'r>` is a GAT**: Resume types can contain borrowed or local data. Sendable execution requires `Send` for the stored resumes; local execution does not. Nothing shares stored resumes, so they never need `Sync`. Keep the computation lifetime independent of temporary state/handler loans, and never let a resume borrow escape such a loan.
- **`Co` vs `CoSend`**: `Co<'a, Effs, Return>` is not `Send` (uses `Pin<Box<dyn Future + 'a>>`). Use `CoSend<'a, Effs, Return>` (backed by `Sendable` locality) for `Send`-able coroutines compatible with `tokio::spawn`. Both are type aliases for `GenericCo<'a, Effs, Return, L>` parameterized by a `Locality` marker.
- **`Program` is the primary API**: `Program::new` creates a computation from a closure, `.handle()` attaches handlers one at a time, `.run_sync()`/`.run()` execute once all effects are handled. `Co`/`CoSend` and the direct `run`/`sync::run` functions are advanced/internal.
- **Handler order is flexible**: `.handle()` and `.handle_all()` remove handled effects from `Remaining`. Low-level handler lists also support lookup in any order and can contain extra handlers. Missing and duplicate program handlers remain compile errors.
- **Ordinary async dispatch uses `FnMut(...) -> Future`**: Lending async captures use `local`/`local_stateful`, with one boxed non-`Send` future per call, for every program kind and the low-level runners. Rust 1.93 remains the MSRV.

## Internal Invariants

- Dispatch injects resumes through `InjectResume`; `Yielder::yield_` keeps a checked panic for deliberately mismatched public index parameters.
- Pin projection uses `pin-project-lite`. `Yielder`, `Coroutine`, and `GenericCo` carry an `Invariant<'a, Effs>` marker. The runner creates its frame from the same parameters as the body's yielder, so variance must never change them (I3); the `variance_*` compile-fail tests guard this. Never replace the marker with a covariant or contravariant one.
- Frames are only reached through shared references, and slot values are moved out before they are dropped (I4). A frame takes its id at first registration: constructing a frame must stay free of side effects, or LLVM stops eliding moves of large unstarted bodies, which made deep invoke chains superlinear.
- Invocation futures are returned from inherent functions, never through trait projections or associated types. An auto-trait proof of a body that awaits one erases every lifetime, and normalizing a projection would then need the impl's lifetime relations such as `'a: 'b`, which fails (rust-lang/rust#100013). For the same reason `Forward` shortens resumes through a function pointer created where `'a: 'b` holds. Handled invocations still inherit the documented limitation for lifetime-parameterized effects, because their dispatch futures appear in the body's witness.
- `drive_with` pins the generator where its `async move` block captured it. An `async fn` or `pin!` would store the body twice.
- `Direct` stores each pinned child generator once, next to an `Operation` on the parent frame. It moves each child effect into the parent's slot and the resume back with synchronous conversions, so no level creates a future of its own. An async function that stores both the child argument and its pinned generator causes exponential future-size growth.

## Commands

```sh
cargo clippy --all-targets --all-features
cargo fmt
cargo nextest run
cargo bench
```

## Website

After making any user-facing change to the code (new features, API changes, renamed types, new examples, etc.), always check whether the website (in `docs/`) needs to be updated accordingly. Ask if unsure.

Never modify or commit files in `docs/public/` — those are generated by Zola and are gitignored.

## Changelog

Always update the changelog after making a substantial user-facing change. If not sure whether to update it or not, ask me.

Never modify changelog entries for already released versions. Always add new entries under the `## Unreleased` section (create it if it doesn't exist).

## Release process

1. Update the changelog with a new section for the upcoming version, describing the changes since the last release.
2. Bump the version in `Cargo.toml` (and `Cargo.lock` if applicable) to the new version.
3. Update the version number everywhere in the README and docs (e.g. `corophage = "X.Y.Z"` in dependency snippets).
4. Commit the changes with a message like `Release vX.Y.Z`.
5. Create a git tag for the new version: `git tag vX.Y.Z`. Use the changelog entries as the tag message.
6. Push the commit and tag to the remote repository: `git push origin main --tags`.
