#!/usr/bin/env python3
"""Compare execution revisions in isolated builds without changing the checkout."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import statistics
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
SUITE_FILES = [
    "corophage/benches/execution.rs",
    "corophage/benches/support/execution.rs",
    "corophage/benches/execution/dispatch.rs",
    "corophage/benches/execution/lifecycle.rs",
    "corophage/benches/execution/scheduling.rs",
    "corophage/tests/benchmark_execution.rs",
]
# Keep the parser tied to a known output format, independent of future releases.
DIVAN_VERSION = "0.1.21"


def execute(args, **kwargs):
    completed = subprocess.run(args, text=True, capture_output=True, **kwargs)
    if completed.returncode:
        raise RuntimeError(f"{args}: {completed.returncode}\n{completed.stdout}\n{completed.stderr}")
    return completed


def run(args, **kwargs):
    return execute(args, **kwargs).stdout


def digest_files(files):
    digest = hashlib.sha256()
    for name, content in sorted(files.items()):
        digest.update(name.encode())
        digest.update(b"\0")
        digest.update(content)
    return digest.hexdigest()


def library_sources(repository):
    paths = [repository / "Cargo.toml", repository / "Cargo.lock"]
    for package in ["corophage", "corophage-macros"]:
        paths.append(repository / package / "Cargo.toml")
        paths.extend((repository / package / "src").rglob("*.rs"))
    return {str(path.relative_to(repository)): path.read_bytes() for path in paths}


def probe_source():
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
    return source.replace("// NESTING_FUNCTIONS", "\n\n".join(nesting)).replace(
        "// NESTING_SAMPLES", "\n".join(samples)
    )


def duration_ns(value):
    match = re.fullmatch(r"([0-9]+(?:\.[0-9]+)?) (ps|ns|µs|μs|us|ms|s)", value.strip())
    if not match:
        raise ValueError(f"Unrecognized Divan duration: {value!r}")
    scales = {"ps": 0.001, "ns": 1, "µs": 1000, "μs": 1000, "us": 1000, "ms": 1e6, "s": 1e9}
    return float(match[1]) * scales[match[2]]


def parse_divan(output):
    """Read the pinned Divan tree, rejecting truncated or changed metric rows."""
    stack = []
    rows = []
    for line in output.splitlines():
        match = re.match(r"^([│ ]*)[├╰]─ (\S+)(.*)$", line)
        if not match:
            continue  # Header or empty line; suite entries always have branches.
        if len(match[1]) % 3:
            raise ValueError(f"Malformed Divan tree: {line!r}")
        depth = len(match[1]) // 3
        if depth > len(stack):
            raise ValueError(f"Missing Divan parent: {line!r}")
        stack = stack[:depth] + [match[2]]
        columns = [column.strip() for column in match[3].split("│")]
        if not columns[0]:
            continue
        if len(columns) != 6:
            raise ValueError(f"Incomplete Divan metrics: {line!r}")
        rows.append({
            "name": "::".join(stack),
            "min_ns": duration_ns(columns[0]),
            "max_ns": duration_ns(columns[1]),
            "median_ns": duration_ns(columns[2]),
            "mean_ns": duration_ns(columns[3]),
            "samples": int(columns[4]),
            "iterations": int(columns[5]),
        })
    if not rows or len({row["name"] for row in rows}) != len(rows):
        raise ValueError("Empty or duplicate Divan measurements")
    return rows


def inventory(binary, env):
    output = run([str(binary), "--list", "--format", "terse"], env=dict(env, NEXTEST="1"))
    names = []
    for line in output.splitlines():
        if not line.endswith(": benchmark") or "::" not in line:
            raise ValueError(f"Unexpected Divan inventory: {line!r}")
        names.append(line.removesuffix(": benchmark").split("::", 1)[1])
    if not names or len(names) != len(set(names)):
        raise ValueError("Empty or duplicate Divan inventory")
    return sorted(names)


def unsupported_reason(name):
    if name.startswith("dispatch::catching::nested_forwarding::"):
        return "v0.7.0 catch is a terminal runner wrapper and cannot be invoked or nested this way."
    if name.startswith((
        "dispatch::invocation::partially_handled::",
        "lifecycle::cancellation::handled_sync::",
        "lifecycle::cancellation::handled_async::",
        "scheduling::fairness::short_handled_children::",
    )) or name == "scheduling::fairness::peer_first_poll_latency::2":
        return "v0.7.0 cannot invoke a child with handlers attached."
    raise ValueError(f"Unexpected missing baseline case: {name}")


def comparisons(baseline_runs, candidate_runs):
    """Compare medians across alternating process runs; retain each raw run too."""
    def collect(runs):
        values = {}
        expected = None
        for rows in runs:
            timed = [row for row in rows if "median_ns" in row]
            names = {row["name"] for row in timed}
            if len(names) != len(timed) or (expected is not None and names != expected):
                raise ValueError("Duplicate or inconsistent measurements across runs")
            expected = names
            for row in timed:
                values.setdefault(row["name"], []).append(row["median_ns"])
        if any(len(samples) != len(runs) for samples in values.values()):
            raise ValueError("Measurement coverage changed between runs")
        return values

    baseline, candidate = collect(baseline_runs), collect(candidate_runs)
    result = []
    for name in sorted(baseline.keys() & candidate.keys()):
        before, after = statistics.median(baseline[name]), statistics.median(candidate[name])
        result.append({
            "name": name,
            "baseline_median_ns": before,
            "candidate_median_ns": after,
            "candidate_over_baseline": after / before if before else None,
        })
    return result


def prepare_package(package, repository, files, suite, lock):
    package.mkdir(parents=True)
    for name, content in files.items():
        target = package / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
    extra = f'divan = "={DIVAN_VERSION}"\n' if suite else ""
    tokio_features = ["rt", "macros"] if suite else ["rt"]
    targets = '''[[bin]]
name = "execution-suite"
path = "benches/execution.rs"
[[test]]
name = "benchmark_execution"
path = "tests/benchmark_execution.rs"
''' if suite else ""
    manifest = f'''[package]
name = "execution-{'suite' if suite else 'probe'}"
version = "0.0.0"
edition = "2024"
autobenches = false
[workspace]
[features]
default = ["macros"]
macros = []
candidate = []
extras = ["candidate"]
[dependencies]
corophage = {{ path = {json.dumps(str(repository / "corophage"))} }}
tokio = {{ version = "1", default-features = false, features = {json.dumps(tokio_features)} }}
{extra}[profile.release]
lto = true
codegen-units = 1
[lints.rust]
unexpected_cfgs = {{ level = "warn", check-cfg = ['cfg(comparison_v070)'] }}
{targets}'''
    (package / "Cargo.toml").write_text(manifest)
    # Start from one dependency resolution; Cargo adds fauxgen for v0.7.0.
    (package / "Cargo.lock").write_bytes(lock)



def check_shared_dependencies(baseline_packages, candidate_packages):
    """A library-only dependency may differ; shared registry versions may not."""
    def versions(packages):
        result = {}
        for package in packages:
            if package.get("source"):
                key = (package["name"], package["source"])
                result.setdefault(key, set()).add(package["version"])
        return result

    before, after = versions(baseline_packages), versions(candidate_packages)
    mismatches = [name for name in before.keys() & after.keys() if before[name] != after[name]]
    if mismatches:
        raise ValueError(f"Shared dependency versions differ: {mismatches}")


def build(package, env, features=()):
    target = package / "target"
    command = ["cargo", "build", "--offline", "--release", "--target-dir", str(target)]
    if features:
        command += ["--features", ",".join(features)]
    start = time.monotonic()
    completed = execute(command, cwd=package, env=env)
    seconds = time.monotonic() - start
    binary = target / "release" / package.name
    return binary, {
        "clean_build_seconds": seconds,
        "binary_bytes": binary.stat().st_size,
        "size_output": run(["size", str(binary)]).strip(),
        "resolved_lock_sha256": hashlib.sha256((package / "Cargo.lock").read_bytes()).hexdigest(),
        "build_stderr": completed.stderr,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", default="v0.7.0")
    parser.add_argument("--output", type=Path, default=ROOT / "corophage/benches/results/execution.json")
    parser.add_argument("--runs", type=int, default=2, help="Process runs per revision, alternating order (default: 2)")
    parser.add_argument("--sample-count", type=int, default=100, help="Divan samples per case (default: 100)")
    args = parser.parse_args()
    if args.runs < 2 or args.sample_count < 1:
        parser.error("--runs must be at least 2 and --sample-count must be positive")
    source = probe_source()
    suite_files = {str(Path(name).relative_to("corophage")): (ROOT / name).read_bytes() for name in SUITE_FILES}
    library_snapshot = library_sources(ROOT)
    library_hash = digest_files(library_snapshot)
    # Release builds default to nonincremental. Leave incremental configuration
    # alone; disable the compiler wrapper so these fresh targets are real builds.
    env = dict(os.environ, RUSTC_WRAPPER="")
    baseline = run(["git", "rev-parse", f"{args.baseline}^{{commit}}"], cwd=ROOT).strip()
    result = {
        "schema_version": 2,
        "baseline": baseline,
        "candidate_head": run(["git", "rev-parse", "HEAD"], cwd=ROOT).strip(),
        "candidate_diff_sha256": hashlib.sha256(run(["git", "diff", "HEAD", "--binary"], cwd=ROOT).encode()).hexdigest(),
        "candidate_sources_sha256": library_hash,
        "fixture_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "suite_sha256": digest_files(suite_files),
        "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "rustc": run(["rustc", "-Vv"]).strip(),
        "machine": platform.platform(),
        "cpu_architecture": platform.processor(),
        "host_load_start": os.getloadavg(),
        "profile": "release, opt-level=3, lto=true, codegen-units=1, fresh targets, no rustc wrapper",
        "compiler_environment": {key: env[key] for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET"] if key in env},
        "method": "Separate clean core and Divan suite builds per revision. Repeated process runs alternate baseline/candidate order. Core: seven samples after warmup, allocations counted separately. Suite: pinned Divan, automatic sample sizes except one-operation latency/drop samples; timing only. Depth counts invoke edges.",
        "run_order": [],
        "revisions": {},
        "execution_suite": {"divan_version": DIVAN_VERSION, "sample_count": args.sample_count, "revisions": {}},
    }
    with tempfile.TemporaryDirectory(prefix="corophage-execution-") as temp:
        directory = Path(temp)
        archive = directory / "baseline.tar"
        execute(["git", "archive", "--format=tar", f"--output={archive}", baseline], cwd=ROOT)
        baseline_root = directory / "baseline"
        baseline_root.mkdir()
        execute(["tar", "-xf", str(archive), "-C", str(baseline_root)])
        legacy = "pub struct Catch<P, E>" in (baseline_root / "corophage/src/catch.rs").read_text()
        result["baseline_sources_sha256"] = digest_files(library_sources(baseline_root))
        result["execution_suite"]["baseline_compatibility"] = "v0.7.0" if legacy else "current"
        packages, binaries, environments = {}, {}, {}
        for label, repository in [("baseline", baseline_root), ("candidate", ROOT)]:
            suite_env = env.copy()
            if label == "baseline" and legacy:
                flags = env.get("CARGO_ENCODED_RUSTFLAGS")
                flags = flags.split("\x1f") if flags else shlex.split(env.get("RUSTFLAGS", ""))
                suite_env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([*flags, "--cfg", "comparison_v070"])
            environments[label] = suite_env
            packages[label] = {}
            binaries[label] = {}
            for kind, suite in [("core", False), ("suite", True)]:
                package = directory / label / ("execution-suite" if suite else "execution-probe")
                packages[label][kind] = package
                files = suite_files if suite else {"src/main.rs": source.encode()}
                prepare_package(package, repository, files, suite, library_snapshot["Cargo.lock"])
                print(f"Building {label} {kind}...", flush=True)
                binary, metadata = build(package, suite_env if suite else env, ["candidate"] if label == "candidate" and not suite else [])
                binaries[label][kind] = binary
                destination = result["execution_suite"]["revisions"] if suite else result["revisions"]
                destination[label] = dict(metadata, runs=[])
                if suite:
                    metadata = destination[label]
                    metadata["cfg"] = ["comparison_v070"] if label == "baseline" and legacy else []
                    metadata["inventory"] = inventory(binary, suite_env)
                    print(f"Checking {label} suite fixtures...", flush=True)
                    checks = execute([
                        "cargo", "nextest", "run", "--offline", "--release", "--target-dir", str(package / "target"),
                        "--test", "benchmark_execution",
                    ], cwd=package, env=suite_env)
                    metadata["fixture_checks"] = checks.stdout + checks.stderr
                    execute([str(binary), "--test"], env=suite_env)
        for kind in ["core", "suite"]:
            dependency_sets = [json.loads(run([
                "cargo", "metadata", "--offline", "--format-version", "1",
                "--manifest-path", str(packages[label][kind] / "Cargo.toml"),
            ], env=env))["packages"] for label in ["baseline", "candidate"]]
            check_shared_dependencies(*dependency_sets)
        suite_results = result["execution_suite"]["revisions"]
        before, after = (set(suite_results[label]["inventory"]) for label in ["baseline", "candidate"])
        if before - after:
            raise ValueError(f"Unexpected baseline-only cases: {before - after}")
        missing = after - before
        if missing and not legacy:
            raise ValueError(f"Unexpected missing cases on a current-API baseline: {missing}")
        result["execution_suite"]["unsupported"] = [
            {"name": name, "revision": "baseline", "reason": unsupported_reason(name)} for name in sorted(missing)
        ]
        for iteration in range(args.runs):
            order = ["baseline", "candidate"] if iteration % 2 == 0 else ["candidate", "baseline"]
            result["run_order"].append(order)
            for label in order:
                print(f"Measuring {label}, run {iteration + 1}/{args.runs}...", flush=True)
                rows = [json.loads(line) for line in run([str(binaries[label]["core"])], env=env).splitlines()]
                result["revisions"][label]["runs"].append(rows)
                timed = execute([
                    str(binaries[label]["suite"]), "--bench", "--sample-count", str(args.sample_count), "--color", "never",
                ], env=environments[label])
                rows = parse_divan(timed.stdout)
                if {row["name"] for row in rows} != set(suite_results[label]["inventory"]):
                    raise ValueError(f"Incomplete {label} Divan results")
                suite_results[label]["runs"].append({"measurements": rows, "stdout": timed.stdout, "stderr": timed.stderr})
        # Preserve the original result keys for consumers of the core probe.
        for revision in result["revisions"].values():
            revision["measurements"] = revision["runs"][0]
        result["comparisons"] = comparisons(*(result["revisions"][label]["runs"] for label in ["baseline", "candidate"]))
        result["execution_suite"]["comparisons"] = comparisons(*(
            [sample["measurements"] for sample in suite_results[label]["runs"]] for label in ["baseline", "candidate"]
        ))
        print("Measuring original candidate extras...", flush=True)
        binary, _ = build(packages["candidate"]["core"], env, ["extras"])
        result["candidate_extras"] = [
            row for row in (json.loads(line) for line in run([str(binary)], env=env).splitlines())
            if row["name"].startswith(("boxed_", "local_lending_"))
        ]
    if (
        digest_files(library_sources(ROOT)) != library_hash
        or probe_source() != source
        or hashlib.sha256(Path(__file__).read_bytes()).hexdigest() != result["harness_sha256"]
        or any((ROOT / "corophage" / name).read_bytes() != content for name, content in suite_files.items())
    ):
        raise RuntimeError("Sources changed during measurement; refusing to write mixed-revision results")
    result["host_load_end"] = os.getloadavg()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"Wrote {args.output}: {len(after)} suite cases, {len(before)} comparisons, {len(missing)} baseline-unsupported", flush=True)


if __name__ == "__main__":
    main()
