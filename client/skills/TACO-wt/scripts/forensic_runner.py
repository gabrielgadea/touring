#!/usr/bin/env python3
"""forensic_runner — Execute all sub-scripts of a wave in parallel.

Uses ThreadPoolExecutor (I/O-bound sub-scripts → threading wins over multiprocessing).
Discovers sub-scripts under ``<plan_dir>/<wave>/*.py`` (excluding validators and tests),
spawns each as a subprocess, captures their JSON output, and aggregates a wave report.

Honors L9: --apply must be explicit. Default is dry-run.

Usage
-----
    python3 forensic_runner.py --plan-dir scripts/<plan> --wave W12
    python3 forensic_runner.py --plan-dir scripts/<plan> --wave W12 --apply-all
    python3 forensic_runner.py --plan-dir scripts/<plan> --wave W12 --workers 8 -j
"""

from __future__ import annotations

import argparse
import json
import logging
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from lib import (  # noqa: E402  pylint: disable=wrong-import-position
    EXIT_FAIL,
    EXIT_INTERRUPTED,
    EXIT_OK,
    EXIT_STRUCTURAL,
    append_jsonl,
    is_wave_id,
    learning_path,
    touring_learning_reward,
    utcnow_iso,
    write_json_atomic,
)


def discover_sub_scripts(wave_dir: Path) -> list[Path]:
    """List forensic sub-scripts in a wave directory.

    Excludes ``validate_*.py`` (handled separately), ``conftest.py``, ``__init__.py``,
    and anything inside a ``tests/`` subdirectory.
    """
    if not wave_dir.exists() or not wave_dir.is_dir():
        return []
    skip_prefixes = ("validate_", "_")
    skip_exact = {"conftest.py", "__init__.py"}
    return sorted(
        p for p in wave_dir.glob("*.py")
        if not p.name.startswith(skip_prefixes)
        and p.name not in skip_exact
    )


def run_sub_script(
    script: Path,
    *,
    apply_mutation: bool,
    timeout_seconds: int = 300,
) -> dict[str, Any]:
    """Execute a single sub-script and capture its JSON stdout."""
    started = time.monotonic()
    cmd = ["python3", str(script), "-j"]
    if apply_mutation:
        cmd.append("--apply")
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return {
            "script": script.name,
            "status": "TIMEOUT",
            "duration_ms": int((time.monotonic() - started) * 1000),
            "stderr": f"Exceeded {timeout_seconds}s timeout",
        }
    except FileNotFoundError as exc:
        return {
            "script": script.name,
            "status": "MISSING_INTERPRETER",
            "duration_ms": 0,
            "stderr": str(exc),
        }

    duration_ms = int((time.monotonic() - started) * 1000)
    parsed: dict[str, Any] | None = None
    try:
        parsed = json.loads(result.stdout)
    except json.JSONDecodeError:
        parsed = None

    return {
        "script": script.name,
        "status": (parsed or {}).get("status", "OK" if result.returncode == 0 else "FAIL"),
        "exit_code": result.returncode,
        "duration_ms": duration_ms,
        "stdout_json": parsed,
        "stderr": result.stderr if result.returncode != 0 else "",
    }


def run_wave_parallel(
    wave_dir: Path,
    *,
    apply_mutation: bool,
    workers: int = 4,
    timeout_seconds: int = 300,
) -> dict[str, Any]:
    """Run all sub-scripts of a wave in parallel via ThreadPoolExecutor."""
    scripts = discover_sub_scripts(wave_dir)
    if not scripts:
        return {
            "wave": wave_dir.name,
            "sub_scripts_total": 0,
            "results": [],
            "warning": f"No sub-scripts discovered in {wave_dir}",
        }

    results: list[dict[str, Any]] = []
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=workers) as executor:
        futures = {
            executor.submit(run_sub_script, s,
                            apply_mutation=apply_mutation,
                            timeout_seconds=timeout_seconds): s
            for s in scripts
        }
        for future in as_completed(futures):
            try:
                results.append(future.result())
            except Exception as exc:  # noqa: BLE001
                results.append({
                    "script": futures[future].name,
                    "status": "EXCEPTION",
                    "stderr": str(exc),
                })

    wall_time_ms = int((time.monotonic() - started) * 1000)
    results.sort(key=lambda r: r.get("script", ""))

    return {
        "wave": wave_dir.name,
        "sub_scripts_total": len(scripts),
        "wall_time_ms": wall_time_ms,
        "ok_count": sum(1 for r in results if r.get("status") == "OK"),
        "fail_count": sum(1 for r in results if r.get("status") in {"FAIL", "TIMEOUT", "EXCEPTION"}),
        "results": results,
    }


