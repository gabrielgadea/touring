#!/usr/bin/env python3
"""O «menor grafo» MEDIDO dos journals ADW — que nós discriminam, que pares sobram.

Isenberg: o valor de um grafo está nos nós que SEPARAM casos. Um nó que devolveu
o mesmo veredito em todas as execuções nunca separou nada — o veredito do fluxo
não carrega a informação dele (Lei L2 pelo avesso: o runner encerra o loop, mas
nada ali é capaz de dizer «não»). E um par de nós que concordou em toda execução
comum é redundância estrutural: o segundo não separa nada que o primeiro já não
separasse.

A afirmação exige amostra: abaixo de `--piso` execuções (default 5) o veredito é
`indeterminado`, nunca «redundante» — reprovar trabalho sadio com N=1 ensina o
operador a ignorar o gate (a mesma disciplina de `sinal ruidoso é pior que sinal
nenhum`).

Lê `<root>/.touring/adw-runs/*/journal.jsonl` (eventos `node_completed`, campo
`verdict`). Origem: bundle `docs/plans/2026-08-23-graph-engineering`.
RECONSTRUÍDO em 23/08/2026 — o original morreu com o scratchpad da sessão
interrompida; `measurements/redundancia.json` fixa o contrato de saída e o
handoff fixa o texto do caso strategy-loop (104 runs · 5 nós · 10 pares).
"""
from __future__ import annotations

import argparse
import itertools
import json
import sys
from collections import defaultdict
from pathlib import Path

PISO = 5


def ler_journals(root: Path) -> tuple[int, dict[str, list[dict]]]:
    """journals lidos + {fluxo: [ {nó: [vereditos]} por run ]}.

    O nome do fluxo vem do evento `run_started` (campo `adw`) — o prefixo do
    diretório truncaria fluxos com `-` no nome.
    """
    por_fluxo: dict[str, list[dict]] = defaultdict(list)
    lidos = 0
    for j in sorted((root / ".touring" / "adw-runs").glob("*/journal.jsonl")):
        fluxo, nos = None, defaultdict(list)
        try:
            with j.open(encoding="utf-8") as fh:
                for linha in fh:
                    try:
                        ev = json.loads(linha)
                    except json.JSONDecodeError:
                        continue
                    if ev.get("event") == "run_started":
                        fluxo = ev.get("adw")
                    elif ev.get("event") == "node_completed":
                        nos[str(ev.get("node"))].append(str(ev.get("verdict")))
        except OSError:
            continue
        if fluxo:
            lidos += 1
            por_fluxo[fluxo].append(dict(nos))
    return lidos, dict(por_fluxo)


def medir_fluxo(runs: list[dict], piso: int) -> dict:
    """O laudo de um fluxo: nós que nunca discriminaram, indeterminados, pares."""
    execucoes: dict[str, list[str]] = defaultdict(list)   # nó -> vereditos (todas as runs)
    for run in runs:
        for no, vs in run.items():
            execucoes[no].extend(vs)
    nunca, indet = [], []
    for no in sorted(execucoes):
        vs = execucoes[no]
        distintos = sorted(set(vs))
        if len(vs) < piso:
            indet.append({
                "no": no, "execucoes": len(vs), "vereditos": distintos,
                "veredito": "indeterminado",
                "porque": f"{len(vs)} execução(ões) — abaixo do piso de {piso}; "
                          "a amostra não sustenta a afirmação"})
        elif len(distintos) == 1:
            nunca.append({
                "no": no, "execucoes": len(vs), "vereditos": distintos,
                "veredito": "nunca discriminou",
                "porque": f"em {len(vs)} execuções devolveu sempre '{distintos[0]}' "
                          "— não separou caso algum"})
    # pares: concordância por RUN (o último veredito de cada nó na run), só entre
    # nós com amostra suficiente de execuções comuns
    pares = []
    nos_ordenados = sorted(execucoes)
    for a, b in itertools.combinations(nos_ordenados, 2):
        comuns = concordes = 0
        for run in runs:
            if a in run and b in run:
                comuns += 1
                if run[a][-1] == run[b][-1]:
                    concordes += 1
        if comuns >= piso and concordes == comuns:
            pares.append({
                "a": a, "b": b, "execucoes_comuns": comuns,
                "porque": "concordaram em todas — o segundo não separou nada "
                          "que o primeiro já não separasse"})
    return {"runs": len(runs), "nos": len(execucoes),
            "nunca_discriminaram": nunca, "indeterminados": indet,
            "pares_redundantes": pares}


def relatar(fluxo: str, m: dict) -> list[str]:
    """As linhas humanas de um fluxo — o resumo que o documento interpreta."""
    plural_r = "runs" if m["runs"] != 1 else "run"
    plural_n = "nós" if m["nos"] != 1 else "nó"
    L = [f"▸ {fluxo}  ({m['runs']} {plural_r} · {m['nos']} {plural_n})"]
    nunca = m["nunca_discriminaram"]
    if nunca and len(nunca) == m["nos"] and len({n['vereditos'][0] for n in nunca}) == 1:
        v, e = nunca[0]["vereditos"][0], nunca[0]["execucoes"]
        L.append(f"    ⚠ TODOS os {m['nos']} nós devolveram '{v}' em {e} execuções "
                 "— nenhum separou caso algum")
    else:
        for n in nunca:
            L.append(f"    ⚠ {n['no']}: {n['porque']}")
    pares = m["pares_redundantes"]
    if pares and len(pares) == m["nos"] * (m["nos"] - 1) // 2:
        L.append(f"    ⚠ os {len(pares)} pares concordaram em "
                 f"{pares[0]['execucoes_comuns']}/{pares[0]['execucoes_comuns']}")
    else:
        for p in pares:
            L.append(f"    ⚠ par {p['a']} × {p['b']}: {p['porque']} "
                     f"({p['execucoes_comuns']} comuns)")
    if m["indeterminados"] and not nunca and not pares:
        L.append(f"    indeterminado: {m['indeterminados'][0]['porque']}")
    return L


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description="Mede, dos journals ADW, quais nós nunca discriminaram e "
                    "quais pares são redundantes — o «menor grafo» por evidência.")
    ap.add_argument("--root", type=Path, default=Path("."),
                    help="raiz do projeto (contém .touring/adw-runs)")
    ap.add_argument("--piso", type=int, default=PISO,
                    help=f"execuções mínimas para afirmar algo (default {PISO})")
    ap.add_argument("--json", type=Path, default=None,
                    help="grava o laudo completo neste arquivo")
    a = ap.parse_args(argv)
    lidos, por_fluxo = ler_journals(a.root.resolve())
    laudo = {"journals": lidos, "piso": a.piso,
             "fluxos": {f: medir_fluxo(runs, a.piso)
                        for f, runs in sorted(por_fluxo.items())}}
    if a.json:
        a.json.write_text(json.dumps(laudo, ensure_ascii=False, indent=1) + "\n",
                          encoding="utf-8")
    print(f"journals: {lidos} · piso: {a.piso}")
    for fluxo in sorted(laudo["fluxos"]):
        for linha in relatar(fluxo, laudo["fluxos"][fluxo]):
            print(linha)
    return 0


if __name__ == "__main__":
    sys.exit(main())
