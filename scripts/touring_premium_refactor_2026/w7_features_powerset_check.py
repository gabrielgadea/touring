#!/usr/bin/env python3
"""W7.8 — Feature-powerset validation for touring-bindings + sensitive crates.

W7's most error-prone risk: Cargo features compose multiplicatively. A crate
with 6 features has 2^6 = 64 possible feature sets — each must compile.
Without enumeration, `cargo check --all-features` only validates the FULL
set, missing single-feature regressions.

Strategy:
  1. Discover crates with multiple features (via cargo_metadata)
  2. For each crate, generate feature combinations up to --max-cardinality
     (default 3 — i.e. 1-feature, 2-feature, 3-feature subsets)
  3. Run `cargo check -p <crate> --no-default-features --features <set>`
     for each combination
  4. Report failing combinations with their error output excerpts

Outputs:
  - data/w7-feature-powerset-report.json
  - staging/w7-failing-combinations.md

Usage
-----
    python3 w7_features_powerset_check.py                              # default scope
    python3 w7_features_powerset_check.py --crate touring-bindings     # narrow
    python3 w7_features_powerset_check.py --max-cardinality 2          # cheaper
    python3 w7_features_powerset_check.py --dry-run                    # list combinations only
"""
from __future__ import annotations

import argparse
import itertools
import json
import logging
import re
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

# Heuristic-extracted feature names from `[features]` section
FEATURE_RE = re.compile(r"^([a-zA-Z][a-zA-Z0-9_-]+)\s*=", re.MULTILINE)
FEATURE_SECTION_RE = re.compile(r"^\s*\[features\]\s*$", re.MULTILINE)

# Reserved Cargo features we don't enumerate
RESERVED = {"default", "_dev_only"}


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w7_features_powerset_check", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--crate", type=str, default="",
                   help="Limit check to one crate (default: all multi-feature crates).")
    p.add_argument("--max-cardinality", type=int, default=3,
                   help="Max features per combination (default: 3).")
    p.add_argument("--dry-run", action="store_true",
                   help="List combinations without running cargo check.")
    p.add_argument("--timeout", type=int, default=600,
                   help="Per-check cargo check timeout (default: 600s).")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def extract_features(cargo_path: Path) -> list[str]:
    """Parse Cargo.toml [features] section, return feature names."""
    if not cargo_path.exists():
        return []
    try:
        content = cargo_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    sect_match = FEATURE_SECTION_RE.search(content)
    if not sect_match:
        return []
    # Slice from [features] header to next section header or EOF
    start = sect_match.end()
    next_section = re.search(r"^\s*\[", content[start:], re.MULTILINE)
    end = start + next_section.start() if next_section else len(content)
    section = content[start:end]
    features = [m.group(1) for m in FEATURE_RE.finditer(section)]
    return [f for f in features if f not in RESERVED]


def discover_multi_feature_crates() -> dict[str, list[str]]:
    """Return {crate_name: [feature_list]} for crates with ≥ 2 features."""
    result: dict[str, list[str]] = {}
    for crate_dir in sorted(_CRATES_DIR.glob("touring-*")):
        features = extract_features(crate_dir / "Cargo.toml")
        if len(features) >= 2:
            result[crate_dir.name] = features
    return result


def generate_combinations(features: list[str],
                           max_cardinality: int) -> list[list[str]]:
    """Generate non-empty subsets up to max_cardinality."""
    combos: list[list[str]] = []
    for r in range(1, min(max_cardinality, len(features)) + 1):
        combos.extend(list(combo) for combo in itertools.combinations(features, r))
    return combos


