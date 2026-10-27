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

What counts as an activation
----------------------------
Only two things, never a text match (a path `.../skills/<skill>/SKILL.md` appears
all over transcripts — prompts, tool results, API blobs — and once inflated 3 real
activations to 66):
  - a `tool_use` of the Skill tool whose `input.skill` names the skill
    (deduplicated by tool_use id: resumed sessions re-copy their history);
  - a `/skill-name` the user typed (command record, or first word of the prompt).

Two activation modes are separated, because they mean different things:
  - explicit  — the user named the skill (typed `/skill-name`); this does NOT
    test the description.
  - automatic — Claude chose the skill from a plain task description; THIS is
    what the `description` field controls, so the term analysis uses only these.

Where the skill is looked up
----------------------------
By name: `--skills-dir` dirs (in order), `<--project>/.claude/skills`,
`<cwd>/.claude/skills`, then `~/.claude/skills`; first match wins and the report
names the directory it read. Or by path: pass the skill directory (or its SKILL.md)
as the argument. Transcripts are always read from `~/.claude/projects`, all projects.

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

Output is a human report, or JSON with --json. The report always states its
`evidence`: "automatic" (the description was exercised), "explicit_only" or "none"
(it was NOT measured — zero activations is not an approval).
Exit code: 0 = report produced (check `evidence`), 2 = bad arguments,
3 = skill not found (nothing was measured).
"""
from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any, Sequence

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402 — local sibling module

EXIT_NOT_FOUND = 3

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
# Punctuation that may hug a `/skill` word in prose ("use /skill, then ...").
_PUNCT = ".,;:!?()[]{}\"'`"


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


def _names_skill(ref: str, names: frozenset[str]) -> bool:
    """True when a skill reference (``name``, ``/name`` or ``plugin:name``) is one of ``names``.

    Exact match — ``external-apis-extra`` or ``plan-protocol`` are other skills, not
    hits for ``external-apis`` / ``plan``.
    """
    ref = ref.strip().lstrip("/").lower()
    return bool(ref) and (ref in names or ref.rsplit(":", 1)[-1] in names)


def _skill_call_id(block: Any, names: frozenset[str]) -> str | None:
    """Return the tool_use id of a ``Skill`` call naming the skill; None for any other block.

    An id-less call yields ``""`` (it cannot be deduplicated).
    """
    if not (isinstance(block, dict) and block.get("type") == "tool_use"
            and block.get("name") == "Skill"):
        return None
    inp = block.get("input")
    if isinstance(inp, dict) and _names_skill(
            str(inp.get("skill") or inp.get("command") or ""), names):
        return str(block.get("id") or "")
    return None


def skill_tool_use_ids(entry: dict[str, Any], names: frozenset[str]) -> list[str]:
    """Return the ids of the ``Skill`` tool_use blocks in an entry that name the skill.

    A real tool_use of the Skill tool is the only proof Claude activated it.
    """
    msg = entry.get("message")
    content = msg.get("content") if isinstance(msg, dict) else None
    if not isinstance(content, list):
        return []
    ids = (_skill_call_id(block, names) for block in content)
    return [tool_id for tool_id in ids if tool_id is not None]


def prompt_mentions_slash(prompt: str, names: frozenset[str]) -> bool:
    """True when the prompt has a standalone ``/<skill>`` word — the user named it directly.

    A path such as ``/home/u/.claude/skills/<skill>/SKILL.md`` is one word that is
    NOT ``/<skill>``, so quoting a path does not turn an automatic activation explicit.
    """
    for token in prompt.split():
        word = token.strip(_PUNCT)
        if word.startswith("/") and _names_skill(word, names):
            return True
    return False


def is_typed_slash(entry: dict[str, Any], names: frozenset[str]) -> bool:
    """True when the USER typed ``/<skill>`` as the command of this message (explicit).

    Reads the command record (``<command-name>``) when present, else the first
    word of the human prompt. Only ``user`` entries qualify — tool results, API
    blobs and assistant text can quote ``/<skill>`` without anyone typing it.
    """
    if entry.get("type") != "user":
        return False
    text = lib.message_text(entry)
    command = _CMD_NAME.search(text)
    if command:
        return _names_skill(command.group(1), names)
    words = clean_user_prompt(text).split(None, 1)
    return bool(words) and prompt_mentions_slash(words[0], names)


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


def last_user_prompt(entries: list[dict[str, Any]], index: int) -> tuple[int, str]:
    """Return ``(position, text)`` of the latest genuine human prompt before ``index``.

    ``(-1, "")`` when there is none.
    """
    for j in range(index - 1, -1, -1):
        if entries[j].get("type") == "user":
            prompt = clean_user_prompt(lib.message_text(entries[j]))
            if prompt:
                return j, prompt
    return -1, ""


def _first_time(seen: set[str], kind: str, key: Any) -> bool:
    """Record ``kind:key`` in ``seen``; False when it was already there.

    A missing key cannot be deduplicated, so it always counts.
    """
    if not key:
        return True
    token = f"{kind}:{key}"
    if token in seen:
        return False
    seen.add(token)
    return True


def scan_activations(entries: list[dict[str, Any]], names: frozenset[str],
                     seen: set[str]) -> list[tuple[str, str]]:
    """Return the real activations of one transcript as ``(kind, human prompt)``.

    ``kind`` is ``"explicit"`` (the user typed ``/skill`` or quoted it as a word) or
    ``"auto"`` (Claude chose the skill). ``seen`` carries tool_use ids and command
    uuids across transcripts so a resumed session's re-copied history counts once.
    """
    found: list[tuple[str, str]] = []
    counted_prompt = -1  # position of a typed /skill already counted as an activation
    for index, entry in enumerate(entries):
        if is_typed_slash(entry, names):
            counted_prompt = index
            if _first_time(seen, "slash", entry.get("uuid")):
                found.append(("explicit", clean_user_prompt(lib.message_text(entry))))
            continue
        for tool_id in skill_tool_use_ids(entry, names):
            if not _first_time(seen, "tool", tool_id):
                continue
            prompt_at, prompt = last_user_prompt(entries, index)
            if prompt_at == counted_prompt:
                continue  # the Skill call that expands a typed /skill: same activation
            kind = "explicit" if prompt_mentions_slash(prompt, names) else "auto"
            found.append((kind, prompt))
    return found


def collect_corpus(names: frozenset[str], projects_dir: Path | None = None) -> dict[str, Any]:
    """Single pass over all transcripts.

    Returns the skill's real activations (counts split auto/explicit, plus the
    automatic triggering prompts) and the global document-frequency table over
    every user prompt — the background a TF-IDF distinctiveness score needs.
    """
    trigger_auto: list[str] = []
    counts: Counter[str] = Counter()
    global_df: Counter[str] = Counter()
    seen: set[str] = set()
    n_docs = 0
    sessions = 0
    scanned = 0
    for path in lib.iter_transcripts(projects_dir):
        scanned += 1
        entries = list(lib.iter_jsonl(path))
        for entry in entries:
            if entry.get("type") == "user":
                prompt = clean_user_prompt(lib.message_text(entry))
                if prompt:
                    n_docs += 1
                    for term in set(tokenize(prompt)):
                        global_df[term] += 1
        hits = scan_activations(entries, names, seen)
        sessions += bool(hits)
        for kind, prompt in hits:
            counts[kind] += 1
            if kind == "auto" and prompt:
                trigger_auto.append(prompt)
    return {
        "trigger_auto": trigger_auto,
        "auto_count": counts["auto"],
        "explicit_count": counts["explicit"],
        "global_df": global_df,
        "n_docs": n_docs,
        "sessions": sessions,
        "transcripts_scanned": scanned,
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


def evidence_of(auto: int, explicit: int) -> str:
    """Classify what the transcripts prove about the description.

    ``"automatic"`` — Claude chose the skill, so the description was exercised;
    ``"explicit_only"`` — only typed ``/skill``, which never tests it;
    ``"none"`` — no real activation at all: nothing was measured.
    """
    if auto:
        return "automatic"
    return "explicit_only" if explicit else "none"


def audit(skill_ref: str, skills_dirs: Sequence[Path] | None = None,
          projects_dir: Path | None = None) -> dict[str, Any]:
    """Produce the full triggering audit for a skill (by name or by path)."""
    dirs = list(skills_dirs) if skills_dirs is not None else lib.default_skill_dirs()
    found = lib.find_skill(skill_ref, dirs)
    if found is None:
        searched = [skill_ref] if lib.is_path_ref(skill_ref) else [str(d) for d in dirs]
        return {
            "skill": skill_ref,
            "skill_found": False,
            "searched_dirs": searched,
            "description_present": False,
            "evidence": "skill_not_found",
        }
    skill = found["name"]
    names = frozenset({skill.lower(), os.path.basename(found["dir"]).lower()})
    corpus = collect_corpus(names, projects_dir)
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
    description = found["description"]
    desc_terms = set(tokenize(description))

    return {
        "skill": skill,
        "skill_found": True,
        "skill_dir": found["dir"],
        "searched_dirs": [str(d) for d in dirs],
        "description_present": bool(description),
        "evidence": evidence_of(corpus["auto_count"], corpus["explicit_count"]),
        "transcripts_scanned": corpus["transcripts_scanned"],
        "automatic_activations": corpus["auto_count"],
        "unique_trigger_prompts": len(unique_auto),
        "small_sample": len(unique_auto) < 5,
        "explicit_activations": corpus["explicit_count"],
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


def print_header(report: dict[str, Any]) -> None:
    """Print the activation counts and the Touring enrichment (any evidence level)."""
    print(f"== triggering audit: {report['skill']} ==\n")
    print(f"skill read from:       {report['skill_dir']}")
    if not report["description_present"]:
        print("warning: the SKILL.md has no `description` in its frontmatter.")
    print(f"transcripts scanned:   {report['transcripts_scanned']}")
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
        return
    print(f"  index references (tantivy): {tour['index_hits']}")
    if tour["memory_hits"]:
        print("  memory recall:")
        for hit in tour["memory_hits"]:
            print(f"    - {hit}")
    else:
        print("  memory recall: (no entries)")


def print_no_evidence(report: dict[str, Any]) -> None:
    """State, unmistakably, that the description was not measured."""
    skill = report["skill"]
    if report["evidence"] == "none":
        print(f"\nNO EVIDENCE: 0 real activations of '{skill}' (Skill tool_use or typed "
              f"/{skill}) in {report['transcripts_scanned']} transcript(s).")
        print("This is not an approval — the description was NOT measured. Re-run once the")
        print("skill accumulates real usage; until then it stands on its authoring guidance.")
    else:
        print(f"\nNO AUTOMATIC EVIDENCE: the {report['explicit_activations']} activation(s) "
              f"of '{skill}' were typed /{skill}, which names the skill directly and so")
        print("does not test the description. This is not an approval — the description was")
        print("NOT measured. Re-run once Claude chooses the skill from a plain task description.")


def print_terms(report: dict[str, Any]) -> None:
    """Print the sample prompts and the description-vs-triggers term analysis."""
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


def main(argv: Sequence[str] | None = None) -> int:
    """Audit a skill's real triggering behaviour from session history + Touring."""
    parser = argparse.ArgumentParser(
        prog="triggering_audit.py",
        description="Audit a skill's real triggering from history + Touring (zero LLM).",
        epilog=(
            "Skill lookup (first match wins; the report names the directory read): "
            "--skills-dir dirs, --project/.claude/skills, <cwd>/.claude/skills, "
            "~/.claude/skills — or pass a path. "
            "Activations are real Skill tool_use or typed /skill, never text matches. "
            "Exit: 0 report produced (read 'evidence': none/explicit_only is NOT an "
            "approval), 2 bad arguments, 3 skill not found."
        ),
    )
    parser.add_argument("skill", help="Skill name, or the path of a skill directory "
                                      "(or its SKILL.md) — e.g. a project skill.")
    parser.add_argument("--project", metavar="DIR",
                        help="Project root: also search DIR/.claude/skills.")
    parser.add_argument("--skills-dir", action="append", default=[], metavar="DIR",
                        help="Extra directory holding <skill>/SKILL.md; repeatable; searched first.")
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    args = parser.parse_args(argv)

    dirs = lib.default_skill_dirs(args.skills_dir, args.project)
    report = audit(args.skill, dirs)

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
    if not report["skill_found"]:
        print(f"error: skill not found: {args.skill!r} — searched: "
              f"{', '.join(report['searched_dirs'])}. Nothing was measured.", file=sys.stderr)
        return EXIT_NOT_FOUND
    if args.json:
        return 0

    print_header(report)
    if report["evidence"] != "automatic":
        print_no_evidence(report)
        return 0
    print_terms(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
