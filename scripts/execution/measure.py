#!/usr/bin/env python3
"""Compare execution revisions in isolated builds without changing the checkout."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]


def run(args, **kwargs):
    completed = subprocess.run(args, text=True, capture_output=True, **kwargs)
    if completed.returncode:
        raise RuntimeError(f"{args}: {completed.returncode}\n{completed.stdout}\n{completed.stderr}")
    return completed.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", default="fa68b5a")
    parser.add_argument("--output", type=Path, default=ROOT / "corophage/benches/results/execution.json")
    args = parser.parse_args()
    source = Path(__file__).with_name("probe.rs").read_text()
    nesting = ["#[effectful(Tick)]\nfn nested_0() -> u64 { yield_!(Tick(black_box(1))) }"]
    for depth in range(1, 21):
        nesting.append(f"#[effectful(Tick)]\nfn nested_{depth}() -> u64 {{ invoke!(nested_{depth - 1}()) }}")
    samples = []
    for depth in [1, 5, 10, 20]:
        samples.append(f'''
    size("nested_{depth}_future_bytes", &nested_{depth}().handle(async_tick).run());
    sample("nested_{depth}", || {{
        assert_eq!(black_box(nested_{depth}().handle(tick).run_sync()), Ok(1));
    }});''')
    source = source.replace("// NESTING_FUNCTIONS", "\n\n".join(nesting))
    source = source.replace("// NESTING_SAMPLES", "\n".join(samples))
    env = dict(os.environ, RUSTC_WRAPPER="", CARGO_INCREMENTAL="0")
    baseline = run(["git", "rev-parse", args.baseline], cwd=ROOT).strip()
    sources = hashlib.sha256()
    for package_name in ["corophage", "corophage-macros"]:
        for path in sorted((ROOT / package_name / "src").rglob("*.rs")):
            sources.update(str(path.relative_to(ROOT)).encode())
            sources.update(path.read_bytes())
    result = {
        "baseline": baseline,
        "candidate_head": run(["git", "rev-parse", "HEAD"], cwd=ROOT).strip(),
        "candidate_diff_sha256": hashlib.sha256(run(["git", "diff"], cwd=ROOT).encode()).hexdigest(),
        "candidate_sources_sha256": sources.hexdigest(),
        "fixture_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "rustc": run(["rustc", "-Vv"]).strip(),
        "machine": platform.platform(),
        "cpu_architecture": platform.processor(),
        "profile": "release, opt-level=3, lto=true, codegen-units=1, fresh targets, no rustc wrapper",
        "method": "One clean offline build per revision including dependencies; seven timing samples after warmup, median nanoseconds per whole operation; allocations counted separately. Nesting depth counts invoke edges, plus one leaf program. Runtimes run sequentially.",
        "revisions": {},
    }
    with tempfile.TemporaryDirectory(prefix="corophage-execution-") as temp:
        directory = Path(temp)
        archive = directory / "baseline.tar"
        subprocess.run(["git", "archive", "--format=tar", f"--output={archive}", baseline], check=True, cwd=ROOT)
        baseline_root = directory / "baseline"
        baseline_root.mkdir()
        subprocess.run(["tar", "-xf", str(archive), "-C", str(baseline_root)], check=True)
        for label, repository in [("baseline", baseline_root), ("candidate", ROOT)]:
            package = directory / label / "probe"
            package.mkdir(parents=True)
            (package / "src").mkdir()
            (package / "src/main.rs").write_text(source)
            manifest = f'''[package]
name = "execution-probe"
version = "0.0.0"
edition = "2024"
[workspace]
[features]
candidate = []
extras = ["candidate"]
[dependencies]
corophage = {{ path = {json.dumps(str(repository / "corophage"))} }}
tokio = {{ version = "1", default-features = false, features = ["rt"] }}
[profile.release]
lto = true
codegen-units = 1
'''
            (package / "Cargo.toml").write_text(manifest)
            # Copy the same resolved versions into both standalone builds. Cargo
            # only adds the probe package and prunes unused dev dependencies.
            (package / "Cargo.lock").write_text((ROOT / "Cargo.lock").read_text())
            command = ["cargo", "build", "--offline", "--release"]
            if label == "candidate":
                command += ["--features", "candidate"]
            print(f"Building {label}...", flush=True)
            start = time.monotonic()
            completed = subprocess.run(command, cwd=package, env=env, text=True, capture_output=True)
            seconds = time.monotonic() - start
            if completed.returncode:
                raise RuntimeError(completed.stderr)
            binary = package / "target/release/execution-probe"
            measurements = [json.loads(line) for line in run([str(binary)]).splitlines()]
            result["revisions"][label] = {
                "clean_build_seconds": seconds,
                "binary_bytes": binary.stat().st_size,
                "size_output": run(["size", str(binary)]).strip(),
                "measurements": measurements,
            }
            print(f"{label}: {seconds:.2f}s build, {binary.stat().st_size} bytes", flush=True)
            if label == "candidate":
                run(["cargo", "build", "--offline", "--release", "--features", "extras"], cwd=package, env=env)
                result["candidate_extras"] = [json.loads(line) for line in run([str(binary)]).splitlines() if any(name in line for name in ["boxed_", "local_lending_"])]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"Wrote {args.output}")


if __name__ == "__main__":
    main()
