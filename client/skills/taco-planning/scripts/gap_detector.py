#!/usr/bin/env python3
"""gap_detector — Detect P0-P3 gaps in a plan draft.

Specialized for AUTHORING — distinct from TACO-wt's gap_detector (which scans
wave artifacts). Here we scan a plan markdown + optional ground_truth and flag:

  P0  BLOCKED_INVENTED_SYMBOL    — cited symbol not in ground_truth.vgp_verifications
  P0  MISSING_EVIDENCE            — claim without confidence tag OR without command
  P1  VAGUE_BLAST                 — modified file without `touring ast blast` reference
  P1  MISSING_TEST_NAME           — code change without named test
  P1  EMPTY_ENABLES               — REGRA #0 violation
  P2  ASYMMETRIC_CROSSREF         — declared cross-reference target absent
  P2  ORPHAN_NOT_ADDRESSED         — ground_truth orphan not mentioned in plan
  P3  WEAK_CONFIDENCE_DENSITY     — < 50% of claims tagged FACT/INFERENCE/SPECULATION

Usage
-----
    python3 gap_detector.py plan.md
    python3 gap_detector.py plan.md --ground-truth data/ground_truth.json --fail-on P0
"""

from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from collections import defaultdict
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
    EXIT_WARN,
    safe_load_json,
    utcnow_iso,
    write_json_atomic,
)

_RE_FILE_CITATION = re.compile(r"`?([a-z_][a-z_0-9/]*\.(?:rs|py|ts|tsx|js|go))(?::(\d+))?`?")
_RE_PASCAL_SYMBOL = re.compile(r"\b([A-Z][a-zA-Z0-9]{2,}(?:[A-Z][a-zA-Z0-9]{2,})*)\b")
_RE_SUBTASK_BLOCK = re.compile(
    r"(?:^#{2,4}\s+|^\s*-\s*\*\*)S-(\d+(?:\.\d+)?)\s*[:—-]",
    re.MULTILINE,
)
_RE_ENABLES_LINE = re.compile(r"Enables[^\n:]*:\s*([^\n]*)", re.IGNORECASE)
_RE_TEST_LINE = re.compile(r"Test[^\n:]*:\s*`([^`]+)`", re.IGNORECASE)
_RE_CONFIDENCE = re.compile(r"\b(?:FACT|INFERENCE|SPECULATION)\b\s*\[?(?:\d+\.\d+)?", re.IGNORECASE)
_RE_BLAST = re.compile(r"touring\s+ast\s+blast", re.IGNORECASE)


def _split_subtask_blocks(plan_md: str) -> list[tuple[str, str]]:
    """Return list of (subtask_id, body) tuples."""
    matches = list(_RE_SUBTASK_BLOCK.finditer(plan_md))
    blocks: list[tuple[str, str]] = []
    for idx, match in enumerate(matches):
        start = match.start()
        end = matches[idx + 1].start() if idx + 1 < len(matches) else len(plan_md)
        blocks.append((match.group(1), plan_md[start:end]))
    return blocks


def detect_invented_symbols(plan_md: str, ground_truth: dict[str, Any] | None) -> list[dict[str, str]]:
    """Flag PascalCase symbols cited in the plan but not in ground_truth."""
    if ground_truth is None:
        return []
    verified = {v.get("name", "") for v in ground_truth.get("vgp_verifications", []) if v.get("verified")}
    if not verified:
        return []
    gaps: list[dict[str, str]] = []
    seen: set[str] = set()
    for match in _RE_PASCAL_SYMBOL.finditer(plan_md):
        sym = match.group(1)
        if sym in seen or sym in verified:
            continue
        seen.add(sym)
        # Heuristic: only flag symbols that look like code refs (mentioned in code-fence proximity)
        if "`" + sym in plan_md or "::" + sym in plan_md or sym + "(" in plan_md:
            gaps.append({
                "id": f"G-INV-{sym}",
                "severity": "P0",
                "code": "BLOCKED_INVENTED_SYMBOL",
                "symbol": sym,
                "current_state": f"Symbol `{sym}` cited but absent from ground_truth.vgp_verifications.",
                "target_state": "Verify via `touring index find` before keeping the citation.",
                "remediation": f"touring index find {sym} → if absent, remove citation or add to design as new symbol explicitly.",
            })
    return gaps


