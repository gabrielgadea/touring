#!/usr/bin/env python3
"""
validate_phase02.py — Validation script for Phase 2: Graph Analytics + Phase Registry
Plan: touring-v19-improvements

Runs all validation checks for phase 2 and exits 1 if any FAIL.
Usage: python validate_phase02.py [--verbose]
"""
import argparse
import shlex
import subprocess
import sys
from pathlib import Path

WORKSPACE = Path("/home/gabrielgadea/.claude/rust")
PLAN_DIR = Path(__file__).parent
VERBOSE = False

PASS = "\033[92mPASS\033[0m"
FAIL = "\033[91mFAIL\033[0m"
SKIP = "\033[93mSKIP\033[0m"


def log(msg: str) -> None:
    if VERBOSE:
        print(msg)


def check(name: str, fn) -> bool:
    try:
        result = fn()
        status = PASS if result else FAIL
        print(f"  {status}: {name}")
        return bool(result)
    except Exception as exc:
        print(f"  {FAIL}: {name} — {exc}")
        return False


def run_cmd(cmd: str, cwd=None) -> tuple[int, str]:
    """Run a shell command and return (returncode, stdout+stderr)."""
    r = # OWASP A03 (W8 fix): shell=True replaced by shlex.split list-of-args
    subprocess.run(shlex.split(cmd), capture_output=True, text=True, cwd=cwd or WORKSPACE, shell=False)
    return r.returncode, r.stdout + r.stderr


def main() -> int:
    global VERBOSE
    parser = argparse.ArgumentParser(description="Validate Phase 2")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    VERBOSE = args.verbose

    print(f"=== Phase 2 Validation: Graph Analytics + Phase Registry ===")
    results = []

    results.append(check(
        "cargo check --workspace exits 0",
        lambda: run_cmd("cargo check --workspace")[0] == 0
    ))
    # INS-3: Plugin/Phase Registry Dynamic
    results.append(check(
        "INS-3: src/aco/registry.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/registry.rs").exists()
    ))
    results.append(check(
        "INS-3: MutableGeneratorGraph struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct MutableGeneratorGraph\\|pub enum MutableGeneratorGraph' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-3: test test_registry_register_and_execute exists",
        lambda tc="test_registry_register_and_execute": tc in run_cmd(
            "grep -r 'test_registry_register_and_execute' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-3: test test_registry_duplicate_registration_fails exists",
        lambda tc="test_registry_duplicate_registration_fails": tc in run_cmd(
            "grep -r 'test_registry_duplicate_registration_fails' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-3: test test_registry_execute_unknown_phase_fails exists",
        lambda tc="test_registry_execute_unknown_phase_fails": tc in run_cmd(
            "grep -r 'test_registry_execute_unknown_phase_fails' crates/", cwd=WORKSPACE
        )[1]
    ))
    # INS-4: Deterministic Topological Sort + Graph Analytics
    results.append(check(
        "INS-4: src/aco/graph.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/graph.rs").exists()
    ))
    results.append(check(
        "INS-4: MutableGeneratorGraph struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct MutableGeneratorGraph\\|pub enum MutableGeneratorGraph' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-4: GeneratorGraphModel struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct GeneratorGraphModel\\|pub enum GeneratorGraphModel' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-4: test test_topo_sort_deterministic_simple exists",
        lambda tc="test_topo_sort_deterministic_simple": tc in run_cmd(
            "grep -r 'test_topo_sort_deterministic_simple' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-4: test test_topo_sort_deterministic_stable_across_insertions exists",
        lambda tc="test_topo_sort_deterministic_stable_across_insertions": tc in run_cmd(
            "grep -r 'test_topo_sort_deterministic_stable_across_insertions' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-4: test test_topo_sort_detects_cycle exists",
        lambda tc="test_topo_sort_detects_cycle": tc in run_cmd(
            "grep -r 'test_topo_sort_detects_cycle' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "cargo clippy --workspace -D warnings exits 0",
        lambda: run_cmd("cargo clippy --workspace -- -D warnings")[0] == 0
    ))
    code, out = run_cmd("cargo test --workspace --exclude touring-python 2>&1 | tail -5")
    results.append(check(
        "cargo test --workspace: 0 failed",
        lambda: "0 failed" in out
    ))
    if VERBOSE:
        print(f"  Test output: {out.strip()}")
    results.append(check(
        "Checkpoint checkpoints/phase-02-graph-registry.toon exists",
        lambda: (PLAN_DIR / "checkpoints/phase-02-graph-registry.toon").exists()
    ))

    passed = sum(results)
    total = len(results)
    print(f"\n{'=' * 40}")
    print(f"Phase 2 Validation: {{passed}}/{{total}} PASS")
    if passed < total:
        print("FAIL — fix issues before proceeding")
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