def run_cargo_check(crate: str, feature_set: list[str],
                     timeout: int, dry_run: bool) -> dict:
    """Run cargo check -p crate --no-default-features --features ... ."""
    features_csv = ",".join(feature_set)
    cmd = ["cargo", "check", "-p", crate, "--no-default-features",
           "--features", features_csv]
    if dry_run:
        return {"command": " ".join(cmd), "status": "DRY_RUN",
                "features": feature_set, "crate": crate}
    try:
        proc = subprocess.run(cmd, cwd=_ROOT, capture_output=True,
                               timeout=timeout)
        return {
            "command": " ".join(cmd),
            "crate": crate, "features": feature_set,
            "exit_code": proc.returncode,
            "status": "OK" if proc.returncode == 0 else "FAIL",
            "stderr_excerpt": proc.stderr.decode("utf-8",
                                                  errors="replace")[:500] if proc.returncode else "",
        }
    except subprocess.TimeoutExpired:
        return {"command": " ".join(cmd), "crate": crate, "features": feature_set,
                "status": "TIMEOUT", "timeout_sec": timeout}
    except FileNotFoundError as e:
        return {"command": " ".join(cmd), "crate": crate, "features": feature_set,
                "status": "MISSING_TOOL", "error": str(e)}


def render_md(report: dict) -> str:
    """Render human-readable failing combinations report."""
    md = ["# W7.8 — Feature Powerset Validation Report", "",
          f"Generated: {report['timestamp']}",
          f"Total combinations: {report['totals']['total_combinations']}",
          f"OK: **{report['totals']['ok']}**",
          f"FAIL: **{report['totals']['fail']}**",
          f"TIMEOUT: {report['totals']['timeout']}",
          f"DRY_RUN: {report['totals']['dry_run']}",
          ""]
    failing = [r for r in report["results"] if r.get("status") == "FAIL"]
    if failing:
        md.extend(["## Failing combinations", "",
                   "| Crate | Features | Excerpt |",
                   "|-------|----------|---------|"])
        for f in failing[:30]:
            excerpt = f.get("stderr_excerpt", "")[:80].replace("|", "\\|").replace("\n", " ")
            features = ",".join(f.get("features", []))
            md.append(f"| `{f['crate']}` | `{features}` | {excerpt} |")
        if len(failing) > 30:
            md.append(f"\n_... {len(failing) - 30} more failures truncated._")
    else:
        md.append("✅ No failing combinations.")
    return "\n".join(md) + "\n"


def run(args: argparse.Namespace) -> dict:
    """Discover features + enumerate + check."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    if args.crate:
        features = extract_features(_CRATES_DIR / args.crate / "Cargo.toml")
        crates = {args.crate: features} if features else {}
    else:
        crates = discover_multi_feature_crates()
    all_results: list[dict] = []
    for crate, features in crates.items():
        combos = generate_combinations(features, args.max_cardinality)
        LOGGER.info("%s: %d features → %d combinations",
                     crate, len(features), len(combos))
        for combo in combos:
            result = run_cargo_check(crate, combo, args.timeout, args.dry_run)
            all_results.append(result)
    totals = {
        "crates_audited": len(crates),
        "total_combinations": len(all_results),
        "ok": sum(1 for r in all_results if r.get("status") == "OK"),
        "fail": sum(1 for r in all_results if r.get("status") == "FAIL"),
        "timeout": sum(1 for r in all_results if r.get("status") == "TIMEOUT"),
        "dry_run": sum(1 for r in all_results if r.get("status") == "DRY_RUN"),
        "missing_tool": sum(1 for r in all_results if r.get("status") == "MISSING_TOOL"),
    }
    report = {
        "script": "w7_features_powerset_check",
        "wave": "W7", "subtask_refs": ["W7.8"],
        "timestamp": datetime.now(UTC).isoformat(),
        "args": {"crate": args.crate, "max_cardinality": args.max_cardinality,
                  "dry_run": args.dry_run},
        "discovered_crates": {k: v for k, v in crates.items()},
        "totals": totals,
        "results": all_results,
    }
    json_path = args.output_dir / "w7-feature-powerset-report.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    md_path = _STAGING_DIR / "w7-failing-combinations.md"
    md_path.write_text(render_md(report), encoding="utf-8")
    return {**report, "status": "FAIL" if totals["fail"] else "OK",
             "json_path": str(json_path.relative_to(_ROOT)),
             "md_path": str(md_path.relative_to(_ROOT))}


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
            "discovered_crates": list(result["discovered_crates"].keys()),
            "json_path": result["json_path"],
            "md_path": result["md_path"],
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
