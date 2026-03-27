mod effect;
mod effectful;

/// Derive an `Effect` implementation for a struct.
///
/// The attribute argument specifies the resume type. Use `'r` for the GAT
/// lifetime if the resume type needs to borrow from the handler.
///
/// # Examples
///
/// ```ignore
/// #[effect(bool)]
/// pub struct Ask(i32);
///
/// #[effect(&'r str)]
/// pub struct GetConfig;
///
/// #[effect(())]
/// pub struct Log<'a>(pub &'a str);
/// ```
#[proc_macro_attribute]
pub fn effect(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match effect::expand(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Mark a function as an effectful computation.
///
/// Transforms the function to return a `Program` and enables the `yield_!()`
/// macro inside the function body.
///
/// # Arguments
///
/// - Effect types (required): comma-separated list of effect types
/// - `...Alias` (optional): spread an existing effects type alias (must be last effect argument)
/// - `send` (optional): makes the program `Send`-able
/// - Explicit lifetime (optional): first argument can be a lifetime
///
/// # Examples
///
/// ```ignore
/// #[effectful(Ask)]
/// fn my_prog(x: i32) -> bool {
///     yield_!(Ask(x))
/// }
///
/// #[effectful(Ask, Log<'a>)]
/// fn with_lifetime<'a>(msg: &'a str) -> bool {
///     yield_!(Log(msg));
///     yield_!(Ask(42))
/// }
///
/// #[effectful(Ask, send)]
/// fn sendable(x: i32) -> bool {
///     yield_!(Ask(x))
/// }
/// ```
///
/// ## Spreading an effects type alias
///
/// Use `...Alias` to reference a pre-defined effects type, following the same
/// syntax as `Coprod!(...Tail)` from frunk:
///
/// ```ignore
/// type MyEffs = Effects![Ask, Log];
///
/// #[effectful(...MyEffs)]
/// fn using_alias() -> bool {
///     yield_!(Log("hello".into()));
///     yield_!(Ask(42))
/// }
///
/// // Extra inline effects can precede the spread:
/// #[effectful(GetConfig, ...MyEffs)]
/// fn with_extra() -> String {
///     let cfg = yield_!(GetConfig);
///     yield_!(Log(cfg.to_string()));
///     yield_!(Ask(42));
///     cfg.to_owned()
/// }
/// ```
#[proc_macro_attribute]
pub fn effectful(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match effectful::expand(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
