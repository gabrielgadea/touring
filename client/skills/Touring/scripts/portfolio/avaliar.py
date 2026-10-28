#!/usr/bin/env python3
# #tags: kind:script purpose:measure domain:code-mode process:measure lang:python
"""avaliar.py — score a portfolio retriever on the frozen gold set (Phase 0, S-0.4).

A retriever is any callable ``(consulta: Item, biblioteca: list[Item]) -> Veredito``. The verdict uses
the four states of ``veredito_gate/1``; only ``reprovado`` ("an equivalent exists, reuse it") is the
strong claim that P5 may one day turn into a deny, so it is the one scored:

* ``tem_equivalente`` query: TP if reprovado and the top candidate is a labeled equivalent; FP if
  reprovado with a wrong top; FN if not reprovado;
* ``sem_equivalente`` query: FP (false alarm) if reprovado, TN otherwise;
* ``ambigua`` / ``sem_biblioteca``: not scored, counted;
* an exception or ``inaferivel``: **unscored** (harness2: a failed evaluation is never a zero).

Counts are reported per label method (real Jaccard vs synthetic construction) so a precision built on
26 positives is read with its sample size. Baselines shipped here: the CURRENT hook matcher (imported
from the installed hook, not re-implemented) and the SimHash body signal alone (tau=10).

Usage:
    python3 avaliar.py --corpus <corpus.jsonl.gz> --gold <gold.json> [--particao held_out] [--campeoes <dir>]
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from collections import Counter
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from types import ModuleType

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import gold as gold_mod
import recuperar
import simhash

HOOK = AQUI.parent / "hooks" / "script_portfolio_gate.py"
ESTADOS = ("reprovado", "aprovado", "inaferivel", "nao_aplicavel")


@dataclass(frozen=True)
class Veredito:
    """Retriever answer in ``veredito_gate/1`` terms; ``candidatos`` best first as (id, sinal)."""

    estado: str
    candidatos: tuple[tuple[str, float], ...] = ()

    def __post_init__(self) -> None:
        if self.estado not in ESTADOS:
            raise ValueError(f"estado {self.estado!r} fora de {ESTADOS}")


Recuperador = Callable[[gold_mod.Item, list[gold_mod.Item]], Veredito]


def _classificar(v: Veredito, rotulo: dict) -> str:
    forte = v.estado == "reprovado"
    if rotulo["classe"] == "tem_equivalente":
        if not forte:
            return "fn"
        return "tp" if v.candidatos and v.candidatos[0][0] in rotulo["equivalentes"] else "fp_errado"
    return "fp_alarme" if forte else "tn"


def _metricas(c: Counter) -> dict:
    tp, fp = c["tp"], c["fp_errado"] + c["fp_alarme"]
    positivos = c["tp"] + c["fn"] + c["fp_errado"]
    negativos = c["fp_alarme"] + c["tn"]
    return {
        **dict(sorted(c.items())),
        "precisao": round(tp / (tp + fp), 4) if tp + fp else None,
        "recall": round(tp / positivos, 4) if positivos else None,
        "taxa_falso_alarme": round(c["fp_alarme"] / negativos, 4) if negativos else None,
        "n_positivos": positivos,
        "n_negativos": negativos,
    }


def avaliar(recuperador: Recuperador, itens: list[gold_mod.Item], gold: dict, particao: str = "held_out") -> dict:
    """Score one retriever on one partition; unscored and not-scored classes stay visible."""
    rotulos = gold["rotulos"]
    por_metodo: dict[str, Counter] = {}
    fora = Counter()
    for q in itens:
        r = rotulos.get(q.id)
        if r is None or r["particao"] != particao:
            continue
        if r["classe"] not in ("tem_equivalente", "sem_equivalente"):
            fora[r["classe"]] += 1
            continue
        c = por_metodo.setdefault(r["metodo"], Counter())
        try:
            v = recuperador(q, gold_mod.biblioteca_de(q, itens))
        except Exception:
            c["unscored"] += 1
            continue
        if v.estado == "inaferivel":
            c["unscored"] += 1
            continue
        c[_classificar(v, r)] += 1
    total = sum(por_metodo.values(), Counter())
    return {
        "particao": particao,
        "geral": _metricas(total),
        "por_metodo": {m: _metricas(c) for m, c in sorted(por_metodo.items())},
        "nao_pontuadas": dict(fora),
    }


def recuperador_simhash(tau: int = simhash.TAU_FORTE) -> Recuperador:
    """Body signal alone: nearest library item by Hamming; reprovado when <= tau."""
    cache: dict[str, int] = {}

    def h(i: gold_mod.Item) -> int:
        if i.id not in cache:
            cache[i.id] = simhash.simhash64(i.content)
        return cache[i.id]

    def rec(q: gold_mod.Item, bib: list[gold_mod.Item]) -> Veredito:
        if not bib:
            return Veredito("aprovado")
        hq = h(q)
        dist, melhor = min((simhash.hamming(hq, h(c)), c.id) for c in bib)
        return Veredito("reprovado" if dist <= tau else "aprovado", ((melhor, float(dist)),))

    return rec


def recuperador_portfolio(params: recuperar.Parametros | None = None) -> Recuperador:
    """The production decision (``recuperar.decidir``) on the query's library — the same code the hook runs."""
    escolhidos = params or recuperar.Parametros()
    cache: dict[str, recuperar.Candidato] = {}

    def cand(i: gold_mod.Item) -> recuperar.Candidato:
        if i.id not in cache:
            cache[i.id] = recuperar.Candidato(id=i.id, nome=i.nome, conteudo=i.content)
        return cache[i.id]

    def rec(q: gold_mod.Item, bib: list[gold_mod.Item]) -> Veredito:
        v = recuperar.decidir(q.nome, q.content, [cand(c) for c in bib], (), escolhidos)
        return Veredito(v.estado, tuple((cid, float(-1 if d is None else d)) for cid, _s, d in v.candidatos))

    return rec


