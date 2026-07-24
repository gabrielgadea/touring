#!/usr/bin/env python3
"""
validate_phase04.py — Validation script for Phase 4: Performance: Parallel Engine + ESAA 24
Plan: touring-v19-improvements

Runs all validation checks for phase 4 and exits 1 if any FAIL.
Usage: python validate_phase04.py [--verbose]
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
    parser = argparse.ArgumentParser(description="Validate Phase 4")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    VERBOSE = args.verbose

    print(f"=== Phase 4 Validation: Performance: Parallel Engine + ESAA 24 ===")
    results = []

    results.append(check(
        "cargo check --workspace exits 0",
        lambda: run_cmd("cargo check --workspace")[0] == 0
    ))
    # INS-7: Parallel Generator Engine
    results.append(check(
        "INS-7: src/aco/graph.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/graph.rs").exists()
    ))
    results.append(check(
        "INS-7: MutableGeneratorGraph struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct MutableGeneratorGraph\\|pub enum MutableGeneratorGraph' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-7: GeneratorGraphModel struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct GeneratorGraphModel\\|pub enum GeneratorGraphModel' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-7: test test_parallel_execute_independent_nodes exists",
        lambda tc="test_parallel_execute_independent_nodes": tc in run_cmd(
            "grep -r 'test_parallel_execute_independent_nodes' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-7: test test_parallel_groups_sequential_constraint exists",
        lambda tc="test_parallel_groups_sequential_constraint": tc in run_cmd(
            "grep -r 'test_parallel_groups_sequential_constraint' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-7: test test_parallelizable_groups_detection exists",
        lambda tc="test_parallelizable_groups_detection": tc in run_cmd(
            "grep -r 'test_parallelizable_groups_detection' crates/", cwd=WORKSPACE
        )[1]
    ))
    # INS-8: ESAA Complete 24 Subsystems
    results.append(check(
        "INS-8: src/aco/esaa.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/esaa.rs").exists()
    ))
    results.append(check(
        "INS-8: test test_esaa_coordinator_register_all_24 exists",
        lambda tc="test_esaa_coordinator_register_all_24": tc in run_cmd(
            "grep -r 'test_esaa_coordinator_register_all_24' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-8: test test_esaa_router_routes_correctly exists",
        lambda tc="test_esaa_router_routes_correctly": tc in run_cmd(
            "grep -r 'test_esaa_router_routes_correctly' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-8: test test_esaa_planner_processes_input exists",
        lambda tc="test_esaa_planner_processes_input": tc in run_cmd(
            "grep -r 'test_esaa_planner_processes_input' crates/", cwd=WORKSPACE
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
        "Checkpoint checkpoints/phase-04-performance-esaa.toon exists",
        lambda: (PLAN_DIR / "checkpoints/phase-04-performance-esaa.toon").exists()
    ))

    passed = sum(results)
    total = len(results)
    print(f"\n{'=' * 40}")
    print(f"Phase 4 Validation: {{passed}}/{{total}} PASS")
    if passed < total:
        print("FAIL — fix issues before proceeding")
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
