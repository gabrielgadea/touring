#!/usr/bin/env python3
"""TACO-skilling triggering auditor — description optimization, zero LLM.

Measures the REAL triggering behaviour of a skill so its `description` can be
tuned from evidence. It NEVER invokes an LLM: the session history already
records what happened, and the Touring daemon is deterministic local code
intelligence — both are read, not simulated with paid `claude -p` calls.

Two deterministic data sources
------------------------------
1. Session transcripts (~/.claude/projects/*.jsonl) — the canonical record of
   what activated the skill and of every user prompt (the background corpus).
2. The Touring daemon — `memory recall` and the `tantivy` full-text index,
   mined for the intelligence Touring already holds about the skill.

Two activation modes are separated, because they mean different things:
  - explicit  — the user typed `/skill-name`; this does NOT test the
    description (the user named the skill directly).
  - automatic — Claude chose the skill from a plain task description; THIS is
    what the `description` field controls, so the term analysis uses only these.

Technique
---------
Best practices from scikit-learn text feature extraction (CountVectorizer /
TfidfVectorizer), applied in pure stdlib so the skill stays dependency-free:
  - Unicode-aware tokenization (``\\w`` — accented Portuguese words stay whole;
    the naive ``[a-zA-Z]`` regex fragments "excelência" into "excel"+"ncia");
  - unigrams + bigrams (ngram_range 1-2 — preserves local word order, so
    "auditoria cruzada" is one term);
  - TF-IDF-style distinctiveness — a term ranks high when it is frequent in the
    prompts that triggered the skill yet rare across the whole prompt corpus;
  - min_df filtering — a term must occur in >= 2 triggering prompts to count.

Output is a human report, or JSON with --json.
Exit code: 0 = report produced, 2 = bad arguments.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys
from collections import Counter
from typing import Any, Sequence

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402 — local sibling module

# Words too common to carry triggering signal (English + Portuguese + generic
# command verbs). TF-IDF demotes ubiquitous terms anyway; this trims obvious noise.
STOPWORDS: frozenset[str] = frozenset({
    # English
    "the", "a", "an", "to", "of", "for", "and", "or", "in", "on", "with", "is",
    "it", "be", "this", "that", "you", "we", "my", "me", "can", "could", "would",
    "should", "do", "does", "so", "if", "as", "at", "by", "from", "but", "not",
    "all", "any", "one", "have", "has", "was", "are", "want", "need", "get",
    # Portuguese — accented forms, matching the Unicode tokenizer's real output
    # (ASCII forms like "nao"/"voce" never appear once \w keeps accents whole).
    "um", "uma", "de", "da", "do", "das", "dos", "para", "que", "com", "ou",
    "no", "na", "nos", "nas", "em", "por", "se", "eu", "você", "nós", "meu",
    "minha", "não", "sim", "mais", "como", "isso", "esse", "essa", "esta",
    "este", "ao", "à", "então", "ele", "ela", "são", "está", "foi", "ter",
    "tem", "ser", "também", "vamos", "quero", "preciso", "faça", "fazer", "agora",
})

_CMD_ARGS = re.compile(r"<command-args>(.*?)</command-args>", re.DOTALL)
_CMD_NAME = re.compile(r"<command-name>(.*?)</command-name>", re.DOTALL)
_SYS_REMINDER = re.compile(r"<system-reminder>.*?</system-reminder>", re.DOTALL)
# \w is Unicode-aware in Python 3 — accented characters match, unlike [a-zA-Z].
_WORD = re.compile(r"\w[\w-]+")


def clean_user_prompt(text: str) -> str:
    """Extract the genuine human prompt from a user-message entry.

    Filters harness injections (skill bodies, system reminders) so the audit
    measures what the human asked, not what the harness prepended. For a slash
    command, returns the command name + its args. Returns "" when the entry
    carries no human prompt.
    """
    stripped = text.strip()
    if not stripped:
        return ""
    if stripped.startswith("Base directory for this skill:"):
        return ""
    args = _CMD_ARGS.search(text)
    if args:
        name = _CMD_NAME.search(text)
        prefix = name.group(1).strip() if name else ""
        return f"{prefix} {args.group(1).strip()}".strip()
    if stripped.startswith("<system-reminder>") and stripped.endswith("</system-reminder>"):
        return ""
    return _SYS_REMINDER.sub("", text).strip()


def activates_skill(entry: dict[str, Any], skill: str) -> bool:
    """Return True when a transcript entry activates the named skill."""
    msg = entry.get("message")
    if isinstance(msg, dict):
        content = msg.get("content")
        if isinstance(content, list):
            for block in content:
                if (isinstance(block, dict)
                        and block.get("type") == "tool_use"
                        and block.get("name") == "Skill"):
                    inp = block.get("input", {})
                    if isinstance(inp, dict):
                        value = f"{inp.get('skill', '')}{inp.get('command', '')}"
                        if skill.lower() in value.lower():
                            return True
    return f"/{skill}" in lib.message_text(entry)


def tokenize(text: str, ngram_max: int = 2) -> list[str]:
    """Return Unicode-aware tokens of a text: unigrams plus n-grams up to ngram_max.

    Stopwords are dropped before n-grams form, so a bigram never contains one.
    """
    words = [w for w in _WORD.findall(text.lower()) if w not in STOPWORDS]
    grams = list(words)
    for size in range(2, ngram_max + 1):
        grams.extend(
            " ".join(words[i:i + size]) for i in range(len(words) - size + 1)
        )
    return grams


def user_prompt_before(entries: list[dict[str, Any]], index: int) -> str:
    """Return the most recent genuine human prompt before ``index``."""
    for j in range(index - 1, -1, -1):
        if entries[j].get("type") == "user":
            prompt = clean_user_prompt(lib.message_text(entries[j]))
            if prompt:
                return prompt
    return ""


def collect_corpus(skill: str) -> dict[str, Any]:
    """Single pass over all transcripts.

    Returns the skill's triggering prompts (split auto/explicit) plus the global
    document-frequency table over every user prompt — the background a TF-IDF
    distinctiveness score needs.
    """
    trigger_auto: list[str] = []
    trigger_explicit: list[str] = []
    global_df: Counter[str] = Counter()
    n_docs = 0
    sessions = 0
    slash = f"/{skill}".lower()
    for path in lib.iter_transcripts():
        entries = list(lib.iter_jsonl(path))
        for entry in entries:
            if entry.get("type") == "user":
                prompt = clean_user_prompt(lib.message_text(entry))
                if prompt:
                    n_docs += 1
                    for term in set(tokenize(prompt)):
                        global_df[term] += 1
        session_hit = False
        for index, entry in enumerate(entries):
            if not activates_skill(entry, skill):
                continue
            session_hit = True
            prompt = user_prompt_before(entries, index)
            if not prompt:
                continue
            (trigger_explicit if slash in prompt.lower() else trigger_auto).append(prompt)
        if session_hit:
            sessions += 1
    return {
        "trigger_auto": trigger_auto,
        "trigger_explicit": trigger_explicit,
        "global_df": global_df,
        "n_docs": n_docs,
        "sessions": sessions,
    }


def distinctive_terms(trigger_prompts: list[str], global_df: Counter[str],
                      n_docs: int, top: int = 15, min_df: int = 2) -> list[dict[str, Any]]:
    """Rank terms by TF-IDF-style distinctiveness to the triggering prompts.

    Score = (fraction of triggering prompts containing the term) × smoothed IDF
    over the whole prompt corpus. A term ubiquitous across all prompts gets a
    low IDF and sinks; a term concentrated in the triggers rises.
    """
    trigger_df: Counter[str] = Counter()
    for prompt in trigger_prompts:
        for term in set(tokenize(prompt)):
            trigger_df[term] += 1
    n_trig = max(len(trigger_prompts), 1)
    n_bg = max(n_docs, 1)
    scored: list[dict[str, Any]] = []
    for term, freq in trigger_df.items():
        if freq < min_df:
            continue
        idf = math.log((1 + n_bg) / (1 + global_df.get(term, 0))) + 1.0
        scored.append({
            "term": term,
            "in_triggers": freq,
            "score": round((freq / n_trig) * idf, 3),
        })
    scored.sort(key=lambda row: row["score"], reverse=True)
    return scored[:top]


def description_of(skill: str) -> str:
    """Return the current description from a skill's SKILL.md frontmatter."""
    for installed in lib.list_skills():
        if installed["name"] == skill or os.path.basename(installed["dir"]) == skill:
            return installed["description"]
    return ""


