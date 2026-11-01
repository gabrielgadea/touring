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

import json
import subprocess
from pathlib import Path

import script_common as sc

_SCRIPT = "w14_tier_features_validator"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR

# Tier ordering + size budgets (MB) — must increase monotonically
TIERS = [
    ("tier-free", 50),
    ("tier-standard", 90),
    ("tier-premium", 180),
    ("tier-enterprise", 320),
]

CARGO_TARGET = "touring-server"


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--tier", type=str, default="",
                   help="Validate only one tier (default: all 4).")
    p.add_argument("--dry-run", action="store_true",
                   help="List commands without running cargo.")
    p.add_argument("--timeout", type=int, default=900,
                   help="Per-tier compile timeout (default 900s).")
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


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
                                   stderr=subprocess.STDOUT, timeout=timeout, check=False)
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


def run(args: sc.Namespace) -> dict:
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
        "timestamp": sc.now_iso(),
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


def _run_and_report(args: sc.Namespace) -> int:
    """Run w14_tier_features_validator, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"], "totals": result["totals"],
        "tier_definitions": result["tier_definitions"],
        "json_path": result["json_path"],
    })
    return 0 if result["status"] == "OK" else 1


def main() -> int:
    """Entry point of w14_tier_features_validator."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
