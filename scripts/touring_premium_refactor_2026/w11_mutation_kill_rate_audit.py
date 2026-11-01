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

import json
import re
import subprocess
from pathlib import Path

import script_common as sc

_SCRIPT = "w11_mutation_kill_rate_audit"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR
_CACHE_DIR = _ROOT / ".touring-cache" / "mutation-test"

KILLED_RE = re.compile(r"caught:\s*(\d+)", re.IGNORECASE)
MISSED_RE = re.compile(r"missed:\s*(\d+)", re.IGNORECASE)
TIMEOUT_RE = re.compile(r"timeout:\s*(\d+)", re.IGNORECASE)


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--threshold", type=float, default=0.70)
    p.add_argument("--force", action="store_true")
    p.add_argument("--cache-only", action="store_true")
    p.add_argument("--crates", type=str, default="")
    p.add_argument("--timeout", type=int, default=1800)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


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
                               timeout=timeout, check=False)
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


def run(args: sc.Namespace) -> dict:
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
        "timestamp": sc.now_iso(),
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


def _run_and_report(args: sc.Namespace) -> int:
    """Run w11_mutation_kill_rate_audit, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"], "totals": result["totals"],
        "low_kill_top_3": [{"crate": r["crate"],
                              "kill_rate": r.get("kill_rate")}
                             for r in result["low_kill_rate_crates"][:3]],
        "json_path": result["json_path"],
    })
    return 0


def main() -> int:
    """Entry point of w11_mutation_kill_rate_audit."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
