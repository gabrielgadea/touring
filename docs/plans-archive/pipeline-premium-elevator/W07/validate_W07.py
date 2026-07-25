#!/usr/bin/env python3
"""validate_W07 — Per-wave validator for pipeline-premium-elevator.

Reads ``data/W07-*.json`` artifacts produced by the wave's sub-scripts
and returns a status/score envelope consumed by ``cross_audit.py``.

This validator is read-only — ``--apply`` is a no-op.

Status taxonomy
---------------
  PASS    score >= 0.8 — wave fully delivers its declared outcomes
  WARN    0.5 <= score < 0.8 — partial; review findings before merge
  FAIL    score <  0.5 — wave did not deliver
  PENDING no sub-script JSON found in ``data/``

Usage
-----
    python3 validate_W07.py            # default — table output
    python3 validate_W07.py -j         # machine-readable JSON
    python3 validate_W07.py --strict   # exit 1 on PENDING (CI mode)
"""

from __future__ import annotations

import argparse
import json
import logging
import sys
from datetime import UTC, datetime
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[2]
_DATA_DIR = _ROOT / "scripts/pipeline-premium-elevator/data"
_WAVE = "W07"

# Expected sub-script names (without ``.py`` extension). Used to detect missing evidence.
EXPECTED_SUBS: tuple[str, ...] = (    "audit",    "execute",)

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog=f"validate_{_WAVE}", description=__doc__)
    parser.add_argument("--apply", action="store_true", help="No-op for validators.")
    parser.add_argument("--data-dir", type=Path, default=_DATA_DIR)
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("--strict", action="store_true",
                        help="Exit non-zero on PENDING (CI mode).")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def _load_sub_reports(data_dir: Path) -> list[dict]:
    """Load every ``<wave>-*.json`` artifact for this wave."""
    if not data_dir.exists():
        return []
    reports: list[dict] = []
    for path in sorted(data_dir.glob(f"{_WAVE}-*.json")):
        try:
            reports.append(json.loads(path.read_text(encoding="utf-8")))
        except (OSError, json.JSONDecodeError):
            logging.getLogger(__name__).warning("Skipping unreadable artifact: %s", path)
    return reports


def _missing_evidence(reports: list[dict]) -> list[str]:
    """Sub-scripts that are EXPECTED but did not produce JSON."""
    if not EXPECTED_SUBS:
        return []
    observed = {r.get("script", "") for r in reports}
    return [s for s in EXPECTED_SUBS if s not in observed]


def _compute_score(reports: list[dict]) -> float:
    """Compose a 0.0..1.0 score from sub-script outcomes.

    Default policy:
      - Each sub with ``status="OK"`` contributes 1.0.
      - Findings of severity P0 each subtract 0.25 from that sub's contribution.
      - Findings P1 subtract 0.1; P2 subtract 0.02; P3 subtract 0.0.
      - Sub-scripts with non-OK status contribute 0.0.
      - Score is the bounded mean across sub-scripts.

    Override this function in a per-wave validator when richer scoring is needed.
    """
    if not reports:
        return 0.0
    weights = {"P0": 0.25, "P1": 0.10, "P2": 0.02, "P3": 0.0}
    sub_scores: list[float] = []
    for report in reports:
        if report.get("status") != "OK":
            sub_scores.append(0.0)
            continue
        score = 1.0
        for finding in report.get("findings", []) or []:
            score -= weights.get(finding.get("severity", "P2"), 0.02)
        sub_scores.append(max(0.0, score))
    return round(sum(sub_scores) / len(sub_scores), 3)


def _status_from_score(score: float, has_evidence: bool) -> str:
    """Map (score, has_evidence) → status string."""
    if not has_evidence:
        return "PENDING"
    if score >= 0.8:
        return "PASS"
    if score >= 0.5:
        return "WARN"
    return "FAIL"


def run(args: argparse.Namespace) -> dict:
    """Aggregate ``data/{_WAVE}-*.json`` into a status envelope."""
    reports = _load_sub_reports(args.data_dir)
    missing = _missing_evidence(reports)
    score = _compute_score(reports)
    status = _status_from_score(score, has_evidence=bool(reports))

    return {
        "status": status,
        "score": score,
        "wave": _WAVE,
        "evidence_files": [
            str((args.data_dir / f"{r['script']}.json").relative_to(_ROOT)
                if (args.data_dir / f"{r['script']}.json").exists()
                else (args.data_dir / f"{_WAVE}-{r['script']}.json"))
            for r in reports
            if r.get("script")
        ],
        "missing_evidence": missing,
        "child_results": {
            r.get("script", "?"): r.get("status", "?")
            for r in reports
        },
        "timestamp": datetime.now(UTC).isoformat(),
    }


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
        if args.strict and result["status"] == "PENDING":
            return _EXIT_FAIL
        return _EXIT_OK if result["status"] in {"PASS", "PENDING"} else _EXIT_FAIL
    except KeyboardInterrupt:
        return _EXIT_INTERRUPTED
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("validator failed")
        return _EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
