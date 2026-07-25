#!/usr/bin/env python3
"""plan_scaffolder — Generate a Pln2 skeleton from intent + ground truth.

Reads canonical Jinja2 templates from assets/templates/ and renders a
plan markdown skeleton with frontmatter, all 4 stages stubbed, sample
phase + subtask blocks, and the Potentiation Matrix template.

Usage
-----
    python3 plan_scaffolder.py --intent "implement async write-back cache" \\
                              --ground-truth data/ground_truth.json \\
                              --out plans/2026-05-24-cache.md

    python3 plan_scaffolder.py --intent "..." --apply   # mutating
"""

from __future__ import annotations

import argparse
import json
import logging
import sys
from datetime import UTC, datetime
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
    is_kebab,
    safe_load_json,
    utcnow_iso,
)

try:
    from jinja2 import Environment, FileSystemLoader, select_autoescape
    _JINJA2_AVAILABLE = True
except ImportError:
    _JINJA2_AVAILABLE = False
    Environment = None  # type: ignore[assignment]
    FileSystemLoader = None  # type: ignore[assignment]
    select_autoescape = None  # type: ignore[assignment]

_ASSETS_DIR = _SCRIPT_DIR.parent / "assets" / "templates"


def _jinja_env() -> Any:
    """Build the Jinja2 environment pointed at assets/templates/."""
    if not _JINJA2_AVAILABLE:
        logging.getLogger(__name__).warning(
            "Jinja2 not available — falling back to simple substitution.",
        )
        return None
    return Environment(
        loader=FileSystemLoader(str(_ASSETS_DIR)),
        autoescape=select_autoescape(disabled_extensions=("j2", "tmpl", "md")),
        keep_trailing_newline=True,
        trim_blocks=True,
        lstrip_blocks=True,
    )


def _slug_from_intent(intent: str, *, max_words: int = 5) -> str:
    """Build a kebab slug from the intent (best-effort)."""
    cleaned = "".join(c.lower() if c.isalnum() else " " for c in intent)
    words = [w for w in cleaned.split() if w and not w.isdigit()]
    chosen = "-".join(words[:max_words]).strip("-")
    return chosen or "plan"


def _seed_dimensions() -> list[dict[str, Any]]:
    """Default 9-dimension stub rows (scores TBD by dimension_scorer)."""
    return [
        {"name": d, "current": 0.0, "target": 8.5, "delta": 8.5,
         "amplification": "to be measured"}
        for d in (
            "precision", "scalability", "performance", "functionality", "quality",
            "detail", "integration", "dependencies", "potentiation",
        )
    ]


def _seed_phases(intent: str) -> list[dict[str, Any]]:
    """Default 1-phase stub with one subtask."""
    return [{
        "number": 1,
        "name": "Initial forensic + scaffolding",
        "mode": "sequential",
        "subtasks": [{
            "id": "1",
            "action": f"Discover and verify ground truth for: {intent}",
            "severity": "P0",
            "confidence": "INFERENCE",
            "file": "TBD",
            "line": 0,
            "lang": "rust",
            "source_truth": "(populate via touring ast overview)",
            "change": "(describe inline)",
            "blast_radius": 0,
            "test_name": "test_TBD",
            "test_assertion": "assert outcome == expected",
            "dimensions_impact": ["a", "g"],
            "enables": "subsequent phases by establishing verified facts",
            "evidence_cmd": "touring index find <symbol> -j",
        }],
    }]


