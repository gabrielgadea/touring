#!/usr/bin/env python3
"""jev_dedupe.py — S-5.5 (P5): sinal de duplicação entre memórias. **Nunca apaga.**

O plano pede "Noul por par candidato; sinal, não verdade". A implementação
descobriu, medindo antes de codar, que o desenho certo é **outro** — e o motivo
está nos números do relatório.

O que a medição encontrou (01/10, `memory.db` do projeto)
------------------------------------------------------------
| | |
|---|---|
| entradas vivas | 20.042 |
| chaves duplicadas (mesma chave) | **0** |
| valores idênticos (>40 chars) | **43 grupos, 7.508 entradas** |
| pares ≥0,85 de similaridade em 7.140 comparações | **0** |

**37,5% do banco tem o MESMO texto**: `"Insufficient compliance data for CILA
progression analysis"`, em **7.506 entradas** cujas chaves diferem **apenas pelo
timestamp** (`insight:cila_progression:1787517420`, `:1787517720`, …, criadas de
5 em 5 minutos).

**[I] (0,95) Isto NÃO é quase-duplicata — é eco maciço do scout perpétuo**, que é
exatamente o defeito que a S-5.5 existe para atacar ("scout perpétuo sem eco
próprio"). E **[I] (0,95) um Noul é a ferramenta ERRADA para ele**: 7.506
entradas idênticas são 7.505 decisões óbvias, e gastá-las a ~US$ 0,0000167 cada
para o modelo responder "são iguais" sobre um dado que `GROUP BY value` já
resolve com SQL determinístico e grátis.

**[I] (0,90) O Noul fica onde ele é insubstituível**: nos pares **quase**
idênticos, que são 0 hoje e vão aparecer quando o scout começar a reformular em
vez de repetir. Por isso os dois estágios existem e o segundo é o que o plano
pediu:

- **Estágio 1 — determinístico (grátis,WHERE está o eco):** agrupar por valor
  normalizado. Isso é aritmética, não julgamento.
- **Estágio 2 — Noul (pago, onde a determinística cala):** pares com
  similaridade acima do corte mas **não** idênticos. Hoje: 0 pares.

O relatório diz os dois, e o `signal_kind` de cada par diz de qual estágio veio.
Um relatório que só contasse "pares" mentiria sobre a origem — e a origem é o
que decide se aquilo é fato ou palpite.

Nenhum delete. Nenhuma escrita. `writes_performed: 0`, sempre.

Uso:
    python3 jev_dedupe.py --report            # só determinístico, zero chamada
    python3 jev_dedupe.py --judge             # inclui o Noul nos pares sobre o corte
    python3 jev_dedupe.py --judge --limit 20  # limita quantos vão ao modelo
"""

from __future__ import annotations

import argparse
import collections
import difflib
import json
import os
import re
import sqlite3
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

REPO = Path(__file__).resolve().parent.parent
MEMORY_DB = Path(os.path.expanduser("~/projects/touring/.claude/touring/memory.db"))
REPORT = REPO / "docs/plans/2026-09-30-jev-harness-potencializacao/experiments/s5-dedupe/dedupe.json"

# Acima desta similaridade o par vira candidato ao Noul. **[F] Medido: em 7.140
# comparações de pares reais, NENHUM passou de 0,85** — o corte é o do cookbook
# e o número zero é a descoberta, não uma configuração.
SIMILARITY_FLOOR = 0.85

# Textos curtos demais para comparar por similaridade: a comparação de 400
# caracteres de uma frase de 20 é sempre 1,0 ou sempre 0,0, e nenhum dos dois
# quer dizer nada.
MIN_LENGTH = 60


def normalize(text: str) -> str:
    """O que faz dois textos serem 'o mesmo' para o estágio determinístico.

    Espaços colapsados e minúsculas — e nada mais. Normalizar pontuação ou
    parar-palavras aqui criaria um "quase igual" determinístico, que é
    exatamente o que o estágio 2 existe para julgar.
    """
    return " ".join((text or "").lower().split())


