#!/usr/bin/env python3
"""G1 review sheet — the 10% a human checks, and the agreement it produces.

Two modes:

  build   sample ~10% per task, stratified by the teacher's confidence (the doubtful
          cases first, since those are the ones a threshold would route away), and write
          a markdown sheet with one line per item: the text, the teacher's label and an
          empty box for the human verdict.
  score   read the filled sheet back and compute agreement per task and per confidence
          band — the number the G1 gate asks for (≥ 0.85).

The sheet lives outside the repository with the corpus: it quotes real prompts.
"""
from __future__ import annotations

import argparse
import json
import random
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

CORPUS = Path.home() / ".claude/touring/g1-corpus"
SHEET = CORPUS / "review_sheet.md"
SEED = 20260922
TASK_FILES = {
    "prompt_intent": ("prompts.jsonl", "text"),
    "memory_kind": ("memories.jsonl", "text"),
    "recall_relevance": ("pairs.jsonl", "text"),
}
BANDS = (("baixa", 0.0, 0.6), ("média", 0.6, 0.8), ("alta", 0.8, 1.01))


def load_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def band_of(confidence: float | None) -> str:
    value = confidence or 0.0
    return next(name for name, lo, hi in BANDS if lo <= value < hi)


def build(share: float) -> Path:
    rng = random.Random(SEED)
    lines = [
        "# G1 — folha de revisão humana",
        "",
        "Marque cada item: escreva o rótulo correto em `humano:` quando discordar do",
        "professor, ou deixe `=` quando concordar. Depois rode:",
        "",
        "```bash",
        "python3 g1_review_sheet.py score",
        "```",
        "",
        "A amostra é estratificada por faixa de confiança do professor, com peso nos",
        "casos duvidosos: são eles que um gate de confiança mandaria para o fallback.",
        "",
    ]
    total = 0
    for task, (source, field) in TASK_FILES.items():
        labels = {row["id"]: row for row in load_jsonl(CORPUS / f"labels_{task}.jsonl")}
        rows = {row["id"]: row for row in load_jsonl(CORPUS / source)}
        by_band: dict[str, list[dict]] = defaultdict(list)
        for ident, entry in labels.items():
            by_band[band_of(entry.get("confidence"))].append(entry)
        want = max(10, int(len(labels) * share))
        # Half the budget goes to the low-confidence band when it exists.
        quota = {"baixa": want // 2, "média": want // 4, "alta": want - want // 2 - want // 4}
        picked: list[dict] = []
        for name, _, _ in BANDS:
            pool = by_band.get(name, [])
            rng.shuffle(pool)
            picked.extend(pool[: quota[name]])
        # Fill from whatever is left if a band was short.
        leftovers = [e for e in labels.values() if e not in picked]
        rng.shuffle(leftovers)
        picked.extend(leftovers[: max(0, want - len(picked))])

        lines += [f"## {task} — {len(picked)} itens", ""]
        for entry in picked:
            row = rows.get(entry["id"], {})
            text = " ".join((row.get(field) or "").split())[:300]
            head = f"**{entry['id']}** · professor: `{entry['label']}` · conf {entry.get('confidence')}"
            if task == "recall_relevance":
                query = " ".join((row.get("query") or "").split())[:160]
                lines += [head, f"- consulta: {query}", f"- candidata: {text}", "- humano: =", ""]
            else:
                if task == "memory_kind" and row.get("weak_label"):
                    head += f" · prefixo: `{row['weak_label']}`"
                lines += [head, f"- texto: {text}", "- humano: =", ""]
            total += 1
    SHEET.write_text("\n".join(lines), encoding="utf-8")
    print(json.dumps({"sheet": str(SHEET), "items": total, "share": share}, ensure_ascii=False))
    return SHEET


def score() -> dict:
    if not SHEET.is_file():
        raise SystemExit(f"{SHEET} not found — run `build` first")
    text = SHEET.read_text(encoding="utf-8")
    task = None
    current: dict | None = None
    results: dict[str, list[tuple[str, str, str]]] = defaultdict(list)
    for line in text.splitlines():
        if line.startswith("## "):
            task = line[3:].split(" — ")[0].strip()
        elif line.startswith("**"):
            m = re.match(r"\*\*(\S+)\*\* · professor: `([^`]+)` · conf ([\d.]+|None)", line)
            if m:
                current = {"id": m.group(1), "teacher": m.group(2),
                           "band": band_of(None if m.group(3) == "None" else float(m.group(3)))}
        elif line.startswith("- humano:") and current and task:
            verdict = line.split(":", 1)[1].strip()
            human = current["teacher"] if verdict in {"=", ""} else verdict.strip("` ")
            results[task].append((current["teacher"], human, current["band"]))
            current = None

    report: dict = {"gate": 0.85, "tasks": {}}
    for task, rows in results.items():
        agree = sum(1 for t, h, _ in rows if t == h)
        by_band = defaultdict(lambda: [0, 0])
        for t, h, band in rows:
            by_band[band][1] += 1
            by_band[band][0] += int(t == h)
        report["tasks"][task] = {
            "n": len(rows),
            "agreement": round(agree / max(1, len(rows)), 3),
            "by_band": {b: {"agreement": round(a / max(1, n), 3), "n": n}
                        for b, (a, n) in sorted(by_band.items())},
            "disagreements": dict(Counter(f"{t}→{h}" for t, h, _ in rows if t != h).most_common(8)),
        }
    report["passes_gate"] = all(t["agreement"] >= 0.85 for t in report["tasks"].values())
    (CORPUS / "review_score.json").write_text(json.dumps(report, ensure_ascii=False, indent=1))
    print(json.dumps(report, ensure_ascii=False, indent=1))
    return report


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("mode", choices=["build", "score"])
    ap.add_argument("--share", type=float, default=0.10)
    args = ap.parse_args(argv)
    if args.mode == "build":
        build(args.share)
    else:
        score()
    return 0


if __name__ == "__main__":
    sys.exit(main())
