#!/usr/bin/env python3
"""cross_audit — Aggregate per-wave validators into a composite plan score.

Two modes:
  * --baseline: PENDING waves are EXCLUDED from the composite. Status reads
    BASELINE when 100% PENDING; otherwise it averages only run waves.
  * (default) normal mode: PENDING waves count as 0.0 → composite reflects
    how much of the plan has actually shipped.

For each wave, this script:
  1. Looks for ``<plan_dir>/<wave>/validate_<wave>.py``.
  2. Invokes it (read-only, default flags) and captures the JSON envelope.
  3. Falls back to scanning ``<plan_dir>/data/<wave>-*.json`` to detect PENDING.

Outputs the canonical ``CrossAuditReport`` envelope (see lib.py).

Usage
-----
    python3 cross_audit.py --plan-dir scripts/<plan>                # normal mode
    python3 cross_audit.py --plan-dir scripts/<plan> --baseline     # baseline mode
    python3 cross_audit.py --plan-dir scripts/<plan> --emit -j

Exit codes
----------
    0 PASS / BASELINE
    1 WARN
    2 FAIL
    3 structural error
"""

from __future__ import annotations

import argparse
import json
import logging
import re
import subprocess
import sys
from collections import defaultdict
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
    EXIT_WARN,
    safe_load_json,
    touring_memory_store,
    utcnow_iso,
    write_json_atomic,
)

_RE_VALIDATOR = re.compile(r"validate_(W\d{1,3}(?:\.\d+)?)\.py$")


def discover_waves(plan_dir: Path) -> list[str]:
    """Discover every wave id by looking for validate_W<N>.py and W<N>/ dirs."""
    waves: set[str] = set()
    for path in plan_dir.rglob("validate_W*.py"):
        match = _RE_VALIDATOR.search(path.name)
        if match:
            waves.add(match.group(1))
    for entry in plan_dir.iterdir():
        if entry.is_dir() and re.match(r"^W\d{1,3}(?:\.\d+)?$", entry.name):
            waves.add(entry.name)
    return sorted(waves)