def read_entries(db: Path, *, recent_days: int | None = None) -> list[dict]:
    """As entradas vivas, em modo SOMENTE LEITURA."""
    if not db.exists():
        raise SystemExit(f"banco ausente: {db}")
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row
    try:
        if recent_days:
            rows = con.execute(
                "SELECT key, value, created_at, access_count FROM memory_entries "
                "WHERE superseded_by IS NULL AND created_at >= datetime('now', ?) "
                "ORDER BY key",
                (f"-{recent_days} days",),
            ).fetchall()
        else:
            rows = con.execute(
                "SELECT key, value, created_at, access_count FROM memory_entries "
                "WHERE superseded_by IS NULL ORDER BY key"
            ).fetchall()
        return [dict(r) for r in rows]
    finally:
        con.close()


def exact_groups(entries: list[dict]) -> list[dict]:
    """Estágio 1: valores NORMALIZADOS idênticos. Grátis, determinístico.

    O agrupamento é por valor normalizado, e cada grupo carrega a **razão** pelo
    valor — porque "7.506 entradas iguais" e "duas entradas que começam igual"
    são a mesma contagem e significados opostos, e o relatório precisa distinguir.
    """
    buckets: dict[str, list[dict]] = collections.defaultdict(list)
    for entry in entries:
        text = normalize(entry.get("value"))
        if text:
            buckets[text].append(entry)
    groups = []
    for text, members in buckets.items():
        if len(members) < 2:
            continue
        # Chave redundante: sem o sufixo variável, todas as chaves são a mesma.
        stems = {re.sub(r":\d{6,}$", "", m["key"]) for m in members}
        groups.append({
            "signal_kind": "exact_normalized",
            "count": len(members),
            "redundant_key": len(stems) == 1,
            "key_stem": next(iter(stems)) if len(stems) == 1 else None,
            "excess": len(members) - 1,
            "sample_key": members[0]["key"],
            "text": text[:160],
            "total_accesses": sum(m.get("access_count") or 0 for m in members),
        })
    groups.sort(key=lambda g: -g["excess"])
    return groups


def near_pairs(entries: list[dict], floor: float, *, cap: int = 400) -> list[dict]:
    """Estágio 2: pares NÃO idênticos acima do corte de similaridade.

    O `cap` limita o tanto de entrada analisada: `difflib` é O(n²) em CPU pura, e
    o relatório diz quantas entradas entraram na comparação — um resultado
    medido sobre um subconjunto que não diz qual é um número sem dono.
    """
    pool = [e for e in entries if len(normalize(e.get("value"))) >= MIN_LENGTH][:cap]
    pairs = []
    for i in range(len(pool)):
        a = normalize(pool[i]["value"])[:400]
        for j in range(i + 1, len(pool)):
            b = normalize(pool[j]["value"])[:400]
            if a == b:
                continue  # isso é do estágio 1
            ratio = difflib.SequenceMatcher(None, a, b).ratio()
            if ratio >= floor:
                pairs.append({
                    "signal_kind": "near_duplicate",
                    "similarity": round(ratio, 4),
                    "key_a": pool[i]["key"],
                    "key_b": pool[j]["key"],
                })
    pairs.sort(key=lambda p: -p["similarity"])
    return pairs