def touring_intelligence(skill: str) -> dict[str, Any]:
    """Extract what the Touring daemon already knows about the skill.

    Touring is deterministic local code intelligence — not an LLM. Querying its
    semantic memory and full-text index is the cheap, correct way to enrich the
    audit with data Touring already holds. Degrades cleanly when the daemon is
    down.
    """
    if not lib.touring_available():
        return {"available": False, "memory_hits": [], "index_hits": 0}
    memory_hits: list[str] = []
    memory = lib.touring("memory", "recall", skill)
    if isinstance(memory, dict):
        for entry in memory.get("entries", [])[:6]:
            if isinstance(entry, dict):
                memory_hits.append(str(entry.get("value", ""))[:140])
    tantivy = lib.touring("tantivy", "search", skill)
    if isinstance(tantivy, list):
        index_hits = len(tantivy)
    elif isinstance(tantivy, dict):
        index_hits = len(tantivy.get("hits", tantivy.get("results", [])))
    else:
        index_hits = 0
    return {"available": True, "memory_hits": memory_hits, "index_hits": index_hits}


def audit(skill: str) -> dict[str, Any]:
    """Produce the full triggering audit for a skill."""
    corpus = collect_corpus(skill)
    auto = corpus["trigger_auto"]
    # Deduplicate before analysis: one prompt that triggered the skill several
    # times in a session must not dominate the TF-IDF score — repetition would
    # inflate every term's document frequency equally and flatten the ranking.
    unique_auto = list(dict.fromkeys(auto))
    # With few distinct prompts, require min_df=1 (any signal is indicative);
    # with a real sample, require min_df=2 to drop one-off noise.
    min_df = 2 if len(unique_auto) >= 5 else 1
    distinctive = distinctive_terms(
        unique_auto, corpus["global_df"], corpus["n_docs"], min_df=min_df
    )

    trigger_terms: set[str] = set()
    for prompt in unique_auto:
        trigger_terms.update(tokenize(prompt))
    description = description_of(skill)
    desc_terms = set(tokenize(description))

    return {
        "skill": skill,
        "description_present": bool(description),
        "automatic_activations": len(auto),
        "unique_trigger_prompts": len(unique_auto),
        "small_sample": len(unique_auto) < 5,
        "explicit_activations": len(corpus["trigger_explicit"]),
        "sessions": corpus["sessions"],
        "background_prompts": corpus["n_docs"],
        "sample_automatic_prompts": unique_auto[:10],
        "distinctive_terms": distinctive,
        "live_keywords": sorted(t for t in desc_terms if t in trigger_terms),
        "dead_keywords": sorted(t for t in desc_terms if t not in trigger_terms),
        "missing_distinctive": sorted(
            d["term"] for d in distinctive if d["term"] not in desc_terms
        ),
        "touring": touring_intelligence(skill),
    }


