#!/usr/bin/env python3
"""s54_replay — replay G1: rerank Jev × ranking do recall vivo (gate da wave S-5.4).

Desenho (S-5.6, aprovado): para cada query única do test split de pares,
(1) o ranking ATUAL é a ordem do `touring memory recall` vivo, fundida por
score descrescente (proxy documentado do RRF de produção), restrita aos
candidatos rotulados do corpus; (2) o ranking Jev ordena os MESMOS candidatos
por P(sim) do Noul (candidata dentro das instructions — anti-context-rot).
Métricas: top-1 (o topo é relevante?) e MRR sobre os 6 pares "yes".
Caveat declarado: 33 queries, 6 "yes" — números pequenos; a wave decide por
direção + magnitude, não por significância estatística.
"""

from __future__ import annotations

import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import jev_client  # noqa: E402

CORPUS = Path("~/.claude/touring/g1-corpus").expanduser()
OUT = Path("docs/plans/2026-09-30-jev-harness-potencializacao/experiments/s54-replay")


def load_test_groups() -> dict[str, list[dict]]:
    pairs = {}
    for line in (CORPUS / "pairs.jsonl").open():
        d = json.loads(line)
        pairs[d["id"]] = d
    labels = {}
    for line in (CORPUS / "labels_recall_relevance.jsonl").open():
        d = json.loads(line)
        labels[d["id"]] = d["label"]
    split = json.loads((CORPUS / "split.json").read_text())
    test_ids = sorted(k for k, v in split["sets"]["pairs"]["assignment"].items() if v == "test")
    groups: dict[str, list[dict]] = defaultdict(list)
    for iid in test_ids:
        p = pairs[iid]
        groups[p["query"]].append({"id": iid, "candidate_key": p["candidate_key"], "text": p["text"], "label": labels.get(iid)})
    return dict(groups)


def live_recall_ranking(query: str) -> list[str]:
    """Ordem do recall vivo, fundida por score desc (proxy RRF documentado)."""
    proc = subprocess.run(
        ["touring", "memory", "recall", query[:300]],
        capture_output=True,
        text=True,
        timeout=30,
    )
    payload = json.loads(proc.stdout)
    cases = payload.get("cases") or {}
    entries = [e for g in cases.values() if isinstance(g, list) for e in g if isinstance(e, dict)]
    entries.sort(key=lambda e: e.get("score", 0.0), reverse=True)
    return [str(e.get("key", "")) for e in entries]


def jev_rerank(query: str, candidates: list[dict]) -> dict[str, float]:
    """P(sim) por candidato, UMA chamada (candidata nas instructions)."""
    questions = {
        f"c{i}": {
            "type": "noul",
            "instructions": {
                "candidate": c["text"][:2500],
                "question": "Does `candidate` contain information that directly helps with the task in `query`?",
            },
        }
        for i, c in enumerate(candidates)
    }
    answers = jev_client.system_one({"query": query[:1500]}, questions)
    if answers is None:
        return {}
    return {candidates[int(qid[1:])]["candidate_key"]: a.get("noul", 0.0) for qid, a in answers.items()}


def jev_score_gate(query: str, candidates: list[dict]) -> dict[str, float]:
    """S-7 (plan r2, "Classifying RAG passages"): Score de utilidade por candidato,
    UMA chamada — o código decide o que injeta (gate ≥ τ medido), não o modelo."""
    questions = {
        f"c{i}": {
            "type": "score",
            "instructions": {
                "candidate": c["text"][:2500],
                "question": "Rate how directly this candidate helps with the task in `query`: 0 = irrelevant, 1 = tangential, 2 = directly useful.",
            },
            "criteria": ["irrelevant to the query", "tangential or background", "directly useful for the query"],
        }
        for i, c in enumerate(candidates)
    }
    answers = jev_client.system_one({"query": query[:1500]}, questions)
    if answers is None:
        return {}
    return {candidates[int(qid[1:])]["candidate_key"]: float(a.get("score", 0.0)) for qid, a in answers.items()}


def metrics(groups: dict[str, list[dict]], gate_tau: float = 1.0) -> dict:
    per_system = {
        "current": {"top1": 0, "rr": 0.0},
        "jev": {"top1": 0, "rr": 0.0},
        "score_gate": {"top1": 0, "rr": 0.0, "injected": 0, "possible": 0},
    }
    missing = 0
    for query, candidates in groups.items():
        yes_keys = {c["candidate_key"] for c in candidates if c["label"] == "yes"}
        live_order = live_recall_ranking(query)
        present = [c for c in candidates if c["candidate_key"] in live_order]
        missing += len(candidates) - len(present)
        jev_scores = jev_rerank(query, candidates)
        gate_scores = jev_score_gate(query, candidates)
        current_order = [k for k in live_order if k in {c["candidate_key"] for c in candidates}]
        jev_order = [k for k, _ in sorted(jev_scores.items(), key=lambda kv: kv[1], reverse=True)]
        gated_order = [k for k, s in sorted(gate_scores.items(), key=lambda kv: kv[1], reverse=True) if s >= gate_tau]
        per_system["score_gate"]["injected"] += len(gated_order)
        per_system["score_gate"]["possible"] += len(candidates)
        for system, order in (("current", current_order), ("jev", jev_order), ("score_gate", gated_order)):
            if order and order[0] in yes_keys:
                per_system[system]["top1"] += 1
            rr = 0.0
            for rank, key in enumerate(order, start=1):
                if key in yes_keys:
                    rr = 1.0 / rank
                    break
            per_system[system]["rr"] += rr
    n_q = len(groups)
    sg = per_system["score_gate"]
    return {
        "n_queries": n_q,
        "n_yes_pairs": sum(1 for g in groups.values() for c in g if c["label"] == "yes"),
        "candidates_missing_from_live_recall": missing,
        "current": {"top1": per_system["current"]["top1"], "mrr": round(per_system["current"]["rr"] / n_q, 4)},
        "jev": {"top1": per_system["jev"]["top1"], "mrr": round(per_system["jev"]["rr"] / n_q, 4)},
        "score_gate": {
            "top1": sg["top1"],
            "mrr": round(sg["rr"] / n_q, 4),
            "gate_tau": gate_tau,
            "injected_share": round(sg["injected"] / sg["possible"], 4) if sg["possible"] else None,
        },
    }


def main() -> int:
    groups = load_test_groups()
    OUT.mkdir(parents=True, exist_ok=True)
    result = metrics(groups)
    result["caveat"] = "33 queries / 6 yes — ler direção+magnitude, não significância; proxy RRF = ordem do recall vivo por score desc"
    (OUT / "results.json").write_text(json.dumps(result, ensure_ascii=False, indent=1))
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
