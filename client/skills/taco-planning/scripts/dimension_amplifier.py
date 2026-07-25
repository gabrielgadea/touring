#!/usr/bin/env python3
"""dimension_amplifier — Emit concrete amplifications for dimensions below threshold.

Reads the dimension_scorer report and, for each dimension < threshold (default
7.0), emits one or more amplification actions drawn from the strategy catalog
in references/amplification-strategies.md.

The output is a structured action list the author applies to lift the plan
from Pln1 → Pln2.

Usage
-----
    python3 dimension_amplifier.py plan.md --threshold 7
    python3 dimension_amplifier.py plan.md --threshold 7 --ground-truth data/ground_truth.json
    python3 dimension_amplifier.py plan.md --emit -j
"""

from __future__ import annotations

import argparse
import json
import logging
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from lib import (  # noqa: E402  pylint: disable=wrong-import-position
    EXIT_FAIL,
    EXIT_INTERRUPTED,
    EXIT_OK,
    EXIT_STRUCTURAL,
    safe_load_json,
    utcnow_iso,
    write_json_atomic,
)

_STRATEGIES: dict[str, list[dict[str, str]]] = {
    "precision": [
        {"id": "P-1", "strategy": "Replace prose with file:LINE",
         "action": "For each prose location ('around line N', 'the auth module'), run `touring ast find <symbol>` and embed exact `file:LINE — signature`."},
        {"id": "P-2", "strategy": "Add numeric targets",
         "action": "Convert every adjective ('fast', 'low memory') to a number with a unit (P99 < 50ms; <20MB RSS)."},
    ],
    "scalability": [
        {"id": "S-1", "strategy": "Pattern over special-case",
         "action": "Replace `if tenant == X` with `TenantPolicy` trait + registry."},
        {"id": "S-2", "strategy": "Document horizontal-scaling strategy",
         "action": "Add: 'X horizontally scales via Y; concurrency control via Z.'"},
    ],
    "performance": [
        {"id": "Pf-1", "strategy": "Target + workload + bench",
         "action": "Declare 'P99 < N ms under M RPS; bench: `criterion::cache_hit_bench`.'"},
        {"id": "Pf-2", "strategy": "Add complexity",
         "action": "For every hot path: 'O(n) over n = active sessions (~10k typical).'"},
    ],
    "functionality": [
        {"id": "F-1", "strategy": "Wire orphans",
         "action": "For each orphan in ground_truth.wiring_orphans not addressed by the plan, add a subtask wiring it (or document why deferred)."},
        {"id": "F-2", "strategy": "Surface area declaration",
         "action": "Name every new `pub` symbol + at least one consumer."},
    ],
    "quality": [
        {"id": "Q-1", "strategy": "Named tests + assertion",
         "action": "Replace 'add tests' with `test_<name>` + the exact assertion."},
        {"id": "Q-2", "strategy": "0 unwrap",
         "action": "Replace `unwrap()` with `?` or `.unwrap_or_else(|e| ...)`."},
    ],
    "detail": [
        {"id": "D-1", "strategy": "Embed schemas",
         "action": "Paste the Pydantic / serde struct inline for every API mentioned."},
        {"id": "D-2", "strategy": "Enumerate edges",
         "action": "List edge cases: null, empty, oversize, timeout — what does each branch do?"},
    ],
    "integration": [
        {"id": "I-1", "strategy": "Map every connection",
         "action": "Run `touring wiring audit -j` — for each subtask, name the caller and the callee."},
        {"id": "I-2", "strategy": "Cross-module diagram",
         "action": "Add Mermaid flow A→B→C with actual function names."},
    ],
    "dependencies": [
        {"id": "Dep-1", "strategy": "Pin everything",
         "action": "Replace `tokio = \"*\"` with `tokio = { version = \"1.42\", features = [...] }`."},
        {"id": "Dep-2", "strategy": "MSRV note",
         "action": "Document MSRV / Python range and why it's required."},
    ],
    "potentiation": [
        {"id": "Pt-1", "strategy": "REGRA #0 — fill Enables",
         "action": "For every subtask with empty `Enables`, rewrite so the change exposes a hook / trait / extension point."},
        {"id": "Pt-2", "strategy": "Compounding effect",
         "action": "Add a Potentiation Matrix showing how each subtask compounds."},
    ],
}