def main(argv: Sequence[str] | None = None) -> int:
    """Audit a skill's real triggering behaviour from session history + Touring."""
    parser = argparse.ArgumentParser(
        prog="triggering_audit.py",
        description="Audit a skill's real triggering from history + Touring (zero LLM).",
    )
    parser.add_argument("skill", help="Skill name to audit.")
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    args = parser.parse_args(argv)

    report = audit(args.skill)

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0

    print(f"== triggering audit: {report['skill']} ==\n")
    if not report["description_present"]:
        print("warning: no installed skill found with that name.\n")
    print(f"automatic activations: {report['automatic_activations']} "
          f"({report['unique_trigger_prompts']} distinct prompt(s) — these test the description)")
    print(f"explicit activations:  {report['explicit_activations']} "
          "(typed /slash — these do NOT test the description)")
    print(f"sessions:              {report['sessions']}")
    print(f"background prompts:     {report['background_prompts']} "
          "(corpus for TF-IDF distinctiveness)")

    tour = report["touring"]
    print("\n-- Touring intelligence (deterministic, zero LLM) --")
    if not tour["available"]:
        print("  daemon down — Touring enrichment skipped (daemon_degraded)")
    else:
        print(f"  index references (tantivy): {tour['index_hits']}")
        if tour["memory_hits"]:
            print("  memory recall:")
            for hit in tour["memory_hits"]:
                print(f"    - {hit}")
        else:
            print("  memory recall: (no entries)")

    if report["automatic_activations"] == 0:
        print("\nno automatic triggering history yet — the skill was never chosen")
        print("from a plain task description in a recorded session. Description")
        print("optimization from history needs real automatic usage; re-run this")
        print("audit once that accumulates. Until then the current description")
        print("stands on its authoring guidance.")
        return 0

    print("\nsample automatic-triggering prompts (deduplicated):")
    for prompt in report["sample_automatic_prompts"]:
        print(f"  - {prompt[:140]}")
    if report["small_sample"]:
        print("\n⚠ small sample (< 5 distinct triggering prompts) — the terms below")
        print("  are indicative only; statistical signal needs more real usage.")
    print("\ndistinctive triggering terms (TF-IDF — frequent in triggers, rare overall):")
    for row in report["distinctive_terms"]:
        print(f"  {row['score']:6.3f}  ×{row['in_triggers']:<2d}  {row['term']}")
    print(f"\nlive description keywords (seen in real prompts): "
          f"{', '.join(report['live_keywords']) or '(none)'}")
    print(f"dead description keywords (never in real prompts): "
          f"{', '.join(report['dead_keywords']) or '(none)'}")
    print(f"distinctive terms missing from description: "
          f"{', '.join(report['missing_distinctive']) or '(none)'}")
    print("\nThe model reads this and decides the description edits — "
          "the script did the analysis, zero LLM calls.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