def detect_missing_evidence(plan_md: str) -> list[dict[str, str]]:
    """Claims without a confidence tag adjacent to a Touring command reference."""
    gaps: list[dict[str, str]] = []
    blocks = _split_subtask_blocks(plan_md)
    for sub_id, body in blocks:
        if not _RE_CONFIDENCE.search(body):
            gaps.append({
                "id": f"G-EV-{sub_id}",
                "severity": "P0",
                "code": "MISSING_EVIDENCE",
                "subtask": f"S-{sub_id}",
                "current_state": f"S-{sub_id} has no FACT/INFERENCE/SPECULATION tag.",
                "target_state": "Every subtask carries a confidence tag with evidence.",
                "remediation": "Run `confidence_tagger.py --autofill` or add tag manually.",
            })
    return gaps


def detect_vague_blast(plan_md: str) -> list[dict[str, str]]:
    """Subtask citing a file change but no blast radius."""
    gaps: list[dict[str, str]] = []
    for sub_id, body in _split_subtask_blocks(plan_md):
        if _RE_FILE_CITATION.search(body) and not _RE_BLAST.search(body) and "blast" not in body.lower():
            gaps.append({
                "id": f"G-BLAST-{sub_id}",
                "severity": "P1",
                "code": "VAGUE_BLAST",
                "subtask": f"S-{sub_id}",
                "current_state": f"S-{sub_id} touches a file without documenting blast radius.",
                "target_state": "Every modified file has `touring ast blast` evidence.",
                "remediation": "Run `touring ast blast <file>` and embed the count + impacted files.",
            })
    return gaps


def detect_missing_test_name(plan_md: str) -> list[dict[str, str]]:
    """Subtasks with code change but no `Test: \\`name\\`` row."""
    gaps: list[dict[str, str]] = []
    for sub_id, body in _split_subtask_blocks(plan_md):
        has_code_change = "Change" in body or "```" in body
        if has_code_change and not _RE_TEST_LINE.search(body):
            gaps.append({
                "id": f"G-TEST-{sub_id}",
                "severity": "P1",
                "code": "MISSING_TEST_NAME",
                "subtask": f"S-{sub_id}",
                "current_state": f"S-{sub_id} declares a change but no named test.",
                "target_state": "Every change has a `Test: \\`test_<name>\\`` + assertion.",
                "remediation": "Add `Test: \\`test_<descriptive>\\`` with the assertion.",
            })
    return gaps


def detect_empty_enables(plan_md: str) -> list[dict[str, str]]:
    """REGRA #0 — every subtask must have non-empty Enables."""
    gaps: list[dict[str, str]] = []
    for sub_id, body in _split_subtask_blocks(plan_md):
        match = _RE_ENABLES_LINE.search(body)
        if match is None:
            gaps.append({
                "id": f"G-ENABLES-{sub_id}",
                "severity": "P1",
                "code": "EMPTY_ENABLES",
                "subtask": f"S-{sub_id}",
                "current_state": f"S-{sub_id} has no Enables row.",
                "target_state": "Every subtask names what future work it unlocks (REGRA #0).",
                "remediation": "Add `Enables: <future capability>` row.",
            })
            continue
        content = match.group(1).strip()
        if not content or content in {"—", "-", "(empty)", "None", "none"}:
            gaps.append({
                "id": f"G-ENABLES-{sub_id}",
                "severity": "P1",
                "code": "EMPTY_ENABLES",
                "subtask": f"S-{sub_id}",
                "current_state": f"S-{sub_id} Enables row is empty.",
                "target_state": "Non-empty Enables value (REGRA #0).",
                "remediation": "Rewrite the subtask to potentialize — what does it unlock?",
            })
    return gaps


