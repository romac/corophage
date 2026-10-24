use corophage::prelude::*;

#[effect(())]
struct Log<'a>(&'a str);

// A yielder must be invariant in its lifetime and effect set. Otherwise a body
// could yield a short borrow to a runner that expects a longer one.
fn shorten_effects<'a, 's>(
    y: Yielder<'a, Effects![Log<'static>]>,
) -> Yielder<'a, Effects![Log<'s>]> {
    y
}

fn lengthen_effects<'a, 's>(
    y: Yielder<'a, Effects![Log<'s>]>,
) -> Yielder<'a, Effects![Log<'static>]> {
    y
}

fn shorten_lifetime<'s>(
    y: Yielder<'static, Effects![Log<'static>]>,
) -> Yielder<'s, Effects![Log<'static>]> {
    y
}

fn lengthen_lifetime<'s>(
    y: Yielder<'s, Effects![Log<'static>]>,
) -> Yielder<'static, Effects![Log<'static>]> {
    y
}

fn main() {}
