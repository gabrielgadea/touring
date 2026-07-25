#!/usr/bin/env python3
"""confidence_tagger — Tag every plan claim with FACT/INFERENCE/SPECULATION.

Scans a plan markdown, identifies claims (subtask blocks, evidence statements),
classifies each by the strength of its evidence, and either:
  * --autofill : rewrites the file in place inserting the tags
  * --report   : emits a JSON report of proposed tags without modifying

Classification rules:
  FACT [1.0]        → cited `file:LINE` + Touring command referenced in same block
  INFERENCE [0.8]   → cited symbol but no command + no line number
  SPECULATION [0.5] → unverified claim, no evidence at all
  Downgrade by 0.2 when ground_truth.daemon_degraded is true.

Usage
-----
    python3 confidence_tagger.py plan.md --report -j
    python3 confidence_tagger.py plan.md --autofill   # mutates the plan
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
    safe_load_json,
    utcnow_iso,
    write_json_atomic,
)

_RE_SUBTASK_HEADER = re.compile(
    r"^(#{2,4}\s+S-(\d+(?:\.\d+)?)[^\n]*?)$",
    re.MULTILINE,
)
_RE_CONFIDENCE_PRESENT = re.compile(
    r"\bconfidence\s*[:=]\s*(?:FACT|INFERENCE|SPECULATION)\b",
    re.IGNORECASE,
)
_RE_FILE_LINE = re.compile(r"`[a-z_][a-z_0-9/]*\.(?:rs|py|ts|tsx|js|go):\d+")
_RE_TOURING_CMD = re.compile(r"touring\s+\w+")
_RE_PASCAL_SYMBOL = re.compile(r"`?\b[A-Z][a-zA-Z0-9]{2,}\b`?")


def classify_block(body: str) -> tuple[str, float]:
    """Classify a subtask block. Returns (level, score)."""
    has_file_line = bool(_RE_FILE_LINE.search(body))
    has_touring_cmd = bool(_RE_TOURING_CMD.search(body))
    has_symbol = bool(_RE_PASCAL_SYMBOL.search(body))

    if has_file_line and has_touring_cmd:
        return "FACT", 1.0
    if has_file_line or (has_symbol and has_touring_cmd):
        return "INFERENCE", 0.85
    if has_symbol:
        return "INFERENCE", 0.7
    return "SPECULATION", 0.5


def _adjust_for_degraded(level: str, score: float, degraded: bool) -> tuple[str, float]:
    """Daemon-down downgrades affected claims."""
    if not degraded:
        return level, score
    if level == "FACT":
        return "INFERENCE", max(0.7, score - 0.2)
    return level, max(0.4, score - 0.1)


def _enumerate_subtask_blocks(plan_md: str) -> list[dict[str, Any]]:
    """Return [{sub_id, header_text, header_start, header_end, body, body_end}, ...]."""
    matches = list(_RE_SUBTASK_HEADER.finditer(plan_md))
    blocks: list[dict[str, Any]] = []
    for idx, match in enumerate(matches):
        end = matches[idx + 1].start() if idx + 1 < len(matches) else len(plan_md)
        body = plan_md[match.end():end]
        blocks.append({
            "sub_id": match.group(2),
            "header_text": match.group(1),
            "header_start": match.start(),
            "header_end": match.end(),
            "body": body,
            "body_end": end,
            "already_tagged": bool(_RE_CONFIDENCE_PRESENT.search(match.group(0))),
        })
    return blocks


def tag_plan(plan_md: str, *, degraded: bool = False) -> tuple[str, list[dict[str, Any]]]:
    """Return (rewritten_md, proposals) — proposals describe per-block tagging."""
    blocks = _enumerate_subtask_blocks(plan_md)
    proposals: list[dict[str, Any]] = []

    # Build rewrite from end to start to preserve offsets
    pieces: list[str] = []
    cursor = 0
    for block in blocks:
        if block["already_tagged"]:
            continue
        level, score = classify_block(block["body"])
        level, score = _adjust_for_degraded(level, score, degraded)
        new_header = block["header_text"].rstrip()
        if "[confidence:" not in new_header:
            new_header += f" [confidence: {level} ({score:.2f})]"
        proposals.append({
            "subtask": f"S-{block['sub_id']}",
            "proposed_level": level,
            "proposed_score": score,
            "rationale": _explain_classification(block["body"], level),
        })
        pieces.append(plan_md[cursor:block["header_start"]])
        pieces.append(new_header)
        cursor = block["header_end"]

    pieces.append(plan_md[cursor:])
    return "".join(pieces), proposals


def _explain_classification(body: str, level: str) -> str:
    """One-line rationale used in the proposals report."""
    if level == "FACT":
        return "file:LINE + touring command present."
    if level == "INFERENCE":
        if _RE_FILE_LINE.search(body):
            return "file:LINE present but no touring command — needs verification."
        return "symbol/concept present but no exact location."
    return "no evidence anchors detected."


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="confidence_tagger", description=__doc__)
    parser.add_argument("path", type=Path, help="Plan markdown.")
    parser.add_argument("--ground-truth", type=Path, default=None,
                        help="ground_truth.json (read daemon_degraded flag).")
    parser.add_argument("--autofill", action="store_true",
                        help="MUTATING — rewrite the plan file in place adding tags.")
    parser.add_argument("--report", action="store_true",
                        help="Emit a proposals JSON without modifying.")
    parser.add_argument("--apply", action="store_true",
                        help="Alias for --autofill (consistency with other scripts).")
    parser.add_argument("--emit", action="store_true",
                        help="Write data/confidence_proposals.json.")
    parser.add_argument("--output-dir", type=Path, default=Path("data"))
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Tag."""
    if not args.path.exists():
        msg = f"Plan file not found: {args.path}"
        raise FileNotFoundError(msg)
    plan_md = args.path.read_text(encoding="utf-8")
    ground_truth = safe_load_json(args.ground_truth) if args.ground_truth else None
    degraded = bool(ground_truth and ground_truth.get("daemon_degraded"))

    rewritten, proposals = tag_plan(plan_md, degraded=degraded)
    mutate = args.autofill or args.apply

    if mutate:
        args.path.write_text(rewritten, encoding="utf-8")

    report = {
        "status": "OK",
        "script": "confidence_tagger",
        "timestamp": utcnow_iso(),
        "source": str(args.path),
        "mutating": mutate,
        "degraded": degraded,
        "proposals_count": len(proposals),
        "proposals": proposals,
    }
    if args.emit:
        out = args.output_dir / "confidence_proposals.json"
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
        logging.getLogger(__name__).exception("confidence_tagger failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
