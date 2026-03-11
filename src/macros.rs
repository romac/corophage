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

/// Declare an effect type.
///
/// # Supported forms
///
/// **No fields** — creates a unit struct:
/// ```ignore
/// declare_effect!(MyEffect -> ());
/// ```
///
/// **Tuple fields**:
/// ```ignore
/// declare_effect!(MyEffect(pub String) -> bool);
/// ```
///
/// **Named fields**:
/// ```ignore
/// declare_effect!(MyEffect { path: String, recursive: bool } -> Vec<u8>);
/// ```
///
/// **Lifetime parameter** — the lifetime is added to the struct and impl:
/// ```ignore
/// declare_effect!(MyEffect<'a>(&'a str) -> bool);
/// ```
///
/// **Generic type parameter** — `Sync + Send` bounds are added automatically
/// on the impl:
/// ```ignore
/// declare_effect!(MyEffect<T: std::fmt::Debug>(T) -> T);
/// ```
///
/// The resume type may reference the GAT lifetime `'r` to return borrowed data:
/// ```ignore
/// declare_effect!(MyEffect(i32) -> &'r str);
/// ```
#[macro_export]
macro_rules! declare_effect {
    // No fields: declare_effect!(Name -> ResumeType)
    ($name:ident -> $resume:ty) => {
        struct $name;

        impl $crate::Effect for $name {
            type Resume<'r> = $resume;
        }
    };

    // With fields: declare_effect!(Name(fields) -> ResumeType)
    ($name:ident($($field:tt)*) -> $resume:ty) => {
        struct $name($($field)*);

        impl $crate::Effect for $name {
            type Resume<'r> = $resume;
        }
    };

    // With named fields: declare_effect!(Name { field: Type } -> ResumeType)
    ($name:ident { $($field:tt)* } -> $resume:ty) => {
        struct $name { $($field)* }

        impl $crate::Effect for $name {
            type Resume<'r> = $resume;
        }
    };

    // With lifetime and tuple fields: declare_effect!(Name<'a>(fields) -> ResumeType)
    ($name:ident<$lt:lifetime>($($field:tt)*) -> $resume:ty) => {
        struct $name<$lt>($($field)*);

        impl<$lt> $crate::Effect for $name<$lt> {
            type Resume<'r> = $resume;
        }
    };

    // With lifetime and named fields: declare_effect!(Name<'a> { field: Type } -> ResumeType)
    ($name:ident<$lt:lifetime> { $($field:tt)* } -> $resume:ty) => {
        struct $name<$lt> { $($field)* }

        impl<$lt> $crate::Effect for $name<$lt> {
            type Resume<'r> = $resume;
        }
    };

    // With generic and tuple fields: declare_effect!(Name<T: Bound>(fields) -> ResumeType)
    ($name:ident<$T:ident : $bound:path>($($field:tt)*) -> $resume:ty) => {
        struct $name<$T: $bound>($($field)*);

        impl<$T: $bound + Sync + Send> $crate::Effect for $name<$T> {
            type Resume<'r> = $resume;
        }
    };

    // With generic and named fields: declare_effect!(Name<T: Bound> { field: Type } -> ResumeType)
    ($name:ident<$T:ident : $bound:path> { $($field:tt)* } -> $resume:ty) => {
        struct $name<$T: $bound> { $($field)* }

        impl<$T: $bound + Sync + Send> $crate::Effect for $name<$T> {
            type Resume<'r> = $resume;
        }
    };
}
