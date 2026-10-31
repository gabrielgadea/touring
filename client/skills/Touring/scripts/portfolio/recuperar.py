#!/usr/bin/env python3
# #tags: kind:script purpose:gate domain:code-mode process:measure lang:python
"""recuperar.py — the single portfolio retrieval: body + purpose signals, answered as ``veredito_gate/1``.

One function decides for the Write/Bash hook, for the CLI and for the evaluator, so the three can never
disagree (Phase 1 of the portfolio plan, option C approved 28/10/2026).

Decision rule (calibrated on the TRAIN half of the frozen gold, reported on held-out):
* ``reprovado`` ("an equivalent exists, reuse it") needs BODY evidence: SimHash Hamming <= ``tau`` against
  a candidate, and both bodies above the substance floor (the Rust repeat gate's own floor: fewer than 5
  non-empty lines is too noisy to claim equivalence — and cheaper to rewrite than to look up);
* purpose (rare tokens of name and docstring) has no labeled precision yet, so it only RANKS and ADVISES;
  it never produces ``reprovado`` by itself;
* if a source failed and nothing strong was found, the answer is ``inaferivel`` — "I could not look" is
  never "nothing exists";
* ``aprovado``: every source answered and nothing crossed the bar; ``nao_aplicavel``: not a script.

Usage:
    python3 recuperar.py --arquivo <script> [--cwd <dir>]     # JSON verdict; exit 0/1/2 by the contract
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from collections.abc import Iterable
from dataclasses import asdict, dataclass, field
from pathlib import Path

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import simhash

SCRIPT_EXTS = {".py", ".sh", ".js", ".mjs", ".ts", ".rb", ".pl", ".bash"}
MANIFEST = Path.home() / ".claude" / "scripts-portfolio" / "MANIFEST.json"
EXIT = {"aprovado": 0, "nao_aplicavel": 0, "reprovado": 1, "inaferivel": 2}
_PALAVRA = re.compile(r"[a-záéíóúâêôãõç]{4,}")
_VAZIAS = {"import", "from", "return", "print", "self", "none", "true", "false", "with", "para", "como", "uma", "def"}


@dataclass(frozen=True)
class Parametros:
    """Calibrated knobs; defaults are the values chosen on the train half (see ``avaliar.py --grade``)."""

    tau: int = 8
    piso_linhas: int = 5
    tau_fraco: int = simhash.TAU_FRACO


@dataclass(slots=True)
class Candidato:
    """A library entry, whatever the source: the minimal contract every federated source exposes.

    ``slots`` and not ``frozen``: a frozen dataclass sets every field through ``object.__setattr__``, and building
    the 7 893 candidates of the analise index cost 69 ms of a 150 ms hook budget (measured 28/10/2026). The
    instance was never truly immutable anyway — it carries a lazy cache.
    """

    id: str
    nome: str
    caminho: str = ""
    conteudo: str = ""
    proposito: str = ""
    origem: str = ""
    #: Precomputed by an exporter (P2 federation) so a hook process never re-reads thousands of files.
    hash_pre: int | None = None
    linhas_pre: int | None = None
    termos_pre: frozenset[str] | None = None
    _cache: dict[str, object] = field(default_factory=dict, compare=False, repr=False)

    def simhash(self) -> int | None:
        if self.hash_pre is not None:
            return self.hash_pre
        if not self.conteudo:
            return None
        if "hash" not in self._cache:
            self._cache["hash"] = simhash.simhash64(self.conteudo, distintos=True)
        return self._cache["hash"]  # type: ignore[return-value]

    def linhas(self) -> int:
        if self.linhas_pre is not None:
            return self.linhas_pre
        if "linhas" not in self._cache:
            self._cache["linhas"] = linhas_substantivas(self.conteudo)
        return self._cache["linhas"]  # type: ignore[return-value]

    def termos(self) -> frozenset[str] | set[str]:
        if self.termos_pre is not None:
            return self.termos_pre  # read-only use (intersection): no copy per query
        if "termos" not in self._cache:
            self._cache["termos"] = termos_de(self.nome, self.proposito or proposito_de(self.conteudo))
        return self._cache["termos"]  # type: ignore[return-value]


@dataclass(slots=True)
class Registro:
    """A project-index entry: the signals a ``Candidato`` exposes, without the cost of a candidate.

    The index already carries ``simhash``, ``linhas`` and ``termos``; building a full ``Candidato``
    per entry cost 2,3 µs — 18,5 ms over 7 973 entries, measured 31/10/2026 — and the hook runs as
    a fresh process on every ``Write``. The id and the name are derived only when an entry reaches
    the verdict or the federation.
    """

    caminho: str
    rel: str
    hash_pre: int | None
    linhas_pre: int | None
    termos_pre: frozenset[str]
    conteudo: str = ""
    origem: str = "projeto"

    @property
    def id(self) -> str:
        return f"projeto:{self.rel}"

    @property
    def nome(self) -> str:
        return self.caminho.rsplit("/", 1)[-1]

    def simhash(self) -> int | None:
        return self.hash_pre

    def linhas(self) -> int:
        return self.linhas_pre or 0

    def termos(self) -> frozenset[str]:
        return self.termos_pre


@dataclass(frozen=True)
class Veredito:
    """``veredito_gate/1`` answer; ``candidatos`` best first as (id, sinal, distancia or None)."""

    estado: str
    candidatos: tuple[tuple[str, str, int | None], ...] = ()
    motivo: str = ""
    fontes_falhas: tuple[str, ...] = ()

    @property
    def exit_code(self) -> int:
        return EXIT[self.estado]


def linhas_substantivas(conteudo: str) -> int:
    return sum(1 for ln in conteudo.split("\n") if ln.strip())


def proposito_de(conteudo: str) -> str:
    """First meaningful docstring/comment line — what the script says it is for."""
    for linha in conteudo.split("\n")[:40]:
        s = linha.strip().strip("#/\"' ")
        if len(s) > 8 and not s.startswith(("import ", "from ", "!/usr")):
            return s[:200]
    return ""


def _termos(texto: str) -> set[str]:
    return {t for t in _PALAVRA.findall(texto.lower().replace("_", " ")) if t not in _VAZIAS}


def termos_de(nome: str, proposito: str) -> set[str]:
    """Purpose terms of a script: its stem plus its stated purpose. Exporters call this to precompute."""
    return _termos(f"{Path(nome).stem} {proposito}")


def _sinal_proposito(alvo: set[str], c: Candidato) -> int:
    return len(alvo & c.termos())


def _rrf(*rankings: list[str], k: int = 60) -> list[str]:
    """Reciprocal rank fusion of id rankings (best first)."""
    score: dict[str, float] = {}
    for ranking in rankings:
        for pos, cid in enumerate(ranking):
            score[cid] = score.get(cid, 0.0) + 1.0 / (k + pos + 1)
    return sorted(score, key=lambda cid: -score[cid])


def _distancias(conteudo: str, cands: list[Candidato], piso: int) -> dict[str, int]:
    """Hamming distance to every candidate whose body, like the query's, clears the substance floor."""
    if linhas_substantivas(conteudo) < piso:
        return {}
    hq = simhash.simhash64(conteudo, distintos=True)  # banner-robust form; see simhash.simhash64
    out: dict[str, int] = {}
    for c in cands:
        hc = c.simhash() if c.linhas() >= piso else None
        if hc is not None:
            out[c.id] = simhash.hamming(hq, hc)
    return out


