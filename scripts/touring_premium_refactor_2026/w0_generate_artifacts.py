#!/usr/bin/env python3
"""W0.9 — Wrapper invocando generate_plan.py --all --force; emits checkpoint.

Triggers full plan regeneration + emits a checkpoint manifest noting which
artifacts (markdown + validators + sub-scripts) were emitted.
"""
from __future__ import annotations

import argparse
import json
import logging
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_SCRIPTS_DIR = Path(__file__).resolve().parent
_DATA_DIR = _SCRIPTS_DIR / "data"


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w0_generate_artifacts", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def invoke_generator(dry_run: bool) -> dict:
    """Invoke generate_plan.py --all."""
    generator = _SCRIPTS_DIR / "generate_plan.py"
    cmd = [sys.executable, str(generator), "--all"]
    if dry_run:
        return {"status": "DRY_RUN", "command": " ".join(cmd)}
    try:
        proc = subprocess.run(cmd, cwd=_SCRIPTS_DIR, capture_output=True,
                               timeout=120)
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


def run(args: argparse.Namespace) -> dict:
    """Trigger regeneration + emit checkpoint."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    invoke_result = invoke_generator(args.dry_run)
    report = {
        "script": "w0_generate_artifacts",
        "wave": "W0", "subtask_refs": ["W0.9"],
        "timestamp": datetime.now(UTC).isoformat(),
        "dry_run": args.dry_run,
        "invoke_result": invoke_result,
        "status": invoke_result.get("status", "UNKNOWN"),
    }
    json_path = args.output_dir / "w0-generate-artifacts-checkpoint.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    report["json_path"] = str(json_path.relative_to(_ROOT))
    return report


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
        return 0 if result["status"] in ("OK", "DRY_RUN") else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
