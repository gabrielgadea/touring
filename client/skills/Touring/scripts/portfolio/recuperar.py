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


@dataclass(frozen=True)
class Candidato:
    """A library entry, whatever the source: the minimal contract every federated source exposes."""

    id: str
    nome: str
    caminho: str = ""
    conteudo: str = ""
    proposito: str = ""
    origem: str = ""
    _cache: dict[str, object] = field(default_factory=dict, compare=False, repr=False)

    def simhash(self) -> int | None:
        if not self.conteudo:
            return None
        if "hash" not in self._cache:
            self._cache["hash"] = simhash.simhash64(self.conteudo, distintos=True)
        return self._cache["hash"]  # type: ignore[return-value]

    def linhas(self) -> int:
        if "linhas" not in self._cache:
            self._cache["linhas"] = linhas_substantivas(self.conteudo)
        return self._cache["linhas"]  # type: ignore[return-value]

    def termos(self) -> set[str]:
        if "termos" not in self._cache:
            self._cache["termos"] = _termos(f"{Path(self.nome).stem} {self.proposito or proposito_de(self.conteudo)}")
        return self._cache["termos"]  # type: ignore[return-value]


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


def decidir(
    nome: str,
    conteudo: str,
    candidatos: Iterable[Candidato],
    fontes_falhas: Iterable[str] = (),
    params: Parametros = Parametros(),
) -> Veredito:
    """The one decision. Pure: no IO, so the hook, the CLI and the evaluator share it exactly."""
    falhas = tuple(fontes_falhas)
    if Path(nome).suffix.lower() not in SCRIPT_EXTS:
        return Veredito("nao_aplicavel", motivo="não é script")
    cands = list(candidatos)
    distancias = _distancias(conteudo, cands, params.piso_linhas)
    por_corpo = sorted(distancias, key=lambda cid: distancias[cid])
    alvo = _termos(f"{Path(nome).stem} {proposito_de(conteudo)}")
    prop = {c.id: _sinal_proposito(alvo, c) for c in cands}
    por_proposito = sorted((cid for cid, s in prop.items() if s >= 2), key=lambda cid: -prop[cid])
    ranking = _rrf(por_corpo[:10], por_proposito[:10])
    if por_corpo and distancias[por_corpo[0]] <= params.tau:
        return _reprovado(por_corpo[0], ranking, distancias, params, falhas)
    fracos = _fracos(ranking, distancias, prop, params.tau_fraco)
    if falhas:
        return Veredito("inaferivel", fracos, f"fonte(s) sem resposta: {', '.join(falhas)}", falhas)
    curto = linhas_substantivas(conteudo) < params.piso_linhas
    return Veredito("aprovado", fracos, "corpo curto demais para afirmar equivalência" if curto else "nada acima do limiar")


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
        out.append(Candidato(id=f"global:{e.get('path', '')}", nome=p.name, caminho=str(p), conteudo=_ler(p),
                             proposito=str(e.get("purpose", "")), origem="global"))
    return out


def fonte_tantivy(consulta: str, cwd: str | None = None, timeout: float = 2.5) -> list[Candidato]:
    """Indexed scripts close to the query terms. Any failure raises: the caller turns it into ``inaferivel``."""
    if not consulta.strip():
        return []
    r = subprocess.run(
        ["touring", "tantivy", "search", consulta, "5"],
        capture_output=True, text=True, timeout=timeout, cwd=cwd, check=False,
    )
    if r.returncode != 0:
        raise RuntimeError(f"tantivy rc={r.returncode}")
    rows = json.loads(r.stdout or "[]")
    base = Path(cwd or ".")
    out = []
    for row in rows if isinstance(rows, list) else []:
        fp = str(row.get("file_path", ""))
        if not fp or Path(fp).suffix.lower() not in SCRIPT_EXTS:
            continue
        p = Path(fp) if Path(fp).is_absolute() else base / fp
        out.append(Candidato(id=f"tantivy:{fp}", nome=p.name, caminho=str(p), conteudo=_ler(p),
                             proposito=str(row.get("symbol_name", ""))[:200], origem="tantivy"))
    return out


def consultar(caminho: str, conteudo: str, cwd: str | None = None, params: Parametros = Parametros()) -> Veredito:
    """Query every source, name the ones that failed, and decide once."""
    nome = Path(caminho).name
    if Path(nome).suffix.lower() not in SCRIPT_EXTS:
        return Veredito("nao_aplicavel", motivo="não é script")
    candidatos: list[Candidato] = []
    falhas: list[str] = []
    try:
        candidatos += fonte_manifest()
    except (OSError, ValueError) as exc:
        falhas.append(f"global ({type(exc).__name__})")
    termos = " ".join(sorted(_termos(f"{Path(nome).stem} {proposito_de(conteudo)}"))[:6])
    try:
        candidatos += fonte_tantivy(termos, cwd)
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as exc:
        falhas.append(f"tantivy ({type(exc).__name__})")
    vistos: dict[str, Candidato] = {}
    for c in candidatos:
        if c.caminho and str(Path(c.caminho)) == str(Path(caminho)):
            continue  # the file being written is not its own equivalent
        vistos.setdefault(c.caminho or c.id, c)
    return decidir(nome, conteudo, vistos.values(), falhas, params)


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
