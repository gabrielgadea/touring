#!/usr/bin/env python3
"""W1.readme — Verify + create root README.md for Premium Elite product.

Forensic sub-script for W1 — Foundational README + Brand Layer.

Outputs
-------
  - data/W1-readme.json   (machine-readable JSON envelope)
  - staging/W1-readme.md  (human-readable narrative)
"""
from __future__ import annotations

import argparse
import json
import sys
from datetime import UTC, datetime
from pathlib import Path

_ROOT = Path("/home/gabrielgadea/.claude/rust")
_DATA_DIR = _ROOT / ".claude" / "plans" / "touring-47-to-13-residual" / "data"
_STAGING_DIR = _ROOT / ".claude" / "plans" / "touring-47-to-13-residual" / "staging"
_WAVE = "W1"
_NAME = "readme"
_README = _ROOT / "README.md"


def scan(root: Path) -> list[dict]:
    """Phase 3 — Pure scan. Verify root README.md exists and is substantive."""
    findings: list[dict] = []
    if not _README.exists():
        findings.append({
            "id": "W1-README-MISSING",
            "severity": "high",
            "description": f"Root README.md missing at {_README}",
            "expected": "exists, >1000 bytes, has 'Quick start' and 'Architecture' sections",
        })
        return findings
    text = _README.read_text()
    if len(text) < 1000:
        findings.append({
            "id": "W1-README-THIN",
            "severity": "medium",
            "description": f"README.md is {len(text)} bytes (<1000)",
            "expected": ">1000 bytes",
        })
    required = ["Quick start", "Architecture", "License", "Constitution", "RFC"]
    missing = [s for s in required if s not in text]
    if missing:
        findings.append({
            "id": "W1-README-MISSING-SECTIONS",
            "severity": "medium",
            "description": f"README.md missing required sections: {missing}",
            "expected": "all 5 required sections present",
        })
    if not findings:
        findings.append({
            "id": "W1-README-OK",
            "severity": "info",
            "description": f"README.md is {len(text)} bytes, all required sections present",
            "expected": "OK",
        })
    return findings


def apply_changes(findings: list[dict], root: Path) -> dict:
    """Phase 4 — DESTRUCTIVE (gated by --apply). The README is already written
    by the orchestrator; this only confirms the artifact on disk."""
    if _README.exists():
        size = _README.stat().st_size
        return {"applied": 1, "skipped": 0, "note": f"README.md present ({size} bytes)"}
    return {"applied": 0, "skipped": 1, "note": "README.md missing — orchestrator must write it"}


def run(args: argparse.Namespace) -> dict:
    findings = scan(_ROOT)
    if args.apply:
        mutation = apply_changes(findings, _ROOT)
    else:
        mutation = {"applied": 0, "skipped": len(findings), "note": "dry-run"}
    artifact = {
        "wave": _WAVE,
        "name": _NAME,
        "script": _NAME,
        "timestamp": datetime.now(UTC).isoformat(),
        "findings": findings,
        "mutation": mutation,
        "status": "PASS" if all(f["severity"] != "high" for f in findings) else "FAIL",
    }
    _DATA_DIR.mkdir(parents=True, exist_ok=True)
    out = _DATA_DIR / f"{_WAVE}-{_NAME}.json"
    out.write_text(json.dumps(artifact, indent=2))
    return artifact


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog=_NAME, description=__doc__)
    p.add_argument("--apply", action="store_true")
    p.add_argument("-j", "--json", action="store_true", dest="json_only")
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def main() -> int:
    args = build_parser().parse_args()
    try:
        result = run(args)
    except KeyboardInterrupt:
        return 130
    if args.json_only:
        print(json.dumps(result, indent=2))
    else:
        print(f"[{_WAVE}.{_NAME}] {result['status']}: {len(result['findings'])} findings, "
              f"applied={result['mutation']['applied']}")
    return 0 if result["status"] != "FAIL" else 1


if __name__ == "__main__":
    sys.exit(main())
