#!/usr/bin/env python3
"""mcts_wrapper — Multi-path planning via `touring mcts search`.

For L4+ multi-path decisions: composes a root_state from candidates +
optional ground_truth context, invokes `touring mcts search`, parses the
result, emits a markdown-friendly comparison table.

Cache keyed by blake2b(canonical(root_state)); 10-min TTL.

Usage
-----
    python3 mcts_wrapper.py --intent "cache backend" \\
                           --candidates "tokio-mutex;dashmap;moka" \\
                           --rollouts 200 --max-depth 5

    python3 mcts_wrapper.py --intent "..." --candidates "A;B;C" \\
                           --ground-truth data/ground_truth.json \\
                           --emit-markdown
"""

from __future__ import annotations

import argparse
import json
import logging
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
    cache_get,
    cache_put,
    compute_intent_cache_key,
    run_touring,
    safe_load_json,
    touring_available,
    utcnow_iso,
    write_json_atomic,
)

_MAX_CANDIDATES = 5
_MIN_ROLLOUTS = 50
_MAX_ROLLOUTS = 500
_MIN_DEPTH = 3
_MAX_DEPTH = 7


def build_root_state(
    intent: str,
    candidates: list[str],
    ground_truth: dict[str, Any] | None,
) -> dict[str, Any]:
    """Compose the root_state JSON the Touring command expects."""
    enriched_candidates: list[dict[str, Any]] = []
    for idx, candidate in enumerate(candidates):
        enriched_candidates.append({
            "id": chr(ord("A") + idx),
            "description": candidate,
        })

    context: dict[str, Any] = {}
    if ground_truth:
        context["current_e2e"] = ground_truth.get("e2e", {}).get("composite_score")
        context["orphan_count"] = len(ground_truth.get("wiring_orphans") or [])
        drift = ground_truth.get("evolution_drift") or {}
        context["drift_alert"] = drift.get("alert_level", "none")
        lessons = ground_truth.get("memory_lessons") or []
        context["past_lessons"] = [str(entry.get("key", ""))[:80] for entry in lessons[:3]]

    return {
        "intent": intent,
        "candidates": enriched_candidates,
        "context": context,
    }


def invoke_mcts(
    root_state: dict[str, Any],
    *,
    rollouts: int,
    max_depth: int,
) -> dict[str, Any]:
    """Invoke `touring mcts search` and parse output. Fail-open with skip mode."""
    if not touring_available():
        return {
            "mode": "skip",
            "reason": "daemon_unavailable",
            "fallback_recommendation": "decide via pros/cons table; tag confidence INFERENCE [0.7]",
        }
    payload = json.dumps(root_state, ensure_ascii=False)
    candidate_ids = ",".join(c["id"] for c in root_state["candidates"])
    result = run_touring([
        "mcts", "search", payload,
        "--candidate-actions", candidate_ids,
        "--num-rollouts", str(rollouts),
        "--max-depth", str(max_depth),
        "-j",
    ], timeout=60)
    if not isinstance(result, dict):
        return {
            "mode": "skip",
            "reason": "mcts_returned_non_dict",
            "raw": result,
        }
    result["mode"] = "ok"
    return result


