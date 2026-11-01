#!/usr/bin/env python3
"""Sela o holdout web no formato que o VALIDADOR ATUAL aceita, e mede o que o
selo anterior só declarava.

O que muda em relação ao `meta.json` (o selo que existia):

- **o selo mora FORA do corpus** — o `sha_tree` do corpus incluiria o próprio
  selo, e `confere_selo.py` reprova um selo DENTRO do corpus;
- **os campos que o validador exige**: `sha_tree`, `arquivos` e `bytes` (o
  `meta.json` não tinha nenhum dos três — ele declarava um método de selo, não
  um selo que o validador conferisse);
- **a concentração é MEDIDA** (por arquivo e por sessão, com a fração e o
  topo), não declarada: um selo que declara um teto não restringe nada;
- **os denominadores por classe** (população, selecionados, alvo, `n` para o
  limite superior de Clopper-Pearson de 1 %) e a **situação da prova**: uma
  classe com n < 299 sai como **NÃO PROVADA** — o limite superior de 95 % com
  0 falsos não fica em 1 %, e o número não é inflado para parecer provado.

Uso: python3 selar_holdout_web.py [--corpus DIR] [--selo PATH] [--json PATH]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from collections import Counter
from pathlib import Path

VG = Path.home() / ".claude/touring/vgp"
PADRAO_CORPUS = VG / "corpus_holdout_web_20261012"

# Menor n com 0 falsos cujo limite superior de Clopper-Pearson 95% fica <= 1%:
# 1 - 0,05^(1/n) <= 0,01  =>  n >= ln(0,05)/ln(0,99) = 298,07 -> 299.
N_PARA_CP95_1PCT = math.ceil(math.log(0.05) / math.log(1 - 0.01))


def sha_tree(raiz: Path) -> str:
    """`sha256(concat de relpath + sha256(conteudo), em ordem)` — o método do corpus."""
    h = hashlib.sha256()
    for f in sorted(p for p in raiz.rglob("*") if p.is_file()):
        h.update(str(f.relative_to(raiz)).encode() + hashlib.sha256(f.read_bytes()).digest())
    return h.hexdigest()[:16]


def entradas_de(corpus: Path, lang: str) -> list[dict]:
    p = corpus / lang / "entradas.jsonl"
    if not p.is_file():
        return []
    return [json.loads(x) for x in p.read_text(encoding="utf-8").splitlines() if x.strip()]


def mede_classe(corpus: Path, lang: str, alvo: int) -> dict:
    """Os denominadores e a concentração REAL de uma classe."""
    regs = entradas_de(corpus, lang)
    n = len(regs)
    arq = Counter(r.get("path") or "?" for r in regs)
    sess = Counter(r.get("session") or "?" for r in regs)
    max_arq = max(arq.values()) if arq else 0
    max_sess = max(sess.values()) if sess else 0
    prova = n >= N_PARA_CP95_1PCT
    return {
        "lang": lang,
        "n": n,
        "alvo": alvo,
        "atinge_alvo": n >= alvo,
        "arquivos_distintos": len(arq),
        "sessoes_distintas": len(sess),
        "concentracao_max_arquivo": max_arq,
        "concentracao_max_arquivo_frac": round(max_arq / n, 4) if n else 0.0,
        "concentracao_max_sessao": max_sess,
        "concentracao_max_sessao_frac": round(max_sess / n, 4) if n else 0.0,
        "topo_arquivo": arq.most_common(3),
        "topo_sessao": sess.most_common(3),
        "n_para_cp95_1pct": N_PARA_CP95_1PCT,
        "prova_cp95_1pct": prova,
        "situacao": "provada" if prova else "NAO PROVADA",
        "motivo": (
            None
            if prova
            else (
                f"n={n} < {N_PARA_CP95_1PCT}: o limite superior de Clopper-Pearson 95% "
                "com 0 falsos nao fica em 1% — a classe sai como NAO PROVADA, sem "
                "inflar o numero"
            )
        ),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", type=Path, default=PADRAO_CORPUS)
    ap.add_argument("--selo", type=Path, default=None)
    ap.add_argument("--json", type=Path, default=None)
    ap.add_argument("--alvo", type=int, default=300)
    a = ap.parse_args()

    corpus = a.corpus
    selo_path = a.selo or corpus.with_name(corpus.name + ".selo.json")
    json_path = a.json or corpus.with_name(corpus.name + ".denominadores.json")
    if not corpus.is_dir():
        print(f"corpus inexistente: {corpus}")
        return 2

    meta = {}
    mp = corpus / "meta.json"
    if mp.is_file():
        meta = json.loads(mp.read_text(encoding="utf-8"))
    alvo = int(meta.get("alvo") or a.alvo)

    classes = [mede_classe(corpus, lang, alvo) for lang in ("html", "javascript")]
    arqs = sorted(p for p in corpus.rglob("*") if p.is_file())

    selo = {
        "sha_tree": sha_tree(corpus),
        "arquivos": len(arqs),
        "bytes": sum(p.stat().st_size for p in arqs),
        "corpus": corpus.name,
        "criado_em": "2026-10-31",
        "metodo": "sha256(concat de relpath + sha256(conteudo), em ordem)",
        "classes": {c["lang"]: c for c in classes},
        "denominadores": {
            c["lang"]: {
                "n": c["n"],
                "alvo": c["alvo"],
                "atinge_alvo": c["atinge_alvo"],
                "prova_cp95_1pct": c["prova_cp95_1pct"],
                "situacao": c["situacao"],
                "n_para_cp95_1pct": c["n_para_cp95_1pct"],
            }
            for c in classes
        },
        "concentracao": {
            c["lang"]: {
                "max_arquivo": c["concentracao_max_arquivo"],
                "max_arquivo_frac": c["concentracao_max_arquivo_frac"],
                "max_sessao": c["concentracao_max_sessao"],
                "max_sessao_frac": c["concentracao_max_sessao_frac"],
                "arquivos_distintos": c["arquivos_distintos"],
                "sessoes_distintas": c["sessoes_distintas"],
            }
            for c in classes
        },
        "nota": (
            "o selo mora FORA do corpus (o sha_tree incluiria o proprio selo); "
            "confira com confere_selo.py --corpus <dir> --selo <selo.json>"
        ),
    }
    selo_path.write_text(json.dumps(selo, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    json_path.write_text(
        json.dumps({"classes": classes, "alvo": alvo}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(f"selo: {selo_path}")
    print(f"denominadores: {json_path}")
    for c in classes:
        print(
            f"  {c['lang']:11} n={c['n']:>4}/{c['alvo']}  {c['situacao']:11}"
            f"  concentracao: {c['concentracao_max_arquivo_frac']:.1%} por arquivo,"
            f" {c['concentracao_max_sessao_frac']:.1%} por sessao"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
