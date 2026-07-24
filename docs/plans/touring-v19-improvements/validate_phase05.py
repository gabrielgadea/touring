#!/usr/bin/env python3
"""
validate_phase05.py — Validation script for Phase 5: Cognitive Patterns: Time-Travel + State Machine
Plan: touring-v19-improvements

Runs all validation checks for phase 5 and exits 1 if any FAIL.
Usage: python validate_phase05.py [--verbose]
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
    parser = argparse.ArgumentParser(description="Validate Phase 5")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    VERBOSE = args.verbose

    print(f"=== Phase 5 Validation: Cognitive Patterns: Time-Travel + State Machine ===")
    results = []

    results.append(check(
        "cargo check --workspace exits 0",
        lambda: run_cmd("cargo check --workspace")[0] == 0
    ))
    # INS-9: Time-Travel Debugging
    results.append(check(
        "INS-9: src/aco/time_travel.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/time_travel.rs").exists()
    ))
    results.append(check(
        "INS-9: MutableGeneratorGraph struct verified",
        lambda: run_cmd(
            "grep -r 'pub struct MutableGeneratorGraph\\|pub enum MutableGeneratorGraph' crates/", cwd=WORKSPACE
        )[0] == 0
    ))
    results.append(check(
        "INS-9: test test_capture_state_returns_epoch exists",
        lambda tc="test_capture_state_returns_epoch": tc in run_cmd(
            "grep -r 'test_capture_state_returns_epoch' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-9: test test_state_at_epoch_found exists",
        lambda tc="test_state_at_epoch_found": tc in run_cmd(
            "grep -r 'test_state_at_epoch_found' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-9: test test_state_at_epoch_not_found exists",
        lambda tc="test_state_at_epoch_not_found": tc in run_cmd(
            "grep -r 'test_state_at_epoch_not_found' crates/", cwd=WORKSPACE
        )[1]
    ))
    # INS-10: Agent State Machine Complete
    results.append(check(
        "INS-10: src/agent_state_machine.rs exists",
        lambda: Path("/home/gabrielgadea/.claude/rust/crates/touring-cognitive/src/agent_state_machine.rs").exists()
    ))
    results.append(check(
        "INS-10: test test_state_machine_starts_idle exists",
        lambda tc="test_state_machine_starts_idle": tc in run_cmd(
            "grep -r 'test_state_machine_starts_idle' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-10: test test_valid_transition_sequence exists",
        lambda tc="test_valid_transition_sequence": tc in run_cmd(
            "grep -r 'test_valid_transition_sequence' crates/", cwd=WORKSPACE
        )[1]
    ))
    results.append(check(
        "INS-10: test test_invalid_transition_rejected exists",
        lambda tc="test_invalid_transition_rejected": tc in run_cmd(
            "grep -r 'test_invalid_transition_rejected' crates/", cwd=WORKSPACE
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
        "Checkpoint checkpoints/phase-05-cognitive-patterns.toon exists",
        lambda: (PLAN_DIR / "checkpoints/phase-05-cognitive-patterns.toon").exists()
    ))

    passed = sum(results)
    total = len(results)
    print(f"\n{'=' * 40}")
    print(f"Phase 5 Validation: {{passed}}/{{total}} PASS")
    if passed < total:
        print("FAIL — fix issues before proceeding")
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