def to_markdown_table(
    root_state: dict[str, Any],
    mcts: dict[str, Any],
) -> str:
    """Pretty-print MCTS result as a markdown table for embedding in the plan."""
    candidates_by_id = {c["id"]: c for c in root_state.get("candidates", [])}
    best = mcts.get("best_action", "?")
    confidence = mcts.get("confidence", 0.0)
    alternatives = mcts.get("alternative_actions", [])

    lines = [
        "## MCTS Decision",
        "",
        f"**Intent**: {root_state.get('intent', '?')}",
        f"**Best action**: `{best}` — {candidates_by_id.get(best, {}).get('description', '?')}",
        f"**Confidence**: {confidence:.2f}",
        f"**Rollouts**: {mcts.get('total_rollouts', '?')} | **Tree depth**: {mcts.get('tree_depth', '?')}",
        "",
        "| Action | Description | Value | Confidence | Rationale |",
        "|--------|-------------|------:|------------:|-----------|",
    ]
    main_value = mcts.get("value", 0.0)
    lines.append(
        f"| **{best}** | "
        f"{candidates_by_id.get(best, {}).get('description', '?')} | "
        f"**{main_value:.2f}** | **{confidence:.2f}** | best |"
    )
    for alt in alternatives:
        alt_id = alt.get("id", "?")
        lines.append(
            f"| {alt_id} | "
            f"{candidates_by_id.get(alt_id, {}).get('description', '?')} | "
            f"{alt.get('value', 0.0):.2f} | {alt.get('confidence', 0.0):.2f} | "
            f"{alt.get('rationale', '')} |"
        )
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="mcts_wrapper", description=__doc__)
    parser.add_argument("--intent", required=True,
                        help="Free-form intent string (decision context).")
    parser.add_argument("--candidates", required=True,
                        help="Semicolon-separated candidate actions (max 5).")
    parser.add_argument("--ground-truth", type=Path, default=None,
                        help="ground_truth.json (enriches MCTS context).")
    parser.add_argument("--rollouts", type=int, default=200,
                        help="MCTS rollout count (50-500).")
    parser.add_argument("--max-depth", type=int, default=5,
                        help="MCTS max tree depth (3-7).")
    parser.add_argument("--no-cache", action="store_true")
    parser.add_argument("--emit-markdown", action="store_true",
                        help="Also emit a Markdown decision table.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (wrapper itself does not mutate the plan).")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("--emit", action="store_true",
                        help="Write data/mcts_decision.json.")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def _validate(args: argparse.Namespace) -> list[str]:
    """Return list of validation errors."""
    errs: list[str] = []
    candidates = [c.strip() for c in args.candidates.split(";") if c.strip()]
    if len(candidates) < 2:
        errs.append("--candidates must contain at least 2 entries (separated by ;)")
    if len(candidates) > _MAX_CANDIDATES:
        errs.append(f"--candidates max is {_MAX_CANDIDATES}; got {len(candidates)}")
    if not (_MIN_ROLLOUTS <= args.rollouts <= _MAX_ROLLOUTS):
        errs.append(f"--rollouts must be in [{_MIN_ROLLOUTS}, {_MAX_ROLLOUTS}]")
    if not (_MIN_DEPTH <= args.max_depth <= _MAX_DEPTH):
        errs.append(f"--max-depth must be in [{_MIN_DEPTH}, {_MAX_DEPTH}]")
    return errs


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Wrap."""
    errs = _validate(args)
    if errs:
        msg = "; ".join(errs)
        raise ValueError(msg)

    candidates = [c.strip() for c in args.candidates.split(";") if c.strip()]
    ground_truth = safe_load_json(args.ground_truth) if args.ground_truth else None
    root_state = build_root_state(args.intent, candidates, ground_truth)

    cache_key = compute_intent_cache_key(
        args.intent, extra=f"|cand={'|'.join(candidates)}|r={args.rollouts}",
    )
    if not args.no_cache:
        cached = cache_get(cache_key, ttl_seconds=600)
        if cached:
            cached["_from_cache"] = True
            return cached

    mcts_result = invoke_mcts(root_state, rollouts=args.rollouts, max_depth=args.max_depth)

    report: dict[str, Any] = {
        "status": "OK" if mcts_result.get("mode") == "ok" else "DEGRADED",
        "script": "mcts_wrapper",
        "timestamp": utcnow_iso(),
        "intent": args.intent,
        "candidate_count": len(candidates),
        "rollouts": args.rollouts,
        "max_depth": args.max_depth,
        "root_state": root_state,
        "mcts": mcts_result,
    }
    if args.emit_markdown and mcts_result.get("mode") == "ok":
        report["markdown_table"] = to_markdown_table(root_state, mcts_result)

    if not args.no_cache:
        cache_put(cache_key, report)
    if args.emit:
        out = args.output_dir / "mcts_decision.json"
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
    except (ValueError, FileNotFoundError) as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("mcts_wrapper failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