def grade(itens: list[gold_mod.Item], gold: dict, taus: tuple[int, ...], pisos: tuple[int, ...]) -> list[dict]:
    """Calibration grid on the TRAIN half only (held-out stays untouched until the choice is made)."""
    linhas = []
    for piso in pisos:
        for tau in taus:
            rel = avaliar(recuperador_portfolio(recuperar.Parametros(tau=tau, piso_linhas=piso)), itens, gold, "treino")
            g = rel["geral"]
            linhas.append({"tau": tau, "piso": piso, "precisao": g["precisao"], "recall": g["recall"],
                           "falso_alarme": g["taxa_falso_alarme"], "tp": g.get("tp", 0),
                           "fp": g.get("fp_errado", 0) + g.get("fp_alarme", 0), "unscored": g.get("unscored", 0)})
    return linhas


def _carregar_hook() -> ModuleType:
    spec = importlib.util.spec_from_file_location("script_portfolio_gate_avaliado", HOOK)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"não foi possível carregar {HOOK}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class _ManifestoEmMemoria:
    """Stands in for the hook's MANIFEST path: same ``read_text`` contract, library as content."""

    def __init__(self) -> None:
        self.texto = '{"scripts": []}'

    def read_text(self, *_args: object, **_kwargs: object) -> str:
        return self.texto


def _proposito(content: str) -> str:
    for linha in content.splitlines():
        s = linha.strip().strip("#/\"' ")
        if len(s) > 8:
            return s[:160]
    return ""


def recuperador_hook_atual() -> Recuperador:
    """The installed hook's matcher, run on the query's library: its own keywords, candidates and rule."""
    hook = _carregar_hook()
    manifesto = _ManifestoEmMemoria()
    hook.MANIFEST = manifesto  # the hook reads MANIFEST.read_text(); same contract, library as content
    hook.PORTFOLIO = Path()
    entradas: dict[str, dict] = {}

    def entrada(i: gold_mod.Item) -> dict:
        if i.id not in entradas:
            entradas[i.id] = {"slug": Path(i.nome).stem, "purpose": _proposito(i.content),
                              "domain": "", "tags": "", "path": f"{i.id}/{i.nome}"}
        return entradas[i.id]

    def rec(q: gold_mod.Item, bib: list[gold_mod.Item]) -> Veredito:
        manifesto.texto = json.dumps({"scripts": [entrada(c) for c in bib]})
        cands = hook.manifest_candidates(hook.keywords_of(q.nome, q.content))
        if not cands:
            return Veredito("aprovado")
        mesmo_nome = any(Path(c["path"]).name == q.nome for c in cands)
        forte = mesmo_nome or any(c.get("hits", 0) >= 2 for c in cands)  # the hook's would_avoid rule
        top = ((Path(c["path"]).parent.name, float(c.get("hits", 0))) for c in cands)
        return Veredito("reprovado" if forte else "aprovado", tuple(top))

    return rec


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="avalia recuperadores do portfólio no gold congelado")
    ap.add_argument("--corpus", type=Path, required=True)
    ap.add_argument("--gold", type=Path, required=True)
    ap.add_argument("--campeoes", type=Path)
    ap.add_argument("--particao", default="held_out", choices=("held_out", "treino"))
    ap.add_argument("--grade", action="store_true", help="calibra tau x piso só no treino")
    args = ap.parse_args(argv)
    manifesto = gold_mod.MANIFESTO
    esperado = json.loads(manifesto.read_text(encoding="utf-8"))["sha256_conteudo"] if manifesto.exists() else None
    itens = gold_mod.carregar_corpus(args.corpus, esperado)
    if args.campeoes:
        itens += gold_mod.sinteticos(args.campeoes)[0]
    gold = json.loads(args.gold.read_text(encoding="utf-8"))
    if args.grade:
        print(json.dumps(grade(itens, gold, (4, 6, 8, 10), (5, 8, 12)), ensure_ascii=False, indent=1))
        return 0
    relatorio = {
        nome: avaliar(rec, itens, gold, args.particao)
        for nome, rec in (("hook_atual", recuperador_hook_atual()), ("simhash_tau10", recuperador_simhash()),
                          ("portfolio_p1", recuperador_portfolio()))
    }
    print(json.dumps(relatorio, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
