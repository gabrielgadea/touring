#!/usr/bin/env python3
"""
validate_phase03.py — Validation script for Phase 3: Persistence Patterns: CQRS + Saga
Plan: touring-v19-improvements

Runs all validation checks for phase 3 and exits 1 if any FAIL.
Usage: python validate_phase03.py [--verbose]
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
    parser = argparse.ArgumentParser(description="Validate Phase 3")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    VERBOSE = args.verbose

    print(f"=== Phase 3 Validation: Persistence Patterns: CQRS + Saga ===")
    results = []

    results.append(check(
        "cargo check --workspace exits 0",
        lambda: run_cmd("cargo check --workspace")[0] == 0
    ))
    # INS-5: CQRS Read Model
    results.append(check(
        "INS-5: src/aco/read_model.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/read_model.rs").exists()
    ))
    results.append(check(
        "INS-5: MutableGeneratorGraph struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct MutableGeneratorGraph\\|pub enum MutableGeneratorGraph' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-5: test test_read_model_apply_and_get exists",
        lambda tc="test_read_model_apply_and_get": tc in run_cmd(
            "grep -r 'test_read_model_apply_and_get' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-5: test test_read_model_invalidate_clears_access exists",
        lambda tc="test_read_model_invalidate_clears_access": tc in run_cmd(
            "grep -r 'test_read_model_invalidate_clears_access' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-5: test test_read_model_rebuild_from_graph exists",
        lambda tc="test_read_model_rebuild_from_graph": tc in run_cmd(
            "grep -r 'test_read_model_rebuild_from_graph' crates/", cwd=WORKSPACE
        )[1]
    ))
    # INS-6: Saga Pattern with Compensating Transactions
    results.append(check(
        "INS-6: src/aco/saga.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/saga.rs").exists()
    ))
    results.append(check(
        "INS-6: test test_saga_all_steps_succeed exists",
        lambda tc="test_saga_all_steps_succeed": tc in run_cmd(
            "grep -r 'test_saga_all_steps_succeed' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-6: test test_saga_rollback_on_step_failure exists",
        lambda tc="test_saga_rollback_on_step_failure": tc in run_cmd(
            "grep -r 'test_saga_rollback_on_step_failure' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-6: test test_saga_compensation_order_reversed exists",
        lambda tc="test_saga_compensation_order_reversed": tc in run_cmd(
            "grep -r 'test_saga_compensation_order_reversed' crates/", cwd=WORKSPACE
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
        "Checkpoint checkpoints/phase-03-persistence-patterns.toon exists",
        lambda: (PLAN_DIR / "checkpoints/phase-03-persistence-patterns.toon").exists()
    ))

    passed = sum(results)
    total = len(results)
    print(f"\n{'=' * 40}")
    print(f"Phase 3 Validation: {{passed}}/{{total}} PASS")
    if passed < total:
        print("FAIL — fix issues before proceeding")
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