def _find_subtasks_without_enables(plan_md: str) -> list[str]:
    """Locate subtasks (S-N) whose Enables row is empty/missing."""
    issues: list[str] = []
    # Heuristic: each subtask block starts with `S-` or `#### S-` and ends at next blank line cluster
    blocks = re.split(r"(?=^#{2,4}\s+S-\d+|^S-\d+:|^- \*\*S-\d+)", plan_md, flags=re.MULTILINE)
    for block in blocks:
        m = re.search(r"S-(\d+)", block)
        if not m:
            continue
        if "Enables" not in block:
            issues.append(f"S-{m.group(1)}")
            continue
        enables_line = re.search(r"Enables[^\n]*:\s*([^\n]*)", block)
        if enables_line and not enables_line.group(1).strip() or (
            enables_line and enables_line.group(1).strip() in {"—", "-", "(empty)", ""}
        ):
            issues.append(f"S-{m.group(1)}")
    return issues


def amplify_dimension(
    dim_report: dict[str, Any],
    plan_md: str,
    ground_truth: dict[str, Any] | None,
    threshold: float,
) -> list[dict[str, Any]]:
    """Return list of amplification actions for one dimension."""
    name = dim_report.get("name", "")
    current = float(dim_report.get("current", 0.0))
    if current >= threshold:
        return []

    actions: list[dict[str, Any]] = []
    catalog = _STRATEGIES.get(name, [])
    for strategy in catalog:
        action = {
            "dim": name,
            "current": current,
            "threshold": threshold,
            "delta": round(threshold - current, 2),
            "strategy_id": strategy["id"],
            "strategy": strategy["strategy"],
            "action": strategy["action"],
        }
        if name == "potentiation":
            empty_subtasks = _find_subtasks_without_enables(plan_md)
            if empty_subtasks:
                action["affected_subtasks"] = empty_subtasks
        if name == "functionality" and ground_truth:
            orphans = ground_truth.get("wiring_orphans") or []
            uncovered = [
                o.get("name", "") for o in orphans
                if o.get("name") and str(o["name"]) not in plan_md
            ]
            if uncovered:
                action["uncovered_orphans"] = uncovered[:10]
        if name == "precision" and ground_truth:
            verifications = ground_truth.get("vgp_verifications") or []
            unverified = [v.get("name", "") for v in verifications if not v.get("verified")]
            if unverified:
                action["unverified_symbols"] = unverified[:10]
        actions.append(action)
    return actions


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="dimension_amplifier", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown to amplify.")
    parser.add_argument("--scores", type=Path, default=None,
                        help="dimension_scores.json. If absent, runs dimension_scorer inline.")
    parser.add_argument("--ground-truth", type=Path, default=None)
    parser.add_argument("--threshold", type=float, default=7.0,
                        help="Amplify dimensions strictly below this score.")
    parser.add_argument("--emit", action="store_true",
                        help="Write data/amplifications.json.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (amplifier is read-only).")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def _load_or_score(args: argparse.Namespace) -> dict[str, Any]:
    """Load scores from --scores; otherwise invoke dimension_scorer inline."""
    if args.scores and args.scores.exists():
        loaded = safe_load_json(args.scores)
        if isinstance(loaded, dict):
            return loaded
    scorer = _SCRIPT_DIR / "dimension_scorer.py"
    cmd = ["python3", str(scorer), str(args.path), "-j"]
    if args.ground_truth:
        cmd += ["--ground-truth", str(args.ground_truth)]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30, check=False)
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return {"dimensions": [], "composite_current": 0.0}


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Amplify."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)
    plan_md = args.path.read_text(encoding="utf-8")
    ground_truth = safe_load_json(args.ground_truth) if args.ground_truth else None
    scores = _load_or_score(args)

    amplifications: list[dict[str, Any]] = []
    for dim_report in scores.get("dimensions", []):
        amplifications.extend(amplify_dimension(dim_report, plan_md, ground_truth, args.threshold))

    report = {
        "status": "OK" if not amplifications else "WARN",
        "script": "dimension_amplifier",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "threshold": args.threshold,
        "composite_current": scores.get("composite_current", 0.0),
        "below_threshold_dims": [
            d["name"] for d in scores.get("dimensions", [])
            if float(d.get("current", 0.0)) < args.threshold
        ],
        "amplifications_total": len(amplifications),
        "amplifications": amplifications,
    }
    if args.emit:
        out = args.output_dir / "amplifications.json"
        write_json_atomic(out, report)
        report["json_path"] = str(out)
    return report


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("dimension_amplifier failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