def build_context(
    intent: str,
    *,
    title: str,
    level: str,
    ground_truth: dict[str, Any] | None,
) -> dict[str, Any]:
    """Compose Jinja2 context from intent + optional ground truth."""
    plan_slug = _slug_from_intent(intent)
    authored = datetime.now(UTC).date().isoformat()

    verified_symbols: list[dict[str, Any]] = []
    lessons: list[dict[str, str]] = []
    gotchas: list[dict[str, str]] = []
    doctor_overall = "UNVERIFIED"
    e2e_composite = "?"
    orphan_count: Any = "?"
    index_symbols: Any = "?"
    drift_alert = "none"
    lessons_count = 0

    if ground_truth:
        for v in ground_truth.get("vgp_verifications", []):
            if v.get("verified"):
                verified_symbols.append({
                    "name": v.get("name", ""),
                    "file": v.get("file", ""),
                    "line": v.get("line", 0),
                    "signature": v.get("signature", "(no signature)"),
                })
        for entry in ground_truth.get("memory_lessons", [])[:5]:
            lessons.append({
                "key": str(entry.get("key", "")),
                "summary": str(entry.get("value", ""))[:200],
            })
        for path, matches in ground_truth.get("gotcha_per_file", {}).items():
            for m in matches[:2]:
                gotchas.append({
                    "file": path,
                    "pattern": str(m.get("pattern", "")),
                    "description": str(m.get("description", ""))[:160],
                })
        doctor_overall = "DEGRADED" if ground_truth.get("daemon_degraded") else "OK"
        e2e = ground_truth.get("e2e", {}) or {}
        e2e_composite = e2e.get("composite_score", e2e.get("score", "?"))
        orphan_count = len(ground_truth.get("wiring_orphans") or [])
        status = ground_truth.get("status_snapshot", {}) or {}
        index = status.get("index", {}) if isinstance(status, dict) else {}
        if isinstance(index, dict):
            index_symbols = index.get("symbol_count", "?")
        drift = ground_truth.get("evolution_drift", {}) or {}
        if isinstance(drift, dict):
            drift_alert = str(drift.get("alert_level", "none"))
        lessons_count = len(lessons)

    dimensions = _seed_dimensions()
    phases = _seed_phases(intent)
    all_subtasks = [
        {"id": st["id"], "action_short": st["action"][:60], "enables": st["enables"]}
        for phase in phases for st in phase["subtasks"]
    ]

    return {
        "plan_slug": plan_slug,
        "title": title or plan_slug.replace("-", " ").title(),
        "authored": authored,
        "level": level,
        "intent": intent,
        "doctor_overall": doctor_overall,
        "e2e_composite": e2e_composite,
        "orphan_count": orphan_count,
        "index_symbols": index_symbols,
        "drift_alert": drift_alert,
        "lessons_count": lessons_count,
        "verified_symbols": verified_symbols,
        "lessons": lessons,
        "gotchas": gotchas,
        "dimensions": dimensions,
        "composite_current": sum(d["current"] for d in dimensions) / len(dimensions),
        "composite_target": sum(d["target"] for d in dimensions) / len(dimensions),
        "composite_delta": (
            sum(d["target"] for d in dimensions) / len(dimensions)
            - sum(d["current"] for d in dimensions) / len(dimensions)
        ),
        "phases": phases,
        "all_subtasks": all_subtasks,
        "dag_mermaid": "graph LR\n  start([Start]) --> P1[Phase 1] --> done([Pln2 ready])",
        "dag_textual": "P1 (1 sub) -> ... (more phases as authoring proceeds)",
    }


def render_skeleton(context: dict[str, Any]) -> str:
    """Render the canonical plan_pln2.md.j2 template."""
    env = _jinja_env()
    if env is None:
        # Fallback: emit a minimal markdown with the most important fields
        return (
            f"# {context['title']} (Pln2)\n\n"
            f"> Intent: {context['intent']}\n"
            f"> Authored: {context['authored']}\n\n"
            f"## 1. Ground Truth Summary\n(populate)\n\n"
            f"## 2. 9-Dimension Scores\n(populate)\n\n"
            f"## 3. Phases\n(populate)\n\n"
            f"## 4. DAG\n(populate)\n\n"
            f"## 5. Verification Protocol\n(populate)\n\n"
            f"## 6. Potentiation Matrix\n(populate)\n"
        )
    template = env.get_template("plan_pln2.md.j2")
    return template.render(**context)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="plan_scaffolder", description=__doc__)
    parser.add_argument("--intent", required=True,
                        help="Free-form intent string.")
    parser.add_argument("--title", default="", help="Plan title (defaults from intent).")
    parser.add_argument("--level", default="L3",
                        help="Plan level L0-L5 (default L3).")
    parser.add_argument("--ground-truth", type=Path, default=None,
                        help="ground_truth.json from ground_truth_collector.")
    parser.add_argument("--out", type=Path, default=None,
                        help="Output plan markdown path (default: plans/<slug>.md).")
    parser.add_argument("--apply", action="store_true",
                        help="Write to disk. Default is dry-run (stdout only).")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Scaffold."""
    ground_truth = safe_load_json(args.ground_truth) if args.ground_truth else None
    context = build_context(
        args.intent,
        title=args.title,
        level=args.level,
        ground_truth=ground_truth,
    )

    rendered = render_skeleton(context)
    slug = context["plan_slug"]

    out_path = args.out or Path("plans") / f"{datetime.now(UTC).date().isoformat()}-{slug}.md"
    if args.apply:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(rendered, encoding="utf-8")

    return {
        "status": "OK",
        "script": "plan_scaffolder",
        "timestamp": utcnow_iso(),
        "apply": args.apply,
        "intent": args.intent,
        "plan_slug": slug,
        "level": args.level,
        "output_path": str(out_path),
        "bytes_rendered": len(rendered),
        "verified_symbols_count": len(context["verified_symbols"]),
        "lessons_count": context["lessons_count"],
        "preview": rendered[:500] + ("..." if len(rendered) > 500 else ""),
    }


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        if args.level and not args.level.startswith("L"):
            msg = f"--level must be L0-L5 (got '{args.level}')"
            raise ValueError(msg)
        if not is_kebab(_slug_from_intent(args.intent)) and len(args.intent.strip()) < 3:
            msg = "intent too short — provide a meaningful intent"
            raise ValueError(msg)
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except (ValueError, FileNotFoundError) as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("plan_scaffolder failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
