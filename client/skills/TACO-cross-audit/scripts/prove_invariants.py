#!/usr/bin/env python3
"""TACO-cross-audit invariant prover — Phase 6 of the cross-audit.

Detects a project's toolchain, runs its real test suite, and verifies the
exit-zero invariant. A cross-audit proves claims by *running* them — this
script produces the executed evidence the Phase 7 report carries.

Output is a human report, or JSON with ``--json``.
Exit code: 0 = invariant holds, 1 = violated or unproven, 2 = bad arguments.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any, Sequence

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402 — local sibling module

# Per-toolchain test command. The audit runs the real suite, not a mock.
TEST_COMMANDS: dict[str, list[str]] = {
    "rust": ["cargo", "test", "--workspace", "--quiet"],
    "python": ["python3", "-m", "pytest", "-q"],
    "node": ["npm", "test", "--silent"],
}


def prove(root: Path, timeout: float) -> dict[str, Any]:
    """Run the project's test suite and capture the exit-zero invariant."""
    kind = lib.detect_project_kind(root)
    if kind == "unknown":
        return {
            "project_kind": "unknown",
            "ran": False,
            "exit_zero": None,
            "detail": "no Cargo.toml / pyproject.toml / package.json — "
                      "cannot auto-detect a test suite (UNVERIFIED)",
        }
    result = lib.run(TEST_COMMANDS[kind], cwd=root, timeout=timeout)
    return {
        "project_kind": kind,
        "ran": not result.timed_out,
        "command": result.command,
        "exit_code": result.exit_code,
        "exit_zero": result.exit_code == 0,
        "timed_out": result.timed_out,
        "stdout_tail": "\n".join(result.stdout.splitlines()[-15:]),
        "stderr_tail": "\n".join(result.stderr.splitlines()[-8:]),
    }


def main(argv: Sequence[str] | None = None) -> int:
    """Run a project's tests and verify the exit-zero invariant."""
    parser = argparse.ArgumentParser(
        prog="prove_invariants.py",
        description="Run a project's test suite and verify the exit-zero invariant.",
    )
    parser.add_argument("directory", help="Project root directory.")
    parser.add_argument("--timeout", type=float, default=600.0,
                        help="Seconds to allow the test suite to run.")
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    args = parser.parse_args(argv)

    root = Path(args.directory).expanduser().resolve()
    if not root.is_dir():
        print(f"error: not a directory: {root}", file=sys.stderr)
        return 2

    report = prove(root, args.timeout)

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0 if report.get("exit_zero") else 1

    print(f"== invariant proof: {root} ==\n")
    print(f"project kind: {report['project_kind']}")
    if not report["ran"]:
        print(f"UNVERIFIED — {report.get('detail', 'the suite did not run')}")
        return 1
    print(f"command:      {report['command']}")
    print(f"exit code:    {report['exit_code']}")
    print(f"exit-0 invariant: {'HOLDS' if report['exit_zero'] else 'VIOLATED'}")
    if report["stdout_tail"]:
        print("\n-- stdout (tail) --")
        print(report["stdout_tail"])
    if not report["exit_zero"] and report["stderr_tail"]:
        print("\n-- stderr (tail) --")
        print(report["stderr_tail"])
    return 0 if report["exit_zero"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
