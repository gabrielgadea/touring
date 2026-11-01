#!/usr/bin/env python3
"""w42_eval — avaliação do aluno Laya fine-tunado no test split congelado (S-4.3).

Gate G3 (plano): aluno ≥ keyword (0,536) e ≥ 0,85 × professor (0,696 → piso 0,592);
ECE ≤ 0,10; p99 < 30 ms. Baselines do G2: keyword-produção 0,536 (n=97),
Jev 0,533 (n=120), professor 0,696 (n=23). Tudo no test split congelado
(split.json, seed 20260922), temperatura calibrada em held-out pelo trainer.
"""

from __future__ import annotations

import json
import statistics
import sys
import time
from pathlib import Path

CORPUS = Path("~/.claude/touring/g1-corpus").expanduser()
MODEL_DIR = Path("~/.claude/touring/laya-w42-intent/output").expanduser()
INTENT_CLASSES = ["general", "code", "debug", "refactor", "test", "analysis", "plan", "creative"]


def ece(per_item: list[tuple[float, bool]], bins: int = 10) -> float | None:
    if not per_item:
        return None
    total = len(per_item)
    acc = 0.0
    for b in range(bins):
        lo, hi = b / bins, (b + 1) / bins
        bucket = [(p, ok) for p, ok in per_item if lo <= p < hi or (b == bins - 1 and p == 1.0)]
        if not bucket:
            continue
        conf = statistics.mean(p for p, _ in bucket)
        acc_bin = sum(1 for _, ok in bucket if ok) / len(bucket)
        acc += (len(bucket) / total) * abs(acc_bin - conf)
    return round(acc, 4)


def main() -> int:
    import laya

    split = json.loads((CORPUS / "split.json").read_text())
    test_ids = sorted(k for k, v in split["sets"]["prompts"]["assignment"].items() if v == "test")
    prompts = {}
    for line in (CORPUS / "prompts.jsonl").open():
        d = json.loads(line)
        prompts[d["id"]] = d["text"]
    labels = {}
    for line in (CORPUS / "labels_prompt_intent.jsonl").open():
        d = json.loads(line)
        labels[d["id"]] = d["label"]

    agent = laya.Agent(model_id_or_path=str(MODEL_DIR), device="cuda")
    question = {
        "intent": {
            "type": "choice",
            "instructions": "What kind of work is the user asking for in `prompt`? Answer with the dominant intent.",
            "criteria": {c: None for c in INTENT_CLASSES},
        }
    }
    rows: list[dict] = []
    lat: list[float] = []
    for iid in test_ids:
        text = prompts[iid]
        started = time.monotonic()
        result = agent.predict([text], question)
        lat.append((time.monotonic() - started) * 1000)
        ans = result.answers["intent"] if hasattr(result, "answers") else result["answers"]["intent"]
        get = (lambda k: getattr(ans, k, None)) if not isinstance(ans, dict) else (lambda k: ans.get(k))
        pred = get("choice") or get("values", {}).get("intent")
        conf = get("answer_confidence") or get("confidence")
        rows.append({"pred": pred, "gold": labels[iid], "correct": pred == labels[iid], "conf": conf})

    n = len(rows)
    accuracy = round(sum(1 for r in rows if r["correct"]) / n, 4)
    ece_val = ece([(float(r["conf"]), r["correct"]) for r in rows if r.get("conf") is not None])
    lat_sorted = sorted(lat)
    out = {
        "model": "laya-w42-intent (fine-tune RLCD, 4 épocas, G1 train 560)",
        "n_test": n,
        "accuracy": accuracy,
        "ece": ece_val,
        "p50_ms": round(statistics.median(lat), 1),
        "p99_ms": round(lat_sorted[max(0, int(0.99 * n) - 1)], 1),
        "baselines_g2": {"keyword_producao": 0.536, "jev": 0.533, "professor": 0.696},
        "gate_g3": {
            "beats_keyword": accuracy > 0.536,
            "beats_085_professor": accuracy >= 0.85 * 0.696,
            "ece_below_010": (ece_val or 1.0) <= 0.10,
            "p99_below_30ms": lat_sorted[max(0, int(0.99 * n) - 1)] < 30,
        },
    }
    out_path = Path("docs/plans/2026-09-30-jev-harness-potencializacao/experiments/w42-student")
    out_path.mkdir(parents=True, exist_ok=True)
    (out_path / "results.json").write_text(json.dumps(out, ensure_ascii=False, indent=1))
    print(json.dumps(out, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
