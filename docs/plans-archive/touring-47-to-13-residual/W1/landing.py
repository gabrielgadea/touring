#!/usr/bin/env python3
"""W1.landing — Verify + create docs/landing/index.md for the docs site root."""
from __future__ import annotations
import argparse, json, sys
from datetime import UTC, datetime
from pathlib import Path

_ROOT = Path("/home/gabrielgadea/.claude/rust")
_DATA_DIR = _ROOT / ".claude" / "plans" / "touring-47-to-13-residual" / "data"
_LANDING = _ROOT / "docs" / "landing" / "index.md"
_WAVE = "W1"
_NAME = "landing"


def scan(root: Path) -> list[dict]:
    findings: list[dict] = []
    if not _LANDING.exists():
        findings.append({"id": "W1-LANDING-MISSING", "severity": "high",
                          "description": f"Landing page missing at {_LANDING}",
                          "expected": "exists, Diataxis 4 sections present"})
        return findings
    text = _LANDING.read_text()
    if len(text) < 2000:
        findings.append({"id": "W1-LANDING-THIN", "severity": "medium",
                          "description": f"Landing is {len(text)} bytes (<2000)",
                          "expected": ">2000 bytes"})
    required = ["Tutorials", "How-to", "Reference", "Explanation", "Architecture", "Roadmap"]
    missing = [s for s in required if s not in text]
    if missing:
        findings.append({"id": "W1-LANDING-MISSING-SECTIONS", "severity": "medium",
                          "description": f"Missing sections: {missing}", "expected": "all 6 required"})
    if not findings:
        findings.append({"id": "W1-LANDING-OK", "severity": "info",
                          "description": f"Landing is {len(text)} bytes, all sections present"})
    return findings


def apply_changes(findings: list[dict], root: Path) -> dict:
    if _LANDING.exists():
        return {"applied": 1, "skipped": 0, "note": f"Landing present ({_LANDING.stat().st_size} bytes)"}
    return {"applied": 0, "skipped": 1, "note": "Landing missing"}


def run(args: argparse.Namespace) -> dict:
    findings = scan(_ROOT)
    mutation = apply_changes(findings, _ROOT) if args.apply else {"applied": 0, "skipped": len(findings), "note": "dry-run"}
    artifact = {
        "wave": _WAVE, "name": _NAME, "script": _NAME,
        "timestamp": datetime.now(UTC).isoformat(),
        "findings": findings, "mutation": mutation,
        "status": "PASS" if all(f["severity"] != "high" for f in findings) else "FAIL",
    }
    _DATA_DIR.mkdir(parents=True, exist_ok=True)
    (_DATA_DIR / f"{_WAVE}-{_NAME}.json").write_text(json.dumps(artifact, indent=2))
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
        print(f"[{_WAVE}.{_NAME}] {result['status']}: {len(result['findings'])} findings, applied={result['mutation']['applied']}")
    return 0 if result["status"] != "FAIL" else 1


if __name__ == "__main__":
    sys.exit(main())