def _sinal(cid: str, distancias: dict[str, int], tau_fraco: int) -> str:
    return "corpo" if distancias.get(cid, tau_fraco + 1) <= tau_fraco else "proposito"


def _reprovado(
    topo: str, ranking: list[str], distancias: dict[str, int], params: Parametros, falhas: tuple[str, ...]
) -> Veredito:
    resto = [(cid, _sinal(cid, distancias, params.tau_fraco), distancias.get(cid)) for cid in ranking if cid != topo]
    lista = ((topo, "corpo", distancias[topo]), *resto[:2])
    return Veredito("reprovado", lista, f"corpo a {distancias[topo]} bits (τ={params.tau})", falhas)


def _fracos(
    ranking: list[str], distancias: dict[str, int], prop: dict[str, int], tau_fraco: int
) -> tuple[tuple[str, str, int | None], ...]:
    """Advisory candidates: weak body match or at least two shared purpose terms."""
    out = []
    for cid in ranking:
        if distancias.get(cid, tau_fraco + 1) <= tau_fraco or prop.get(cid, 0) >= 2:
            out.append((cid, _sinal(cid, distancias, tau_fraco), distancias.get(cid)))
    return tuple(out[:3])


def _top(heap: list[tuple], item: tuple, k: int = 10) -> None:
    """Keep the ``k`` best by the tuple's key: equal values keep the EARLIEST candidate.

    ``sorted`` is stable, so the old code broke ties by candidate order; the key carries the order
    so the bounded heap breaks them the same way.
    """
    heap.append(item)
    heap.sort()
    if len(heap) > k:
        heap.pop()


