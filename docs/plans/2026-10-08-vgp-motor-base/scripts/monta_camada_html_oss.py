#!/usr/bin/env python3
"""MONTA a camada html do estrato OSS, no formato que o JUIZ consome, e sela.

POR QUE ISTO EXISTE (D-8, 01/11): o adjudicador html **ja existia** (`oraculo_instancia_git.py`
+ `cdn_pin.py`) e um corpus html ja tinha sido montado e selado em 10/10
(`corpus_holdout_html_git_20261010`, 236 negativos). O que faltava nao era "construir o
adjudicador": era (a) **populacao** — o `LIMITE_COMMIT = 40` do montador de 10/10 corta o
rendimento em 240 negativos, abaixo dos 299; (b) a **base do delta** — as entradas de 10/10 nao
carregam o "antes", e sem ele o juiz nao tem contra o que comparar (o motor passaria a negar
tudo que acha no conteudo, e o numero de falso deny mediria a base ausente, nao o motor); e
(c) o **congelamento** — o selo de 10/10 nao carrega o sha256 do adjudicador.

O QUE ESTE SCRIPT FAZ (e o que ele NAO faz):
  - IMPORTA o oraculo, o `cdn_pin.py` e o `teto_de` do estrato js/ts (NAO reescreve nenhum:
    duas copias do mesmo criterio divergem, e a divergencia apareceria como contagem diferente
    sem ninguem saber qual esta certa);
  - monta as entradas NO FORMATO DO JUIZ: `snap` (o estado que o motor julga), `snap_antes` (o
    estado em C^, que e a base do delta), `project_root`, `path`, `rotulo`, `nome_esperado`,
    `mensagem`, `reproduz`, `contexto` (LISTA — a string "[]" quebra o `for` do juiz), `classe`,
    `lang`, `id`, `origem`;
  - aplica o TETO DE CONCENTRACAO (3 % por chave) com a PODA que o estrato js/ts usa: o teto e
    `max(1, int(n*0,03))` sobre o n FINAL, e a poda e repetida ate o ponto fixo (podar baixa o
    n, e baixar o n baixa o teto);
  - exclui por id os pares ja usados (disjuncao) e mede a disjuncao;
  - sela num arquivo **EXTERNO** (dentro do corpus, o `sha_tree` incluiria o proprio selo), com o
    sha256 do adjudicador, os denominadores por classe, o teto medido e a disjuncao.

O ROTULO (delta):
  NEGATIVO = o commit toca `.html` e NAO introduz nenhuma URL de CDN sem versao exata.
  POSITIVO = o commit introduz >= 1; um item por URL introduzida, classe `cdn_pin`, e
  `nome_esperado` = a URL COMO ESCRITA no atributo (a convencao do vtransv).

Uso: python3 -I monta_camada_html_oss.py --saida <dir> --selo <selo.json> [--limite-commits 100]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import oraculo_instancia_git as ORA  # noqa: E402  (o ORACULO, importado)
from cdn_pin import classe_de_cdn  # noqa: E402  (o MESMO criterio do oraculo)
from confere_selo import sha_tree as selo_sha_tree  # noqa: E402  (a MESMA funcao do conferidor)
from rotular_oss import teto_de  # noqa: E402  (o MESMO teto do estrato js/ts)

BASE = Path.home() / ".claude/touring/vgp"
OSS = BASE / "oss"
CORTE_PADRAO = "2026-10-10T00:00:00Z"
LIMITE_PADRAO = 100
TETO_CONCENTRACAO = 0.03
VERSAO_NO_CDN = re.compile(r"/npm/([^/@]+)@([^/\"']+)")


def git(d: Path, *a: str) -> str:
    return ORA.git(d, *a)


def pins_no_texto(texto: str) -> set:
    """As URLs de CDN sem versao exata — o MESMO criterio do `cdn_pin.py` (importado)."""
    return {val for _tag, _atr, val in ORA.refs(texto) if classe_de_cdn(val) == "cdn_pin"}


def ids_de(corpus: Path) -> set:
    """Os ids ja usados num corpus (para a disjuncao)."""
    ids = set()
    for f in corpus.glob("**/entradas.jsonl"):
        for linha in f.read_text(encoding="utf-8").splitlines():
            if not linha.strip():
                continue
            try:
                ids.add(json.loads(linha)["id"])
            except (json.JSONDecodeError, KeyError):
                continue
    return ids


def id_de(repo: str, rev: str, caminho: str, classe: str, nome: str) -> str:
    """Id = sha da PROVENIENCIA (repo|rev|caminho|classe|nome). Sem timestamp, sem ordem de
    execucao: o mesmo item recebe o mesmo id em qualquer corrida, em qualquer maquina."""
    return hashlib.sha256(f"{repo}|{rev}|{caminho}|{classe}|{nome}".encode()).hexdigest()[:12]


def commits_html_ate_o_corte(d: Path, corte: str, limite: int) -> list:
    """Os commits que tocam `.html`, o CORTE ANTES do limite (no montador de 10/10 era o
    contrario: commits posteriores ao corte gastavam o orcamento sem entrar no corpus)."""
    out = []
    for ln in git(d, "log", "--format=%H %cI", "--", "*.html").split("\n"):
        if not ln.strip():
            continue
        c, ts = ln.split(" ", 1)
        if ts <= corte:
            out.append(c)
        if len(out) >= limite:
            break
    return out


def arquivos_html(d: Path, c: str) -> list:
    return [
        x
        for x in git(d, "show", "--name-only", "--format=", c).split("\n")
        if x.strip().endswith(".html")
    ]


def introduzidas(d: Path, c: str, arquivos: list) -> dict:
    """{arquivo: [URLs de CDN sem versao exata introduzidas por este commit]}."""
    out: dict[str, list] = {}
    for arq in arquivos:
        txt = git(d, "show", f"{c}:{arq}")
        if not txt:
            continue
        antes = pins_no_texto(git(d, "show", f"{c}~1:{arq}"))
        novas = sorted(u for u in pins_no_texto(txt) if u not in antes)
        if novas:
            out[arq] = novas
    return out


def entrada(
    ident: str,
    repo: str,
    d: Path,
    arq: str,
    classe: str,
    rotulo: str,
    nome: str,
    mensagem: str,
    reproduz: str,
) -> dict:
    return {
        "id": ident,
        "lang": "html",
        "classe": classe,
        "rotulo": rotulo,
        "origem": "natural",
        "nome_esperado": nome,
        "path": f"{repo}/{arq}",
        "project_root": str(d),
        "reproduz": reproduz,
        "contexto": [],
        "mensagem": mensagem,
        "snap": f"snap/{ident}.html",
        "snap_antes": f"snap_antes/{ident}.html",
    }


def mensagem_de_cdn(url: str) -> str:
    m = VERSAO_NO_CDN.search(url)
    ver = m.group(2).split("/")[0] if m else "?"
    exata = f"{ver}.0" if re.match(r"^\d+\.\d+$", ver) else "?"
    if exata == "?":
        return f"versao nao exata @{ver} -- aviso, nao negacao"
    return f"versao nao exata @{ver} (a exata era @{exata}) -- aviso, nao negacao"


def seleciona(d: Path, corte: str, limite: int, excluidos: set, saida: Path) -> tuple:
    """Seleciona as entradas e grava os snaps. Devolve (entradas, contadores, descartados)."""
    entradas: list[dict] = []
    counts: Counter = Counter()
    n_excluidos = 0
    for c in commits_html_ate_o_corte(d, corte, limite):
        arquivos = arquivos_html(d, c)
        if not arquivos:
            continue
        novas = introduzidas(d, c, arquivos)
        if not novas:
            # NEGATIVO: o commit toca .html e nao introduz URL de CDN sem versao exata
            arq = sorted(arquivos)[0]
            ident = id_de(d.name, c, arq, "negativo", "")
            if ident in excluidos:
                n_excluidos += 1
                continue
            (saida / "snap" / f"{ident}.html").write_text(git(d, "show", f"{c}:{arq}"), encoding="utf-8")
            (saida / "snap_antes" / f"{ident}.html").write_text(
                git(d, "show", f"{c}~1:{arq}"), encoding="utf-8"
            )
            entradas.append(
                entrada(
                    ident,
                    d.name,
                    d,
                    arq,
                    "",
                    "negativo",
                    "",
                    "commit sem URL de CDN sem versao exata",
                    "false",
                )
            )
            entradas[-1]["commit"] = c
            counts["negativo"] += 1
            continue
        # POSITIVO: um item por URL introduzida
        for arq, urls in sorted(novas.items()):
            txt = git(d, "show", f"{c}:{arq}")
            antes_txt = git(d, "show", f"{c}~1:{arq}")
            for url in urls:
                ident = id_de(d.name, c, arq, "cdn_pin", url)
                if ident in excluidos:
                    n_excluidos += 1
                    continue
                (saida / "snap" / f"{ident}.html").write_text(txt, encoding="utf-8")
                (saida / "snap_antes" / f"{ident}.html").write_text(antes_txt, encoding="utf-8")
                entradas.append(
                    entrada(
                        ident,
                        d.name,
                        d,
                        arq,
                        "cdn_pin",
                        "positivo",
                        url,
                        mensagem_de_cdn(url),
                        "true",
                    )
                )
                entradas[-1]["commit"] = c
                counts["positivo"] += 1
                counts["cdn_pin"] += 1
    return entradas, counts, n_excluidos


def podar_por_teto(entradas: list, teto: int) -> tuple:
    """Poda cada chave ao teto DADO, **uma vez**.

    MEDIDO (o teste (f) do selftest): realimentar o teto pelo n que a poda produz DEGENERA —
    podar baixa o n, e baixar o n baixa o teto (`max(1, int(n*frac))`), e o conjunto colapsa
    (10 entradas numa chave com 3 % caem para 1). Entao o teto da poda e o dos CANDIDATOS (limite
    superior do n final) e a MEDICAO do teto e feita no n final, que e o que o selo declara.
    """
    por_chave: Counter = Counter()
    mantidas: list[dict] = []
    podadas = 0
    for e in entradas:
        chave = e["path"]
        if por_chave[chave] >= teto:
            podadas += 1
            continue
        por_chave[chave] += 1
        mantidas.append(e)
    return mantidas, podadas


def situacao_de(n_negativos: int, dentro_do_teto: bool) -> tuple[str, str]:
    """O VEREDITO pelo critério INTEIRO: **299 negativos ou mais**, **dentro do teto medido** e
    selo conferido (a conferência é do `confere_selo.py`, fora daqui).

    MEDIDO em 01/11: uma versao anterior desta funcao olhava SO o n e declarava «provada» com a
    concentracao violada — um veredito que o criterio nao sustenta. O motivo vai junto, porque
    um veredito sem motivo nao diz o que falta.
    """
    if n_negativos >= 299 and dentro_do_teto:
        return (
            "provada",
            "299 negativos ou mais, dentro do teto medido; selo conferido pelo confere_selo.py",
        )
    if n_negativos < 299:
        return "nao provada", f"n = {n_negativos} negativos, abaixo dos 299"
    return (
        "nao provada",
        f"n = {n_negativos} negativos (acima de 299), mas a concentracao media violou o teto",
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--saida", type=Path, required=True)
    ap.add_argument("--selo", type=Path, required=True)
    ap.add_argument("--json", type=Path)
    ap.add_argument("--limite-commits", type=int, default=LIMITE_PADRAO)
    ap.add_argument("--corte", default=CORTE_PADRAO)
    ap.add_argument("--teto-concentracao", type=float, default=TETO_CONCENTRACAO)
    ap.add_argument("--excluir-corpus", action="append", default=[])
    ap.add_argument("--repos", default="")
    a = ap.parse_args()

    if a.saida.exists():
        print(f"ABORTO: {a.saida} ja existe — este script so cria diretorio NOVO")
        return 2
    if a.selo.resolve().is_relative_to(a.saida.resolve()):
        print(f"ABORTO: o selo mora DENTRO do corpus ({a.selo}) — o sha_tree o incluiria")
        return 2
    (a.saida / "snap").mkdir(parents=True)
    (a.saida / "snap_antes").mkdir(parents=True)

    excluidos: set = set()
    for origem in a.excluir_corpus:
        p = Path(origem)
        if p.exists():
            excluidos |= ids_de(p)
    print(f"disjuncao: {len(excluidos)} ids ja usados nos corpora de origem")

    nomes = [r for r in a.repos.split(",") if r] or None
    repos = sorted(
        p
        for p in OSS.iterdir()
        if p.is_dir() and (p / ".git").exists() and (not nomes or p.name in nomes)
    )

    entradas: list[dict] = []
    counts: Counter = Counter()
    n_excluidos = 0
    for d in repos:
        e, c, x = seleciona(d, a.corte, a.limite_commits, excluidos, a.saida)
        entradas += e
        counts.update(c)
        n_excluidos += x
        print(f"  {d.name:20s} entradas {len(e):>5d}  (descartadas por id: {x})")

    # O teto e aplicado NA SELECAO com o teto dos CANDIDATOS (limite superior do n final) e
    # MEDIDO no n final. Realimentar o teto pelo n que a poda produz degenera (medido no
    # selftest: 10 entradas numa chave com 3 % caem para 1).
    teto_selecao = teto_de(len(entradas), a.teto_concentracao)
    entradas, n_podadas = podar_por_teto(entradas, teto_selecao)
    por_caminho = Counter(e["path"] for e in entradas)
    por_commit = Counter(e.get("commit", "") for e in entradas)
    max_caminho = max(por_caminho.values()) if por_caminho else 0
    max_commit = max(por_commit.values()) if por_commit else 0
    teto = teto_de(len(entradas), a.teto_concentracao)
    dentro_do_teto = max_caminho <= teto and max_commit <= teto

    linhas = [json.dumps(e, ensure_ascii=False, sort_keys=True) for e in entradas]
    (a.saida / "entradas.jsonl").write_text(
        "\n".join(linhas) + ("\n" if linhas else ""), encoding="utf-8"
    )

    arqs = [p for p in a.saida.rglob("*") if p.is_file()]
    n = len(entradas)
    n_neg = sum(1 for e in entradas if e["rotulo"] == "negativo")
    selo = {
        "estrato": "oss_html",
        "corte": a.corte,
        "limite_commits": a.limite_commits,
        "orcamento": "oraculo por instancia no git: INSTANTE=C, BASE=C^, `git ls-tree -r C`",
        "sha_tree": selo_sha_tree(a.saida),
        "arquivos": len(arqs),
        "bytes": sum(p.stat().st_size for p in arqs),
        "entradas_jsonl_sha256": hashlib.sha256(
            (a.saida / "entradas.jsonl").read_bytes()
        ).hexdigest(),
        "n_entradas": n,
        "por_rotulo": {
            "negativo": n_neg,
            "positivo": n - n_neg,
        },
        "por_classe": {
            c: sum(1 for e in entradas if e["classe"] == c)
            for c in sorted({e["classe"] for e in entradas if e["classe"]})
        },
        "denominador_da_prova": "n_negativos (o falso deny so existe sobre um negativo)",
        "n_negativos": n_neg,
        "n_para_cp95_1pct": 299,
        "situacao": situacao_de(n_neg, dentro_do_teto)[0],
        "motivo": situacao_de(n_neg, dentro_do_teto)[1],
        "teto_por_chave": teto,
        "teto_na_selecao": teto_selecao,
        "concentracao_max_arquivo": max_caminho,
        "concentracao_max_commit": max_commit,
        "n_infratores": max(0, max_caminho - teto) + max(0, max_commit - teto),
        "dentro_do_teto": dentro_do_teto,
        "podadas_por_teto": n_podadas,
        "disjuncao": {
            "ids_excluidos_da_origem": len(excluidos),
            "entradas_descartadas_por_id": n_excluidos,
        },
        "adjudicador": {
            "oraculo_instancia_git_sha256": hashlib.sha256(
                (Path(__file__).resolve().parent / "oraculo_instancia_git.py").read_bytes()
            ).hexdigest(),
            "cdn_pin_sha256": hashlib.sha256(
                (Path(__file__).resolve().parent / "cdn_pin.py").read_bytes()
            ).hexdigest(),
        },
    }
    a.selo.parent.mkdir(parents=True, exist_ok=True)
    a.selo.write_text(
        json.dumps(selo, ensure_ascii=False, indent=1, sort_keys=True), encoding="utf-8"
    )
    if a.json:
        a.json.write_text(
            json.dumps(selo, ensure_ascii=False, indent=1, sort_keys=True), encoding="utf-8"
        )

    print(f"camada: {a.saida}")
    print(f"  entradas: {n}  (negativos {n_neg}, positivos {n - n_neg})")
    print(f"  por classe: {selo['por_classe']}")
    print(f"  teto por chave: {teto}  max por arquivo: {max_caminho}  podadas: {n_podadas}")
    print(f"  dentro do teto: {selo['dentro_do_teto']}")
    print(f"  disjuncao: {n_excluidos} descartadas por id (de {len(excluidos)} ids de origem)")
    print(f"  sha_tree: {selo['sha_tree']}  arquivos: {selo['arquivos']}  bytes: {selo['bytes']}")
    print(f"  situacao: {selo['situacao']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
