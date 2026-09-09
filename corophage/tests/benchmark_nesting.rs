#![cfg(feature = "macros")]
#![recursion_limit = "256"]

use std::cell::Cell;

use corophage::prelude::*;

#[path = "../benches/support/nesting.rs"]
mod nesting;
use nesting::*;

#[derive(Default)]
struct Depth {
    active: Cell<usize>,
    peak: Cell<usize>,
}

impl Depth {
    fn enter(&self) -> Level<'_> {
        let active = self.active.get() + 1;
        self.active.set(active);
        self.peak.set(self.peak.get().max(active));
        Level(self)
    }
}

struct Level<'a>(&'a Depth);

impl Drop for Level<'_> {
    fn drop(&mut self) {
        self.0.active.set(self.0.active.get() - 1);
    }
}

macro_rules! assert_fixture {
    ($fixture:ident, $depth:expr, $effects:expr, $result:expr) => {{
        let depth = Depth::default();
        let mut effects = 0;
        let result = $fixture(|| depth.enter())
            .handle(|_: Noop| {
                effects += 1;
                Control::resume(())
            })
            .run_sync();

        assert_eq!(result, Ok($result), stringify!($fixture));
        // Sequential children increase the total work, but cannot increase this
        // peak: each guard stays alive until its program body completes.
        assert_eq!(depth.peak.get(), $depth, stringify!($fixture));
        assert_eq!(effects, $effects, stringify!($fixture));
        assert_eq!(depth.active.get(), 0, stringify!($fixture));
    }};
}

#[test]
fn leaf_only_fixtures_reach_their_labelled_depth() {
    assert_fixture!(nest_leaf, 1, 1, ());
    assert_fixture!(nest_2, 2, 1, ());
    assert_fixture!(nest_3, 3, 1, ());
    assert_fixture!(nest_5, 5, 1, ());
    assert_fixture!(nest_10, 10, 1, ());
    assert_fixture!(nest_15, 15, 1, ());
    assert_fixture!(nest_20, 20, 1, ());
}

#[test]
fn yielding_fixtures_reach_their_labelled_depth_and_yield_once_per_level() {
    assert_fixture!(nest_yield_leaf, 1, 1, 1);
    assert_fixture!(nest_yield_2, 2, 2, 2);
    assert_fixture!(nest_yield_3, 3, 3, 3);
    assert_fixture!(nest_yield_5, 5, 5, 5);
    assert_fixture!(nest_yield_10, 10, 10, 10);
}