def _sinais(conteudo: str, cands: list, piso: int, alvo: set[str], k: int = 10) -> tuple[int | None, list, list]:
    """ONE pass: the body distance and the purpose signal, each in a bounded top-K.

    The old shape built a dict of 7 973 distances, a dict of 7 973 purpose counts and sorted both.
    The hook runs as a fresh process on every ``Write``, so those collections are pure cost: only
    the best few of each signal ever reach the verdict. The finalists' signals are recomputed
    exactly afterwards, so the verdict does not depend on the bound being wide enough.
    """
    hq = simhash.simhash64(conteudo, distintos=True) if linhas_substantivas(conteudo) >= piso else None
    top_d: list[tuple] = []
    top_p: list[tuple] = []
    for ordem, c in enumerate(cands):
        if hq is not None and c.linhas() >= piso:
            hc = c.simhash()
            if hc is not None:
                _top(top_d, (simhash.hamming(hq, hc), ordem, c), k)
        s = _sinal_proposito(alvo, c)
        if s >= 2:
            _top(top_p, (-s, ordem, c), k)
    return hq, top_d, top_p


def decidir(
    nome: str,
    conteudo: str,
    candidatos: Iterable[Candidato],
    fontes_falhas: Iterable[str] = (),
    params: Parametros = Parametros(),
) -> Veredito:
    """The one decision. Pure: no IO, so the hook, the CLI and the evaluator share it exactly.

    Accepts a ``Candidato`` or a ``Registro``: both expose ``id``, ``nome``, ``simhash()``,
    ``linhas()`` and ``termos()``, and the rule below never looks at anything else.
    """
    falhas = tuple(fontes_falhas)
    if Path(nome).suffix.lower() not in SCRIPT_EXTS:
        return Veredito("nao_aplicavel", motivo="não é script")
    cands = list(candidatos)
    alvo = _termos(f"{Path(nome).stem} {proposito_de(conteudo)}")
    hq, top_d, top_p = _sinais(conteudo, cands, params.piso_linhas, alvo)

    # The finalists' signals, recomputed EXACTLY: the verdict reads a distance and a purpose count
    # for the ranking candidates only, and every ranking candidate is a finalist.
    distancias: dict[str, int] = {}
    prop: dict[str, int] = {}
    for _, _, c in [*top_d, *top_p]:
        cid = c.id
        if cid in prop:
            continue
        if hq is not None and c.linhas() >= params.piso_linhas:
            hc = c.simhash()
            if hc is not None:
                distancias[cid] = simhash.hamming(hq, hc)
        prop[cid] = _sinal_proposito(alvo, c)

    por_corpo = [c.id for _, _, c in top_d]
    por_proposito = [c.id for _, _, c in top_p]
    ranking = _rrf(por_corpo[:10], por_proposito[:10])
    if por_corpo and distancias.get(por_corpo[0], params.tau + 1) <= params.tau:
        return _reprovado(por_corpo[0], ranking, distancias, params, falhas)
    fracos = _fracos(ranking, distancias, prop, params.tau_fraco)
    if falhas:
        return Veredito("inaferivel", fracos, f"fonte(s) sem resposta: {', '.join(falhas)}", falhas)
    curto = linhas_substantivas(conteudo) < params.piso_linhas
    return Veredito(
        "aprovado", fracos, "corpo curto demais para afirmar equivalência" if curto else "nada acima do limiar"
    )


# ---- sources (IO lives here, never in ``decidir``) -------------------------------------------------


def _ler(caminho: Path) -> str:
    try:
        return caminho.read_text(encoding="utf-8", errors="replace")[:200_000]
    except OSError:
        return ""


def fonte_manifest(manifest: Path = MANIFEST) -> list[Candidato]:
    """The global portfolio. Missing file is an empty source; unreadable JSON raises (the caller names it)."""
    if not manifest.exists():
        return []
    raiz = manifest.parent
    out = []
    for e in json.loads(manifest.read_text(encoding="utf-8")).get("scripts", []):
        p = raiz / e.get("path", "")
        out.append(
            Candidato(
                id=f"global:{e.get('path', '')}",
                nome=p.name,
                caminho=str(p),
                conteudo=_ler(p),
                proposito=str(e.get("purpose", "")),
                origem="global",
            )
        )
    return out


