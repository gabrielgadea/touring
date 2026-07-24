#!/usr/bin/env python3
"""W11.3 — Audit mutation test kill rates per crate.

Measures TEST QUALITY (vs. coverage): runs cargo-mutants per crate and reports
kill rate (killed mutations / total). Reads cached results from
<ws>/.touring-cache/mutation-test/ to avoid re-running long jobs.

Usage
-----
    python3 w11_mutation_kill_rate_audit.py --cache-only
    python3 w11_mutation_kill_rate_audit.py --force
    python3 w11_mutation_kill_rate_audit.py --threshold 0.80
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CACHE_DIR = _ROOT / ".touring-cache" / "mutation-test"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

KILLED_RE = re.compile(r"caught:\s*(\d+)", re.I)
MISSED_RE = re.compile(r"missed:\s*(\d+)", re.I)
TIMEOUT_RE = re.compile(r"timeout:\s*(\d+)", re.I)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w11_mutation_kill_rate_audit", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--threshold", type=float, default=0.70)
    p.add_argument("--force", action="store_true")
    p.add_argument("--cache-only", action="store_true")
    p.add_argument("--crates", type=str, default="")
    p.add_argument("--timeout", type=int, default=1800)
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def parse_mutation_summary(output: str) -> dict:
    """Parse cargo-mutants summary output."""
    killed = int(m.group(1)) if (m := KILLED_RE.search(output)) else 0
    missed = int(m.group(1)) if (m := MISSED_RE.search(output)) else 0
    timeout = int(m.group(1)) if (m := TIMEOUT_RE.search(output)) else 0
    total = killed + missed + timeout
    return {"killed": killed, "missed": missed, "timeout": timeout,
             "total_mutations": total,
             "kill_rate": (killed / total) if total else 0.0}


def find_cached_result(crate: str) -> dict | None:
    """Look for cached result."""
    if not _CACHE_DIR.exists():
        return None
    cache_files = list(_CACHE_DIR.glob(f"{crate}-*.json"))
    if not cache_files:
        return None
    latest = max(cache_files, key=lambda p: p.stat().st_mtime)
    try:
        return json.loads(latest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def run_mutation_test(crate: str, timeout: int, force: bool) -> dict:
    """Dispatch `touring mutation-test`."""
    cmd = ["touring", "mutation-test", "--package", crate]
    if force:
        cmd.append("--force")
    try:
        proc = subprocess.run(cmd, cwd=_ROOT, capture_output=True,
                               timeout=timeout)
        if proc.returncode != 0:
            return {"crate": crate, "status": "FAIL",
                     "exit_code": proc.returncode}
        result = parse_mutation_summary(
            proc.stdout.decode("utf-8", errors="replace"))
        return {"crate": crate, "status": "OK", **result}
    except subprocess.TimeoutExpired:
        return {"crate": crate, "status": "TIMEOUT"}
    except FileNotFoundError:
        return {"crate": crate, "status": "MISSING_TOOL"}


def has_test_modules(crate: Path) -> bool:
    """Check for #[cfg(test)] or #[test]."""
    src = crate / "src"
    if not src.exists():
        return False
    for rs in src.rglob("*.rs"):
        try:
            content = rs.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "#[cfg(test)]" in content or "#[test]" in content:
            return True
    return False


def discover_crates() -> list[str]:
    """Find crates likely to have tests."""
    crates_dir = _ROOT / "crates"
    if not crates_dir.exists():
        return []
    return [c.name for c in sorted(crates_dir.glob("touring-*"))
            if (c / "src").exists() and has_test_modules(c)]


def run(args: argparse.Namespace) -> dict:
    """Audit kill rates."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    crates = (args.crates.split(",") if args.crates else discover_crates())
    results: list[dict] = []
    for crate in crates:
        cached = find_cached_result(crate)
        if cached and not args.force:
            results.append({"crate": crate, "source": "cache", **cached})
            continue
        if args.cache_only:
            results.append({"crate": crate, "status": "NO_CACHE",
                              "source": "cache_only_mode"})
            continue
        result = run_mutation_test(crate, args.timeout, args.force)
        result["source"] = "live"
        results.append(result)
    low_kill = [r for r in results
                 if "kill_rate" in r and r["kill_rate"] < args.threshold]
    report = {
        "script": "w11_mutation_kill_rate_audit",
        "wave": "W11", "subtask_refs": ["W11.3"],
        "timestamp": datetime.now(UTC).isoformat(),
        "threshold": args.threshold,
        "totals": {
            "crates_audited": len(crates),
            "crates_with_data": sum(1 for r in results if "kill_rate" in r),
            "crates_below_threshold": len(low_kill),
            "missing_tool": sum(1 for r in results if r.get("status") == "MISSING_TOOL"),
            "no_cache": sum(1 for r in results if r.get("status") == "NO_CACHE"),
        },
        "results": results,
        "low_kill_rate_crates": low_kill,
    }
    json_path = args.output_dir / "w11-mutation-audit.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    return {**report, "status": "OK",
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
            "low_kill_top_3": [{"crate": r["crate"],
                                  "kill_rate": r.get("kill_rate")}
                                 for r in result["low_kill_rate_crates"][:3]],
            "json_path": result["json_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
