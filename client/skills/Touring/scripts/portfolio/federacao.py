#!/usr/bin/env python3
# #tags: kind:script purpose:gate domain:code-mode process:measure lang:python
"""federacao.py — read the portfolio from several sources WITHOUT copying any of them (P2, option C).

Sources, highest precedence first:
  1. the PROJECT index — ``<project>/.claude/portfolio/indice.json``, written by the project's own exporter
     (in analise: ``scripts/lexhub/portfolio_export.py``, from the FACT catalog), found by walking up from the
     event's ``cwd``;
  2. the GLOBAL portfolio — ``~/.claude/scripts-portfolio/MANIFEST.json``;
  3. tantivy (consulted by ``recuperar.consultar``; not a federated list, a search).

Rule shared with ``portfolio_graph.py``: **the project wins by name** — a global entry whose file name also
exists in the project index is dropped, so a project's own version is the one suggested. Within one source,
homonyms stay (``probe.py`` in two directories are two scripts).

Every project entry carries the minimal contract ``{caminho, proposito, familia, simhash, linhas, termos}``;
simhash, lines and terms are precomputed by the exporter, because the hook is a fresh process per ``Write``
and must never re-read thousands of files. A missing index means the project does not export (the source is
simply not consulted); a present but unreadable or wrong-version index RAISES, so the caller can answer
``inaferivel`` instead of pretending the project has nothing.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

from recuperar import Candidato

INDICE_REL = Path(".claude") / "portfolio" / "indice.json"
VERSAO = 1


class IndiceInvalido(ValueError):
    """The project index exists but cannot be trusted (unreadable JSON, wrong version, wrong shape)."""


def achar_indice(cwd: str | None) -> Path | None:
    """Walk up from ``cwd`` to the first ``.claude/portfolio/indice.json``; None when no ancestor exports."""
    if not cwd:
        return None
    atual = Path(cwd).resolve()
    for pasta in (atual, *atual.parents):
        candidato = pasta / INDICE_REL
        if candidato.is_file():
            return candidato
    return None


def fonte_projeto(indice: Path) -> list[Candidato]:
    """Candidates of one project index; raises ``IndiceInvalido`` instead of returning an empty list."""
    try:
        dados = json.loads(indice.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise IndiceInvalido(f"{indice}: ilegível ({type(exc).__name__})") from exc
    if not isinstance(dados, dict) or dados.get("versao") != VERSAO or not isinstance(dados.get("entradas"), list):
        raise IndiceInvalido(f"{indice}: versão ou forma inesperada (esperava versao={VERSAO} e lista 'entradas')")
    raiz = str(indice.parents[2])
    out = []
    for e in dados["entradas"]:
        rel = str(e["caminho"])
        # string ops, not Path: two Path objects per entry cost ~40 ms over 7 893 entries (measured 28/10/2026)
        out.append(
            Candidato(
                id=f"projeto:{rel}",
                nome=rel.rsplit("/", 1)[-1],
                caminho=f"{raiz}/{rel}",
                proposito=str(e.get("proposito", ""))[:200],
                origem="projeto",
                hash_pre=int(e["simhash"], 16) if e.get("simhash") else None,
                linhas_pre=e.get("linhas"),
                termos_pre=frozenset(e.get("termos", ())),
            )
        )
    return out


def federar(*fontes: list[Candidato]) -> list[Candidato]:
    """Merge sources in precedence order; a name already offered by a higher source drops the lower one."""
    vistos: set[str] = set()
    out: list[Candidato] = []
    for fonte in fontes:
        nomes_desta = set()
        for c in fonte:
            if c.nome in vistos:
                continue
            out.append(c)
            nomes_desta.add(c.nome)
        vistos |= nomes_desta
    return out
