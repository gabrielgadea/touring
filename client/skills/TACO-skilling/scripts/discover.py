#!/usr/bin/env python3
"""TACO-skilling discovery — CREATE Phase 1.

Answers the three questions that decide whether a new skill should exist:

1. Does an existing skill already cover this intent? (dedup — Rule #3)
2. Has the user actually done this task repeatedly? (Rule #1 — a skill earns its
   permanent context cost only for recurring work)
3. What past lessons apply? (``touring memory recall``)

Output is a human report, or JSON with ``--json``. This script never creates
anything — it only informs the create / extend / compose / decline decision.

Exit code: 0 always (a discovery run cannot "fail"); 2 on bad arguments.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from typing import Any, Sequence

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402 — local sibling module

STOPWORDS = {
    "the", "a", "an", "to", "of", "for", "and", "or", "in", "on", "with",
    "skill", "create", "build", "make", "that", "this", "is", "it", "be",
    "um", "uma", "de", "para", "que", "com", "criar", "uma",
}


def keywords(text: str) -> set[str]:
    """Return the lowercased significant words of a free-text intent."""
    words = re.findall(r"[a-zA-Z][a-zA-Z0-9_-]{2,}", text.lower())
    return {w for w in words if w not in STOPWORDS}


def overlap_score(intent_kw: set[str], skill: dict[str, str]) -> float:
    """Fraction of intent keywords appearing in a skill's name + description."""
    if not intent_kw:
        return 0.0
    haystack = (skill["name"] + " " + skill["description"]).lower()
    hits = sum(1 for kw in intent_kw if kw in haystack)
    return hits / len(intent_kw)


def find_overlaps(intent: str, threshold: float = 0.5) -> list[dict[str, Any]]:
    """Return existing skills whose keyword overlap with the intent clears the threshold."""
    intent_kw = keywords(intent)
    matches: list[dict[str, Any]] = []
    for skill in lib.list_skills():
        score = overlap_score(intent_kw, skill)
        if score >= threshold:
            matches.append({
                "name": skill["name"],
                "dir": skill["dir"],
                "overlap": round(score, 2),
            })
    return sorted(matches, key=lambda m: m["overlap"], reverse=True)


def count_repetition(intent: str, max_files: int = 400) -> int:
    """Count transcripts whose user messages touch the intent keywords.

    A rough proxy for "is this task actually repeated" — a skill that would fire
    only once does not earn its permanent description cost (Rule #1).
    """
    intent_kw = keywords(intent)
    if not intent_kw:
        return 0
    # Require a high fraction of the intent's keywords to co-occur in a single
    # user message. A loose threshold saturates (most sessions mention 2-3
    # common words), turning the repetition signal into noise.
    needed = max(3, round(len(intent_kw) * 0.7))
    hits = 0
    for index, path in enumerate(lib.iter_transcripts()):
        if index >= max_files:
            break
        for entry in lib.iter_jsonl(path):
            if entry.get("type") != "user":
                continue
            text = lib.message_text(entry).lower()
            if sum(1 for kw in intent_kw if kw in text) >= needed:
                hits += 1
                break
    return hits


def recall_lessons(intent: str) -> list[str]:
    """Return past lessons relevant to the intent via touring memory (empty if down)."""
    result = lib.touring("memory", "recall", intent)
    lessons: list[str] = []
    if isinstance(result, dict):
        for entry in result.get("entries", [])[:8]:
            if isinstance(entry, dict):
                lessons.append(str(entry.get("value", ""))[:160])
    return lessons


def recommend(overlaps: list[dict[str, Any]], repetition: int) -> str:
    """Return the create / extend / compose / decline verdict."""
    if overlaps and overlaps[0]["overlap"] >= 0.7:
        top = overlaps[0]
        return (f"EXTEND — '{top['name']}' already overlaps {top['overlap']:.0%}; "
                f"refine it instead of creating a new skill")
    if overlaps:
        names = ", ".join(o["name"] for o in overlaps)
        return f"COMPOSE — partial overlap with {names}; compose rather than duplicate"
    if repetition == 0:
        return ("DECLINE? — no prior occurrences in session history; this may be a "
                "one-off better served by a plain prompt")
    return (f"CREATE — no overlapping skill; task seen in ~{repetition} past "
            f"session(s), repetition justifies a skill")


def main(argv: Sequence[str] | None = None) -> int:
    """Run discovery for a proposed skill intent."""
    parser = argparse.ArgumentParser(
        prog="discover.py",
        description="TACO-skilling discovery — dedup, repetition check, lesson recall.",
    )
    parser.add_argument("intent", help="Free-text description of the proposed skill.")
    parser.add_argument("--json", action="store_true", help="Emit JSON instead of a report.")
    parser.add_argument("--threshold", type=float, default=0.5,
                        help="Keyword-overlap threshold for flagging an existing skill.")
    args = parser.parse_args(argv)

    overlaps = find_overlaps(args.intent, args.threshold)
    repetition = count_repetition(args.intent)
    lessons = recall_lessons(args.intent)
    verdict = recommend(overlaps, repetition)
    daemon = lib.touring_available()

    report = {
        "intent": args.intent,
        "overlapping_skills": overlaps,
        "repetition_sessions": repetition,
        "past_lessons": lessons,
        "daemon_available": daemon,
        "recommendation": verdict,
    }

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0

    print("== TACO-skilling discovery ==")
    print(f"intent: {args.intent}\n")
    print(f"overlapping skills ({len(overlaps)}):")
    for over in overlaps:
        print(f"  - {over['name']:32s} overlap {over['overlap']:.0%}")
    if not overlaps:
        print("  (none — no existing skill covers this)")
    print(f"\nrepetition: task seen in ~{repetition} past session(s)")
    if not daemon:
        print("past lessons: (touring daemon down — recall skipped, daemon_degraded)")
    else:
        print(f"past lessons ({len(lessons)}):")
        for lesson in lessons:
            print(f"  - {lesson}")
    print(f"\nRECOMMENDATION: {verdict}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
