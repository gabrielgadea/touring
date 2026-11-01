#!/usr/bin/env python3
"""Confere um corpus selado contra o seu `selo.json` ANTES de qualquer juiz abri-lo.

O `sha_tree` é o MESMO método que selou o held-out 20261011 (instrumentos-p1, 09/10/2026): sha256 sobre
`relpath + sha256(conteúdo).digest()` em ordem de `Path`, 16 hex. Além dele, cada campo explícito do selo
(arquivos, bytes, `*_sha256` de arquivos de índice) é conferido. `sha_tree`, `arquivos` e `bytes` são OBRIGATÓRIOS:
selo sem eles não sela a árvore (o `snap/` ficaria fora) e é VIOLADO, nunca «intacto por omissão» (10/10/2026: um selo
só com o sha do `entradas.jsonl` passava). O selo mora FORA do corpus — dentro, o `sha_tree` incluiria o próprio selo.
Controle positivo embutido (`--selftest`): o corpus selado passa; 1 byte trocado, selo sem `sha_tree` e selo dentro
do corpus reprovam.

Uso: python3 confere_selo.py --corpus <dir> --selo <selo.json>   (exit 0 = intacto; 1 = violado; 2 = uso)
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path

OBRIGATORIOS = ("sha_tree", "arquivos", "bytes")

INDICES = {
    "holdout_meta_sha256": "holdout_meta.json",
    "entradas_jsonl_sha256": "entradas.jsonl",
    "entradas_inline_neg_jsonl_sha256": "entradas_inline_neg.jsonl",
}


def sha_tree(raiz: Path) -> str:
    h = hashlib.sha256()
    for f in sorted(p for p in raiz.rglob("*") if p.is_file()):
        h.update(
            str(f.relative_to(raiz)).encode() + hashlib.sha256(f.read_bytes()).digest()
        )
    return h.hexdigest()[:16]


def conferir(corpus: Path, selo: dict, selo_path: Path | None = None) -> list[str]:
    """Each violated field, with expected and found; empty = intact."""
    faltam = [c for c in OBRIGATORIOS if c not in selo]
    if faltam:
        return [f"selo incompleto: faltam {faltam} — sem eles a árvore não está selada"]
    if selo_path is not None and selo_path.resolve().is_relative_to(corpus.resolve()):
        return [
            f"o selo mora DENTRO do corpus ({selo_path}): o sha_tree o incluiria; mova-o para fora"
        ]
    arqs = [p for p in corpus.rglob("*") if p.is_file()]
    medido = {
        "sha_tree": sha_tree(corpus),
        "arquivos": len(arqs),
        "bytes": sum(p.stat().st_size for p in arqs),
    }
    for campo, nome in INDICES.items():
        if campo in selo:
            alvo = corpus / nome
            medido[campo] = (
                hashlib.sha256(alvo.read_bytes()).hexdigest()
                if alvo.is_file()
                else "AUSENTE"
            )
    return [
        f"{campo}: selo {selo[campo]} · medido {medido[campo]}"
        for campo in medido
        if campo in selo and selo[campo] != medido[campo]
    ]


def selftest() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp) / "c"
        (d / "snap").mkdir(parents=True)
        (d / "entradas.jsonl").write_text('{"id": "a"}\n')
        (d / "snap" / "a.py").write_text("x = 1\n")
        arqs = [p for p in d.rglob("*") if p.is_file()]
        selo = {
            "sha_tree": sha_tree(d),
            "arquivos": len(arqs),
            "bytes": sum(p.stat().st_size for p in arqs),
            "entradas_jsonl_sha256": hashlib.sha256(
                (d / "entradas.jsonl").read_bytes()
            ).hexdigest(),
        }
        intacto = conferir(d, selo) == []
        (d / "snap" / "a.py").write_text(
            "x = 2\n"
        )  # same size: only the content hash can catch it
        violado = conferir(d, selo)
        sem_tree = conferir(d, {k: v for k, v in selo.items() if k != "sha_tree"})
        dentro = conferir(d, selo, d / "selo.json")
    ok = (
        intacto
        and any(v.startswith("sha_tree") for v in violado)
        and any("incompleto" in v for v in sem_tree)
        and any("DENTRO" in v for v in dentro)
    )
    print(
        f"selftest: {'ok' if ok else 'FALHA'} (intacto passa {intacto} · 1 byte reprova {bool(violado)} · sem sha_tree reprova {bool(sem_tree)} · selo dentro reprova {bool(dentro)})"
    )
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", type=Path)
    ap.add_argument("--selo", type=Path)
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    if not (a.corpus and a.selo):
        ap.print_usage(sys.stderr)
        return 2
    selo = json.loads(a.selo.read_text())
    violacoes = conferir(a.corpus.expanduser(), selo, a.selo.expanduser())
    for v in violacoes:
        print(f"VIOLADO {v}")
    print(
        f"selo {'INTACTO' if not violacoes else 'VIOLADO'}: {a.corpus} (sha_tree {selo.get('sha_tree')})"
    )
    return 0 if not violacoes else 1


if __name__ == "__main__":
    sys.exit(main())
