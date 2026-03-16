/// Fallback `yield_!` macro that produces a clear error when used outside
/// an `#[effectful]` function.
///
/// Inside an `#[effectful]` function, this macro is shadowed by a local
/// `macro_rules!` definition that expands to the correct yielder call.
#[macro_export]
macro_rules! yield_ {
    ($($tt:tt)*) => {
        ::core::compile_error!("yield_!() can only be used inside an #[effectful] function")
    };
}

/// Fallback `invoke!` macro that produces a clear error when used outside
/// an `#[effectful]` function.
///
/// Inside an `#[effectful]` function, this macro is shadowed by a local
/// `macro_rules!` definition that expands to the correct yielder call.
#[macro_export]
macro_rules! invoke {
    ($($tt:tt)*) => {
        ::core::compile_error!("invoke!() can only be used inside an #[effectful] function")
    };
}

/// Constructs a coproduct type from a list of [`Effect`](crate::Effect) types.
///
/// `Effects![A, B, C]` expands to `Coprod!(A, B, C)` from `frunk_core`.
///
/// # Example
///
/// ```ignore
/// type MyEffects = Effects![Log, Ask];
/// ```
#[macro_export]
macro_rules! Effects {
    [$($effect:ty),*] => {
        ::frunk_core::Coprod!($($effect),*)
    };
}
