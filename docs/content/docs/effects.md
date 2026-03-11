+++
title = "Effects"
weight = 2
description = "Define effects — the building blocks of your effectful programs."
+++

An **Effect** is a struct that represents a request for a side effect. It's a message from your computation to the outside world.

## The `Effect` trait

To define an effect, implement the `Effect` trait. The most important part is the associated type `Resume<'r>` (a generic associated type), which defines the type of value the computation receives back after the effect is handled.

```rust
use corophage::{Effect, Never};

// An effect to request logging a message.
// It doesn't need any data back, so we resume with `()`.
pub struct Log<'a>(pub &'a str);
impl<'a> Effect for Log<'a> {
    type Resume<'r> = ();
}

// An effect to request reading a file.
// It expects the file's contents back, so we resume with `String`.
pub struct FileRead(pub String);
impl Effect for FileRead {
    type Resume<'r> = String;
}

// An effect that cancels the computation.
// It will never resume, so we use the special `Never` type.
pub struct Cancel;
impl Effect for Cancel {
    type Resume<'r> = Never;
}
```

## The `#[effect]` attribute macro

The easiest way to define effects is with the `#[effect(ResumeType)]` attribute macro:

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

## The `declare_effect!` macro

Alternatively, you can use the `declare_effect!` macro for a more concise syntax:

```rust
use corophage::prelude::*;

declare_effect!(Log(String) -> ());
declare_effect!(FileRead(String) -> String);
declare_effect!(Cancel -> Never);
```

The macro supports lifetimes, generics, named fields, and borrowed resume types:

```rust
// Lifetime parameters
declare_effect!(Borrow<'a>(&'a str) -> bool);

// Generic parameters
declare_effect!(Generic<T: std::fmt::Debug>(T) -> T);

// Named fields
declare_effect!(FileRead { path: String, recursive: bool } -> Vec<u8>);

// The resume type may reference the GAT lifetime 'r
declare_effect!(Lookup(String) -> &'r str);
```

## Effect sets with `Effects!`

Effects are grouped into sets using the `Effects!` macro:

```rust
type MyEffects = Effects![Log, FileRead, Cancel];
```

This creates a type-level list (coproduct) of effects. The type system tracks which effects have been handled and prevents you from running a program until all effects have handlers.
