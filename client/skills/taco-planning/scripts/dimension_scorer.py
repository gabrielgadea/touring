#!/usr/bin/env python3
"""dimension_scorer — Score a plan markdown across 9 quality dimensions.

Specialized for AUTHORING — measures keyword density PLUS:
  * symbol verification rate (dim a) — every `file:LINE` resolves via ground_truth
  * schema completeness (dim f) — fraction of APIs with embedded schemas
  * wiring orphan coverage (dim d) — orphans mentioned in plan vs orphans in repo
  * potentiation enables column (dim i) — every subtask has non-empty Enables

Adapted from pln2_generator/dimension_analyzer.py — same 9-dim canonical
taxonomy but with authoring-time evidence checks.

Usage
-----
    python3 dimension_scorer.py path/to/plan.md
    python3 dimension_scorer.py path/to/plan.md --ground-truth data/ground_truth.json
    python3 dimension_scorer.py path/to/plan.md --emit -j
"""

from __future__ import annotations

import argparse
import json
import logging
import re
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
    VALID_DIMENSIONS,
    safe_load_json,
    utcnow_iso,
    write_json_atomic,
)

_DIMENSION_KEYWORDS: dict[str, list[str]] = {
    "precision": [
        r"\d+\.\d+", r"\d+%", r"≥\d+", r"<\d+", r">\d+",
        r"\d+ms", r"\d+\s*req/s", r"\d+x", r"P50", r"P99",
        r"blake2b", r"0\s+errors", r"file:\d+", r":\d+",
    ],
    "scalability": [
        r"scal(?:e|ing|ability)", r"horizontal", r"shard", r"factory",
        r"trait", r"registry", r"generic", r"dispatch",
        r"async", r"parallel", r"distributed", r"partition",
        r"batch", r"concurrent", r"worker",
    ],
    "performance": [
        r"latency", r"throughput", r"P50", r"P99", r"\d+ms",
        r"req/s", r"SIMD", r"optim", r"cache\s*hit", r"speedup",
        r"<\d+s", r"benchmark", r"criterion", r"bench", r"O\([^)]+\)",
    ],
    "functionality": [
        r"\d+\s*skills?", r"\d+\s*motors?", r"\d+\s*principles?",
        r"\d+\s*hooks?", r"\d+\s*rules?", r"feature", r"capabilit",
        r"orchestrat", r"pipeline", r"framework", r"engine", r"system",
        r"expose", r"surface\s*area", r"pub\s+(?:fn|struct|trait)",
    ],
    "quality": [
        r"[Rr]uff", r"[Pp]yright", r"[Cc]overage", r"[Cc]lippy",
        r"lint", r"type\s*(hint|check|safe)", r"frozen\s*=\s*True",
        r"strict\s*mode", r"0\s*errors", r"CC\s*[<≤]=?\s*\d+",
        r"unwrap", r"panic", r"expect\(", r"\?\;",
        r"test_\w+", r"#\[test\]", r"def test_", r"assert",
    ],
    "detail": [
        r"\.py\b", r"\.md\b", r"\.rs\b", r"\.json\b", r"\.toml\b",
        r"LOC", r"\d+\s*lines", r"file:", r"path:", r"directory",
        r"example", r"pseudocode", r"implementation", r"concrete",
        r"```(?:rust|python|typescript|bash|sh|toml|json|yaml)",
        r"schema", r"BaseModel", r"pydantic",
        r"edge case", r"null", r"empty", r"boundary",
    ],
    "integration": [
        r"cross[_-]?ref", r"integrat", r"hook\s*->", r"->", r"<->",
        r"PreToolUse", r"PostToolUse", r"SessionSt", r"wir(?:e|ing)",
        r"sequence", r"pipeline", r"dispatch", r"chain", r"composes",
    ],
    "dependencies": [
        r">=\d+\.\d+", r"v\d+\.\d+", r"compatib", r"version\s*=",
        r"pin", r"PyO3", r"pydantic", r"ruff", r"pyright",
        r"feature\s*=", r"workspace\s*=\s*true", r"MSRV", r"require",
    ],
    "potentiation": [
        r"flywheel", r"multiplier", r"compound", r"exponential",
        r"growth", r"autonomous", r"self[_-]?heal", r"drift",
        r"mutation", r"evolv", r"accumulat", r"\d+\.\d+x",
        r"enables?", r"unlocks?", r"paves\s*the\s*way", r"building\s*block",
    ],
}