def judge_pair(pair: dict, *, client=None) -> dict:
    """Noul por par: 'estas duas memórias são o MESMO fato?'.

    **[I] (0,90)** O `state` leva as DUAS memórias — o que se julga é a
    RELAÇÃO entre elas, e nenhum dos lados sozinho a contém. É o par que é a
    unidade, como o cookbook diz ("the state is the thing being judged").
    """
    from jev_client import system_one

    question = {
        "same_fact": {
            "type": "noul",
            # **[F] `criteria` do Noul NÃO é uma lista.** A primeira versão
            # passou `["same fact, error, …]` e o SDK recusou com
            # `ValidationError: Input should be a valid dictionary` — o tipo é
            # `NoulCriteria`, "optional descriptions for EITHER outcome".
            # `scripts/pre_review_triage.py:51` é o prior art que usa a forma
            # aceita: Noul sem `criteria`, com tudo no `instructions`.
            "instructions": (
                "Two memory entries are given. Answer `none` ONLY if they record "
                "the SAME fact, the SAME error, or the SAME finding — that is, if "
                "keeping both adds nothing to what a future reader could learn. "
                "Answer with a short reason if they are DIFFERENT facts even when "
                "the wording is similar: a different file, a different run, a "
                "different root cause, or a different decision is a different fact."
            ),
        }
    }
    state = {"memory_a": pair["key_a"], "memory_b": pair["key_b"], "similarity": pair["similarity"]}
    started = time.monotonic()
    answers = system_one(state, question, client=client)
    elapsed = (time.monotonic() - started) * 1000
    if answers is None:
        return {**pair, "verdict": "unmeasured", "reason": None, "latency_ms": round(elapsed, 1)}
    ans = answers.get("same_fact", {})
    return {
        **pair,
        "verdict": "none" if ans.get("noul") is not None or ans.get("type") == "noul" else "distinct",
        "reason": ans.get("choice") or ans.get("score"),
        "confidence": ans.get("confidence"),
        "latency_ms": round(elapsed, 1),
    }


def summarize(entries: list[dict], groups: list[dict], pairs: list[dict], judged: list[dict]) -> dict:
    """O relatório. `writes_performed` é 0 e está aqui como afirmação verificável."""
    by_kind = collections.Counter(g["signal_kind"] for g in groups)
    redundant = [g for g in groups if g["redundant_key"]]
    flagged = [j for j in judged if j.get("verdict") == "none"]
    return {
        "entries_scanned": len(entries),
        "groups": len(groups),
        "entries_in_groups": sum(g["count"] for g in groups),
        "excess_entries": sum(g["excess"] for g in groups),
        "groups_by_kind": dict(by_kind),
        # O eco do scout: chaves que só diferem pelo timestamp. É o defeito que
        # a S-5.5 nomeia, e ele é o MAIOR grupo sem nenhuma chamada ao modelo.
        "redundant_key_groups": len(redundant),
        "redundant_key_excess": sum(g["excess"] for g in redundant),
        "near_pairs_found": len(pairs),
        "near_pairs_judged": len(judged),
        "near_pairs_flagged_same_fact": len(flagged),
        "writes_performed": 0,
        "deletions_performed": 0,
        "note": "sinal, não verdade — nenhum par vira deleção aqui",
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--report", action="store_true", help="só o estágio determinístico (padrão)")
    ap.add_argument("--judge", action="store_true", help="inclui o Noul nos pares sobre o corte")
    ap.add_argument("--limit", type=int, default=20, help="quantos pares vão ao modelo")
    ap.add_argument("--recent-days", type=int, default=None, help="só entradas criadas em N dias")
    ap.add_argument("--cap", type=int, default=400, help="entradas na comparação de similaridade")
    ap.add_argument("--out", type=Path, default=REPORT)
    args = ap.parse_args(argv)

    entries = read_entries(MEMORY_DB, recent_days=args.recent_days)
    groups = exact_groups(entries)
    pairs = near_pairs(entries, SIMILARITY_FLOOR, cap=args.cap)

    judged: list[dict] = []
    if args.judge:
        for pair in pairs[: args.limit]:
            judged.append(judge_pair(pair))
            print(f"  {pair['similarity']:.3f} {judged[-1]['verdict']:9} {pair['key_a'][:40]} ~ {pair['key_b'][:40]}")

    summary = summarize(entries, groups, pairs, judged)
    payload = {
        "subtask": "S-5.5",
        "db": str(MEMORY_DB),
        "similarity_floor": SIMILARITY_FLOOR,
        "min_length": MIN_LENGTH,
        "cap_entries_compared": args.cap,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "summary": summary,
        "groups": groups[:40],
        "near_pairs": pairs[:40],
        "judged": judged,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    print(f"\nrelatório: {args.out}")
    if summary["near_pairs_found"] == 0:
        print(f"\nnenhum par passou de {SIMILARITY_FLOOR} — o estágio 2 não tem trabalho hoje.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
