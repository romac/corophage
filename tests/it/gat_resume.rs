use std::collections::HashMap;

use corophage::prelude::*;
use corophage::{Co, run, sync};

/// An effect whose resume type borrows data via the GAT lifetime.
/// The handler provides a `&'r str` instead of an owned `String`,
/// demonstrating that `Resume<'r>` can carry borrowed data.
struct GetConfig;

impl Effect for GetConfig {
    type Resume<'r> = &'r str;
}

struct Log<'a>(pub &'a str);

impl<'a> Effect for Log<'a> {
    type Resume<'r> = ();
}

/// The handler resumes with a `&str` that borrows from a local `String`,
/// proving `Resume<'r>` works with non-`'static` borrows.
///
/// The `Log` effect borrows `log_msg`, making the `Co` lifetime non-`'static`.
/// The `GetConfig` handler resumes with `&config_data`, a borrow of another
/// local `String`. Both lifetimes unify under the Co's `'a`.
#[test]
fn sync_gat_resume_borrows_local_data() {
    type Effs<'a> = Effects![GetConfig, Log<'a>];

    let config_data = String::from("local-config");
    let log_msg = String::from("fetched config");

    let config_ref = config_data.as_str();
    let log_ref = log_msg.as_str();

    let co: Co<'_, Effs<'_>, String> = Co::new(move |y| async move {
        let config: &str = y.yield_(GetConfig).await;
        y.yield_(Log(log_ref)).await;
        config.to_owned()
    });

    let mut logged: Vec<String> = vec![];
    let result = sync::run(
        co,
        &mut hlist![|_: GetConfig| CoControl::resume(config_ref), |Log(msg)| {
            logged.push(msg.to_string());
            CoControl::resume(())
        },],
    );

    assert_eq!(result, Ok("local-config".to_string()));
    assert_eq!(logged, vec!["fetched config"]);
}

/// Stateful handler that resumes with a `&str` borrowing from handler state.
#[test]
fn sync_gat_resume_with_stateful_handler() {
    type Effs<'a> = Effects![GetConfig, Log<'a>];

    let config_data = String::from("stateful-config");
    let log_msg = String::from("log");

    let config_ref = config_data.as_str();
    let log_ref = log_msg.as_str();

    let co: Co<'_, Effs<'_>, String> = Co::new(move |y| async move {
        let c1: &str = y.yield_(GetConfig).await;
        y.yield_(Log(log_ref)).await;
        let c2: &str = y.yield_(GetConfig).await;
        format!("{c1}+{c2}")
    });

    let mut call_count: u32 = 0;
    let result = sync::run_stateful(
        co,
        &mut call_count,
        &mut hlist![
            |s: &mut u32, _: GetConfig| {
                *s += 1;
                // Resume with a non-'static &str borrowed from local data
                CoControl::resume(config_ref)
            },
            |_s: &mut u32, Log(_)| CoControl::resume(()),
        ],
    );

    assert_eq!(result, Ok("stateful-config+stateful-config".to_string()));
    assert_eq!(call_count, 2);
}

/// Async handler that resumes with a `&str` borrowing from local data.
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_gat_resume_borrows_local_data() {
    type Effs<'a> = Effects![GetConfig, Log<'a>];

    let config_data = String::from("async-local-config");
    let log_msg = String::from("async log");

    let config_ref = config_data.as_str();
    let log_ref = log_msg.as_str();

    let co: Co<'_, Effs<'_>, String> = Co::new(move |y| async move {
        let config: &str = y.yield_(GetConfig).await;
        y.yield_(Log(log_ref)).await;
        config.to_owned()
    });

    let mut logged: Vec<String> = vec![];
    let result = run(
        co,
        &mut hlist![
            async |_: GetConfig| CoControl::resume(config_ref),
            async |Log(msg)| {
                logged.push(msg.to_string());
                CoControl::resume(())
            },
        ],
    )
    .await;

    assert_eq!(result, Ok("async-local-config".to_string()));
    assert_eq!(logged, vec!["async log"]);
}

/// An effect that carries a reference to a map and a key.
/// The handler looks up the key in the map and resumes with a `&str`
/// borrowing directly from the map's values — the same data the effect
/// points to.
struct Lookup<'a> {
    map: &'a HashMap<String, String>,
    key: &'a str,
}

impl<'a> Effect for Lookup<'a> {
    type Resume<'r> = &'r str;
}

/// The resumption value borrows from data carried by the effect itself.
///
/// `Lookup` holds `&'a HashMap<…>` and `&'a str`. The handler
/// destructures the effect, calls `map.get(key)`, and resumes with the
/// resulting `&'a str` — a borrow into the same map the effect
/// references. Without the GAT on `Resume`, the handler would have to
/// clone the value into an owned `String`.
#[test]
fn sync_gat_resume_borrows_from_effect() {
    type Effs<'a> = Effects![Lookup<'a>];

    let map = HashMap::from([
        ("db_host".to_string(), "localhost".to_string()),
        ("db_port".to_string(), "5432".to_string()),
    ]);

    let co: Co<'_, Effs<'_>, String> = Co::new({
        let map = &map;
        move |y| async move {
            let host: &str = y
                .yield_(Lookup {
                    map,
                    key: "db_host",
                })
                .await;
            let port: &str = y
                .yield_(Lookup {
                    map,
                    key: "db_port",
                })
                .await;
            format!("{host}:{port}")
        }
    });

    let result = sync::run(
        co,
        &mut hlist![|Lookup { map, key }| {
            let value = map.get(key).unwrap();
            CoControl::resume(value.as_str())
        }],
    );

    assert_eq!(result, Ok("localhost:5432".to_string()));
}

/// Same pattern with `run_stateful` and stateful tracking.
#[test]
fn sync_gat_resume_borrows_from_effect_with_state() {
    type Effs<'a> = Effects![Lookup<'a>];

    let map = HashMap::from([
        ("url".to_string(), "https://example.com".to_string()),
        ("token".to_string(), "abc123".to_string()),
    ]);

    let co: Co<'_, Effs<'_>, String> = Co::new({
        let map = &map;
        move |y| async move {
            let url: &str = y.yield_(Lookup { map, key: "url" }).await;
            let token: &str = y.yield_(Lookup { map, key: "token" }).await;
            format!("{url}?token={token}")
        }
    });

    let mut lookups: Vec<String> = vec![];
    let result = sync::run_stateful(
        co,
        &mut lookups,
        &mut hlist![|s: &mut Vec<String>, Lookup { map, key }| {
            s.push(key.to_string());
            let value = map.get(key).unwrap();
            CoControl::resume(value.as_str())
        }],
    );

    assert_eq!(result, Ok("https://example.com?token=abc123".to_string()));
    assert_eq!(lookups, vec!["url", "token"]);
}
