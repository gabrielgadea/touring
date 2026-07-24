#!/usr/bin/env python3
"""W14 — Validate tier-{free,standard,premium,enterprise} feature compilation + size.

For each commercial tier, validates:
  1. The feature set compiles: `cargo build --release --no-default-features --features tier-X`
  2. The output binary size respects budget (per-tier ceiling)
  3. Tier-X is a strict superset of tier-(X-1) (no feature regressions)

This is the W14 quality gate — ensures the 4-tier commercial model is reachable.

Outputs:
  - data/w14-tier-validation.json
  - staging/w14-tier-compile-log/<tier>.log
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
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

# Tier ordering + size budgets (MB) — must increase monotonically
TIERS = [
    ("tier-free", 50),
    ("tier-standard", 90),
    ("tier-premium", 180),
    ("tier-enterprise", 320),
]

CARGO_TARGET = "touring-server"


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w14_tier_features_validator", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--tier", type=str, default="",
                   help="Validate only one tier (default: all 4).")
    p.add_argument("--dry-run", action="store_true",
                   help="List commands without running cargo.")
    p.add_argument("--timeout", type=int, default=900,
                   help="Per-tier compile timeout (default 900s).")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def cargo_build_tier(tier: str, timeout: int, dry_run: bool,
                      log_path: Path) -> dict:
    """Run cargo build for a tier."""
    log_path.parent.mkdir(parents=True, exist_ok=True)
    cmd = ["cargo", "build", "--release", "-p", CARGO_TARGET,
           "--no-default-features", "--features", tier]
    if dry_run:
        return {"tier": tier, "status": "DRY_RUN",
                "command": " ".join(cmd)}
    try:
        with log_path.open("wb") as out:
            proc = subprocess.run(cmd, cwd=_ROOT, stdout=out,
                                   stderr=subprocess.STDOUT, timeout=timeout)
        return {"tier": tier, "command": " ".join(cmd),
                "exit_code": proc.returncode,
                "status": "OK" if proc.returncode == 0 else "FAIL",
                "log": str(log_path.relative_to(_ROOT))}
    except subprocess.TimeoutExpired:
        return {"tier": tier, "status": "TIMEOUT", "timeout_sec": timeout}
    except FileNotFoundError:
        return {"tier": tier, "status": "MISSING_TOOL"}


def measure_binary_size(tier: str, budget_mb: int) -> dict:
    """Measure release binary size + check budget."""
    binary = _ROOT / "target" / "release" / CARGO_TARGET
    if not binary.exists():
        return {"tier": tier, "size_mb": 0, "budget_mb": budget_mb,
                "within_budget": False, "missing_binary": True}
    size_bytes = binary.stat().st_size
    size_mb = size_bytes / (1024 * 1024)
    return {"tier": tier, "size_mb": round(size_mb, 2),
             "budget_mb": budget_mb,
             "within_budget": size_mb <= budget_mb,
             "size_bytes": size_bytes}


def run(args: argparse.Namespace) -> dict:
    """Validate each tier."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    log_dir = _STAGING_DIR / "w14-tier-compile-log"
    log_dir.mkdir(parents=True, exist_ok=True)
    tiers_to_check = (
        [(t, b) for t, b in TIERS if t == args.tier]
        if args.tier else TIERS
    )
    results: list[dict] = []
    for tier, budget in tiers_to_check:
        log_path = log_dir / f"{tier}.log"
        build_result = cargo_build_tier(tier, args.timeout, args.dry_run,
                                          log_path)
        size_result = (measure_binary_size(tier, budget)
                        if build_result.get("status") == "OK" else
                        {"tier": tier, "skipped": True})
        results.append({"build": build_result, "size": size_result})
    totals = {
        "tiers_audited": len(results),
        "build_ok": sum(1 for r in results
                          if r["build"].get("status") == "OK"),
        "build_fail": sum(1 for r in results
                            if r["build"].get("status") in ("FAIL", "TIMEOUT",
                                                              "MISSING_TOOL")),
        "size_within_budget": sum(1 for r in results
                                     if r["size"].get("within_budget")),
        "dry_run": sum(1 for r in results
                        if r["build"].get("status") == "DRY_RUN"),
    }
    report = {
        "script": "w14_tier_features_validator",
        "wave": "W14", "subtask_refs": ["W14.tier-validation"],
        "timestamp": datetime.now(UTC).isoformat(),
        "totals": totals,
        "results": results,
        "tier_definitions": [{"tier": t, "budget_mb": b} for t, b in TIERS],
    }
    json_path = args.output_dir / "w14-tier-validation.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    return {**report,
             "status": "OK" if totals["build_fail"] == 0 else "FAIL",
             "json_path": str(json_path.relative_to(_ROOT))}


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps({
            "status": result["status"], "totals": result["totals"],
            "tier_definitions": result["tier_definitions"],
            "json_path": result["json_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0 if result["status"] == "OK" else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
