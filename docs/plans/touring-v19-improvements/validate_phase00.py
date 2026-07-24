#!/usr/bin/env python3
"""
validate_phase00.py — Validation script for Phase 0: Foundation: DISCOVER, VGP Cache, Checkpoints
Plan: touring-v19-improvements

Runs all validation checks for phase 0 and exits 1 if any FAIL.
Usage: python validate_phase00.py [--verbose]
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
    # OWASP A03 guard (W8 of harness-consolidation): the previous
    # implementation used `shell=True`, which is a command-injection
    # footgun even though `cmd` is a hard-coded literal at every
    # call-site. Switch to the proper idiom — shlex.split gives a list of
    # args, which `subprocess.run` invokes without any shell
    # metacharacter interpretation. Future edits that interpolate
    # user input cannot accidentally trigger injection.
    args = shlex.split(cmd)
    r = subprocess.run(args, shell=False, capture_output=True, text=True, cwd=cwd or WORKSPACE)
    return r.returncode, r.stdout + r.stderr


def main() -> int:
    global VERBOSE
    parser = argparse.ArgumentParser(description="Validate Phase 0")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    VERBOSE = args.verbose

    print(f"=== Phase 0 Validation: Foundation: DISCOVER, VGP Cache, Checkpoints ===")
    results = []

    results.append(check(
        "cargo check --workspace exits 0",
        lambda: run_cmd("cargo check --workspace")[0] == 0
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
        "Checkpoint checkpoints/phase-00-foundation.toon exists",
        lambda: (PLAN_DIR / "checkpoints/phase-00-foundation.toon").exists()
    ))

    passed = sum(results)
    total = len(results)
    print(f"\n{'=' * 40}")
    print(f"Phase 0 Validation: {{passed}}/{{total}} PASS")
    if passed < total:
        print("FAIL — fix issues before proceeding")
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
