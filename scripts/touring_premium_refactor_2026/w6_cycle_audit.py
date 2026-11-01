#!/usr/bin/env python3
"""W6.x — Verify macrociclo 618 ELIMINATED post intelligence fusion.

Runs `touring wiring cycles --min-depth 2` (current state) and compares with
baseline from docs/baselines/cycles-pre-refactor-*.json. Reports:
  - Eliminated cycles (in baseline, gone in current) ✅
  - Persistent cycles (in both) ⚠️
  - New cycles (introduced by refactor) ❌

Outputs:
  - data/w6-cycle-audit.json
  - staging/w6-cycle-comparison.md

Usage
-----
    python3 w6_cycle_audit.py
    python3 w6_cycle_audit.py --baseline docs/baselines/cycles-pre-refactor-2026-05-11.json
"""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

import script_common as sc

_SCRIPT = "w6_cycle_audit"

_ROOT, _DATA_DIR, _STAGING_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR
_BASELINES_DIR = _ROOT / "docs" / "baselines"


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--baseline", type=Path, default=None,
                   help="Path to cycles-pre-refactor-*.json (auto-detected if missing).")
    p.add_argument("--min-depth", type=int, default=2)
    p.add_argument("--timeout", type=int, default=120)
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def auto_detect_baseline() -> Path | None:
    """Find newest cycles-pre-refactor-*.json in docs/baselines/."""
    candidates = sorted(_BASELINES_DIR.glob("cycles-pre-refactor-*.json"))
    return candidates[-1] if candidates else None


def fetch_current_cycles(min_depth: int, timeout: int) -> dict:
    """Run touring wiring cycles."""
    try:
        proc = subprocess.run(
            ["touring", "wiring", "cycles", "--min-depth", str(min_depth),
             "--format", "json"],
            cwd=_ROOT, capture_output=True, timeout=timeout, check=False,
        )
        if proc.returncode != 0:
            return {"error": "touring exited non-zero",
                    "stderr": proc.stderr.decode("utf-8",
                                                   errors="replace")[:200]}
        return json.loads(proc.stdout.decode("utf-8", errors="replace"))
    except subprocess.TimeoutExpired:
        return {"error": "TIMEOUT"}
    except FileNotFoundError:
        return {"error": "MISSING_TOOL"}
    except json.JSONDecodeError as e:
        return {"error": f"invalid JSON: {e}"}


def normalize_cycle(cycle: dict | list) -> str:
    """Convert cycle to canonical string key (path joined)."""
    if isinstance(cycle, list):
        return " → ".join(str(x) for x in cycle)
    if isinstance(cycle, dict):
        path = cycle.get("path", cycle.get("cycle", []))
        return normalize_cycle(path)
    return str(cycle)


def compare_cycles(baseline: dict, current: dict) -> dict:
    """Compute eliminated/persistent/new cycles."""
    b_cycles_raw = baseline.get("cycles", [])
    c_cycles_raw = current.get("cycles", [])
    b_keys = {normalize_cycle(c) for c in b_cycles_raw}
    c_keys = {normalize_cycle(c) for c in c_cycles_raw}
    eliminated = sorted(b_keys - c_keys)
    persistent = sorted(b_keys & c_keys)
    new = sorted(c_keys - b_keys)
    return {
        "baseline_count": len(b_keys),
        "current_count": len(c_keys),
        "eliminated_count": len(eliminated),
        "persistent_count": len(persistent),
        "new_count": len(new),
        "eliminated": eliminated[:20],
        "persistent": persistent[:20],
        "new": new[:20],
    }


def render_md(comparison: dict) -> str:
    """Render markdown comparison."""
    md = ["# W6 — Cycle Audit (post intelligence fusion)", "",
          f"Baseline cycles: {comparison['baseline_count']}",
          f"Current cycles:  {comparison['current_count']}",
          f"**Eliminated**:  {comparison['eliminated_count']} ✅",
          f"**Persistent**:  {comparison['persistent_count']} ⚠️",
          f"**New (intro by refactor)**: {comparison['new_count']} ❌", ""]
    if comparison["new"]:
        md.extend(["## NEW cycles introduced by refactor (BLOCKING)", ""])
        for c in comparison["new"][:10]:
            md.append(f"- `{c}`")
    if comparison["persistent"]:
        md.extend(["", "## Persistent cycles (need separate fix)", ""])
        for c in comparison["persistent"][:10]:
            md.append(f"- `{c}`")
    return "\n".join(md) + "\n"


def run(args: sc.Namespace) -> dict:
    """Run audit."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    baseline_path = args.baseline or auto_detect_baseline()
    if not baseline_path or not baseline_path.exists():
        return {"status": "BASELINE_MISSING",
                "hint": "Run w0_capture_baselines.py first"}
    try:
        baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as e:
        return {"status": "BASELINE_INVALID", "error": str(e)}
    current = fetch_current_cycles(args.min_depth, args.timeout)
    if "error" in current:
        return {"status": "CURRENT_FETCH_FAILED", "error": current["error"]}
    comparison = compare_cycles(baseline, current)
    report = {
        "script": "w6_cycle_audit",
        "wave": "W6", "subtask_refs": ["W6.cycle-audit"],
        "timestamp": sc.now_iso(),
        "baseline_path": str(baseline_path.relative_to(_ROOT)),
        "comparison": comparison,
    }
    json_path = args.output_dir / "w6-cycle-audit.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    md_path = _STAGING_DIR / "w6-cycle-comparison.md"
    md_path.write_text(render_md(comparison), encoding="utf-8")
    # FAIL if new cycles introduced
    status = "FAIL" if comparison["new_count"] > 0 else "OK"
    return {**report, "status": status,
             "json_path": str(json_path.relative_to(_ROOT)),
             "md_path": str(md_path.relative_to(_ROOT))}


def _run_and_report(args: sc.Namespace) -> int:
    """Run w6_cycle_audit, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json(result)
    return 0 if result.get("status") == "OK" else 1


def main() -> int:
    """Entry point of w6_cycle_audit."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
