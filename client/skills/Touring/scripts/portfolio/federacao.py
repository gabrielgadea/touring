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
import marshal
import os
import sys
from pathlib import Path

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

from recuperar import Candidato, Registro

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


#: Binary cache of the parsed index, next to it (``indice.marshal``). D09, 05/11/2026: parsing the 2,7 MB JSON
#: and building 7 973 ``Registro`` cost 31 ms of every hook call (cProfile + perf_counter); marshal of the same
#: rows, frozensets included, rebuilds them in 11 ms, identical (``==``).
#: Trust: marshal, not pickle — ``loads`` rebuilds plain values and never calls code (no __reduce__, no imports),
#: and the rows only feed a slots dataclass. The file lives in the project's git-ignored ``.claude/portfolio/``,
#: the same boundary as ``indice.json``, which this module already trusts; a malformed file only falls back.
_CACHE_FMT = 1


def _cache_de(indice: Path) -> Path:
    return indice.with_suffix(".marshal")


def _do_cache(indice: Path, chave: tuple) -> list[Registro] | None:
    """The cached rows when the header matches ``chave`` (format, mtime_ns, size, root); None otherwise."""
    try:
        cab, linhas = marshal.loads(_cache_de(indice).read_bytes())  # noqa: S302 — trust note above _CACHE_FMT
    except (OSError, ValueError, EOFError, TypeError):
        return None
    if cab != chave:
        return None
    return [Registro(a, b, c, li, t) for a, b, c, li, t in linhas]


def _gravar_cache(indice: Path, chave: tuple, regs: list[Registro]) -> None:
    """Best effort, atomic (tmp + replace): a failed write only costs the next call the JSON path."""
    linhas = [(r.caminho, r.rel, r.hash_pre, r.linhas_pre, r.termos_pre) for r in regs]
    destino = _cache_de(indice)
    tmp = destino.with_name(f"{destino.name}.{os.getpid()}.tmp")
    try:
        tmp.write_bytes(marshal.dumps((chave, linhas)))
        os.replace(tmp, destino)
    except OSError:
        tmp.unlink(missing_ok=True)


def fonte_projeto(indice: Path) -> list[Registro]:
    """The entries of one project index; raises ``IndiceInvalido`` instead of returning an empty list.

    Served from ``indice.marshal`` when its header matches the index's (mtime_ns, size) and root; any mismatch or
    unreadable cache falls back to the JSON, which stays the only source of truth, and rewrites the cache."""
    try:
        st = indice.stat()
    except OSError as exc:
        raise IndiceInvalido(f"{indice}: ilegível ({type(exc).__name__})") from exc
    chave = (_CACHE_FMT, st.st_mtime_ns, st.st_size, str(indice.parents[2]))
    em_cache = _do_cache(indice, chave)
    if em_cache is not None:
        return em_cache
    regs = _fonte_projeto_json(indice)
    _gravar_cache(indice, chave, regs)
    return regs


def _fonte_projeto_json(indice: Path) -> list[Registro]:
    """The JSON path (the truth): parse, validate version and shape, build one ``Registro`` per entry."""
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
        sim = e.get("simhash")
        # string ops, not Path: two Path objects per entry cost ~40 ms over 7 893 entries (measured 28/10/2026)
        out.append(
            Registro(
                caminho=f"{raiz}/{rel}",
                rel=rel,
                hash_pre=int(sim, 16) if sim else None,
                linhas_pre=e.get("linhas"),
                termos_pre=frozenset(e.get("termos", ())),
            )
        )
    return out


def federar(*fontes: list[Candidato | Registro]) -> list[Candidato | Registro]:
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
