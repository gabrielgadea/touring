#!/usr/bin/env python3
"""validate_W15.py — W15 wave validator.

Aggregates audit.json + execute.json → composite score.

Score policy:
  P0 FAIL  → -0.25 per finding
  P1 WARN  → -0.10 per finding
  P2 INFO  → -0.02 per finding
  PASS threshold: >= 0.80
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[1]
_DATA_DIR = _ROOT / "data"
_WAVE = "W15"


def _load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def _compute_score(findings: list[dict]) -> float:
    deductions = {"P0": 0.25, "P1": 0.10, "P2": 0.02, "PASS": 0.0}
    total = 1.0
    for f in findings:
        sev = f.get("severity", "P2")
        total -= deductions.get(sev, 0.02)
    return max(0.0, total)


def run() -> dict:
    audit_path = _DATA_DIR / f"{_WAVE}-audit.json"
    execute_path = _DATA_DIR / f"{_WAVE}-execute.json"

    audit = _load_json(audit_path) if audit_path.exists() else None
    execute = _load_json(execute_path) if execute_path.exists() else None

    findings = []
    if audit:
        findings.extend(audit.get("findings", []))
    if execute:
        findings.extend(execute.get("findings", []))

    score = _compute_score(findings)

    status = "PASS" if score >= 0.80 else "FAIL"
    auditor_evidence = []
    if audit:
        auditor_evidence.append(f"audit: {audit.get('status', 'unknown')}")
    if execute:
        auditor_evidence.append(f"execute: {execute.get('status', 'unknown')}")

    result = {
        "wave": _WAVE,
        "status": status,
        "score": score,
        "findings_total": len(findings),
        "auditor_evidence": auditor_evidence,
        "data_files": {
            "audit": str(audit_path.relative_to(_ROOT)) if audit_path.exists() else None,
            "execute": str(execute_path.relative_to(_ROOT)) if execute_path.exists() else None,
        },
    }
    print(json.dumps(result, indent=2))
    return result


if __name__ == "__main__":
    sys.exit(0 if run()["status"] == "PASS" else 1)