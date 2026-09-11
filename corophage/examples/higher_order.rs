//! An effectful Option map built with the public generic effect-set bounds.
//!
//! The callback constructs an unhandled child only for Some. Its effects are
//! forwarded to the surrounding program, while Program::map formats the result.
//! Run with: `cargo run --example higher_order` (also works without macros).

use std::collections::HashMap;
use std::future::Future;

use corophage::prelude::*;
use corophage::{EmbedEffect, ProjectResume, Resumes};

fn option_map_eff<'a, T: 'a, U: 'a, Es, F, Make, Indices>(
    value: Option<T>,
    make: Make,
) -> Program<'a, Es, impl Future<Output = Option<U>> + 'a>
where
    Es: Effects<'a> + EmbedEffect<Es, Indices>,
    Resumes<'a, Es>: ProjectResume<'a, Es, Indices>,
    F: Future<Output = U> + 'a,
    Make: FnOnce(T) -> Program<'a, Es, F> + 'a,
{
    Program::new(|mut y: Yielder<'a, Es>| async move {
        match value {
            Some(value) => Some(y.invoke(make(value)).await),
            None => None,
        }
    })
}

struct Lookup<'a> {
    entries: &'a HashMap<&'static str, String>,
    key: &'a str,
}

impl Effect for Lookup<'_> {
    type Resume<'r> = &'r str;
    fn shorten_resume<'a: 'b, 'b>(value: &'a str) -> &'b str {
        value
    }
}

fn lookup<'a>(
    entries: &'a HashMap<&'static str, String>,
    key: &'a str,
) -> Program<'a, Effects![Lookup<'a>], impl Future<Output = &'a str> + 'a> {
    Program::new(move |mut y: Yielder<'a, Effects![Lookup<'a>]>| async move {
        y.yield_(Lookup { entries, key }).await
    })
}

fn main() {
    let entries = HashMap::from([("greeting", String::from("hello"))]);
    let mut callback_calls = 0;
    let mut handler_calls = 0;

    for key in [Some("greeting"), None] {
        let result = option_map_eff(key, |key| {
            callback_calls += 1;
            lookup(&entries, key)
        })
        .map(|value| value.map(|text| format!("{text}!")))
        .handle(|Lookup { entries, key }| {
            handler_calls += 1;
            Control::resume(entries[key].as_str())
        })
        .run_sync();
        assert_eq!(result, Ok(key.map(|_| String::from("hello!"))));
        println!("{key:?} -> {result:?}");
    }

    // None skips the callback itself, as well as every effect in its child.
    assert_eq!(callback_calls, 1);
    assert_eq!(handler_calls, 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_lookup_forwards_borrowed_resumes_and_skips_none() {
        main();
    }

    #[test]
    fn generic_callback_preserves_send() {
        let entries = HashMap::from([("greeting", String::from("hello"))]);
        let future = option_map_eff(Some("greeting"), |key| lookup(&entries, key))
            .map(|value| value.map(str::len))
            .handle(|Lookup { entries, key }| Control::resume(entries[key].as_str()))
            .run();
        fn assert_send(_: impl Send) {}
        assert_send(future);
    }
}