# 0..10 score from density (hits / (lines/100))
_SCORE_THRESHOLDS: list[tuple[float, float]] = [
    (0.0, 0.0), (0.1, 1.0), (1.0, 2.0), (2.0, 4.0), (4.0, 6.0),
    (6.0, 9.0), (9.0, 13.0), (13.0, 18.0), (18.0, 25.0),
    (25.0, 35.0), (35.0, 999.0),
]

# Lift modifiers (additive) based on authoring-specific signals
_LIFT_MAX_PER_DIM = 2.0

_RECOMMENDATIONS: dict[str, list[str]] = {
    "precision": [
        "Replace prose 'around line N' with exact `file:LINE` from `touring ast find`.",
        "Embed function signatures verbatim.",
        "Add numeric targets (P50/P99/RPS) with units.",
    ],
    "scalability": [
        "Replace one-off branches with trait + registry pattern.",
        "Document horizontal-scaling strategy.",
        "Identify which subtask creates an extension point.",
    ],
    "performance": [
        "Declare P99 latency target + workload.",
        "Name the criterion bench that proves the claim.",
        "Add complexity O() for hot paths.",
    ],
    "functionality": [
        "Wire every orphan listed by `touring wiring orphans -j` or document why deferred.",
        "Add capability matrix per change.",
        "Name new pub symbols and their consumers.",
    ],
    "quality": [
        "Every change names a test (`test_*`) + the assertion.",
        "Remove every `unwrap()` from the diff.",
        "Add the error branch in the same subtask.",
    ],
    "detail": [
        "Embed input + output schemas for every API change.",
        "Enumerate edge cases (null, empty, oversize, timeout).",
        "Show the concrete pseudocode inline.",
    ],
    "integration": [
        "Run `touring wiring audit -j` and map each subtask to a connection.",
        "Diagram new cross-module flows (Mermaid).",
        "Name the caller(s) of every new pub symbol.",
    ],
    "dependencies": [
        "Pin every version explicitly (no wildcards).",
        "Document required feature flags.",
        "Note MSRV / Python version range.",
    ],
    "potentiation": [
        "Every subtask must have non-empty Enables (REGRA #0).",
        "Rewrite dead-end subtasks as hooks / extension points.",
        "Identify compounding effects in the matrix.",
    ],
}


def _count_keyword_hits(content: str, patterns: list[str]) -> tuple[int, list[str]]:
    """Count regex hits + collect up to 3 evidence samples per pattern."""
    total = 0
    evidence: list[str] = []
    for pat in patterns:
        matches = re.findall(pat, content, re.IGNORECASE)
        total += len(matches)
        for m in matches[:3]:
            if isinstance(m, str) and m.strip():
                evidence.append(m.strip())
    return total, evidence


def _density_to_score(density: float) -> float:
    for idx, (low, high) in enumerate(_SCORE_THRESHOLDS):
        if low <= density < high:
            return float(idx)
    return 10.0


def _compute_target(current: float) -> float:
    return min(max(8.5, current + 3.0), 10.0)