def invoke_validator(plan_dir: Path, wave: str, *, timeout: int = 60) -> dict[str, Any]:
    """Invoke validate_W<N>.py and parse its JSON envelope.

    Falls back to scanning data/ when the validator does not exist.
    """
    candidates = [
        plan_dir / wave / f"validate_{wave}.py",
        plan_dir / f"validate_{wave}.py",
    ]
    validator = next((c for c in candidates if c.exists()), None)

    if validator is None:
        return _infer_status_from_data(plan_dir, wave)

    try:
        result = subprocess.run(
            ["python3", str(validator), "-j"],
            capture_output=True, text=True, timeout=timeout, check=False,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as exc:
        return {"wave": wave, "status": "FAIL",
                "score": 0.0, "error": f"validator invocation failed: {exc}"}

    try:
        parsed = json.loads(result.stdout)
    except json.JSONDecodeError:
        parsed = {
            "wave": wave, "status": "FAIL",
            "score": 0.0, "error": "validator emitted non-JSON output",
        }

    return parsed


def _infer_status_from_data(plan_dir: Path, wave: str) -> dict[str, Any]:
    """If no validator exists, look at data/<wave>-*.json for PENDING detection."""
    data_dir = plan_dir / "data"
    if not data_dir.exists():
        return {"wave": wave, "status": "PENDING", "score": 0.0,
                "evidence_files": [], "missing_evidence": [],
                "note": "no validator + no data/ directory"}
    evidence = sorted(data_dir.glob(f"{wave}-*.json"))
    if not evidence:
        return {"wave": wave, "status": "PENDING", "score": 0.0,
                "evidence_files": [], "missing_evidence": [],
                "note": f"no data/{wave}-*.json files"}
    # Evidence exists but no validator — derive minimal score (0.5 = WARN)
    ok_count = 0
    for path in evidence:
        loaded = safe_load_json(path)
        if isinstance(loaded, dict) and loaded.get("status") == "OK":
            ok_count += 1
    score = ok_count / max(len(evidence), 1)
    status = "PASS" if score >= 0.8 else "WARN" if score >= 0.5 else "FAIL"
    return {
        "wave": wave, "status": status, "score": round(score, 3),
        "evidence_files": [str(p) for p in evidence],
        "missing_evidence": [],
        "note": "score inferred from data/ (no validator script present)",
    }


def composite_score(
    wave_results: dict[str, dict[str, Any]],
    weights: dict[str, float],
    *,
    mode: str,
) -> tuple[float, str]:
    """Compute the composite score + composite status."""
    counted: list[tuple[float, float]] = []  # (score, weight)
    for wave, result in wave_results.items():
        status = result.get("status", "PENDING")
        weight = weights.get(wave, 1.0)
        if mode == "baseline" and status == "PENDING":
            continue
        if status == "PENDING":
            counted.append((0.0, weight))
        else:
            counted.append((float(result.get("score", 0.0)), weight))

    if not counted:
        return 0.0, "BASELINE"

    total_weight = sum(w for _, w in counted)
    if total_weight == 0:
        return 0.0, "BASELINE"
    weighted_score = sum(s * w for s, w in counted) / total_weight

    composite_status = (
        "PASS" if weighted_score >= 0.8
        else "WARN" if weighted_score >= 0.5
        else "FAIL"
    )

    # Special case: all PENDING in baseline mode
    if mode == "baseline" and all(
        wave_results[w].get("status") == "PENDING" for w in wave_results
    ):
        return 0.0, "BASELINE"

    return round(weighted_score, 3), composite_status


def derive_recommendations(wave_results: dict[str, dict[str, Any]]) -> list[str]:
    """Derive operator-facing recommendations from the per-wave results."""
    recs: list[str] = []
    for wave, result in wave_results.items():
        status = result.get("status", "")
        score = float(result.get("score", 0.0))
        if status == "PENDING":
            recs.append(f"{wave}: PENDING. Per L6, re-measure premises before scaffolding.")
        elif status == "WARN":
            recs.append(f"{wave}: WARN (score={score:.2f}). Inspect findings — sub-scripts ran but quality is borderline.")
        elif status == "FAIL":
            recs.append(f"{wave}: FAIL (score={score:.2f}). Re-run forensic_runner with -v and check stderr.")
    return recs


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="cross_audit", description=__doc__)
    parser.add_argument("--plan-dir", type=Path, required=True,
                        help="Plan directory with W<N>/, data/, validators.")
    parser.add_argument("--plan", default="",
                        help="Plan name (defaults to plan_dir.name).")
    parser.add_argument("--baseline", action="store_true",
                        help="Baseline mode: PENDING waves excluded from composite.")
    parser.add_argument("--weights", type=Path, default=None,
                        help="Optional JSON file with per-wave weights {wave_id: float}.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (cross_audit is read-only).")
    parser.add_argument("--persist-lesson", action="store_true",
                        help="Persist composite status as a Touring memory lesson.")
    parser.add_argument("--output-dir", type=Path, default=Path("data"),
                        help="Where to emit cross_audit.json.")
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def _load_weights(path: Path | None) -> dict[str, float]:
    """Load optional weights JSON. Returns {} on missing/invalid."""
    if path is None or not path.exists():
        return {}
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
        return {k: float(v) for k, v in raw.items()}
    except (OSError, json.JSONDecodeError, ValueError):
        return {}


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Aggregate."""
    if not args.plan_dir.exists():
        msg = f"Plan directory not found: {args.plan_dir}"
        raise FileNotFoundError(msg)

    plan = args.plan or args.plan_dir.name
    waves = discover_waves(args.plan_dir)
    weights = _load_weights(args.weights)

    wave_results: dict[str, dict[str, Any]] = {}
    for wave in waves:
        wave_results[wave] = invoke_validator(args.plan_dir, wave)

    mode = "baseline" if args.baseline else "normal"
    score, composite_status = composite_score(wave_results, weights, mode=mode)

    summary: dict[str, int] = defaultdict(int)
    summary["total_waves"] = len(waves)
    for result in wave_results.values():
        summary[result.get("status", "PENDING").lower()] += 1

    missing_evidence = [
        item
        for result in wave_results.values()
        for item in result.get("missing_evidence", [])
    ]

    envelope = {
        "plan": plan,
        "mode": mode,
        "timestamp": utcnow_iso(),
        "composite_score": score,
        "composite_status": composite_status,
        "waves": wave_results,
        "missing_evidence": missing_evidence,
        "summary": dict(summary),
        "recommendations": derive_recommendations(wave_results),
    }

    if args.emit:
        out = args.output_dir / "cross_audit.json"
        write_json_atomic(out, envelope)
        envelope["json_path"] = str(out)

    if args.persist_lesson:
        touring_memory_store(
            f"taco-wt:cross-audit:{plan}",
            f"composite_status={composite_status} score={score} mode={mode}",
            tier="semantic",
        )

    return envelope


def _exit_code(composite_status: str) -> int:
    """Map composite status to a POSIX exit code."""
    if composite_status in {"PASS", "BASELINE"}:
        return EXIT_OK
    if composite_status == "WARN":
        return EXIT_WARN
    if composite_status == "FAIL":
        return 2
    return EXIT_FAIL


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
        return _exit_code(result.get("composite_status", "FAIL"))
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("cross_audit failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
