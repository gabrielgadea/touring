#!/usr/bin/env python3
"""
validate_phase01.py — Validation script for Phase 1: Quick Wins: TemplateLibrary + GoalTracker 9×9
Plan: touring-v19-improvements

Runs all validation checks for phase 1 and exits 1 if any FAIL.
Usage: python validate_phase01.py [--verbose]
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
    parser = argparse.ArgumentParser(description="Validate Phase 1")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    VERBOSE = args.verbose

    print(f"=== Phase 1 Validation: Quick Wins: TemplateLibrary + GoalTracker 9×9 ===")
    results = []

    results.append(check(
        "cargo check --workspace exits 0",
        lambda: run_cmd("cargo check --workspace")[0] == 0
    ))
    # INS-1: TemplateLibrary with Learning
    results.append(check(
        "INS-1: src/aco/template_library.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/template_library.rs").exists()
    ))
    results.append(check(
        "INS-1: LearnedPattern struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct LearnedPattern\\|pub enum LearnedPattern' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-1: EvolutionPackage struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct EvolutionPackage\\|pub enum EvolutionPackage' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-1: test test_record_template_new exists",
        lambda tc="test_record_template_new": tc in run_cmd(
            "grep -r 'test_record_template_new' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-1: test test_record_template_dedup_increments_usage exists",
        lambda tc="test_record_template_dedup_increments_usage": tc in run_cmd(
            "grep -r 'test_record_template_dedup_increments_usage' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-1: test test_find_similar_by_domain exists",
        lambda tc="test_find_similar_by_domain": tc in run_cmd(
            "grep -r 'test_find_similar_by_domain' crates/", cwd=WORKSPACE
        )[1]
    ))
    # INS-2: GoalTracker 9×9 Computational
    results.append(check(
        "INS-2: src/aco/tracker.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/tracker.rs").exists()
    ))
    results.append(check(
        "INS-2: TrackerReport struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct TrackerReport\\|pub enum TrackerReport' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-2: DimResult struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct DimResult\\|pub enum DimResult' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-2: test test_9x9_matrix_completeness exists",
        lambda tc="test_9x9_matrix_completeness": tc in run_cmd(
            "grep -r 'test_9x9_matrix_completeness' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-2: test test_compute_dimensional_score_passes_on_valid exists",
        lambda tc="test_compute_dimensional_score_passes_on_valid": tc in run_cmd(
            "grep -r 'test_compute_dimensional_score_passes_on_valid' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-2: test test_compute_dimensional_score_fails_on_missing_context exists",
        lambda tc="test_compute_dimensional_score_fails_on_missing_context": tc in run_cmd(
            "grep -r 'test_compute_dimensional_score_fails_on_missing_context' crates/", cwd=WORKSPACE
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
        "Checkpoint checkpoints/phase-01-quick-wins.toon exists",
        lambda: (PLAN_DIR / "checkpoints/phase-01-quick-wins.toon").exists()
    ))

    passed = sum(results)
    total = len(results)
    print(f"\n{'=' * 40}")
    print(f"Phase 1 Validation: {{passed}}/{{total}} PASS")
    if passed < total:
        print("FAIL — fix issues before proceeding")
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