def _persist_outcome(wave_report: dict[str, Any], plan: str) -> None:
    """Append a WaveOutcome line to the cross-session learning JSONL."""
    outcome = {
        "timestamp": utcnow_iso(),
        "plan": plan,
        "wave": wave_report.get("wave", ""),
        "status": "OK" if wave_report.get("fail_count", 0) == 0 else "FAIL",
        "score": (
            wave_report.get("ok_count", 0) /
            max(wave_report.get("sub_scripts_total", 1), 1)
        ),
        "duration_ms": float(wave_report.get("wall_time_ms", 0)),
        "lesson": "",
        "hallucinated_assumptions": [],
    }
    try:
        append_jsonl(learning_path(plan), outcome)
    except OSError as exc:
        logging.getLogger(__name__).debug("Could not write learning JSONL: %s", exc)
    # Best-effort RL reward (fail-open)
    touring_learning_reward(
        "orchestrate",
        1.0 if outcome["status"] == "OK" else -1.0,
        context=f"wave:{plan}:{outcome['wave']}:{outcome['status']}",
    )


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="forensic_runner", description=__doc__)
    parser.add_argument("--plan-dir", type=Path, required=True,
                        help="Plan directory containing the wave folder.")
    parser.add_argument("--wave", required=True,
                        help="Wave id (e.g. W12 or W12.3).")
    parser.add_argument("--plan", default="",
                        help="Plan name for the learning JSONL "
                             "(defaults to plan_dir.name).")
    parser.add_argument("--workers", type=int, default=4,
                        help="Parallel ThreadPoolExecutor workers (default 4).")
    parser.add_argument("--timeout", type=int, default=300,
                        help="Per-sub-script timeout in seconds (default 300).")
    parser.add_argument("--apply-all", action="store_true",
                        help="Pass --apply to EVERY sub-script. Mutation default is OFF.")
    parser.add_argument("--apply", action="store_true",
                        help="Alias for --apply-all (consistency with other scripts).")
    parser.add_argument("--output-dir", type=Path, default=Path("data"),
                        help="Where to emit the aggregated JSON report.")
    parser.add_argument("--emit", action="store_true",
                        help="Write data/<wave>-aggregate.json.")
    parser.add_argument("--no-learning", action="store_true",
                        help="Skip cross-session learning persistence.")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Discover + run + aggregate."""
    if not args.plan_dir.exists():
        msg = f"Plan directory not found: {args.plan_dir}"
        raise FileNotFoundError(msg)
    if not is_wave_id(args.wave):
        msg = f"--wave '{args.wave}' does not match W<N> pattern"
        raise ValueError(msg)

    wave_dir = args.plan_dir / args.wave
    apply_mutation = args.apply or args.apply_all
    wave_report = run_wave_parallel(
        wave_dir,
        apply_mutation=apply_mutation,
        workers=args.workers,
        timeout_seconds=args.timeout,
    )

    plan_name = args.plan or args.plan_dir.name
    if not args.no_learning:
        _persist_outcome(wave_report, plan_name)

    envelope = {
        "status": "OK" if wave_report.get("fail_count", 0) == 0 else "WARN",
        "script": "forensic_runner",
        "timestamp": utcnow_iso(),
        "plan": plan_name,
        "wave": args.wave,
        "apply": apply_mutation,
        "workers": args.workers,
        "wave_report": wave_report,
    }

    if args.emit:
        out = args.output_dir / f"{args.wave}-aggregate.json"
        write_json_atomic(out, envelope)
        envelope["json_path"] = str(out)

    return envelope


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        return EXIT_OK if result["status"] == "OK" else EXIT_FAIL
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except (FileNotFoundError, ValueError) as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("forensic_runner failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
