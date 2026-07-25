#!/usr/bin/env python3
"""dimension_scorer — Score a plan markdown across 9 quality dimensions.

Adapted from ``analise/scripts/pln2_generator/dimension_analyzer.py``.
Dimensions: precision, scalability, performance, functionality, code_quality,
detail, integration, dependencies, potentiation.

Each dimension is scored 0-10 via keyword density:
    density = hits / (total_lines / 100)
    score   = monotonic step function of density (10 buckets)

Uses regex-only NLP. No LLM in the path.

Usage
-----
    python3 dimension_scorer.py path/to/plan.md
    python3 dimension_scorer.py path/to/plan.md -j        # JSON output
    python3 dimension_scorer.py path/to/plan.md --target 8.5
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
    utcnow_iso,
    write_json_atomic,
)

# ── Keyword patterns (per dimension) ──────────────────────────────────────

_DIMENSION_KEYWORDS: dict[str, list[str]] = {
    "precision": [
        r"\d+\.\d+", r"\d+%", r"≥\d+", r"<\d+", r">\d+",
        r"\d+ms", r"\d+\s*req/s", r"\d+x", r"P50", r"P99",
        r"blake2b", r"0\s+errors",
    ],
    "scalability": [
        r"scal(?:e|ing|ability)", r"horizontal", r"shard",
        r"load\s*balanc", r"async", r"parallel", r"distributed",
        r"partition", r"batch", r"concurrent", r"worker",
    ],
    "performance": [
        r"latency", r"throughput", r"P50", r"P99", r"\d+ms",
        r"req/s", r"SIMD", r"optim", r"cache\s*hit", r"speedup",
        r"<\d+s", r"benchmark",
    ],
    "functionality": [
        r"\d+\s*skills?", r"\d+\s*motors?", r"\d+\s*principles?",
        r"\d+\s*hooks?", r"\d+\s*rules?", r"feature", r"capabilit",
        r"orchestrat", r"pipeline", r"framework", r"engine", r"system",
    ],
    "code_quality": [
        r"[Rr]uff", r"[Pp]yright", r"[Cc]overage", r"[Cc]lippy",
        r"lint", r"type\s*(hint|check|safe)", r"frozen\s*=\s*True",
        r"strict\s*mode", r"0\s*errors", r"CC\s*[<≤]=?\s*\d+", r"complexity",
    ],
    "detail": [
        r"\.py\b", r"\.md\b", r"\.rs\b", r"\.json\b", r"LOC",
        r"\d+\s*lines", r"file:", r"path:", r"directory", r"example",
        r"pseudocode", r"implementation", r"concrete",
    ],
    "integration": [
        r"cross[_-]?ref", r"integrat", r"hook\s*->", r"->", r"<->",
        r"PreToolUse", r"PostToolUse", r"SessionSt", r"wir(?:e|ing)",
        r"sequence", r"pipeline",
    ],
    "dependencies": [
        r">=\d+", r"v\d+\.\d+", r"compatib", r"version", r"pin",
        r"PyO3", r"pydantic", r"ruff", r"pyright", r"toons", r"require",
    ],
    "potentiation": [
        r"flywheel", r"multiplier", r"compound", r"exponential",
        r"growth", r"autonomous", r"self[_-]?heal", r"drift",
        r"mutation", r"evolv", r"accumulat", r"\d+\.\d+x",
    ],
}

# Density → score thresholds (10 buckets)
_SCORE_THRESHOLDS: list[tuple[float, float]] = [
    (0.0, 0.0), (0.1, 1.0), (1.0, 2.0), (2.0, 4.0), (4.0, 6.0),
    (6.0, 9.0), (9.0, 13.0), (13.0, 18.0), (18.0, 25.0),
    (25.0, 35.0), (35.0, 999.0),
]

# Recommendations per dimension
_RECOMMENDATIONS: dict[str, list[str]] = {
    "precision": [
        "Add exact numeric targets for all metrics.",
        "Include confidence intervals for estimates.",
        "Add error bounds and tolerances.",
    ],
    "scalability": [
        "Define horizontal scaling strategy.",
        "Add load testing thresholds.",
        "Document partition / shard strategy.",
    ],
    "performance": [
        "Add P99 latency targets.",
        "Define throughput benchmarks per phase.",
        "Identify SIMD / parallel optimization opportunities.",
    ],
    "functionality": [
        "Map each principle to a concrete implementation file.",
        "Add feature completeness checklist.",
        "Define capability matrix per skill tier.",
    ],
    "code_quality": [
        "Add mutation testing kill-rate targets.",
        "Define branch-coverage thresholds per module.",
        "Include complexity audit schedule.",
    ],
    "detail": [
        "Add LOC estimates per new module.",
        "Include file-path mapping for all components.",
        "Add concrete pseudocode for the riskier pieces.",
    ],
    "integration": [
        "Map hook → hook dependencies.",
        "Add cross-reference validation rules.",
        "Define integration test matrix.",
    ],
    "dependencies": [
        "Pin all dependency versions explicitly.",
        "Add compatibility matrix for cross-language bridges.",
        "Include upgrade-path documentation.",
    ],
    "potentiation": [
        "Add quarterly compound-growth metrics.",
        "Define autonomous evolution criteria.",
        "Include flywheel-acceleration benchmarks.",
    ],
}


# ── Scoring functions ─────────────────────────────────────────────────────


def _count_keyword_hits(content: str, patterns: list[str]) -> tuple[int, list[str]]:
    """Count regex hits and collect up to 3 evidence samples per pattern."""
    total = 0
    evidence: list[str] = []
    for pat in patterns:
        matches = re.findall(pat, content, re.IGNORECASE)
        total += len(matches)
        for match in matches[:3]:
            if isinstance(match, str) and match.strip():
                evidence.append(match.strip())
    return total, evidence


def _density_to_score(density: float) -> float:
    """Map density to a 0-10 score via the threshold table."""
    for idx, (low, high) in enumerate(_SCORE_THRESHOLDS):
        if low <= density < high:
            return float(idx)
    return 10.0


def _compute_target(current: float, *, floor: float = 8.5) -> float:
    """Compute Pln2 target: max(floor, current + 3.0), capped at 10.0."""
    return min(max(floor, current + 3.0), 10.0)


def score_dimension(content: str, dimension: str, *, target_floor: float = 8.5) -> dict[str, Any]:
    """Score one dimension. Returns a dict with score, evidence, recs, delta."""
    if dimension not in VALID_DIMENSIONS:
        msg = f"Unknown dimension: {dimension}"
        raise ValueError(msg)

    patterns = _DIMENSION_KEYWORDS[dimension]
    total_lines = max(len(content.splitlines()), 1)
    hits, evidence = _count_keyword_hits(content, patterns)
    density = hits / (total_lines / 100)
    score = _density_to_score(density)
    target = _compute_target(score, floor=target_floor)

    # Deduplicate evidence, keep first 10
    seen: set[str] = set()
    unique: list[str] = []
    for sample in evidence:
        if sample not in seen:
            seen.add(sample)
            unique.append(sample)
        if len(unique) >= 10:
            break

    return {
        "dimension": dimension,
        "current_score": round(score, 2),
        "target_score": round(target, 2),
        "delta": round(target - score, 2),
        "hits": hits,
        "density": round(density, 2),
        "evidence": unique,
        "recommendations": _RECOMMENDATIONS.get(dimension, []),
    }


def score_all(content: str, *, target_floor: float = 8.5) -> list[dict[str, Any]]:
    """Score all 9 canonical dimensions."""
    return [score_dimension(content, dim, target_floor=target_floor) for dim in VALID_DIMENSIONS]


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="dimension_scorer", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown to score")
    parser.add_argument("--target", type=float, default=8.5,
                        help="Target floor for the Pln2 score (default 8.5)")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (scorer is read-only; flag kept for symmetry).")
    parser.add_argument("--output-dir", type=Path, default=Path("data"),
                        help="Where to write the JSON report (when --emit).")
    parser.add_argument("--emit", action="store_true",
                        help="Also write a JSON report to <output-dir>/dimension_scores.json")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Score the plan markdown."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)
    content = args.path.read_text(encoding="utf-8")
    dimensions = score_all(content, target_floor=args.target)

    composite_current = sum(d["current_score"] for d in dimensions) / len(dimensions)
    composite_target = sum(d["target_score"] for d in dimensions) / len(dimensions)

    report = {
        "status": "OK",
        "script": "dimension_scorer",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "target_floor": args.target,
        "composite_current": round(composite_current, 2),
        "composite_target": round(composite_target, 2),
        "composite_delta": round(composite_target - composite_current, 2),
        "dimensions": dimensions,
    }

    if args.emit:
        out_path = args.output_dir / "dimension_scores.json"
        write_json_atomic(out_path, report)
        report["json_path"] = str(out_path)

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
