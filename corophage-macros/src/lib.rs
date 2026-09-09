mod crate_path;
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
/// Transforms the function to return a `Program` with a concrete body future and
/// enables `yield_!()` and `invoke!()` inside the function body. Auto traits such
/// as `Send` remain available without a flag when the body permits them.
/// Borrowed wrapper arguments such as `Cow<'_, str>` are supported, including
/// nested wrappers whose placeholder lifetimes are inferred independently.
/// Elided reference lifetimes are also independent, allowing sequential calls
/// with nested references such as `&mut &str` after each program completes.
///
/// # Arguments
///
/// - Effect types (optional): comma-separated list of effect types
/// - `...Alias` (optional): spread an existing effects type alias (must be last effect argument)
/// - `send` (optional): checks an explicit `Send` guarantee on the body future
/// - `boxed` (optional): returns the erased `Effectful` alias; combine with
///   `send` for sendable erasure. Use this at recursion or storage boundaries.
/// - Explicit lifetime (optional): first argument can select a lifetime declared
///   on the function or enclosing `impl`/`trait`, or use `'static` or `'_`.
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
///
/// #[effectful(Ask, boxed, send)]
/// fn recursive(depth: usize) -> bool {
///     if depth == 0 { yield_!(Ask(0)) } else { invoke!(recursive(depth - 1)) }
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