def detect_orphan_not_addressed(plan_md: str, ground_truth: dict[str, Any] | None) -> list[dict[str, str]]:
    """ground_truth orphans not mentioned in the plan."""
    if ground_truth is None:
        return []
    orphans = ground_truth.get("wiring_orphans") or []
    gaps: list[dict[str, str]] = []
    for orphan in orphans[:20]:  # cap to keep output bounded
        name = orphan.get("name") or orphan.get("symbol", "")
        if not name:
            continue
        if str(name) not in plan_md:
            gaps.append({
                "id": f"G-ORPH-{name}",
                "severity": "P2",
                "code": "ORPHAN_NOT_ADDRESSED",
                "symbol": str(name),
                "current_state": f"Orphan `{name}` exists in repo but plan does not address it.",
                "target_state": "Either wire the orphan in a subtask or explicitly defer.",
                "remediation": f"Add subtask that connects `{name}` to a consumer (REGRA #0).",
            })
    return gaps


def detect_weak_confidence_density(plan_md: str) -> list[dict[str, str]]:
    """If < 50% of subtasks carry a confidence tag → flag P3."""
    blocks = _split_subtask_blocks(plan_md)
    if not blocks:
        return []
    tagged = sum(1 for _, body in blocks if _RE_CONFIDENCE.search(body))
    ratio = tagged / len(blocks)
    if ratio >= 0.5:
        return []
    return [{
        "id": "G-CONF-DENSITY",
        "severity": "P3",
        "code": "WEAK_CONFIDENCE_DENSITY",
        "current_state": f"Only {tagged}/{len(blocks)} subtasks carry a confidence tag ({ratio:.0%}).",
        "target_state": "≥ 50% of subtasks (target 100%) carry FACT/INFERENCE/SPECULATION.",
        "remediation": "Run `confidence_tagger.py --autofill`.",
    }]


def prioritize(gaps: list[dict[str, str]]) -> list[dict[str, str]]:
    order = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}
    return sorted(gaps, key=lambda g: order.get(g.get("severity", "P3"), 99))


def _should_fail(gaps: list[dict[str, str]], threshold: str) -> bool:
    if threshold == "none":
        return False
    if threshold == "any":
        return bool(gaps)
    order = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}
    limit = order.get(threshold, 0)
    return any(order.get(g.get("severity", "P3"), 99) <= limit for g in gaps)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="gap_detector", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown to inspect.")
    parser.add_argument("--ground-truth", type=Path, default=None,
                        help="ground_truth.json (enables invented-symbol + orphan checks).")
    parser.add_argument("--fail-on", choices=["P0", "P1", "P2", "P3", "any", "none"],
                        default="P0", help="Exit non-zero if a gap at or above this severity is found.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (gap_detector is read-only).")
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Detect."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)
    plan_md = args.path.read_text(encoding="utf-8")
    ground_truth = safe_load_json(args.ground_truth) if args.ground_truth else None

    gaps: list[dict[str, str]] = []
    gaps.extend(detect_invented_symbols(plan_md, ground_truth))
    gaps.extend(detect_missing_evidence(plan_md))
    gaps.extend(detect_vague_blast(plan_md))
    gaps.extend(detect_missing_test_name(plan_md))
    gaps.extend(detect_empty_enables(plan_md))
    gaps.extend(detect_orphan_not_addressed(plan_md, ground_truth))
    gaps.extend(detect_weak_confidence_density(plan_md))
    gaps = prioritize(gaps)

    counts: dict[str, int] = defaultdict(int)
    for gap in gaps:
        counts[gap.get("severity", "P3")] += 1

    report = {
        "status": "OK" if not gaps else "WARN",
        "script": "gap_detector",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "ground_truth_used": bool(ground_truth),
        "gaps_total": len(gaps),
        "severity_counts": dict(counts),
        "gaps": gaps,
    }
    if args.emit:
        out = args.output_dir / "gap_detection.json"
        write_json_atomic(out, report)
        report["json_path"] = str(out)

    report["_fail_threshold"] = args.fail_on
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
        threshold = result.pop("_fail_threshold", "P0")
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        if _should_fail(result.get("gaps", []), threshold):
            return EXIT_WARN
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("gap_detector failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
