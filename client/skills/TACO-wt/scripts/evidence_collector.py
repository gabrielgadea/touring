#!/usr/bin/env python3
"""evidence_collector — Verify integrity and completeness of wave artifacts.

For a given plan directory:
  * Enumerate ``data/<wave>-*.json`` artifacts.
  * Verify each JSON is parseable and has the standard envelope.
  * Cross-reference against declared sub-scripts (from plan.md if provided).
  * Report missing evidence + integrity violations.

Usage
-----
    python3 evidence_collector.py --plan-dir scripts/<plan>
    python3 evidence_collector.py --plan-dir scripts/<plan> --plan plan.md --strict
    python3 evidence_collector.py --plan-dir scripts/<plan> -j --emit
"""

from __future__ import annotations

import argparse
import json
import logging
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
    utcnow_iso,
    write_json_atomic,
)

_REQUIRED_ENVELOPE_KEYS = {"script", "wave", "timestamp", "status"}


def collect_artifacts(plan_dir: Path) -> list[dict[str, Any]]:
    """Enumerate every ``data/<wave>-*.json`` in the plan directory."""
    data_dir = plan_dir / "data"
    if not data_dir.exists():
        return []
    return [
        {"path": p, "loaded": safe_load_json(p)}
        for p in sorted(data_dir.glob("*.json"))
    ]


def verify_envelope(artifact: dict[str, Any]) -> list[str]:
    """Return a list of envelope violations (empty when clean)."""
    violations: list[str] = []
    loaded = artifact.get("loaded")
    path = artifact["path"]
    if loaded is None:
        violations.append(f"{path.name}: not parseable as JSON")
        return violations
    if not isinstance(loaded, dict):
        violations.append(f"{path.name}: top-level is not an object")
        return violations
    missing = _REQUIRED_ENVELOPE_KEYS - set(loaded.keys())
    for key in sorted(missing):
        violations.append(f"{path.name}: missing required envelope key '{key}'")
    return violations


def group_by_wave(artifacts: list[dict[str, Any]]) -> dict[str, list[dict[str, Any]]]:
    """Group artifacts by their wave id (from the JSON envelope, fall back to filename)."""
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for artifact in artifacts:
        wave = ""
        loaded = artifact.get("loaded")
        if isinstance(loaded, dict):
            wave = str(loaded.get("wave", ""))
        if not wave:
            # Fallback: parse from filename prefix "W<N>-..."
            stem = artifact["path"].stem
            if stem.startswith("W") and "-" in stem:
                wave = stem.split("-", 1)[0]
        if wave:
            grouped[wave].append(artifact)
    return dict(grouped)


def detect_missing_evidence(
    plan_dir: Path,
    expected_subs_by_wave: dict[str, list[str]] | None,
) -> list[dict[str, str]]:
    """Find expected sub-script JSONs that are absent from data/."""
    if not expected_subs_by_wave:
        return []
    missing: list[dict[str, str]] = []
    data_dir = plan_dir / "data"
    for wave, subs in expected_subs_by_wave.items():
        for sub in subs:
            sub_clean = sub.replace(".py", "").strip()
            if not sub_clean:
                continue
            expected = data_dir / f"{wave}-{sub_clean}.json"
            if not expected.exists():
                missing.append({
                    "wave": wave,
                    "sub_script": sub_clean,
                    "expected_path": str(expected),
                })
    return missing


def _parse_expected_subs_from_plan(plan_path: Path) -> dict[str, list[str]]:
    """Extract per-wave declared sub-scripts from a plan markdown."""
    if not plan_path.exists():
        return {}
    text = plan_path.read_text(encoding="utf-8")
    import re
    re_wave = re.compile(r"^###\s+(W\d{1,3}(?:\.\d+)?)\s+[—\-:]", re.MULTILINE)
    re_subs = re.compile(r"\|\s*Sub-scripts\s*\|\s*([^|]+?)\s*\|", re.IGNORECASE)
    waves: dict[str, list[str]] = {}
    matches = list(re_wave.finditer(text))
    for idx, match in enumerate(matches):
        wave_id = match.group(1)
        start = match.start()
        end = matches[idx + 1].start() if idx + 1 < len(matches) else len(text)
        body = text[start:end]
        sub_match = re_subs.search(body)
        if sub_match:
            sub_raw = re.sub(r"[`*]", "", sub_match.group(1).strip())
            waves[wave_id] = [s.strip() for s in sub_raw.split(",") if s.strip()]
    return waves


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="evidence_collector", description=__doc__)
    parser.add_argument("--plan-dir", type=Path, required=True,
                        help="Plan directory with data/, W<N>/, etc.")
    parser.add_argument("--plan", type=Path, default=None,
                        help="Plan markdown (used to expect specific sub-scripts).")
    parser.add_argument("--strict", action="store_true",
                        help="Exit non-zero when there's missing evidence.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (collector is read-only).")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("--emit", action="store_true",
                        help="Write data/evidence_collection.json.")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Collect + verify."""
    if not args.plan_dir.exists():
        msg = f"Plan directory not found: {args.plan_dir}"
        raise FileNotFoundError(msg)

    artifacts = collect_artifacts(args.plan_dir)
    violations: list[str] = []
    for art in artifacts:
        violations.extend(verify_envelope(art))

    grouped = group_by_wave(artifacts)

    expected = _parse_expected_subs_from_plan(args.plan) if args.plan else None
    missing = detect_missing_evidence(args.plan_dir, expected)

    severity_status = "OK"
    if violations or missing:
        severity_status = "WARN" if not args.strict else "FAIL"

    report = {
        "status": severity_status,
        "script": "evidence_collector",
        "timestamp": utcnow_iso(),
        "plan_dir": str(args.plan_dir),
        "artifacts_total": len(artifacts),
        "waves_with_evidence": sorted(grouped.keys()),
        "wave_artifact_count": {w: len(items) for w, items in grouped.items()},
        "envelope_violations": violations,
        "missing_evidence": missing,
    }

    if args.emit:
        out = args.output_dir / "evidence_collection.json"
        write_json_atomic(out, report)
        report["json_path"] = str(out)

    return report


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
        if result["status"] == "FAIL":
            return EXIT_WARN
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("evidence_collector failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
