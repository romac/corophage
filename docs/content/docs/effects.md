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
