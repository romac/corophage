use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
#[cfg(not(miri))]
fn public_macros_do_not_require_direct_frunk_core_dependency() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp_dir = env::temp_dir().join(format!(
        "corophage-downstream-macro-smoke-{}",
        std::process::id()
    ));

    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).expect("failed to clean previous smoke-test dir");
    }
    fs::create_dir_all(temp_dir.join("src")).expect("failed to create smoke-test crate");

    fs::write(
        temp_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "corophage-downstream-macro-smoke"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
corophage = {{ path = "{}" }}
"#,
            crate_dir.display()
        ),
    )
    .expect("failed to write smoke-test Cargo.toml");

    fs::write(
        temp_dir.join("src/main.rs"),
        r#"use corophage::prelude::*;

#[effect(())]
struct Log;

#[effect(&'r str)]
struct Ask;

type Base = Effects![Ask];
type All = Effects![Log, ...Base];
type Empty = Effects![];

#[effectful(...All)]
fn ask() -> String {
    yield_!(Log);
    yield_!(Ask).to_owned()
}

#[effectful]
fn pure() {}

fn manual() -> Effectful<'static, Empty, ()> {
    Program::new(|_: Yielder<'_, Empty>| async move {})
}

fn main() {
    let _ = ask()
        .handle(|_: Log| Control::resume(()))
        .handle(|_: Ask| Control::resume("ok"))
        .run_sync();
    let _ = pure().run_sync();
    let _ = manual().run_sync();
}
"#,
    )
    .expect("failed to write smoke-test main.rs");

    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .arg("check")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(temp_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", temp_dir.join("target"))
        .output()
        .expect("failed to run cargo check for downstream smoke test");

    let _ = fs::remove_dir_all(&temp_dir);

    assert!(
        output.status.success(),
        "downstream smoke test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