def _compute_authoring_lift(
    dimension: str,
    plan_md: str,
    ground_truth: dict[str, Any] | None,
) -> tuple[float, dict[str, Any]]:
    """Authoring-specific bonus + metadata (clamped to _LIFT_MAX_PER_DIM)."""
    extras: dict[str, Any] = {}
    if ground_truth is None:
        return 0.0, extras

    lift = 0.0
    if dimension == "precision":
        verifications = ground_truth.get("vgp_verifications") or []
        if verifications:
            ok = sum(1 for v in verifications if v.get("verified"))
            ratio = ok / len(verifications)
            extras["symbol_verifications_ok"] = ok
            extras["symbol_verifications_total"] = len(verifications)
            lift = _LIFT_MAX_PER_DIM * ratio
    elif dimension == "functionality":
        orphans = ground_truth.get("wiring_orphans") or []
        cited = sum(1 for o in orphans if str(o.get("name", "")) and str(o.get("name", "")) in plan_md)
        if orphans:
            ratio = cited / len(orphans)
            lift = _LIFT_MAX_PER_DIM * ratio
            extras["orphans_addressed"] = cited
            extras["orphans_total"] = len(orphans)
    elif dimension == "detail":
        # schema completeness — fraction of mentions of "schema" near a fenced code block
        fences = len(re.findall(r"```", plan_md))
        schema_mentions = len(re.findall(r"\b(?:schema|BaseModel|Pydantic)\b", plan_md, re.IGNORECASE))
        if schema_mentions > 0:
            ratio = min(1.0, fences / max(schema_mentions, 1))
            extras["schema_completeness"] = round(ratio, 3)
            lift = _LIFT_MAX_PER_DIM * ratio
    elif dimension == "potentiation":
        # how many subtask tables have an "Enables" non-empty cell
        rows = re.findall(r"\|\s*Enables\s*\|\s*([^|]+?)\s*\|", plan_md, re.IGNORECASE)
        non_empty = sum(1 for r in rows if r.strip() and r.strip() != "—")
        if rows:
            ratio = non_empty / len(rows)
            extras["enables_non_empty"] = non_empty
            extras["enables_total"] = len(rows)
            lift = _LIFT_MAX_PER_DIM * ratio

    return min(lift, _LIFT_MAX_PER_DIM), extras


def score_dimension(
    plan_md: str,
    dimension: str,
    ground_truth: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Score one dimension on a plan markdown."""
    if dimension not in VALID_DIMENSIONS:
        msg = f"Unknown dimension: {dimension}"
        raise ValueError(msg)

    patterns = _DIMENSION_KEYWORDS[dimension]
    total_lines = max(len(plan_md.splitlines()), 1)
    hits, evidence = _count_keyword_hits(plan_md, patterns)
    density = hits / (total_lines / 100)
    base_score = _density_to_score(density)

    lift, lift_meta = _compute_authoring_lift(dimension, plan_md, ground_truth)
    current = min(10.0, base_score + lift)
    target = _compute_target(current)

    seen: set[str] = set()
    unique_evidence: list[str] = []
    for sample in evidence:
        if sample not in seen:
            seen.add(sample)
            unique_evidence.append(sample)
        if len(unique_evidence) >= 10:
            break

    return {
        "name": dimension,
        "current": round(current, 2),
        "target": round(target, 2),
        "delta": round(target - current, 2),
        "hits": hits,
        "density": round(density, 2),
        "base_score": round(base_score, 2),
        "authoring_lift": round(lift, 2),
        "lift_meta": lift_meta,
        "evidence": unique_evidence,
        "recommendations": _RECOMMENDATIONS.get(dimension, []),
    }


def score_all(plan_md: str, ground_truth: dict[str, Any] | None = None) -> list[dict[str, Any]]:
    """Score all 9 canonical dimensions."""
    return [score_dimension(plan_md, dim, ground_truth) for dim in VALID_DIMENSIONS]


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="dimension_scorer", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown to score.")
    parser.add_argument("--ground-truth", type=Path, default=None,
                        help="ground_truth.json produced by ground_truth_collector.")
    parser.add_argument("--emit", action="store_true",
                        help="Write data/dimension_scores.json beside the plan.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (scorer is read-only).")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Score."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)
    plan_md = args.path.read_text(encoding="utf-8")
    ground_truth = safe_load_json(args.ground_truth) if args.ground_truth else None

    dimensions = score_all(plan_md, ground_truth)
    composite_current = sum(d["current"] for d in dimensions) / len(dimensions)
    composite_target = sum(d["target"] for d in dimensions) / len(dimensions)

    report = {
        "status": "OK",
        "script": "dimension_scorer",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "ground_truth_used": bool(ground_truth),
        "composite_current": round(composite_current, 2),
        "composite_target": round(composite_target, 2),
        "composite_delta": round(composite_target - composite_current, 2),
        "below_threshold": [d["name"] for d in dimensions if d["current"] < 7.0],
        "dimensions": dimensions,
    }
    if args.emit:
        out = args.output_dir / "dimension_scores.json"
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
        logging.getLogger(__name__).exception("dimension_scorer failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
