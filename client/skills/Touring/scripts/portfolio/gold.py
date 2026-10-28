#!/usr/bin/env python3
# #tags: kind:script purpose:measure domain:code-mode process:measure lang:python
"""gold.py — gold set and frozen train/held-out split for the portfolio retriever (Phase 0, S-0.3).

Reads the frozen corpus (gzip JSONL, sha256 checked against ``corpus_manifest.json``) and labels each
query by a DECLARED method, so a retriever can be scored on the held-out half:

* library of a query = items from OTHER sessions written BEFORE it (an equivalent must already exist —
  otherwise the score would reward finding a script from the future);
* ``tem_equivalente``: some library item has token-set Jaccard >= 0.70; ``equivalentes`` lists them;
* ``sem_equivalente``: every library item is below 0.30;
* ``ambigua`` (0.30–0.70) and ``sem_biblioteca`` are kept visible and never scored;
* synthetic positives: deterministic mutations of the recreation champions, labeled by construction
  (independent of Jaccard), with the champion in the library.

The split is by blake2b(query id + seed): deterministic, disjoint, and the writer refuses to overwrite.
Purpose-only equivalence (rewrite from scratch) has no label yet: it is reported as unscored, not zero.

Usage:
    python3 gold.py --corpus <corpus.jsonl.gz> --saida <gold.json> [--campeoes <dir>]
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import re
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import simhash

ALTO, BAIXO = 0.70, 0.30
SEMENTE = "20261028"
MANIFESTO = AQUI / "corpus_manifest.json"


@dataclass(frozen=True)
class Item:
    """One script creation in the frozen corpus."""

    id: str
    sessao: str
    timestamp: str
    nome: str
    origem: str
    content: str
    toks: frozenset[str] = field(default_factory=frozenset, compare=False, repr=False)


@dataclass(frozen=True)
class Rotulo:
    """Gold label of one query."""

    classe: str  # tem_equivalente | sem_equivalente | ambigua | sem_biblioteca
    equivalentes: tuple[str, ...] = ()
    metodo: str = "jaccard"


class CorpusAdulterado(RuntimeError):
    """The corpus bytes do not match the hash in the manifest."""


def _item(d: dict) -> Item:
    return Item(d["id"], d["sessao"], d["timestamp"], d["nome"], d["origem"], d["content"],
                frozenset(simhash.tokens(d["content"])))


def carregar_corpus(caminho: Path, sha256_esperado: str | None = None) -> list[Item]:
    """Load the frozen corpus; a hash mismatch raises instead of silently scoring other data."""
    dados = gzip.decompress(caminho.read_bytes())
    if sha256_esperado is not None and hashlib.sha256(dados).hexdigest() != sha256_esperado:
        raise CorpusAdulterado(f"{caminho}: sha256 difere do manifesto")
    return [_item(json.loads(linha)) for linha in dados.decode("utf-8").splitlines() if linha.strip()]


def jaccard(a: frozenset[str], b: frozenset[str]) -> float:
    if not a or not b:
        return 0.0
    if min(len(a), len(b)) / max(len(a), len(b)) < BAIXO:  # cheap bound: J <= min/max
        return 0.0
    return len(a & b) / len(a | b)


def biblioteca_de(consulta: Item, itens: list[Item]) -> list[Item]:
    """Items from other sessions written before the query."""
    return [i for i in itens if i.sessao != consulta.sessao and i.timestamp < consulta.timestamp]


def rotular(itens: list[Item]) -> dict[str, Rotulo]:
    """Label every query by the declared Jaccard method."""
    rotulos: dict[str, Rotulo] = {}
    for q in itens:
        bib = biblioteca_de(q, itens)
        if not bib:
            rotulos[q.id] = Rotulo("sem_biblioteca")
            continue
        sims = [(jaccard(q.toks, c.toks), c.id) for c in bib]
        maior = max(s for s, _ in sims)
        if maior >= ALTO:
            rotulos[q.id] = Rotulo("tem_equivalente", tuple(sorted(i for s, i in sims if s >= ALTO)))
        elif maior < BAIXO:
            rotulos[q.id] = Rotulo("sem_equivalente")
        else:
            rotulos[q.id] = Rotulo("ambigua")
    return rotulos


_IDENT = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]{3,}\b")


def mutacoes(content: str) -> list[str]:
    """Deterministic edits that keep the program: drop comments/blanks, rename identifiers, append."""
    sem_coment = "\n".join(
        ln for ln in content.splitlines() if ln.strip() and not ln.strip().startswith(("#", "//"))
    )
    frequentes = [n for n, _ in Counter(_IDENT.findall(content)).most_common(3)]
    renomeado = content
    for n in frequentes:
        renomeado = re.sub(rf"\b{re.escape(n)}\b", f"{n}_x", renomeado)
    return [sem_coment, renomeado, sem_coment + "\nprint('fim')\n"]


def sinteticos(campeoes: Path) -> tuple[list[Item], dict[str, Rotulo]]:
    """Champions (old, in the library) and their mutations (queries), labeled by construction."""
    itens: list[Item] = []
    rotulos: dict[str, Rotulo] = {}
    for p in sorted(campeoes.glob("*.json")):
        d = json.loads(p.read_text(encoding="utf-8"))
        base = _item({"id": f"campeao-{p.stem}", "sessao": f"campeao:{p.stem}", "timestamp": "0000",
                      "nome": d["basename"], "origem": "campeao", "content": d["content"]})
        itens.append(base)
        for k, corpo in enumerate(mutacoes(d["content"])):
            mid = f"mutacao-{p.stem}-{k}"
            itens.append(_item({"id": mid, "sessao": f"mutacao:{p.stem}:{k}", "timestamp": "9999",
                                "nome": d["basename"], "origem": "mutacao", "content": corpo}))
            rotulos[mid] = Rotulo("tem_equivalente", (base.id,), "construcao")
    return itens, rotulos


def particao(item_id: str, semente: str = SEMENTE) -> str:
    """Deterministic 50/50 split by hash; the same id always lands on the same side."""
    h = hashlib.blake2b(f"{item_id}\x00{semente}".encode(), digest_size=8).digest()
    return "held_out" if h[0] % 2 else "treino"


def montar(itens: list[Item], extras: list[Item], extras_rotulos: dict[str, Rotulo]) -> dict:
    """The gold document: labels, split and counts per class and partition."""
    todos = itens + extras
    rotulos = rotular(itens)
    rotulos.update(extras_rotulos)
    for e in extras:
        rotulos.setdefault(e.id, Rotulo("sem_biblioteca"))  # champions are library, not queries
    entradas = {
        i: {"classe": r.classe, "equivalentes": list(r.equivalentes), "metodo": r.metodo, "particao": particao(i)}
        for i, r in rotulos.items()
    }
    contagem = Counter((v["classe"], v["particao"]) for v in entradas.values())
    return {
        "semente": SEMENTE,
        "limiares": {"alto": ALTO, "baixo": BAIXO},
        "itens": len(todos),
        "contagem": {f"{c}/{p}": n for (c, p), n in sorted(contagem.items())},
        "proposito_sem_rotulo": "equivalência só de propósito (reescrita do zero) não tem rótulo: unscored",
        "rotulos": entradas,
    }


def gravar(doc: dict, saida: Path) -> None:
    """Write once: a frozen split is never silently replaced."""
    if saida.exists():
        raise FileExistsError(f"{saida} já existe — o gold congelado não se sobrescreve")
    saida.parent.mkdir(parents=True, exist_ok=True)
    saida.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="gold set e divisão congelada do portfólio")
    ap.add_argument("--corpus", type=Path, required=True)
    ap.add_argument("--saida", type=Path, required=True)
    ap.add_argument("--campeoes", type=Path)
    args = ap.parse_args(argv)
    esperado = json.loads(MANIFESTO.read_text(encoding="utf-8"))["sha256_conteudo"] if MANIFESTO.exists() else None
    itens = carregar_corpus(args.corpus, esperado)
    extras, extras_rotulos = sinteticos(args.campeoes) if args.campeoes else ([], {})
    doc = montar(itens, extras, extras_rotulos)
    gravar(doc, args.saida)
    print(json.dumps({k: doc[k] for k in ("itens", "contagem")}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
