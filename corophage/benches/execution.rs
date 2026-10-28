//! Execution-path benchmarks. See benches/README.md for timing boundaries.
#![recursion_limit = "256"]

#[path = "support/execution.rs"]
mod fixtures;

#[path = "execution/dispatch.rs"]
mod dispatch;
#[path = "execution/lifecycle.rs"]
mod lifecycle;
#[path = "execution/scheduling.rs"]
mod scheduling;

use tokio::runtime::Runtime;

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}

fn main() {
    divan::main();
}
