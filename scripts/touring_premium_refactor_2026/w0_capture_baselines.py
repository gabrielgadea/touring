#!/usr/bin/env python3
"""W0.2+W0.3+W0.4+W0.5 — Orchestrate baseline captures (bench/CI/coverage/wiring).

Runs (any subset) of:
  - cargo bench --workspace --save-baseline pre-refactor-<DATE>  (slow; 30-60+ min)
  - cargo check --workspace --all-targets                         (medium; 1-5 min)
  - cargo test --workspace --no-run --all-targets                 (medium; 2-10 min)
  - cargo llvm-cov --workspace --json                             (slow; 5-30 min)
  - touring wiring audit -j / cycles / status / workspace-info    (fast; <30s)

Outputs into docs/baselines/, indexed by <DATE>. Returns aggregated JSON
status. Long-running steps can be skipped via --skip-* flags.

Usage
-----
    python3 w0_capture_baselines.py --skip-bench --skip-coverage   # fast baselines only
    python3 w0_capture_baselines.py --all                          # full (long)
    python3 w0_capture_baselines.py --dry-run
"""
from __future__ import annotations

import argparse
import json
import logging
import subprocess
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_BASELINES_DIR = _ROOT / "docs" / "baselines"


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w0_capture_baselines", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--date", type=str,
                   default=datetime.now(UTC).strftime("%Y-%m-%d"))
    p.add_argument("--all", action="store_true", help="Run all baselines (slow).")
    p.add_argument("--skip-bench", action="store_true")
    p.add_argument("--skip-coverage", action="store_true")
    p.add_argument("--skip-check", action="store_true")
    p.add_argument("--skip-test-norun", action="store_true")
    p.add_argument("--skip-wiring", action="store_true")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def run_cmd(cmd: list[str], log_path: Path, timeout: int, dry: bool) -> dict:
    """Run a command, redirect output to log file, return result."""
    log_path.parent.mkdir(parents=True, exist_ok=True)
    if dry:
        return {"command": " ".join(cmd), "status": "DRY_RUN",
                "log": str(log_path.relative_to(_ROOT))}
    LOGGER.info("running: %s", " ".join(cmd[:6]) + " ...")
    try:
        with log_path.open("wb") as out:
            proc = subprocess.run(cmd, cwd=_ROOT, stdout=out, stderr=subprocess.STDOUT,
                                  timeout=timeout)
        return {"command": " ".join(cmd), "exit_code": proc.returncode,
                "log": str(log_path.relative_to(_ROOT)),
                "status": "OK" if proc.returncode == 0 else "FAIL"}
    except subprocess.TimeoutExpired:
        return {"command": " ".join(cmd), "exit_code": -1, "status": "TIMEOUT",
                "log": str(log_path.relative_to(_ROOT)), "timeout_sec": timeout}
    except FileNotFoundError as e:
        return {"command": " ".join(cmd), "exit_code": -2, "status": "MISSING_TOOL",
                "error": str(e)}


def run_touring_json(args: list[str], out_path: Path, timeout: int, dry: bool) -> dict:
    """Capture touring CLI JSON output to file."""
    if dry:
        return {"command": "touring " + " ".join(args), "status": "DRY_RUN",
                "output": str(out_path.relative_to(_ROOT))}
    try:
        proc = subprocess.run(["touring", *args], cwd=_ROOT, capture_output=True,
                              timeout=timeout)
        if proc.returncode != 0:
            return {"command": "touring " + " ".join(args), "status": "FAIL",
                    "exit_code": proc.returncode,
                    "stderr": proc.stderr.decode("utf-8", errors="replace")[:500]}
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_bytes(proc.stdout)
        return {"command": "touring " + " ".join(args), "status": "OK",
                "output": str(out_path.relative_to(_ROOT)),
                "bytes": len(proc.stdout)}
    except subprocess.TimeoutExpired:
        return {"command": "touring " + " ".join(args), "status": "TIMEOUT"}
    except FileNotFoundError:
        return {"command": "touring " + " ".join(args), "status": "MISSING_TOOL"}


def run(args: argparse.Namespace) -> dict:
    """Capture all selected baselines."""
    _BASELINES_DIR.mkdir(parents=True, exist_ok=True)
    date = args.date
    results: dict[str, dict] = {}

    # W0.3: cargo check + test --no-run
    if not args.skip_check:
        results["cargo_check"] = run_cmd(
            ["cargo", "check", "--workspace", "--all-targets"],
            _BASELINES_DIR / f"cargo-check-pre-refactor-{date}.log",
            timeout=600, dry=args.dry_run,
        )
    if not args.skip_test_norun:
        results["cargo_test_norun"] = run_cmd(
            ["cargo", "test", "--workspace", "--no-run", "--all-targets"],
            _BASELINES_DIR / f"cargo-test-norun-pre-refactor-{date}.log",
            timeout=1800, dry=args.dry_run,
        )

    # W0.5: touring wiring/status/workspace-info (fast)
    if not args.skip_wiring:
        results["wiring_audit"] = run_touring_json(
            ["wiring", "audit", "-j"],
            _BASELINES_DIR / f"wiring-pre-refactor-{date}.json",
            timeout=120, dry=args.dry_run,
        )
        results["wiring_cycles"] = run_touring_json(
            ["wiring", "cycles", "--min-depth", "2", "--format", "json"],
            _BASELINES_DIR / f"cycles-pre-refactor-{date}.json",
            timeout=60, dry=args.dry_run,
        )
        results["status"] = run_touring_json(
            ["status", "-j"],
            _BASELINES_DIR / f"status-pre-refactor-{date}.json",
            timeout=30, dry=args.dry_run,
        )
        results["workspace_info"] = run_touring_json(
            ["ast", "workspace-info"],
            _BASELINES_DIR / f"workspace-info-pre-refactor-{date}.json",
            timeout=60, dry=args.dry_run,
        )

    # W0.2: cargo bench (long-running)
    if not args.skip_bench:
        results["cargo_bench"] = run_cmd(
            ["cargo", "bench", "--workspace", "--no-fail-fast",
             "--", "--save-baseline", f"pre-refactor-{date}"],
            _BASELINES_DIR / f"bench-pre-refactor-{date}.log",
            timeout=7200, dry=args.dry_run,
        )

    # W0.4: cargo llvm-cov (long-running)
    if not args.skip_coverage:
        results["cargo_llvm_cov"] = run_cmd(
            ["cargo", "llvm-cov", "--workspace", "--json", "--output-path",
             str(_BASELINES_DIR / f"coverage-pre-refactor-{date}.json")],
            _BASELINES_DIR / f"coverage-pre-refactor-{date}.log",
            timeout=3600, dry=args.dry_run,
        )

    summary_ok = sum(1 for r in results.values() if r.get("status") == "OK")
    summary_fail = sum(1 for r in results.values()
                        if r.get("status") in ("FAIL", "TIMEOUT", "MISSING_TOOL"))
    return {
        "script": "w0_capture_baselines",
        "wave": "W0",
        "subtask_refs": ["W0.2", "W0.3", "W0.4", "W0.5"],
        "timestamp": datetime.now(UTC).isoformat(),
        "date": date,
        "dry_run": args.dry_run,
        "summary": {"ok": summary_ok, "fail": summary_fail,
                    "total": len(results)},
        "results": results,
    }


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    if args.all:
        args.skip_bench = False
        args.skip_coverage = False
        args.skip_check = False
        args.skip_test_norun = False
        args.skip_wiring = False
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0 if result["summary"]["fail"] == 0 else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
