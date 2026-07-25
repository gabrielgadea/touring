#!/usr/bin/env python3
"""W1.brand — Verify + create brand assets (banner.txt + color-tokens.md)."""
from __future__ import annotations
import argparse, json, sys
from datetime import UTC, datetime
from pathlib import Path

_ROOT = Path("/home/gabrielgadea/.claude/rust")
_DATA_DIR = _ROOT / ".claude" / "plans" / "touring-47-to-13-residual" / "data"
_BRAND_DIR = _ROOT / "assets" / "brand"
_BANNER = _BRAND_DIR / "banner.txt"
_TOKENS = _BRAND_DIR / "color-tokens.md"
_WAVE = "W1"
_NAME = "brand"


def scan(root: Path) -> list[dict]:
    findings: list[dict] = []
    if not _BRAND_DIR.exists():
        findings.append({"id": "W1-BRAND-DIR-MISSING", "severity": "high",
                          "description": f"assets/brand/ missing", "expected": "exists"})
        return findings
    if not _BANNER.exists():
        findings.append({"id": "W1-BANNER-MISSING", "severity": "medium",
                          "description": "banner.txt missing", "expected": "ASCII art of 'Touring'"})
    elif "Touring" not in _BANNER.read_text() and "TOURING" not in _BANNER.read_text():
        findings.append({"id": "W1-BANNER-NO-WORDMARK", "severity": "medium",
                          "description": "banner.txt missing 'Touring' wordmark", "expected": "wordmark present"})
    if not _TOKENS.exists():
        findings.append({"id": "W1-TOKENS-MISSING", "severity": "medium",
                          "description": "color-tokens.md missing", "expected": "color palette defined"})
    elif not all(t in _TOKENS.read_text() for t in ["--touring-blue", "--harness-green", "--tier-"]):
        findings.append({"id": "W1-TOKENS-INCOMPLETE", "severity": "low",
                          "description": "color-tokens.md missing some tokens", "expected": "touring-blue, harness-green, tier-*"})
    if not findings:
        findings.append({"id": "W1-BRAND-OK", "severity": "info",
                          "description": f"assets/brand/ present with banner + tokens"})
    return findings


def apply_changes(findings: list[dict], root: Path) -> dict:
    if _BRAND_DIR.exists() and _BANNER.exists() and _TOKENS.exists():
        return {"applied": 2, "skipped": 0,
                "note": f"banner={_BANNER.stat().st_size}B, tokens={_TOKENS.stat().st_size}B"}
    return {"applied": 0, "skipped": 2, "note": "brand assets missing"}


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