def _diretorio_existente(cwd: str | None) -> str | None:
    """The cwd for a subprocess, or None when it no longer exists.

    A removed session directory made ``subprocess.run(cwd=…)`` raise FileNotFoundError, so the whole tantivy
    source read as failed (measured 28/10/2026 in the P3 contract tests); the search itself needs no cwd.
    """
    return cwd if cwd and os.path.isdir(cwd) else None


def _candidato_tantivy(row: dict, base: Path) -> Candidato | None:
    """One search hit as a candidate; None for hits that are not scripts."""
    fp = str(row.get("file_path", ""))
    if not fp or Path(fp).suffix.lower() not in SCRIPT_EXTS:
        return None
    p = Path(fp) if Path(fp).is_absolute() else base / fp
    return Candidato(
        id=f"tantivy:{fp}",
        nome=p.name,
        caminho=str(p),
        conteudo=_ler(p),
        proposito=str(row.get("symbol_name", ""))[:200],
        origem="tantivy",
    )


def fonte_tantivy(consulta: str, cwd: str | None = None, timeout: float = 2.5) -> list[Candidato]:
    """Indexed scripts close to the query terms. Any failure raises: the caller turns it into ``inaferivel``."""
    if not consulta.strip():
        return []
    r = subprocess.run(
        ["touring", "tantivy", "search", consulta, "5"],
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
        cwd=_diretorio_existente(cwd),
    )
    if r.returncode != 0:
        raise RuntimeError(f"tantivy rc={r.returncode}")
    rows = json.loads(r.stdout or "[]")
    base = Path(cwd or ".")
    candidatos = (_candidato_tantivy(row, base) for row in (rows if isinstance(rows, list) else []))
    return [c for c in candidatos if c is not None]


def consultar(caminho: str, conteudo: str, cwd: str | None = None, params: Parametros = Parametros()) -> Veredito:
    """Query every source, name the ones that failed, and decide once."""
    nome = Path(caminho).name
    if Path(nome).suffix.lower() not in SCRIPT_EXTS:
        return Veredito("nao_aplicavel", motivo="não é script")
    candidatos, falhas = _coletar(nome, conteudo, cwd)
    # Normalize the target ONCE; per-candidate Path() was 15 852 Path objects and ~50 ms per hook (28/10/2026).
    alvo = os.path.normpath(caminho)
    vistos: dict[str, Candidato] = {}
    for c in candidatos:
        if c.caminho == alvo:
            continue  # the file being written is not its own equivalent
        vistos.setdefault(c.caminho or c.id, c)  # first source wins: project, then global, then tantivy
    return decidir(nome, conteudo, vistos.values(), falhas, params)


def _coletar(nome: str, conteudo: str, cwd: str | None) -> tuple[list[Candidato], list[str]]:
    """Federated sources (project index > global MANIFEST, project wins by name) plus tantivy; failures named."""
    from concurrent.futures import ThreadPoolExecutor

    import federacao  # local import: federacao imports Candidato from this module

    falhas: list[str] = []
    # tantivy is a subprocess and independent of the index load: start it first, overlap both (A7).
    termos = " ".join(sorted(_termos(f"{Path(nome).stem} {proposito_de(conteudo)}"))[:6])
    pool = ThreadPoolExecutor(max_workers=1)
    busca = pool.submit(fonte_tantivy, termos, cwd)
    projeto: list[Candidato] = []
    indice = federacao.achar_indice(cwd)
    if indice is not None:
        try:
            projeto = federacao.fonte_projeto(indice)
        except federacao.IndiceInvalido as exc:
            falhas.append(f"projeto ({exc})"[:160])
    globais: list[Candidato] = []
    try:
        globais = fonte_manifest()
    except (OSError, ValueError) as exc:
        falhas.append(f"global ({type(exc).__name__})")
    achados: list[Candidato] = []
    try:
        achados = busca.result()
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as exc:
        falhas.append(f"tantivy ({type(exc).__name__})")
    finally:
        pool.shutdown(wait=False)
    return federacao.federar(projeto, globais) + achados, falhas


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="consulta o portfólio antes de criar um script (veredito_gate/1)")
    ap.add_argument("--arquivo", type=Path, required=True)
    ap.add_argument("--cwd", default=None)
    args = ap.parse_args(argv)
    v = consultar(str(args.arquivo), _ler(args.arquivo), args.cwd)
    print(json.dumps(asdict(v), ensure_ascii=False))
    return v.exit_code


if __name__ == "__main__":
    sys.exit(main())
