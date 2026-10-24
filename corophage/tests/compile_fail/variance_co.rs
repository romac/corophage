use corophage::coroutine::Co;
use corophage::prelude::*;

#[effect(())]
struct Log<'a>(&'a str);

// An erased coroutine hides its yielder type, so its own invariance is the
// only link between its runner's resume storage and the body's yielder.
fn shorten_effects<'s>(co: Co<'s, Effects![Log<'static>], ()>) -> Co<'s, Effects![Log<'s>], ()> {
    co
}

fn lengthen_effects<'s>(co: Co<'s, Effects![Log<'s>], ()>) -> Co<'s, Effects![Log<'static>], ()> {
    co
}

fn shorten_lifetime<'s>(
    co: Co<'static, Effects![Log<'static>], ()>,
) -> Co<'s, Effects![Log<'static>], ()> {
    co
}

fn lengthen_lifetime<'s>(
    co: Co<'s, Effects![Log<'static>], ()>,
) -> Co<'static, Effects![Log<'static>], ()> {
    co
}

fn main() {}
