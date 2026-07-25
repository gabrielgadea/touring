#!/usr/bin/env python3
"""TACO-skilling transcript miner — REFINE Phase 1.

Scans real Claude Code session history (``~/.claude/projects/*/*.jsonl``) for
every session where a given skill was activated, and extracts the *feedback
signal* around each use: what the user said next, whether they corrected the
output, re-prompted, or hit an error while the skill was active.

This grounds REFINE in evidence instead of memory of the last chat. Pure stdlib,
no daemon required.

Exit code: 0 on success, 2 on bad arguments.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any, Sequence

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402 — local sibling module

# Phrases that signal the user was not satisfied with the skill's output.
CORRECTION_MARKERS = (
    "no,", "não", "nao,", "actually", "instead", "wrong", "that's not",
    "isn't right", "didn't", "not what", "errado", "na verdade", "again",
    "de novo", "fix this", "corrige", "should have", "deveria",
)


def activates_skill(entry: dict[str, Any], skill: str) -> bool:
    """Return True when a transcript entry activates the named skill.

    Recognizes the ``Skill`` tool_use block and the ``/skill`` slash form.
    """
    msg = entry.get("message")
    if isinstance(msg, dict):
        content = msg.get("content")
        if isinstance(content, list):
            for block in content:
                if not isinstance(block, dict):
                    continue
                if block.get("type") == "tool_use" and block.get("name") == "Skill":
                    inp = block.get("input", {})
                    if isinstance(inp, dict):
                        value = f"{inp.get('skill', '')}{inp.get('command', '')}"
                        if skill.lower() in value.lower():
                            return True
    text = lib.message_text(entry)
    return f"/{skill}" in text or f"skill: {skill}" in text.lower()


def scan_session(path: Path, skill: str, window: int) -> dict[str, Any]:
    """Find skill activations in one transcript plus the feedback that followed."""
    entries = list(lib.iter_jsonl(path))
    uses = 0
    corrections = 0
    errors = 0
    samples: list[str] = []
    for index, entry in enumerate(entries):
        if not activates_skill(entry, skill):
            continue
        uses += 1
        for follow in entries[index + 1: index + 1 + window]:
            result = follow.get("toolUseResult")
            if result and "error" in str(result).lower():
                errors += 1
            if follow.get("type") == "user":
                text = lib.message_text(follow)
                lowered = text.lower()
                if text and any(marker in lowered for marker in CORRECTION_MARKERS):
                    corrections += 1
                    if len(samples) < 5:
                        samples.append(text.strip()[:200])
    return {"uses": uses, "corrections": corrections, "errors": errors, "samples": samples}


def mine(skill: str, window: int = 6, max_files: int = 400) -> dict[str, Any]:
    """Aggregate a skill's usage + feedback signal across all sessions."""
    totals = {"uses": 0, "corrections": 0, "errors": 0}
    sessions_with_use = 0
    samples: list[str] = []
    for index, path in enumerate(lib.iter_transcripts()):
        if index >= max_files:
            break
        result = scan_session(path, skill, window)
        if not result["uses"]:
            continue
        sessions_with_use += 1
        totals["uses"] += result["uses"]
        totals["corrections"] += result["corrections"]
        totals["errors"] += result["errors"]
        for sample in result["samples"]:
            if len(samples) < 12:
                samples.append(sample)
    correction_rate = totals["corrections"] / totals["uses"] if totals["uses"] else 0.0
    return {
        "skill": skill,
        "sessions_with_use": sessions_with_use,
        "total_activations": totals["uses"],
        "corrections": totals["corrections"],
        "errors_during_use": totals["errors"],
        "correction_rate": round(correction_rate, 2),
        "correction_samples": samples,
    }


def main(argv: Sequence[str] | None = None) -> int:
    """Mine session transcripts for a skill's usage and feedback signal."""
    parser = argparse.ArgumentParser(
        prog="mine_transcripts.py",
        description="Mine Claude Code session transcripts for a skill's feedback signal.",
    )
    parser.add_argument("skill", help="Skill name to mine for.")
    parser.add_argument("--window", type=int, default=6,
                        help="Number of entries to inspect after each activation.")
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    args = parser.parse_args(argv)

    report = mine(args.skill, window=args.window)

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0

    print(f"== transcript mining: {report['skill']} ==\n")
    print(f"sessions that used the skill: {report['sessions_with_use']}")
    print(f"total activations:           {report['total_activations']}")
    print(f"corrections after use:       {report['corrections']}")
    print(f"errors during use:           {report['errors_during_use']}")
    print(f"correction rate:             {report['correction_rate']:.0%}")
    if report["correction_samples"]:
        print("\ncorrection signal (what users said after the skill ran):")
        for sample in report["correction_samples"]:
            print(f"  - {sample}")
    else:
        print("\nno correction signal found — either the skill performs well,")
        print("or it has not been used enough yet to mine. Treat low data with caution.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
