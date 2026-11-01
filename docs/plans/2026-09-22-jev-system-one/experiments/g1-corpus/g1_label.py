#!/usr/bin/env python3
"""G1 labelling — a teacher labels the corpus with the same typed questions a
System One model would answer (Choice for intent and kind, a probability for relevance).

The teacher is Claude in headless mode (`claude -p`), because the corpus carries the
user's own prompts and memories: that data already went to this provider in the
sessions it came from, so labelling it here adds no new transfer. The output is a soft
target for a local student, so every answer carries the teacher's own confidence.

Contracts that matter (the lessons this repo already paid for):
  * batches are cached by content hash — a rerun costs nothing and is resumable;
  * a malformed or incomplete answer is retried at most 3 times, with the parse error
    fed back as the correction, and the 4th attempt changes nothing silently: the batch
    is recorded as failed;
  * headless children run with TOURING_WORK_OUTER_DISABLED=1 (a spawned agent inherits
    the session hooks otherwise);
  * labels are validated against the vocabulary — never trusted as free text.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

CORPUS = Path.home() / ".claude/touring/g1-corpus"
CACHE = CORPUS / "cache"
MAX_ATTEMPTS = 3

INTENTS = ["code", "debug", "refactor", "test", "analysis", "creative", "plan", "general"]
KINDS = ["lesson", "gotcha", "snippet", "audit", "strategy", "decision", "fix", "pattern",
         "report", "other"]

TASKS: dict[str, dict] = {
    "prompt_intent": {
        "file": "prompts.jsonl",
        "batch": 10,
        "field": "text",
        "limit": 700,
        "vocab": INTENTS,
        "instructions": (
            "Each item is a real prompt a developer sent to a coding agent, in Portuguese or "
            "English. Decide what the person is asking for. Labels: "
            "code (write or implement new code), debug (find or fix a bug, error or failure), "
            "refactor (restructure existing code without changing behaviour), "
            "test (write or run tests), analysis (explain, review, research or analyse), "
            "plan (plan steps, phases or a roadmap), creative (propose ideas, alternatives or "
            "strategies), general (anything else: greetings, short replies, operational chatter). "
            "Judge the dominant request, not the tone."
        ),
    },
    "memory_kind": {
        "file": "memories.jsonl",
        "batch": 8,
        "field": "text",
        "limit": 900,
        "vocab": KINDS,
        "instructions": (
            "Each item is a note an agent stored in its long-term memory. Decide what kind of "
            "note it is: lesson (a reusable principle), gotcha (a specific trap and how to avoid "
            "it), snippet (reusable code or command), audit (findings or verdicts of a review), "
            "strategy (a plan for future work), decision (a decision taken, with its rationale), "
            "fix (a concrete correction applied), pattern (a recurring design or idiom), "
            "report (a status or session report), other (none of the above)."
        ),
    },
    "recall_relevance": {
        "file": "pairs.jsonl",
        "batch": 10,
        "field": "text",
        "limit": 600,
        "vocab": ["yes", "no"],
        "instructions": (
            "Each item is a developer's request (query) and one memory the recall pipeline "
            "retrieved for it (candidate). Answer whether the candidate would actually help "
            "someone working on that request: yes or no. A candidate that merely shares words "
            "with the query is not helpful."
        ),
    },
}


def payload_for(task: str, rows: list[dict]) -> str:
    spec = TASKS[task]
    items = []
    for row in rows:
        item = {"id": row["id"], "text": row[spec["field"]][: spec["limit"]]}
        if task == "recall_relevance":
            item = {"id": row["id"], "query": row["query"][:400],
                    "candidate": row["text"][: spec["limit"]]}
        items.append(item)
    vocab = "|".join(spec["vocab"])
    return (
        f"{spec['instructions']}\n\n"
        f"Answer with JSON only: a list of objects "
        f'[{{"id": "<id>", "label": "<{vocab}>", "confidence": <0.0-1.0>}}], '
        f"one per item, same ids, no prose, no markdown fence. confidence is your own "
        f"certainty about that label.\n\nITEMS:\n"
        + json.dumps(items, ensure_ascii=False)
    )


def call_teacher(prompt: str, model: str, timeout: int) -> str:
    """One headless Claude call; returns the raw result text."""
    env = dict(os.environ, TOURING_WORK_OUTER_DISABLED="1")
    proc = subprocess.run(
        ["claude", "-p", prompt, "--output-format", "json", "--model", model],
        capture_output=True, text=True, timeout=timeout, env=env,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"claude exited {proc.returncode}: {proc.stderr[:300]}")
    try:
        return json.loads(proc.stdout).get("result", "")
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"claude wrapper JSON invalid: {exc}") from exc


def parse_labels(raw: str, expected_ids: set[str], vocab: list[str]) -> list[dict]:
    """Parse and validate the teacher's answer; raises ValueError with what to fix."""
    text = raw.strip()
    match = re.search(r"\[.*\]", text, re.S)
    if not match:
        raise ValueError("no JSON list found in the answer")
    data = json.loads(match.group(0))
    if not isinstance(data, list):
        raise ValueError("the JSON root must be a list")
    out, seen = [], set()
    for entry in data:
        if not isinstance(entry, dict):
            raise ValueError("every element must be an object")
        ident, label = entry.get("id"), str(entry.get("label", "")).lower().strip()
        if ident not in expected_ids:
            raise ValueError(f"unknown id {ident!r}")
        if label not in vocab:
            raise ValueError(f"label {label!r} is outside the vocabulary {vocab}")
        conf = entry.get("confidence", None)
        try:
            conf = min(1.0, max(0.0, float(conf)))
        except (TypeError, ValueError):
            conf = None
        out.append({"id": ident, "label": label, "confidence": conf})
        seen.add(ident)
    missing = expected_ids - seen
    if missing:
        raise ValueError(f"{len(missing)} ids missing: {sorted(missing)[:5]}")
    return out


