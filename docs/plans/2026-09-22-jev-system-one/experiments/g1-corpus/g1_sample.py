#!/usr/bin/env python3
"""G1 sampling — build the labelling corpus from Touring's own data.

Three datasets, seeded and stratified, written OUTSIDE the repository (the touring
repo is public and these rows carry real prompts and memories):

  prompts.jsonl   human prompts from Claude Code transcripts (intent → CILA)
  memories.jsonl  memory entries with their key-prefix weak label (kind)
  pairs.jsonl     (prompt, recalled memory) pairs from the real recall pipeline

The machine-payload filter is read from the Rust executor
(`AUTOMATED_PAYLOAD_PREFIXES` in prompt_enhance.rs), so the corpus and the hook agree
on what a human prompt is by construction — one source of truth, not two lists.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import random
import re
import sqlite3
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

HOME = Path.home()
REPO = HOME / "projects/touring"
RUST_SOURCE = REPO / "crates/touring-hook-runtime/src/prompt_enhance.rs"
OUT_DIR = HOME / ".claude/touring/g1-corpus"
SEED = 20260922

MEMORY_CLASS_BY_PREFIX = {
    "lesson": "lesson", "licao": "lesson", "gotcha": "gotcha", "snippet": "snippet",
    "audit": "audit", "strategy": "strategy", "decisao": "decision", "decision": "decision",
    "fix": "fix", "pattern": "pattern", "report": "report", "insight": "insight",
}


def automated_prefixes() -> list[str]:
    """Read the machine-payload prefixes from the Rust executor (single source of truth)."""
    src = RUST_SOURCE.read_text(encoding="utf-8")
    block = re.search(r"AUTOMATED_PAYLOAD_PREFIXES: &\[&str\] = &\[(.*?)\];", src, re.S)
    if not block:
        raise SystemExit(f"AUTOMATED_PAYLOAD_PREFIXES not found in {RUST_SOURCE}")
    return [m.group(1) for m in re.finditer(r'"((?:[^"\\]|\\.)*)"', block.group(1))]


def is_machine(text: str, prefixes: list[str]) -> bool:
    head = text.lstrip()
    return any(head.startswith(p) for p in prefixes)


def ro(path: Path) -> sqlite3.Connection:
    """Open a Touring database strictly read-only."""
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)


def iter_transcript_prompts(prefixes: list[str]):
    """Yield (project, prompt) for every plausible human prompt in the transcripts."""
    for path in sorted((HOME / ".claude/projects").glob("*/*.jsonl")):
        project = path.parent.name
        try:
            with path.open(errors="ignore") as fh:
                for line in fh:
                    if '"type":"user"' not in line:
                        continue
                    try:
                        content = json.loads(line).get("message", {}).get("content")
                    except json.JSONDecodeError:
                        continue
                    if not isinstance(content, str):
                        continue
                    text = content.strip()
                    if len(text) < 12 or len(text) > 4000 or is_machine(text, prefixes):
                        continue
                    if text.startswith("<command-name>") and "<command-args>" not in text:
                        continue
                    yield project, text
        except OSError:
            continue


def sample_prompts(rng: random.Random, target: int, prefixes: list[str]) -> list[dict]:
    """Stratify by project, deduplicate by normalized text."""
    by_project: dict[str, list[str]] = defaultdict(list)
    seen: set[str] = set()
    for project, text in iter_transcript_prompts(prefixes):
        key = hashlib.sha1(" ".join(text.lower().split()).encode()).hexdigest()
        if key in seen:
            continue
        seen.add(key)
        by_project[project].append(text)
    projects = sorted(by_project)
    per_project = max(1, target // max(1, len(projects)))
    picked: list[dict] = []
    leftovers: list[tuple[str, str]] = []

    def row(project: str, text: str) -> dict:
        return {"id": f"p:{hashlib.sha1(text.encode()).hexdigest()[:12]}",
                "task": "prompt_intent", "project": project, "text": text}

    for project in projects:
        items = by_project[project]
        rng.shuffle(items)
        for text in items[:per_project]:
            picked.append(row(project, text))
        leftovers.extend((project, text) for text in items[per_project:])

    # A small project cannot fill its quota; take the rest from the shared pool so the
    # corpus reaches the target instead of silently shrinking.
    rng.shuffle(leftovers)
    for project, text in leftovers:
        if len(picked) >= target:
            break
        picked.append(row(project, text))
    rng.shuffle(picked)
    return picked[:target]


def sample_memories(rng: random.Random, target: int) -> list[dict]:
    """Stratify by the key-prefix class; keep the weak label for comparison."""
    conn = ro(REPO / ".claude/touring/memory.db")
    rows = conn.execute(
        "select key, value, entry_type from memory_entries where superseded_by is null"
    ).fetchall()
    derived = dict(conn.execute(
        "select entry_key, value from memory_tags where facet='kind' and source in ('auto','backfill')"))
    by_class: dict[str, list[dict]] = defaultdict(list)
    for key, value, entry_type in rows:
        if not value or len(value) < 60:
            continue
        cls = MEMORY_CLASS_BY_PREFIX.get(re.split(r"[:_/]", key)[0].lower())
        if not cls:
            continue
        by_class[cls].append({"id": f"m:{hashlib.sha1(key.encode()).hexdigest()[:12]}",
                              "task": "memory_kind", "key": key, "weak_label": cls,
                              "derived_tag": derived.get(key), "entry_type": entry_type,
                              "text": value[:2000]})
    per_class = max(1, target // max(1, len(by_class)))
    picked: list[dict] = []
    for cls in sorted(by_class):
        items = by_class[cls]
        rng.shuffle(items)
        picked.extend(items[:per_class])
    rng.shuffle(picked)
    return picked[:target]


def recall(query: str, limit: int) -> list[dict]:
    """Ask the real recall pipeline for candidates (CLI, read-only)."""
    # `memory recall` takes the query as positional words only — no --limit flag.
    proc = subprocess.run(
        ["touring", "memory", "recall", query],
        capture_output=True, text=True, timeout=60, cwd=str(REPO),
    )
    try:
        payload = json.loads(proc.stdout[proc.stdout.index("{"):])
    except (ValueError, json.JSONDecodeError):
        return []
    out: list[dict] = []
    # `entries` is the ranked list the recall actually serves (key/value/score/source).
    for item in payload.get("entries") or []:
        if isinstance(item, dict) and item.get("key"):
            out.append({"key": item["key"], "value": (item.get("value") or "")[:1200],
                        "channel": item.get("source") or "entries", "score": item.get("score")})
        if len(out) >= limit:
            break
    return out


def sample_pairs(rng: random.Random, prompts: list[dict], target: int, per_query: int) -> list[dict]:
    """Take real prompts as queries and the real recall output as candidates."""
    queries = [p for p in prompts if len(p["text"]) > 40]
    rng.shuffle(queries)
    pairs: list[dict] = []
    for prompt in queries:
        if len(pairs) >= target:
            break
        for cand in recall(prompt["text"][:300], per_query)[:per_query]:
            pairs.append({
                "id": f"r:{hashlib.sha1((prompt['id'] + cand['key']).encode()).hexdigest()[:12]}",
                "task": "recall_relevance", "query": prompt["text"][:600],
                "candidate_key": cand["key"], "channel": cand["channel"],
                "text": cand["value"],
            })
    return pairs[:target]


def write(name: str, rows: list[dict]) -> Path:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    path = OUT_DIR / name
    with path.open("w", encoding="utf-8") as fh:
        for row in rows:
            fh.write(json.dumps(row, ensure_ascii=False) + "\n")
    return path


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--prompts", type=int, default=800)
    ap.add_argument("--memories", type=int, default=600)
    ap.add_argument("--pairs", type=int, default=300)
    ap.add_argument("--per-query", type=int, default=5)
    args = ap.parse_args(argv)

    rng = random.Random(SEED)
    prefixes = automated_prefixes()
    prompts = sample_prompts(rng, args.prompts, prefixes)
    memories = sample_memories(rng, args.memories)
    pairs = sample_pairs(rng, prompts, args.pairs, args.per_query)

    stats = {
        "seed": SEED,
        "machine_prefixes_from_rust": len(prefixes),
        "prompts": {"n": len(prompts), "by_project": dict(Counter(p["project"] for p in prompts))},
        "memories": {"n": len(memories), "by_weak_label": dict(Counter(m["weak_label"] for m in memories))},
        "pairs": {"n": len(pairs), "queries": len({p["query"] for p in pairs}),
                  "by_channel": dict(Counter(p["channel"] for p in pairs))},
        "out_dir": str(OUT_DIR),
    }
    for name, rows in (("prompts.jsonl", prompts), ("memories.jsonl", memories), ("pairs.jsonl", pairs)):
        write(name, rows)
    (OUT_DIR / "sample_stats.json").write_text(json.dumps(stats, ensure_ascii=False, indent=1))
    print(json.dumps(stats, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
