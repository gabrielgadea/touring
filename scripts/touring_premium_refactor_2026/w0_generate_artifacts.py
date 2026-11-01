#!/usr/bin/env python3
"""W0.9 — Wrapper invocando generate_plan.py --all --force; emits checkpoint.

Triggers full plan regeneration + emits a checkpoint manifest noting which
artifacts (markdown + validators + sub-scripts) were emitted.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import script_common as sc

_SCRIPT = "w0_generate_artifacts"

_ROOT = sc.ROOT
_SCRIPTS_DIR = Path(__file__).resolve().parent
_DATA_DIR = _SCRIPTS_DIR / "data"


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--dry-run", action="store_true")
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def invoke_generator(dry_run: bool) -> dict:
    """Invoke generate_plan.py --all."""
    generator = _SCRIPTS_DIR / "generate_plan.py"
    cmd = [sys.executable, str(generator), "--all"]
    if dry_run:
        return {"status": "DRY_RUN", "command": " ".join(cmd)}
    try:
        proc = subprocess.run(cmd, cwd=_SCRIPTS_DIR, capture_output=True,
                               timeout=120, check=False)
        return {
            "status": "OK" if proc.returncode == 0 else "FAIL",
            "exit_code": proc.returncode,
            "stdout_tail": proc.stdout.decode("utf-8",
                                                errors="replace")[-500:],
        }
    except subprocess.TimeoutExpired:
        return {"status": "TIMEOUT"}
    except FileNotFoundError:
        return {"status": "MISSING_PYTHON"}


def run(args: sc.Namespace) -> dict:
    """Trigger regeneration + emit checkpoint."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    invoke_result = invoke_generator(args.dry_run)
    report = {
        "script": "w0_generate_artifacts",
        "wave": "W0", "subtask_refs": ["W0.9"],
        "timestamp": sc.now_iso(),
        "dry_run": args.dry_run,
        "invoke_result": invoke_result,
        "status": invoke_result.get("status", "UNKNOWN"),
    }
    json_path = args.output_dir / "w0-generate-artifacts-checkpoint.json"
    return sc.write_json_report(json_path, report, _ROOT)


def _run_and_report(args: sc.Namespace) -> int:
    """Run w0_generate_artifacts, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0 if result["status"] in ("OK", "DRY_RUN") else 1


def main() -> int:
    """Entry point of w0_generate_artifacts."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
