+++
title = "Effects"
weight = 2
description = "Define effects — the building blocks of your effectful programs."
+++

An **Effect** is a struct that represents a request for a side effect. It's a message from your computation to the outside world.

## Defining effects with `#[effect]`

To define an effect, annotate a struct with `#[effect(ResumeType)]`. The resume type defines what value the computation receives back after the effect is handled.

```rust
use corophage::prelude::*;

#[effect(())]
pub struct Log(pub String);

#[effect(String)]
pub struct FileRead(pub String);

#[effect(Never)]
pub struct Cancel;
```

The macro supports lifetimes, generics, named fields, and borrowed resume types:

```rust
// Lifetime parameters
#[effect(bool)]
pub struct Borrow<'a>(pub &'a str);

// Generic parameters
#[effect(T)]
pub struct Generic<T: std::fmt::Debug + Send + Sync>(pub T);

// Named fields
#[effect(Vec<u8>)]
pub struct ReadDir { pub path: String, pub recursive: bool }

// The resume type may reference the GAT lifetime 'r
#[effect(&'r str)]
pub struct Lookup(pub String);
```

## Manual `Effect` implementation

If you need to implement the `Effect` trait by hand (without the macro), you must provide both the `Resume` associated type and the `shorten_resume` method:

```rust
struct Ask(pub String);

impl Effect for Ask {
    type Resume<'r> = &'r str;

    fn shorten_resume<'a: 'b, 'b>(resume: &'a str) -> &'b str {
        resume
    }
}
```

The `shorten_resume` method witnesses that `Resume<'r>` is **covariant** in `'r` -- that is, a resume value with a longer lifetime can be safely used where a shorter lifetime is expected. This is required by [`invoke`](@/docs/programs.md#composing-programs-with-invoke) to allow sub-programs to borrow from shorter-lived data than the outer program.

The body is always just `resume`. The `#[effect]` macro generates this automatically, so you only need to write it when implementing `Effect` by hand.

## Effect sets with `Effects!`

Effects are grouped into sets using the `Effects!` macro:

```rust
type MyEffects = Effects![Log, FileRead, Cancel];
```

This creates a type-level list (coproduct) of effects. The type system tracks which effects have been handled and prevents you from running a program until all effects have handlers.

You can also compose effect sets using the `...Alias` spread syntax to splice in an existing type alias:

```rust
type IoEffects = Effects![Log, FileRead];
type AllEffects = Effects![Cancel, ...IoEffects];
// Equivalent to: Effects![Cancel, Log, FileRead]
```

The spread must appear as the last argument. This follows the same convention as frunk's `Coprod!(...Tail)` syntax.
