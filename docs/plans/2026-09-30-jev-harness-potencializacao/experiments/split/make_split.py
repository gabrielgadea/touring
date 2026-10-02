#!/usr/bin/env python3
"""make_split.py — reconstrói o split congelado do corpus G1 (S-2.0).

O gate do G1 (strategy-2026-09-22-jev-system-one.md:622) promete "split congelado
treino/validação/teste". A varredura do W2.0 provou que esse artefato NUNCA foi
produzido: nenhum dos seis JSONL do corpus tem chave de partição, `g1_sample.py`
e `g1_label.py` só usam `str.split`, e a folha de revisão estratifica por
CONFIANÇA, não por item. Ver o Apêndice A de harness-scan-t02.md.

Este script é a reconstrução. Ele é **determinístico por construção**: a partição
de um item é função do (seed, id) e nada mais — nunca da ordem de leitura, do
mtime, ou do conteúdo vizinho. Regerar com a mesma seed reproduz byte-idêntico,
que é o que `test_split_is_frozen` exige.

Por que 70/15/15 e não 80/10/10
-------------------------------
O G2 mede três linhas de base — keyword, Laya zero-shot, professor — e o G3 faz
fine-tune em cima do treino. O held-out do G3 tem que ser grande o bastante para
que "temperatura por (tipo, nº de opções)" tenha sinal, mas o fine-tune depende de
ter dados de treino (R15: nenhum relato de treino do Laya numa GPU de 8 GB, e a
RTX 4060 da máquina é 8 GB). 15% de 800 prompts = 120 itens de teste, ~19 por
classe de intent — suficiente para uma curva de calibração, apertado para p99.
O relatório de W1 (S-1.2) é quem decide se sobe para 80/10/10.

Estratificação
--------------
A partição é feita DENTRO de cada estrato, para que a proporção por classe e por
projeto se mantenha nos três pedaços. O corpus já é estratificado (prompts por
projeto, memórias por classe do prefixo da chave, pares por canal), e um split
global por hash desfaria isso. Para as memórias, o estrato é a CLASSE DE
LABEL do professor (`weak_label`), não o rótulo dele: o rótulo é o alvo, e
estratificar por alvo vaza informação para a partição.

Uso
---
    python3 make_split.py --write     # grava ~/.claude/touring/g1-corpus/split.json
    python3 make_split.py --check     # regrava em memória e compara com o disco
    python3 make_split.py --report    # só imprime o resumo, não toca em nada
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import sys
from collections import defaultdict
from pathlib import Path

# A seed do G1, registrada em g1-corpus/sample_stats.json ("seed": 20260922).
# O plano manda reconstruir com ESTA seed, não com uma nova: é ela que produziu o
# corpus, e mudá-la tornaria o split irreproduzível contra os artefatos existentes.
SPLIT_SEED = 20260922

CORPUS_DIR = Path(os.path.expanduser("~/.claude/touring/g1-corpus"))

# (arquivo de itens, arquivo de rótulos, chave de estrato) por conjunto.
# `None` na chave de estrato = o item não tem uma chave de estrato; nesse caso
# o estrato é o próprio id, o que degenera em partição por hash global.
SETS = (
    ("prompts", "prompts.jsonl", "labels_prompt_intent.jsonl", "project"),
    ("memories", "memories.jsonl", "labels_memory_kind.jsonl", "weak_label"),
    ("pairs", "pairs.jsonl", "labels_recall_relevance.jsonl", "channel"),
)

# Frações de treino/validação/teste por estrato, somando 1.0. Ordinal e fixo.
FRACTIONS = (("train", 0.70), ("val", 0.15), ("test", 0.15))


def read_corpus_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        raise SystemExit(f"corpus ausente: {path}")
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def stratify(items: list[dict], strat_key: str | None) -> dict[str, list[dict]]:
    """Agrupa por estrato. Sem `strat_key`, cada item é seu próprio estrato."""
    groups: dict[str, list[dict]] = defaultdict(list)
    for item in items:
        key = str(item.get(strat_key)) if strat_key else str(item.get("id"))
        groups[key].append(item)
    return groups


def assign(items: list[dict], strat_key: str | None, seed: int) -> dict[str, str]:
    """Devolve {id: part} para um conjunto. Determinístico por construção.

    A ordem dentro de cada estrato é ordenada por id ANTES do shuffle, e o RNG é
    semeado por estrato com o seed global. Sem essa normalização, dois runs com o
    mesmo seed could divergem se a ordem do arquivo mudasse.
    """
    assignment: dict[str, str] = {}
    for stratum in sorted(stratify(items, strat_key).items()):
        name, group = stratum
        ordered = sorted(group, key=lambda row: str(row.get("id")))
        rng = random.Random(f"{seed}:{name}")
        rng.shuffle(ordered)

        n = len(ordered)
        # Cortes por contagem, não por round(): garante que os três pedaços
        # existam mesmo em estratos minúsculos (o menor tem 2 itens), em vez de
        # um deles ficar vazio. Resíduo vai para o maior, treino.
        n_train = max(1, round(n * FRACTIONS[0][1]))
        n_val = max(1, round(n * FRACTIONS[1][1]))
        if n_train + n_val > n - 1 and n >= 3:
            n_train, n_val = max(1, n - 2), 1
        if n < 3:  # estrato de 1-2 itens: tudo vai para treino, nada é segurado
            for row in ordered:
                assignment[str(row.get("id"))] = "train"
            continue

        for idx, row in enumerate(ordered):
            if idx < n_train:
                part = "train"
            elif idx < n_train + n_val:
                part = "val"
            else:
                part = "test"
            assignment[str(row.get("id"))] = part
    return assignment


def build(seed: int) -> dict:
    """Monta o split completo. Nenhuma escrita, nenhuma dependência de disco além do corpus."""
    out: dict[str, dict] = {
        "version": 1,
        "split_seed": seed,
        "corpus_seed": 20260922,
        "fractions": {name: frac for name, frac in FRACTIONS},
        "stratification": "dentro de cada estrato; memórias por weak_label (rótulo fraco), nunca por rótulo do professor",
        "generator": "make_split.py (S-2.0, W2.0 do plano Jev × Touring 30/09)",
        "sets": {},
    }
    for name, items_file, labels_file, strat_key in SETS:
        items = read_corpus_jsonl(CORPUS_DIR / items_file)
        labels = {str(row.get("id")): row for row in read_corpus_jsonl(CORPUS_DIR / labels_file)}
        parts = assign(items, strat_key, seed)

        missing = sorted(set(str(i.get("id")) for i in items) - set(parts))
        if missing:
            raise SystemExit(f"{name}: {len(missing)} itens sem partição (primeiro: {missing[0]})")

        orphan_labels = sorted(set(labels) - set(parts))
        if orphan_labels:
            # Um rótulo sem item é resíduo do corpus. Não é fatal — a partição é
            # sobre itens — mas precisa aparecer no relatório, não desaparecer.
            pass

        counts: dict[str, int] = {p: 0 for p, _ in FRACTIONS}
        for part in parts.values():
            counts[part] += 1
        out["sets"][name] = {
            "n": len(items),
            "items_file": items_file,
            "labels_file": labels_file,
            "strat_key": strat_key,
            "n_strata": len(stratify(items, strat_key)),
            "counts": counts,
            "labels_without_item": len(orphan_labels),
            "assignment": parts,
        }
    return out


def canonical(split: dict) -> bytes:
    """Serialização canônica: mesma split → mesmos bytes. Usada no teste de congelamento."""
    return json.dumps(split, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def split_digest(split: dict) -> str:
    return hashlib.sha256(canonical(split)).hexdigest()


def verify_frozen(path: Path, seed: int) -> tuple[bool, str]:
    """Regera do zero e compara byte a byte com o que está no disco."""
    rebuilt = canonical(build(seed))
    if not path.exists():
        return False, f"split.json ausente em {path}"
    on_disk = path.read_bytes()
    if on_disk == rebuilt:
        return True, split_digest(build(seed))
    return False, (
        f"DIVERGÊNCIA: {len(on_disk)} B no disco vs {len(rebuilt)} B regerado; "
        f"sha256 disco={hashlib.sha256(on_disk).hexdigest()[:16]} "
        f"regerado={hashlib.sha256(rebuilt).hexdigest()[:16]}"
    )


def print_report(split: dict) -> None:
    print(f"split_seed      = {split['split_seed']}")
    print(f"corpus_seed     = {split['corpus_seed']}")
    print(f"fractions       = {split['fractions']}")
    print(f"sha256(split)   = {split_digest(split)}")
    for name, entry in split["sets"].items():
        c = entry["counts"]
        print(
            f"\n{name:9} n={entry['n']:4} estratos={entry['n_strata']:3} "
            f"train={c['train']:4} val={c['val']:3} test={c['test']:3} "
            f"({c['test'] / entry['n'] * 100:.1f}% held-out)"
            + (f" rótulos-órfãos={entry['labels_without_item']}" if entry["labels_without_item"] else "")
        )
        # Amostra de 2 itens de cada parte, para conferência manual.
        sample: dict[str, list[str]] = defaultdict(list)
        for ident, part in sorted(entry["assignment"].items()):
            if len(sample[part]) < 2:
                sample[part].append(ident)
        print(f"          amostra: " + "  ".join(f"{p}={sample[p]}" for p, _ in FRACTIONS))


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--write", action="store_true", help="grava split.json no corpus")
    ap.add_argument("--check", action="store_true", help="regera e compara com o que está no disco")
    ap.add_argument("--report", action="store_true", help="imprime o resumo sem tocar em nada")
    ap.add_argument("--seed", type=int, default=SPLIT_SEED, help=f"seed do split (padrão {SPLIT_SEED})")
    args = ap.parse_args(argv)

    path = CORPUS_DIR / "split.json"

    if args.check:
        ok, detail = verify_frozen(path, args.seed)
        print(f"test_split_is_frozen: {'PASS' if ok else 'FAIL'} — {detail}")
        return 0 if ok else 1

    split = build(args.seed)
    if args.write:
        path.write_bytes(canonical(split))
        print(f"gravado: {path} ({path.stat().st_size} B)")
        # Congelamento é a MEIA parte do contrato: gravar não basta, tem de
        # provar que regerar reproduz o que está gravado.
        ok, detail = verify_frozen(path, args.seed)
        print(f"test_split_is_frozen: {'PASS' if ok else 'FAIL'} — {detail}")
        if not ok:
            return 1

    print_report(split)
    if not (args.write or args.check):
        print("\n(nada gravado — use --write para persistir, --check para verificar o congelamento)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
