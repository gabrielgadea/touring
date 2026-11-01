#!/usr/bin/env python3
"""Sela a camada OSS (estrato de repositorios abertos) no formato que o VALIDADOR ATUAL
aceita, e mede o que o selo anterior so declarava.

Irmao do `selar_holdout_web.py`, para o estrato `oss`:

- **o selo mora FORA do corpus** -- o `sha_tree` do corpus incluiria o proprio selo, e
  `confere_selo.py` reprova um selo DENTRO do corpus;
- **os campos que o validador exige**: `sha_tree`, `arquivos` e `bytes` (o selo interno do
  rotulador nao tem nenhum dos tres: ele mede a concentracao, nao sela a arvore);
- **a situacao da prova por classe**, e o DENOMINADOR CORRETO: o que se prova e a taxa de
  FALSO deny, e o falso deny so acontece sobre um NEGATIVO (uma edicao que nao introduziu
  erro). O `n` que importa e o dos negativos, nao o total de entradas: um positivo e um
  NEGATIVO que o motor DEVERIA negar, e conta-lo no denominador infla a prova. MEDIDO em
  01/11 (V-8): a camada antiga tinha 213 entradas de tsx, das quais 169 negativas -- o
  total quase dobra a forca da prova sem acrescentar um unico falso deny possivel;
- **a DISJUNCAO MEDIDA**: quantos ids desta camada ja aparecem em um corpus anterior. Um
  held-out que repete o holdout anterior nao prova nada, porque o juiz pode ter sido
  calibrado nos mesmos pares;
- **a concentracao MEDIDA** por chave, com a fração e o topo -- um selo que declara um teto
  sem medir e um relatorio, nao uma restricao.

Uso: python3 selar_camada_oss.py [--corpus DIR] [--selo PATH] [--json PATH] [--alvo N]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
import tempfile
from collections import Counter
from pathlib import Path

# `python3 -I` tira o diretorio do script do `sys.path` (o modo isolado nao poe o cwd nem o
# diretorio do script); o `selftest` importa o `confere_selo`, que mora aqui ao lado.
sys.path.insert(0, str(Path(__file__).resolve().parent))

VG = Path.home() / ".claude/touring/vgp"
PADRAO_CORPUS = VG / "oss_rotulado_20261101"

# Menor n de NEGATIVOS com 0 falsos cujo limite superior de Clopper-Pearson 95% fica <= 1%:
# 1 - 0,05^(1/n) <= 0,01  =>  n >= ln(0,05)/ln(0,99) = 298,07 -> 299.
N_PARA_CP95_1PCT = math.ceil(math.log(0.05) / math.log(1 - 0.01))


def sha_tree(raiz: Path) -> str:
    """`sha256(concat de relpath + sha256(conteudo), em ordem)` -- o metodo do corpus."""
    h = hashlib.sha256()
    for f in sorted(p for p in raiz.rglob("*") if p.is_file()):
        h.update(str(f.relative_to(raiz)).encode() + hashlib.sha256(f.read_bytes()).digest())
    return h.hexdigest()[:16]


def limite_sup_cp95(n: int) -> float:
    """O limite superior de Clopper-Pearson 95% com ZERO falsos em n tentativas."""
    if n <= 0:
        return 1.0
    return 1 - 0.05 ** (1 / n)


def entradas_de(corpus: Path, lang: str) -> list[dict]:
    p = corpus / lang / "entradas.jsonl"
    if not p.is_file():
        return []
    out = []
    for ln in p.read_text(encoding="utf-8").splitlines():
        if ln.strip():
            out.append(json.loads(ln))
    return out


def ids_de(corpus: Path, ignorar: Path | None = None) -> set[str]:
    """Todos os ids de um corpus, em qualquer estrato.

    `ignorar` e o corpus que esta sendo selado: quando a origem da comparacao e o historico
    INTEIRO, a propria camada nova esta dentro dele — e a camada se contaria como sobreposicao
    de si mesma, que e um numero sem informacao (MEDIDO em 01/11).
    """
    ids: set[str] = set()
    if not corpus.is_dir():
        return ids
    for jl in corpus.rglob("entradas.jsonl"):
        if ignorar is not None and jl.resolve().is_relative_to(ignorar.resolve()):
            continue
        for ln in jl.read_text(errors="replace").splitlines():
            if not ln.strip():
                continue
            try:
                ids.add(str(json.loads(ln).get("id")))
            except json.JSONDecodeError:
                continue
    return ids


def mede_concentracao(regs: list, negativos: list) -> dict:
    """A concentracao REAL de uma classe: quantas entradas por chave, e a fracao."""
    arq = Counter(str(r.get("path") or "?") for r in regs)
    commit = Counter(str(r.get("commit") or "?") for r in regs)
    arq_neg = Counter(str(r.get("path") or "?") for r in negativos)
    max_arq = max(arq.values()) if arq else 0
    max_commit = max(commit.values()) if commit else 0
    max_arq_neg = max(arq_neg.values()) if arq_neg else 0
    n = len(regs)
    n_neg = len(negativos)
    return {
        "arquivos_distintos": len(arq),
        "commits_distintos": len(commit),
        "concentracao_max_arquivo": max_arq,
        "concentracao_max_arquivo_frac": round(max_arq / n, 4) if n else 0.0,
        "concentracao_max_commit": max_commit,
        "concentracao_max_commit_frac": round(max_commit / n, 4) if n else 0.0,
        "concentracao_max_arquivo_frac_sobre_negativos": (
            round(max_arq_neg / n_neg, 4) if n_neg else 0.0
        ),
        "topo_arquivo": arq.most_common(3),
        "topo_commit": commit.most_common(3),
    }


def mede_teto(regs: list, n: int) -> dict:
    """O TETO de concentracao, MEDIDO contra o disco — o mesmo metodo do `rotular_oss.py`.

    O teto e `3% do n da linguagem` e a infracao e `entradas por chave > teto`. MEDIDO em
    01/11 (V-8): e o METODO que decide, nao o numero que a camada declara. A camada antiga
    (213 entradas de tsx) tinha uma chave com 7 e o proprio selo declarava `teto_por_chave_no_fim`
    = 7 — mas `teto_de(213, 0.03)` = 6, e 7 > 6: a camada estava ACIMA do teto medido. Declarar
    um teto e medir sao coisas diferentes, e so a medicao restringe.
    """
    fracao = 0.03
    teto = max(1, int(max(1, n) * fracao))
    cont = Counter(str(r.get("path") or "?") for r in regs)
    infratores = [
        {"chave": chave, "n": c, "fracao": round(c / n, 4) if n else 0.0, "teto": teto}
        for chave, c in cont.most_common()
        if c > teto
    ]
    return {
        "teto_concentracao": fracao,
        "teto_por_chave": teto,
        "dentro_do_teto": not infratores,
        "infratores": infratores[:20],
        "n_infratores": len(infratores),
    }


def mede_classe(corpus: Path, lang: str, alvo: int) -> dict:
    """Os denominadores e a concentracao REAL de uma classe do estrato OSS."""
    regs = entradas_de(corpus, lang)
    n = len(regs)
    negativos = [r for r in regs if r.get("rotulo") == "negativo"]
    n_neg = len(negativos)
    conc = mede_concentracao(regs, negativos)
    conc.update(mede_teto(regs, n))
    prova = n_neg >= N_PARA_CP95_1PCT
    return {
        "lang": lang,
        "n": n,
        "n_negativos": n_neg,
        "n_positivos": n - n_neg,
        "alvo": alvo,
        "atinge_alvo": n_neg >= alvo,
        **conc,
        "n_para_cp95_1pct": N_PARA_CP95_1PCT,
        "denominador_da_prova": "n_negativos (o falso deny so existe sobre um negativo)",
        "limite_sup_cp95_0_falsos": round(limite_sup_cp95(n_neg), 6),
        "limite_sup_cp95_0_falsos_no_total": round(limite_sup_cp95(n), 6),
        "prova_cp95_1pct": prova,
        "situacao": "provada" if prova else "NAO PROVADA",
        "motivo": (
            None
            if prova
            else (
                f"n_negativos={n_neg} < {N_PARA_CP95_1PCT}: o limite superior de "
                "Clopper-Pearson 95% com 0 falsos nao fica em 1% — a classe sai como NAO "
                "PROVADA, sem inflar o numero"
            )
        ),
    }


def resumo_classes(classes: list) -> dict:
    """Os denominadores por classe, para o selo (o detalhe inteiro fica no .json)."""
    return {
        c["lang"]: {
            "n": c["n"],
            "n_negativos": c["n_negativos"],
            "n_positivos": c["n_positivos"],
            "alvo": c["alvo"],
            "atinge_alvo": c["atinge_alvo"],
            "prova_cp95_1pct": c["prova_cp95_1pct"],
            "situacao": c["situacao"],
            "limite_sup_cp95_0_falsos": c["limite_sup_cp95_0_falsos"],
            "dentro_do_teto": c["dentro_do_teto"],
            "teto_por_chave": c["teto_por_chave"],
            "n_infratores": c["n_infratores"],
        }
        for c in classes
    }


def resumo_concentracao(classes: list) -> dict:
    """A concentracao medida por classe -- medida, nunca declarada."""
    return {
        c["lang"]: {
            "max_arquivo": c["concentracao_max_arquivo"],
            "max_arquivo_frac": c["concentracao_max_arquivo_frac"],
            "max_commit": c["concentracao_max_commit"],
            "max_commit_frac": c["concentracao_max_commit_frac"],
            "arquivos_distintos": c["arquivos_distintos"],
            "commits_distintos": c["commits_distintos"],
        }
        for c in classes
    }


def selo_interno(corpus: Path) -> dict:
    """O selo interno do rotulador: a identidade do oraculo e a politica de exclusao."""
    ip = corpus / "selo.json"
    if not ip.is_file():
        return {}
    s = json.loads(ip.read_text(encoding="utf-8"))
    return {
        "n_por_lang": s.get("n_por_lang"),
        "teto_concentracao": s.get("teto_concentracao"),
        "teto_por_chave_no_fim": s.get("teto_por_chave_no_fim"),
        "exclusao": s.get("exclusao"),
        "identidade_do_oraculo": s.get("identidade_do_oraculo"),
        "gerado_em": s.get("gerado_em"),
        "concentracao_medida": s.get("concentracao_medida"),
    }


def mede_disjuncao(ids_da_camada: set, origens: list, ignorar: Path | None = None) -> tuple[list, int]:
    """Quantos ids desta camada ja estavam em um corpus anterior.

    Um held-out que repete o holdout anterior nao prova nada: o juiz pode ter sido calibrado
    nos mesmos pares, e o numero de falso deny sai otimista sem nenhum sinal.

    `ignorar` e a propria camada: quando a origem e o historico INTEIRO, ela esta dentro dele.
    """
    disjuncao = []
    for origem in origens:
        p = Path(origem).expanduser()
        outros = ids_de(p, ignorar)
        comum = sorted(ids_da_camada & outros)
        disjuncao.append(
            {
                "origem": str(p),
                "n_ids_origem": len(outros),
                "n_ids_em_comum": len(comum),
                "exemplos": comum[:5],
            }
        )
    return disjuncao, sum(d["n_ids_em_comum"] for d in disjuncao)


def linha_da_classe(c: dict) -> str:
    """A linha do resumo de uma classe."""
    teto = "DENTRO" if c["dentro_do_teto"] else f"{c['n_infratores']} infrator(es)"
    return (
        f"  {c['lang']:11} n={c['n']:>4} (negativos {c['n_negativos']:>4}/{N_PARA_CP95_1PCT})"
        f"  {c['situacao']:11}  concentracao: {c['concentracao_max_arquivo_frac']:.1%} por arquivo,"
        f" {c['concentracao_max_commit_frac']:.1%} por commit"
        f"  teto: {teto} (teto {c['teto_por_chave']})"
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", type=Path, default=PADRAO_CORPUS)
    ap.add_argument("--selo", type=Path, default=None)
    ap.add_argument("--json", type=Path, default=None)
    ap.add_argument("--alvo", type=int, default=300)
    ap.add_argument(
        "--verificar-disjuncao-de",
        action="append",
        default=[],
        metavar="DIR",
        help="corpus anterior com o qual esta camada nao pode ter entrada em comum",
    )
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()

    if a.selftest:
        return selftest()

    corpus = a.corpus.expanduser()
    if not corpus.is_dir():
        print(f"corpus inexistente: {corpus}")
        return 2
    selo_path = (a.selo or corpus.with_name(corpus.name + ".selo.json")).expanduser()
    json_path = (a.json or corpus.with_name(corpus.name + ".denominadores.json")).expanduser()

    langs = sorted(
        d.name for d in corpus.iterdir() if d.is_dir() and (d / "entradas.jsonl").is_file()
    )
    classes = [mede_classe(corpus, lang, a.alvo) for lang in langs]

    # a DISJUNCAO, medida: quantos ids desta camada ja estavam em um corpus anterior
    ids_da_camada = ids_de(corpus)
    disjuncao, n_em_comum = mede_disjuncao(ids_da_camada, a.verificar_disjuncao_de, corpus)

    arqs = sorted(p for p in corpus.rglob("*") if p.is_file())
    interno = selo_interno(corpus)

    selo = {
        "sha_tree": sha_tree(corpus),
        "arquivos": len(arqs),
        "bytes": sum(p.stat().st_size for p in arqs),
        "corpus": corpus.name,
        "estrato": "oss",
        "alvo": a.alvo,
        "n_para_cp95_1pct": N_PARA_CP95_1PCT,
        "metodo": "sha256(concat de relpath + sha256(conteudo), em ordem)",
        "classes": {c["lang"]: c for c in classes},
        "denominadores": {
            c["lang"]: {
                "n": c["n"],
                "n_negativos": c["n_negativos"],
                "n_positivos": c["n_positivos"],
                "alvo": c["alvo"],
                "atinge_alvo": c["atinge_alvo"],
                "prova_cp95_1pct": c["prova_cp95_1pct"],
                "situacao": c["situacao"],
                "limite_sup_cp95_0_falsos": c["limite_sup_cp95_0_falsos"],
            }
            for c in classes
        },
        "concentracao": {
            c["lang"]: {
                "max_arquivo": c["concentracao_max_arquivo"],
                "max_arquivo_frac": c["concentracao_max_arquivo_frac"],
                "max_commit": c["concentracao_max_commit"],
                "max_commit_frac": c["concentracao_max_commit_frac"],
                "arquivos_distintos": c["arquivos_distintos"],
                "commits_distintos": c["commits_distintos"],
            }
            for c in classes
        },
        "disjuncao": {
            "origens": disjuncao,
            "n_ids_em_comum": n_em_comum,
            "disjunta": n_em_comum == 0,
            "n_ids_da_camada": len(ids_da_camada),
        },
        "selo_interno_do_rotulador": interno,
        "nota": (
            "o selo mora FORA do corpus (o sha_tree incluiria o proprio selo); "
            "confira com confere_selo.py --corpus <dir> --selo <selo.json>"
        ),
    }
    selo_path.write_text(json.dumps(selo, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    json_path.write_text(
        json.dumps({"classes": classes, "alvo": a.alvo}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(f"selo: {selo_path}")
    print(f"denominadores: {json_path}")
    for c in classes:
        print(linha_da_classe(c))
    print(
        f"  disjuncao: {'DISJUNTA' if n_em_comum == 0 else str(n_em_comum) + ' ids em comum'}"
        f" ({len(ids_da_camada)} ids na camada)"
    )
    return 0


def selftest() -> int:
    """Controle positivo embutido: o corpus selado passa; os mutantes reprovam."""
    with tempfile.TemporaryDirectory() as tmp:
        c = Path(tmp) / "c"
        (c / "typescript").mkdir(parents=True)
        (c / "typescript" / "entradas.jsonl").write_text(
            json.dumps({"id": "a1", "lang": "typescript", "path": "x.ts", "commit": "c1",
                        "rotulo": "negativo"}) + "\n"
        )
        (c / "typescript" / "conteudo").mkdir()
        (c / "typescript" / "conteudo" / "a1.txt").write_text("x\n")
        arqs = sorted(p for p in c.rglob("*") if p.is_file())
        selo = {
            "sha_tree": sha_tree(c),
            "arquivos": len(arqs),
            "bytes": sum(p.stat().st_size for p in arqs),
        }
        from confere_selo import conferir

        intacto = conferir(c, selo) == []
        (c / "typescript" / "conteudo" / "a1.txt").write_text("y\n")
        violado = conferir(c, selo)
        sem_tree = conferir(c, {k: v for k, v in selo.items() if k != "sha_tree"})
        dentro = conferir(c, selo, c / "selo.json")
    ok = (
        intacto
        and any(v.startswith("sha_tree") for v in violado)
        and any("incompleto" in v for v in sem_tree)
        and any("DENTRO" in v for v in dentro)
    )
    print(
        f"selftest: {'ok' if ok else 'FALHA'} (intacto passa {intacto} · 1 byte reprova "
        f"{bool(violado)} · sem sha_tree reprova {bool(sem_tree)} · selo dentro reprova {bool(dentro)})"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
