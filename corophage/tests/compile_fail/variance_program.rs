use corophage::prelude::*;

#[effect(())]
struct Log<'a>(&'a str);

// A program must be invariant in its lifetime and effect set, so its runner
// stores resumes of the same type as the yielder its body receives.
fn shorten_effects<'s>(
    p: Effectful<'s, Effects![Log<'static>], ()>,
) -> Effectful<'s, Effects![Log<'s>], ()> {
    p
}

fn lengthen_effects<'s>(
    p: Effectful<'s, Effects![Log<'s>], ()>,
) -> Effectful<'s, Effects![Log<'static>], ()> {
    p
}

fn shorten_lifetime<'s>(
    p: Effectful<'static, Effects![Log<'static>], ()>,
) -> Effectful<'s, Effects![Log<'static>], ()> {
    p
}

fn lengthen_lifetime<'s>(
    p: Effectful<'s, Effects![Log<'static>], ()>,
) -> Effectful<'static, Effects![Log<'static>], ()> {
    p
}

fn main() {}
