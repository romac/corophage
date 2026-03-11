#[cfg(not(miri))]
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}

use corophage::prelude::*;

// --- #[effect] tests ---

#[effect(bool)]
struct Ask(i32);

#[effect(())]
pub struct Log<'a>(pub &'a str);

#[effect(&'r str)]
struct GetConfig;

#[effect(T)]
#[allow(dead_code)]
struct Identity<T: std::fmt::Debug + Send + Sync>(T);

#[effect(())]
#[allow(dead_code)]
struct Unit;

#[effect(Vec<u8>)]
#[allow(dead_code)]
struct NamedFields {
    path: String,
    recursive: bool,
}

// --- #[effectful] tests ---

#[effectful(Ask)]
fn simple_ask(x: i32) -> bool {
    yield_!(Ask(x))
}

#[test]
fn test_simple_effectful() {
    let result = simple_ask(42)
        .handle(|Ask(n)| Control::resume(n > 10))
        .run_sync();

    assert_eq!(result, Ok(true));
}

#[effectful(Ask)]
fn effectful_with_control_flow(x: i32) -> &'static str {
    if yield_!(Ask(x)) {
        return "yes";
    }
    "no"
}

#[test]
fn test_effectful_control_flow() {
    let result = effectful_with_control_flow(42)
        .handle(|_: Ask| Control::resume(true))
        .run_sync();

    assert_eq!(result, Ok("yes"));

    let result = effectful_with_control_flow(42)
        .handle(|_: Ask| Control::resume(false))
        .run_sync();

    assert_eq!(result, Ok("no"));
}

#[effectful(Ask, GetConfig)]
fn multi_effect() -> String {
    let config = yield_!(GetConfig);
    let answer = yield_!(Ask(42));
    format!("{config}: {answer}")
}

#[test]
fn test_multi_effect() {
    let result = multi_effect()
        .handle(|_: Ask| Control::resume(true))
        .handle(|_: GetConfig| Control::resume("cfg"))
        .run_sync();

    assert_eq!(result, Ok("cfg: true".to_string()));
}

#[effectful(Log<'a>)]
fn with_lifetime<'a>(msg: &'a str) -> () {
    yield_!(Log(msg));
}

#[test]
fn test_with_lifetime() {
    let msg = String::from("hello");
    let result = with_lifetime(&msg)
        .handle(|Log(s)| {
            println!("{s}");
            Control::resume(())
        })
        .run_sync();

    assert_eq!(result, Ok(()));
}

#[effectful(Ask)]
fn no_yields() -> i32 {
    42
}

#[test]
fn test_no_yields() {
    let result = no_yields()
        .handle(|_: Ask| Control::resume(false))
        .run_sync();

    assert_eq!(result, Ok(42));
}

#[effectful(Ask)]
fn yield_in_let() -> bool {
    let x = yield_!(Ask(1));
    let y = yield_!(Ask(2));
    x && y
}

#[test]
fn test_yield_in_let() {
    let result = yield_in_let()
        .handle(|Ask(n)| Control::resume(n > 0))
        .run_sync();

    assert_eq!(result, Ok(true));
}

#[effectful(Ask)]
fn yield_in_match(x: i32) -> &'static str {
    match yield_!(Ask(x)) {
        true => "yes",
        false => "no",
    }
}

#[test]
fn test_yield_in_match() {
    let result = yield_in_match(5)
        .handle(|Ask(n)| Control::resume(n == 5))
        .run_sync();

    assert_eq!(result, Ok("yes"));
}

#[effectful(Ask, send)]
fn sendable_prog(x: i32) -> bool {
    yield_!(Ask(x))
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn test_send_effectful() {
    let result = sendable_prog(42)
        .handle(async |Ask(n)| Control::resume(n > 10))
        .run()
        .await;

    assert_eq!(result, Ok(true));
}

// Test GAT resume type with #[effect]
#[effectful(GetConfig)]
fn use_gat_resume() -> String {
    let config = yield_!(GetConfig);
    config.to_owned()
}

#[test]
fn test_gat_resume() {
    let result = use_gat_resume()
        .handle(|_: GetConfig| Control::resume("hello"))
        .run_sync();

    assert_eq!(result, Ok("hello".to_string()));
}

// Test async execution
#[effectful(Ask)]
fn async_compatible(x: i32) -> bool {
    yield_!(Ask(x))
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn test_async_effectful() {
    let result = async_compatible(42)
        .handle(async |Ask(n)| Control::resume(n > 10))
        .run()
        .await;

    assert_eq!(result, Ok(true));
}