def label_batch(task: str, rows: list[dict], model: str, timeout: int) -> dict:
    """Cached, retried labelling of one batch."""
    spec = TASKS[task]
    prompt = payload_for(task, rows)
    digest = hashlib.sha1(f"{task}|{model}|{prompt}".encode()).hexdigest()
    cache_file = CACHE / f"{task}-{digest}.json"
    if cache_file.is_file():
        return json.loads(cache_file.read_text())

    expected = {row["id"] for row in rows}
    errors: list[str] = []
    for attempt in range(1, MAX_ATTEMPTS + 1):
        ask = prompt if attempt == 1 else (
            f"{prompt}\n\nYour previous answer was rejected: {errors[-1]}. "
            f"Return only the JSON list, with every id exactly once."
        )
        started = time.perf_counter()
        try:
            raw = call_teacher(ask, model, timeout)
            labels = parse_labels(raw, expected, spec["vocab"])
        except (RuntimeError, ValueError, json.JSONDecodeError, subprocess.TimeoutExpired) as exc:
            errors.append(f"{type(exc).__name__}: {exc}")
            continue
        result = {"task": task, "model": model, "attempt": attempt,
                  "seconds": round(time.perf_counter() - started, 1),
                  "labels": labels, "errors": errors}
        CACHE.mkdir(parents=True, exist_ok=True)
        cache_file.write_text(json.dumps(result, ensure_ascii=False))
        return result
    return {"task": task, "model": model, "attempt": MAX_ATTEMPTS, "labels": [],
            "failed": True, "errors": errors, "ids": sorted(expected)}


def load(task: str) -> list[dict]:
    path = CORPUS / TASKS[task]["file"]
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def run_task(task: str, model: str, workers: int, timeout: int, limit: int | None) -> dict:
    rows = load(task)
    if limit:
        rows = rows[:limit]
    size = TASKS[task]["batch"]
    batches = [rows[i:i + size] for i in range(0, len(rows), size)]
    labels: list[dict] = []
    failed_batches, attempts, seconds = 0, Counter(), 0.0
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = {pool.submit(label_batch, task, batch, model, timeout): batch for batch in batches}
        for done, future in enumerate(as_completed(futures), 1):
            result = future.result()
            if result.get("failed"):
                failed_batches += 1
                print(f"  [{task}] batch FAILED after {MAX_ATTEMPTS}: {result['errors'][-1][:120]}",
                      flush=True)
                continue
            labels.extend(result["labels"])
            attempts[result["attempt"]] += 1
            seconds += result.get("seconds", 0.0)
            if done % 10 == 0:
                print(f"  [{task}] {done}/{len(batches)} batches, {len(labels)} labels", flush=True)

    by_id = {row["id"]: row for row in rows}
    out_path = CORPUS / f"labels_{task}.jsonl"
    with out_path.open("w", encoding="utf-8") as fh:
        for entry in labels:
            row = by_id.get(entry["id"], {})
            fh.write(json.dumps({**entry, "weak_label": row.get("weak_label"),
                                 "project": row.get("project"),
                                 "channel": row.get("channel")}, ensure_ascii=False) + "\n")

    stats = {
        "task": task, "rows": len(rows), "batches": len(batches), "labels": len(labels),
        "failed_batches": failed_batches,
        "attempts": {str(k): v for k, v in sorted(attempts.items())},
        "teacher_seconds_total": round(seconds, 1),
        "distribution": dict(Counter(e["label"] for e in labels)),
        "mean_confidence": round(
            sum(e["confidence"] or 0 for e in labels) / max(1, len(labels)), 3),
        "out": str(out_path),
    }
    if task == "memory_kind":
        pairs = [(e["label"], by_id[e["id"]].get("weak_label")) for e in labels if e["id"] in by_id]
        agree = sum(1 for a, b in pairs if a == b)
        stats["agreement_with_key_prefix"] = round(agree / max(1, len(pairs)), 3)
    if task == "recall_relevance":
        yes = sum(1 for e in labels if e["label"] == "yes")
        stats["share_relevant"] = round(yes / max(1, len(labels)), 3)
    return stats


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--tasks", default="prompt_intent,memory_kind,recall_relevance")
    ap.add_argument("--model", default="sonnet")
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=180)
    ap.add_argument("--limit", type=int, default=None, help="first N rows per task (pilot)")
    args = ap.parse_args(argv)

    report = {"model": args.model, "workers": args.workers, "tasks": {}}
    for task in [t.strip() for t in args.tasks.split(",") if t.strip()]:
        print(f"[{task}] starting", flush=True)
        stats = run_task(task, args.model, args.workers, args.timeout, args.limit)
        report["tasks"][task] = stats
        print(f"[{task}] {json.dumps(stats, ensure_ascii=False)}", flush=True)
    (CORPUS / "label_stats.json").write_text(json.dumps(report, ensure_ascii=False, indent=1))
    print(json.dumps(report, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
