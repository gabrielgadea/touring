"""VRS — the NATURAL Rust corpus the VGP motor is measured against (VL0-multilinguagem-spec.md §7).

Same schema `juiz_motor_vgp.py` already reads (`entradas.jsonl` + `snap/` + `estado_no_instante/manifesto.json`), built
from what the sessions really did: no crate, only the transcripts of `~/.claude/projects/-home-gabrielgadea-projects-{analise,touring}`.

POSITIVES — a Bash result carrying a rustc error of an invented-NAME class (E0425 E0432 E0433 E0412 E0599 E0609 E0560
E0559 E0061 E0063 E0026). The file is rebuilt AS IT WAS when the command started (`base_no_instante` + the fold of every
Edit/Write/MultiEdit/heredoc that touched it, ALL sessions merged, only the changes after the base commit). The
historical oracle is textual and the item leaves with a named reason when it does not hold:
  1. the line rustc printed in its own snippet equals the rebuilt line  -> else «linha não bate»;
  2. the expected name is on that line                                  -> else «nome ausente na linha».
`nome_esperado` is always something that sits on the pointed line (E0063: the struct literal's type, E0061: the callee).
`disk_file` = the file BEFORE the edit that introduced the line (what the hook would have seen on disk), when knowable.

NEGATIVES — a `.rs` edit whose NEXT verifiable cargo call (same session, <= 30 min) covered the edit's crate and ended
without any error; no further edit of the file in between and no Bash that rewrote it (python/sed patches the fold cannot
see); the file reachable (`mod` declared, no feature gate) and, when it only compiles under test, a test-covering
command. The evidence is graded and written in each entry (`evidencia`):
  visto      `Checking|Compiling <crate> (<dir>)` + `Finished` (or a `test result:` line: cargo builds every target
             before it runs a test), no error line;
  finished   `Finished`/`test result:` printed (so the build succeeded) + the command's scope (-p / --workspace /
             the cwd's manifest) covers the crate;
  rc0        a bare `cargo ...` (only `&&` after it) that exited 0 + scope;
  grep-erro  `| grep ...error...` that printed no error line, exit 0 + scope.
Stratified by project, seeded, `n` declared per stratum. Snapshot = the file right after the edit.

Deterministic: the universe is frozen at `--corte`; ids are sha1 of the provenance; no timestamp reaches `entradas.jsonl`.

Usage (native — the `touring run` sandbox cannot read ~/.claude/projects):
  TOURING_CODE_MODE=native python3 corpus_rust.py [--saida DIR] [--corte ISO] [--selftest]
"""

from __future__ import annotations

import argparse
import collections
import dataclasses
import hashlib
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
import time
from collections.abc import Callable, Iterator
from datetime import datetime, timedelta, timezone
from pathlib import Path

import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parent))
import corpus_v0d  # pyright: ignore[reportMissingImports]
import replay  # pyright: ignore[reportMissingImports]

VIVO = (
    replay.VGP / "corpus_rust_natural"
)  # own dir: `corpus_rust/` is the synthetic `sanidade` stratum (corpus_rust.py)
SEED, N_NEG_MIN, CAP_ESTRATO = 20261009, 300, 200
CORTE = "2026-10-09T20:40:00.000Z"  # the universe is frozen here: later records do not exist
JANELA = timedelta(minutes=30)
PROJETOS = ("analise", "touring")

CODIGOS = frozenset(
    [
        "E0425",
        "E0432",
        "E0433",
        "E0412",
        "E0599",
        "E0609",
        "E0560",
        "E0559",
        "E0061",
        "E0063",
        "E0026",
    ]
)
CLASSE = {
    "E0425": "nome",
    "E0412": "nome",
    "E0432": "import",
    "E0433": "import",
    "E0599": "atributo",
    "E0609": "atributo",
    "E0559": "atributo",
    "E0026": "atributo",
    "E0560": "kwarg",
    "E0063": "kwarg",
    "E0061": "kwarg",
}

ANSI = re.compile(r"\x1b\[[0-9;]*m")
CABECA = re.compile(r"^(?:error|warning)(?:\[(E\d{4})\])?: (.*)$", re.MULTILINE)
LOCAL = re.compile(r"^\s*--> (.+?):(\d+):(\d+)\s*$", re.MULTILINE)
CD = re.compile(r"""\bcd\s+(?:--\s+)?(?:"([^"]+)"|'([^']+)'|([^\s;&|()]+))""")
CD_INICIAL = re.compile(r"""^cd\s+(?:--\s+)?(?:"([^"]+)"|'([^']+)'|(\S+))""")
MANIFEST_PATH = re.compile(r"--manifest-path[ =]\s*['\"]?([^\s'\";&|]+)")
PREFIXO = (
    r"(?:(?:[A-Za-z_]\w*=\S+|timeout\s+\d+|time|nice(?:\s+-n\s*\d+)?|env"
    r"|hw-slot\s+--class\s+\w+\s+--)\s+)*"
)
INICIO_CARGO = re.compile(
    rf"^{PREFIXO}cargo\s+(?:\+\S+\s+)?(check|test|build|clippy|nextest)\b(.*)$",
    re.DOTALL,
)
COMPILA = re.compile(
    r"^\s*(?:Checking|Compiling)\s+(\S+)\s+v\S+(?:\s+\(([^)]+)\))?", re.MULTILINE
)
FINISHED = re.compile(r"^\s*Finished\b", re.MULTILINE)
TEST_RESULT = re.compile(
    r"^\s*(?:test result:|running \d+ tests?\b|Running (?:unittests|tests/)|test \S+ \.\.\. (?:ok|FAILED|ignored))",
    re.MULTILINE,
)
ERRO_COMPILACAO = re.compile(
    r"^error(?:\[E\d{4}\])?: (?!test failed|\d+ targets? failed|process didn't exit)",
    re.MULTILINE,
)
SEM_SAIDA = (
    "Command running in background",
    "was moved to the background",
    "Running as unit",
)
SNIPPET = re.compile(r"^\s*(\d+) \|(?: (.*))?$")
MARCAS = re.compile(r"^\s*\| (.*)$")
RAIZES_DE_ALVO = frozenset({"lib.rs", "main.rs", "build.rs"})
PASTAS_DE_ALVO = frozenset({"tests", "benches", "examples", "bin"})
# a Bash that rewrites a `.rs` file behind the fold's back (python/sed/perl patches, redirections, git restores)
ESCRITA = re.compile(
    r"sed\s+-[A-Za-z]*i|perl\s+-[A-Za-z]*i|\.write_text\(|\.write\(|open\([^)]*[\"'][wa]|\btee\b|>>?\s*\S*\.rs\b"
    r"|\bmv\b|\bcp\b|git\s+(?:checkout|restore|stash|apply|reset|am|cherry-pick)\b|--rewrite"
)
NOME_RS = re.compile(r"(?:[\w.~$-]+/)*[\w.-]+\.rs\b")
NOMES_COMUNS = frozenset({"mod.rs", "lib.rs", "main.rs", "build.rs", "tests.rs"})


# --------------------------------------------------------------------------- rustc parser


@dataclasses.dataclass(frozen=True)
class Diag:
    """One rustc diagnostic: header, primary location and the whole block (snippet included)."""

    codigo: str
    cabeca: str
    texto_msg: str
    caminho: str
    linha: int
    coluna: int
    bloco: str


def _diag(codigo: str, h: re.Match[str], bloco: str) -> Diag:
    loc = LOCAL.search(bloco)
    onde = (
        (loc.group(1).strip(), int(loc.group(2)), int(loc.group(3)))
        if loc
        else ("", 0, 0)
    )
    return Diag(codigo, h.group(0), h.group(2), *onde, bloco)


def parse_erros(texto: str) -> list[Diag]:
    """Every name-class rustc error of a Bash result; the next error/warning header ends a block."""
    texto = ANSI.sub("", texto)
    cabecas = list(CABECA.finditer(texto))
    saida = []
    for i, h in enumerate(cabecas):
        if h.group(1) not in CODIGOS:
            continue
        fim = cabecas[i + 1].start() if i + 1 < len(cabecas) else len(texto)
        saida.append(_diag(h.group(1), h, texto[h.start() : fim]))
    return saida


def linha_do_snippet(bloco: str, linha: int) -> str | None:
    """The source line `linha` as rustc printed it in the block (None when the block does not show it)."""
    for texto in bloco.split("\n"):
        m = SNIPPET.match(texto)
        if m and int(m.group(1)) == linha:
            return m.group(2) or ""
    return None


def sublinhado(bloco: str, linha: int) -> str | None:
    """The text under the first `^^^` run of the snippet line `linha` (the primary span)."""
    linhas = bloco.split("\n")
    for i, texto in enumerate(linhas[:-1]):
        m = SNIPPET.match(texto)
        if not (m and int(m.group(1)) == linha):
            continue
        marcas = MARCAS.match(linhas[i + 1])
        corrida = re.search(r"\^+", marcas.group(1)) if marcas else None
        if corrida:
            return (m.group(2) or "")[corrida.start() : corrida.end()]
    return None


def _primeiro(msg: str) -> str | None:
    m = re.search(r"`([^`]+)`", msg)
    return m.group(1) if m else None


def _ultimo_segmento(nome: str | None) -> str | None:
    return nome.split("::")[-1] if nome else None


def _nomeado(msg: str) -> str | None:
    m = re.search(r"named `([^`]+)`", msg)
    return m.group(1) if m else None


def _metodo(msg: str) -> str | None:
    """E0599 only when the message says a NAME is missing («no method named `x`», «no variant, associated
    function, or constant named `X`»); «the method `x` exists … trait bounds» names a real symbol."""
    m = re.match(r"no [^`]*?named `([^`]+)`", msg)
    return m.group(1) if m else None


def _importado(msg: str) -> str | None:
    return _ultimo_segmento(_primeiro(msg))


def _estrutura(msg: str) -> str | None:
    m = re.search(r"in initializer of `([^`]+)`", msg)
    return _ultimo_segmento(m.group(1)) if m else None


def _chamado(d: Diag) -> str | None:
    """E0061 has no backticked name: the callee is the text rustc underlined on the pointed line."""
    sub = sublinhado(d.bloco, d.linha)
    if not sub:
        return None
    m = re.match(r"[A-Za-z_]\w*", re.split(r"::|\.", sub.strip())[-1])
    return m.group(0) if m else None


EXTRATORES: dict[str, Callable[[str], str | None]] = {
    "E0425": _primeiro,
    "E0412": _primeiro,
    "E0433": _primeiro,
    "E0609": _primeiro,
    "E0432": _importado,
    "E0599": _metodo,
    "E0560": _nomeado,
    "E0559": _nomeado,
    "E0026": _nomeado,
    "E0063": _estrutura,
}


def nome_esperado(d: Diag) -> tuple[str | None, str]:
    """(name, motive when none). The name is ALWAYS something that sits on the pointed source line: for
    E0063 the struct literal's type (the missing fields are in the message), for E0061 the callee."""
    nome = _chamado(d) if d.codigo == "E0061" else EXTRATORES[d.codigo](d.texto_msg)
    return (
        (nome, "") if nome else (None, f"nome não extraível da mensagem ({d.codigo})")
    )


def no_trecho(nome: str, trecho: str) -> bool:
    return (
        re.search(rf"(?<![A-Za-z0-9_]){re.escape(nome)}(?![A-Za-z0-9_])", trecho)
        is not None
    )


def _normaliza(texto: str) -> str:
    return " ".join(texto.replace("\t", "    ").split())


def _mesma_linha(impressa: str, atual: str) -> bool:
    """rustc crops long lines with «...» at the margins: the printed text must sit inside the rebuilt line."""
    corte = _normaliza(impressa).strip(".").strip()
    return bool(corte) and corte in _normaliza(atual)


def confirmar(texto: str, d: Diag, nome: str) -> tuple[str, dict]:
    """The textual historical oracle. ("", extras) when the rebuilt file shows rustc's own line with the name."""
    linhas = texto.split("\n")
    if not 1 <= d.linha <= len(linhas):
        return "linha não bate (a linha está fora do arquivo reconstruído)", {}
    atual = linhas[d.linha - 1]
    impressa = linha_do_snippet(d.bloco, d.linha)
    if impressa is not None and not _mesma_linha(impressa, atual):
        return (
            "linha não bate (o trecho impresso pelo rustc difere do reconstruído)",
            {},
        )
    if not no_trecho(nome, atual):
        return "nome ausente na linha", {}
    coluna = atual[max(d.coluna - 1, 0) :].startswith(nome) if d.coluna else None
    return "", {"snippet_confere": impressa is not None, "coluna_confere": coluna}


# --------------------------------------------------------------------------- cargo: invocations and results


@dataclasses.dataclass(frozen=True)
class Invocacao:
    """One `cargo check|test|build|clippy|nextest` statement of a Bash command, as the command was written."""

    sub: str
    pacotes: tuple[str, ...]
    workspace: bool
    manifesto: str
    testes: bool
    forma: str  # direto | grep-erro | outra
    base: str  # the last `cd` before it ('' = the session cwd)
    saida_valida: bool  # only `&&` follows it: the command's exit code IS cargo's
    segundo_plano: bool
    pipefail_ok: bool  # `set -o pipefail` and only filters that cannot fail on their own: exit code IS cargo's
    seletores: tuple[
        tuple[str, str], ...
    ] = ()  # (--lib|--bin|--test|--example|--doc, name): only those targets
    features: tuple[str, ...] = ()  # --features a,b / -F a
    todas_features: bool = False
    sem_padrao: bool = False


def dividir(texto: str, separadores: tuple[str, ...]) -> list[tuple[str, str]]:
    """[(segment, separator that follows it)]; a separator inside quotes or after a backslash is text."""
    saida: list[tuple[str, str]] = []
    i = inicio = 0
    aspas = ""
    while i < len(texto):
        c = texto[i]
        if c == "\\" and aspas != "'":
            i += 2
        elif aspas:
            aspas = "" if c == aspas else aspas
            i += 1
        elif c in "'\"":
            aspas = c
            i += 1
        elif sep := next((x for x in separadores if texto.startswith(x, i)), ""):
            saida.append((texto[inicio:i], sep))
            i += len(sep)
            inicio = i
        else:
            i += 1
    saida.append((texto[inicio:], ""))
    return saida


FILTROS_SEGUROS = re.compile(r"(?:tail|head|tee|cat|cut|sed|awk|wc|sort|uniq|tr)\b")


def _forma(filtros: list[str]) -> str:
    if not filtros:
        return "direto"
    if any(re.match(r"(?:grep|egrep|rg)\b.*error", f, re.IGNORECASE) for f in filtros):
        return "grep-erro"
    return "outra"


def _invocacao(
    sub: str, resto: str, base: str, seps: list[str], pipefail: bool
) -> Invocacao:
    cargo_parte, *filtros = [seg.strip() for seg, _ in dividir(resto, ("|",))]
    manifesto = MANIFEST_PATH.search(cargo_parte)
    plano = any(x.rstrip().endswith("&") for x in [cargo_parte, *filtros])
    return Invocacao(
        sub,
        tuple(re.findall(r"(?:^|\s)(?:-p|--package)(?:\s+|=)([\w.-]+)", cargo_parte)),
        re.search(r"(?:^|\s)(?:--workspace\b|--all(?=\s|$))", cargo_parte) is not None,
        manifesto.group(1) if manifesto else "",
        sub in ("test", "nextest")
        or re.search(r"--(?:all-targets|tests|benches)\b", cargo_parte) is not None,
        _forma(filtros),
        base,
        not plano and all(s == "&&" for s in seps),
        plano,
        pipefail and all(FILTROS_SEGUROS.match(f) for f in filtros),
        tuple(
            re.findall(
                r"(?:^|\s)--(lib|bins?|examples?|doc|test)\b(?:[ =]([\w.-]+))?",
                cargo_parte,
            )
        ),
        tuple(
            x
            for lista in re.findall(
                r"(?:--features|-F)(?:\s+|=)([\w,.-]+)", cargo_parte
            )
            for x in lista.split(",")
        ),
        "--all-features" in cargo_parte,
        "--no-default-features" in cargo_parte,
    )


def analisar_cargo(comando: str) -> tuple[Invocacao, ...]:
    """The cargo statements a command RUNS (a string that merely mentions `cargo check` is not one)."""
    partes = dividir(comando, ("&&", "||", ";", "\n"))
    pipefail = "pipefail" in comando
    saida: list[Invocacao] = []
    base = ""
    for i, (segmento, _) in enumerate(partes):
        stmt = segmento.strip().lstrip("({ ")
        if m_cd := CD_INICIAL.match(stmt):
            base = next(g for g in m_cd.groups() if g)
        elif m := INICIO_CARGO.match(stmt):
            depois = [sep for _, sep in partes[i:] if sep]
            saida.append(_invocacao(m.group(1), m.group(2), base, depois, pipefail))
    return tuple(saida)


@dataclasses.dataclass(frozen=True)
class Resumo:
    """What a cargo result PROVES, read from its text. `compilou`: cargo printed `Finished` or a `test result:`
    line — it builds EVERY requested target before it runs any test, so either line means the build succeeded."""

    finished: bool
    test_result: bool
    erro: bool
    segundo_plano: bool
    compilados: frozenset[tuple[str, str]]

    @property
    def compilou(self) -> bool:
        return self.finished or self.test_result


def resumir_cargo(texto: str) -> Resumo:
    texto = ANSI.sub("", texto)
    compilados = frozenset(
        (m.group(1), os.path.normpath(m.group(2) or ""))
        for m in COMPILA.finditer(texto)
    )
    return Resumo(
        FINISHED.search(texto) is not None,
        TEST_RESULT.search(texto) is not None,
        ERRO_COMPILACAO.search(texto) is not None,
        any(m in texto for m in SEM_SAIDA),
        compilados,
    )


def evidencia(inv: Invocacao, r: Resumo, is_error: bool, unico: bool = True) -> str:
    """'' = this call proves nothing (skip it) · 'erro' = it failed · otherwise the grade of the proof it gives.
    A command with several cargo statements cannot attribute its text to one of them: there only a compile line
    naming a crate, a `test result:` line (only `cargo test` prints it) and the exit code of a `&&` chain prove
    anything."""
    if r.segundo_plano or inv.segundo_plano:
        return ""
    if r.erro or (is_error and not r.compilou):
        return "erro"
    atribuivel = (
        unico
        or bool(r.compilados)
        or (r.test_result and inv.sub in ("test", "nextest"))
    )
    if r.compilou and atribuivel:
        return "visto" if r.compilados else "finished"
    exit0 = not is_error
    if exit0 and inv.saida_valida and (inv.forma == "direto" or inv.pipefail_ok):
        return "rc0"
    return "grep-erro" if exit0 and unico and inv.forma == "grep-erro" else ""


@dataclasses.dataclass(frozen=True)
class Crate:
    nome: str
    dir: str


Manifestos = dict[str, dict | None]


def manifesto(d: Path, cache: Manifestos) -> dict | None:
    chave = str(d)
    if chave not in cache:
        arq = d / "Cargo.toml"
        try:
            cache[chave] = (
                tomllib.loads(arq.read_text(encoding="utf-8"))
                if arq.is_file()
                else None
            )
        except (OSError, tomllib.TOMLDecodeError):
            cache[chave] = None
    return cache[chave]


def crate_de(alvo: str, cache: Manifestos) -> Crate | None:
    """The package a source file belongs to: the nearest Cargo.toml up the tree that has a [package]."""
    for d in Path(alvo).parents:
        m = manifesto(d, cache)
        if m and "package" in m:
            return Crate(str(m["package"].get("name", "")), str(d))
    return None


def raiz_ws(d: Path, cache: Manifestos) -> Path | None:
    """The workspace a manifest belongs to: the nearest ancestor-or-self with [workspace], else its own dir."""
    propria = None
    for a in (d, *d.parents):
        m = manifesto(a, cache)
        if m is not None and "workspace" in m:
            return a
        if m is not None and propria is None:
            propria = a
    return propria


def _mesmo_nome(a: str, b: str) -> bool:
    return a.replace("_", "-") == b.replace("_", "-")


def features_ativas(crate: Crate, inv: Invocacao, cache: Manifestos) -> set[str]:
    """The features the command turns on for the crate: its `default` closure (unless --no-default-features), the
    `--features` it names and the closure of each; every declared feature under --all-features."""
    tabela = (manifesto(Path(crate.dir), cache) or {}).get("features", {})
    if inv.todas_features:
        return set(tabela) | {"*"}
    ativas: set[str] = set()
    fila = list(inv.features) + ([] if inv.sem_padrao else ["default"])
    while fila:
        f = fila.pop()
        if f not in ativas:
            ativas.add(f)
            fila += [
                x for x in tabela.get(f, []) if isinstance(x, str) and "/" not in x
            ]
    return ativas


def cobre_por_escopo(
    inv: Invocacao, base: Path, crate: Crate, cache: Manifestos
) -> bool:
    """Does the command, as written and run from `base`, compile the crate? (cargo's own rules: `--manifest-path`,
    `-p`, `--workspace`, else the nearest manifest up from the cwd — a virtual workspace builds every member.)"""
    if inv.manifesto:
        return Path(os.path.normpath(base / inv.manifesto)).parent == Path(crate.dir)
    atual = next(
        (a for a in (base, *base.parents) if manifesto(a, cache) is not None), None
    )
    if atual is None or raiz_ws(atual, cache) != raiz_ws(Path(crate.dir), cache):
        return False
    if inv.workspace:
        return True
    if inv.pacotes:
        return any(_mesmo_nome(crate.nome, p) for p in inv.pacotes)
    return "package" not in (manifesto(atual, cache) or {}) or Path(crate.dir) == atual


# --------------------------------------------------------------------------- the universe (one pass)


@dataclasses.dataclass(frozen=True)
class Chamada:
    """A Bash tool_use, kept small: only what path resolution and the cargo rules need."""

    tid: str
    ts: str
    cwd: str
    sid: str
    proj: str
    cds: tuple[str, ...]
    invs: tuple[Invocacao, ...]


@dataclasses.dataclass(frozen=True)
class Edicao:
    tid: str
    caminho: str
    ts: str
    sid: str
    proj: str


@dataclasses.dataclass(frozen=True)
class Ocorrencia:
    chamada: Chamada
    ts_resultado: str
    diag: Diag


def _texto_result(b: dict) -> str:
    c = b.get("content")
    if isinstance(c, str):
        return c
    if isinstance(c, list):
        return "\n".join(str(x.get("text", "")) for x in c if isinstance(x, dict))
    return ""


def _alvos_heredoc(comando: str) -> Iterator[str]:
    """Literal `.rs` targets of `cat > path <<EOF` in a command (replay.HEREDOC/ASSIGN, same expansion)."""
    env = dict(replay.ASSIGN.findall(comando))
    for _op, alvo, _tag, _corpo in replay.HEREDOC.findall(comando):
        caminho = re.sub(
            r"\$\{?([A-Za-z_]\w*)\}?", lambda m: env.get(m.group(1), m.group(0)), alvo
        )
        if "$" not in caminho and caminho.endswith(".rs"):
            yield caminho


def mutados_por_bash(comando: str, cwd: str) -> set[tuple[str, str]]:
    """(`.rs` mention, directory the command ran in) for every file a command rewrites in a way the fold cannot
    replay (heredoc writes are replayed, so they are not here). The write verb and the file name must share a
    unit: the whole command when it carries a heredoc (a python patch names the path far from `write_text`),
    otherwise one statement."""
    if not ESCRITA.search(comando):
        return set()
    cwd_p = Path(cwd) if cwd else None
    cds = [next(g for g in m.groups() if g) for m in CD.finditer(comando)]
    base = str((_expande(cds[-1], cwd_p) if cds else cwd_p) or "")
    unidades = (
        [comando]
        if "<<" in comando
        else [u for u, _ in dividir(comando, ("&&", "||", ";", "\n"))]
    )
    tratados = {Path(a).name for a in _alvos_heredoc(comando)}
    return {
        (t, base)
        for u in unidades
        if ESCRITA.search(u)
        for t in NOME_RS.findall(u)
        if Path(t).name not in tratados
    }


def _linhas_json(arquivo: Path, descartadas: list[int]) -> Iterator[dict]:
    """replay._records' prefilter, tolerant of a half-written last line (live sessions append to these files)."""
    with arquivo.open(encoding="utf-8", errors="replace") as fh:
        for linha in fh:
            if '"tool_use"' not in linha and '"tool_result"' not in linha:
                continue
            try:
                rec = json.loads(linha)
            except json.JSONDecodeError:
                descartadas.append(1)
                continue
            if isinstance(rec, dict) and isinstance(rec.get("message"), dict):
                yield rec


class Universo:
    """Everything the builder asks the transcripts, gathered in ONE pass over all of them."""

    def __init__(self, corte: str) -> None:
        self.corte = corte
        self.chamadas: dict[str, Chamada] = {}
        self.cargo_res: dict[str, Resumo] = {}
        self.cargo_sessao: dict[str, list[Chamada]] = collections.defaultdict(list)
        self.ocorrencias: list[Ocorrencia] = []
        self.edicoes: list[Edicao] = []
        self.idx: dict[str, list[dict]] = collections.defaultdict(list)
        self.bash_muta: dict[str, list[tuple[str, str, str]]] = collections.defaultdict(
            list
        )
        self.erros: set[str] = set()
        self.vistos: set[str] = set()
        self.resultados: set[str] = set()
        self.stats: collections.Counter = collections.Counter()

    def varrer(self, proj: str, raiz: Path) -> None:
        arquivos = sorted(raiz.rglob("*.jsonl"))
        self.stats[f"transcripts:{proj}"] = len(arquivos)
        descartadas: list[int] = []
        for arq in arquivos:
            sid = arq.relative_to(raiz).parts[0].removesuffix(".jsonl")
            for rec in _linhas_json(arq, descartadas):
                self._registro(proj, sid, rec)
        self.stats["linhas_json_invalidas"] += len(descartadas)

    def fechar(self) -> None:
        """Canonical order, independent of the order files were read."""
        self.edicoes.sort(key=lambda e: (e.ts, e.tid))
        for lista in self.cargo_sessao.values():
            lista.sort(key=lambda c: (c.ts, c.tid))
        self.stats["chamadas_cargo"] = sum(map(len, self.cargo_sessao.values()))

    def _registro(self, proj: str, sid: str, rec: dict) -> None:
        ts = rec.get("timestamp", "")
        if not ts or ts > self.corte:
            return
        for b in replay._contents(rec):
            if b.get("type") == "tool_use":
                self._uso(proj, sid, rec, b)
            elif b.get("type") == "tool_result":
                self._resultado(ts, b)

    def _uso(self, proj: str, sid: str, rec: dict, b: dict) -> None:
        tid = str(b.get("id") or "")
        if not tid or tid in self.vistos:
            return
        self.vistos.add(tid)
        entrada = b.get("input") or {}
        if b.get("name") == "Bash":
            self._bash(proj, sid, rec, tid, b, str(entrada.get("command", "")))
            return
        caminho = str(entrada.get("file_path", ""))
        if b.get("name") in ("Edit", "Write", "MultiEdit") and caminho.endswith(".rs"):
            ts = rec["timestamp"]
            self.idx[caminho].append({"timestamp": ts, "message": {"content": [b]}})
            self.edicoes.append(Edicao(tid, caminho, ts, sid, proj))

    def _bash(
        self, proj: str, sid: str, rec: dict, tid: str, b: dict, cmd: str
    ) -> None:
        cds = tuple(next(g for g in m.groups() if g) for m in CD.finditer(cmd))
        cds += tuple(MANIFEST_PATH.findall(cmd))
        ch = Chamada(
            tid,
            rec["timestamp"],
            str(rec.get("cwd") or ""),
            sid,
            proj,
            cds,
            analisar_cargo(cmd),
        )
        self.chamadas[tid] = ch
        if ch.invs:
            self.cargo_sessao[sid].append(ch)
        if "<<" in cmd and ".rs" in cmd:
            for alvo in _alvos_heredoc(cmd):
                self.idx[alvo].append({"timestamp": ch.ts, "message": {"content": [b]}})
        for token, base in mutados_por_bash(cmd, ch.cwd):
            self.bash_muta[Path(token).name].append((ch.ts, token, base))

    def _resultado(self, ts: str, b: dict) -> None:
        tid = str(b.get("tool_use_id") or "")
        if b.get("is_error"):
            self.erros.add(tid)
        ch = self.chamadas.get(tid)
        if ch is None or tid in self.resultados:
            return
        self.resultados.add(tid)
        self.stats["resultados_bash"] += 1
        texto = _texto_result(b)
        if ch.invs:
            self.cargo_res[tid] = resumir_cargo(texto)
        if "error[E" in texto:
            self.ocorrencias += [
                Ocorrencia(ch, ts, d) for d in parse_erros(texto) if d.codigo in CODIGOS
            ]


# --------------------------------------------------------------------------- paths


def dentro(caminho: str, raizes: tuple[Path, ...]) -> bool:
    return any(Path(caminho).is_relative_to(r) for r in raizes)


def _expande(ref: str, cwd: Path | None) -> Path | None:
    ref = ref.replace("${HOME}", "~").replace("$HOME", "~")
    p = Path(ref).expanduser()
    if p.is_absolute():
        return p
    return cwd / p if cwd else None


def dirs_candidatos(ch: Chamada, raizes: tuple[Path, ...]) -> list[Path]:
    """Where `crates/x/src/y.rs` may be relative to: the last `cd` first (the one cargo ran under), then the
    session cwd, each with its ancestors (a workspace root sits above the crate dir)."""
    cwd = Path(ch.cwd) if ch.cwd else None
    bases = [_expande(r, cwd) for r in reversed(ch.cds)] + [cwd]
    saida: list[Path] = []
    for base in bases:
        if base is None:
            continue
        for d in (base, *base.parents):
            if dentro(str(d), raizes) and d not in saida:
                saida.append(d)
    return saida


def resolver(
    rel: str, ch: Chamada, idx: dict, raizes: tuple[Path, ...]
) -> tuple[str | None, str]:
    """(absolute path, motive when none). A candidate the sessions EDITED beats one that merely exists."""
    if rel.startswith("/"):
        return (rel, "") if dentro(rel, raizes) else (None, "caminho fora do projeto")
    if "/.cargo/registry/" in rel or rel.startswith(("/rustc/", "../")):
        return None, "caminho fora do projeto"
    cands = [str(d / rel) for d in dirs_candidatos(ch, raizes)]
    for conhece in (lambda c: c in idx, lambda c: Path(c).is_file()):
        achado = next((c for c in cands if conhece(c)), None)
        if achado:
            return achado, ""
    return None, "caminho não resolvido (não existe hoje e nenhuma sessão o editou)"


def raiz_git(caminho: str) -> Path | None:
    return next((d for d in Path(caminho).parents if (d / ".git").exists()), None)


def raiz_workspace(caminho: str, raiz: Path) -> Path:
    """The topmost Cargo `[workspace]` between the file and its git root (the motor's project root)."""
    topo = raiz
    for d in Path(caminho).parents:
        arq = d / "Cargo.toml"
        if arq.is_file() and "[workspace]" in arq.read_text(errors="ignore"):
            topo = d
        if d == raiz:
            break
    return topo


# --------------------------------------------------------------------------- rebuilding a file in time


def _git(raiz: Path, *args: str) -> str:
    p = subprocess.run(
        ["git", "-C", str(raiz), *args], capture_output=True, text=True, check=False
    )
    return p.stdout.strip() if p.returncode == 0 else ""


def instante_do_commit(raiz: Path, ate: str) -> str:
    """The committer instant (ISO-Z, ms) of the last commit before `ate` — the same one `base_no_instante` uses."""
    rev = _git(raiz, "rev-list", "-1", f"--before={ate}", "HEAD")
    segundos = _git(raiz, "show", "-s", "--format=%ct", rev) if rev else ""
    if not segundos.isdigit():
        return ""
    return datetime.fromtimestamp(int(segundos), timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%S.000Z"
    )


@dataclasses.dataclass(frozen=True)
class LinhaTempo:
    """`estados[0]` is the committed base; `estados[i + 1]` the file after the edits up to `ts[i]`.
    None = not reconstructible (an Edit that no longer matches, or no base and no Write that created it)."""

    ts: tuple[str, ...]
    estados: tuple[str | None, ...]
    base_existe: bool
    commit_ts: str


def _negadas(recs: list[dict], erros: set[str]) -> dict:
    """A synthetic record carrying the error results of these tool_uses (the fold skips calls a hook denied)."""
    ids = {b.get("id") for r in recs for b in r["message"]["content"]}
    blocos = [
        {"type": "tool_result", "tool_use_id": i, "is_error": True}
        for i in sorted(ids & erros)
    ]
    return {"timestamp": "", "message": {"content": blocos}}


class Reconstrutor:
    def __init__(self, idx: dict[str, list[dict]], erros: set[str]) -> None:
        self.idx = idx
        self.erros = erros
        self._cache: dict[tuple[str, str], LinhaTempo | None] = {}

    def linha_tempo(self, alvo: str, ate: str) -> LinhaTempo | None:
        chave = (alvo, ate)
        if chave not in self._cache:
            self._cache[chave] = self._montar(alvo, ate)
        return self._cache[chave]

    def _montar(self, alvo: str, ate: str) -> LinhaTempo | None:
        raiz = raiz_git(alvo)
        if raiz is None:
            return None
        base = corpus_v0d.base_no_instante(raiz, str(Path(alvo).relative_to(raiz)), ate)
        depois = instante_do_commit(raiz, ate)
        recs = [r for r in self.idx.get(alvo, []) if depois < r["timestamp"] <= ate]
        negadas = _negadas(recs, self.erros)
        instantes = tuple(sorted({r["timestamp"] for r in recs}))
        estados = []
        for ate_i in ("", *instantes):
            texto, quebrados, criado = corpus_v0d._dobra_livre(
                [negadas, *recs], alvo, base or "", ate_i
            )
            estados.append(
                texto if not quebrados and (base is not None or criado) else None
            )
        return LinhaTempo(instantes, tuple(estados), base is not None, depois)

    def texto_em(self, alvo: str, ate: str) -> str | None:
        lt = self.linha_tempo(alvo, ate)
        return lt.estados[-1] if lt else None


def _norm_linha(texto: str) -> str:
    return " ".join(texto.split())


def introducao(estados: tuple[str | None, ...], linha: str) -> int | None:
    """Index of the last state that does NOT yet have as many copies of `linha` as the final one: the file the
    introducing edit started from. -1 = already in the base (not introduced by any edit we can see); None = the
    state before it is not reconstructible."""
    alvo = _norm_linha(linha)
    final = estados[-1]
    if final is None:
        return None
    quer = sum(_norm_linha(x) == alvo for x in final.split("\n"))
    for j in range(len(estados) - 1, -1, -1):
        s = estados[j]
        if s is None or sum(_norm_linha(x) == alvo for x in s.split("\n")) < quer:
            return j if s is not None else None
    return -1


# --------------------------------------------------------------------------- entries

MOD_EXTERNO = re.compile(
    r"^([ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+\w+)[ \t]*;", re.MULTILINE
)


def sintaxe_invalida(texto: str) -> str:
    """'' when `rustfmt` parses the snapshot (an independent syntax oracle: rustc reports a name error only after a
    successful parse, and a clean cargo run parses too — so a snapshot that does not parse is NOT the real file).
    `mod x;` becomes `mod x {}` so rustfmt does not go looking for child files. Not checked without rustfmt."""
    if shutil.which("rustfmt") is None:
        return ""
    p = subprocess.run(
        ["rustfmt", "--edition", "2024", "--emit", "stdout"],
        input=MOD_EXTERNO.sub(r"\1 {}", texto),
        capture_output=True,
        text=True,
        check=False,
    )
    return (
        ""
        if p.returncode == 0
        else "snapshot não é Rust válido (rustfmt): a reconstrução não é o arquivo real"
    )


def ident(*partes: str) -> str:
    """Deterministic id (REGRA #17): the same provenance gives the same id on any machine."""
    return hashlib.sha1(json.dumps(list(partes)).encode()).hexdigest()[:12]


def sha(texto: str) -> str:
    return hashlib.sha256(texto.encode()).hexdigest()


@dataclasses.dataclass(frozen=True)
class Item:
    entrada: dict
    snap: str
    antes: str | None
    motivo_estado: str
    chave_repeticao: tuple = ()


@dataclasses.dataclass(frozen=True)
class Config:
    transcripts: tuple[tuple[str, Path], ...]
    saida: Path
    raizes: tuple[Path, ...]
    corte: str = CORTE
    semente: int = SEED
    n_min: int = N_NEG_MIN
    cap: int = CAP_ESTRATO
    confirmar: bool = True


@dataclasses.dataclass
class Ctx:
    cfg: Config
    uni: Universo
    rec: Reconstrutor
    manifestos: Manifestos = dataclasses.field(default_factory=dict)


def _antes_do_positivo(
    lt: LinhaTempo, linha: str
) -> tuple[str | None, str, bool | None]:
    """(disk_file text, motive, introduced). The disk is the file the introducing edit started from."""
    j = introducao(lt.estados, linha)
    if j is None or (j >= 0 and lt.estados[j] is None):
        return None, "estado anterior à edição não reconstruído", None
    if j == -1:
        return None, "erro já presente no arquivo antes de toda edição visível", False
    if j == 0 and not lt.base_existe:
        return None, "arquivo criado pela edição (sem disco)", True
    return lt.estados[j], "ok", True


def entrada_positiva(
    oc: Ocorrencia, alvo: str, nome: str, texto: str, extras: dict
) -> dict:
    d, ch = oc.diag, oc.chamada
    eid = ident("positivo", ch.tid, d.codigo, alvo, str(d.linha))
    raiz = raiz_git(alvo) or Path(alvo).parent
    return {
        "id": eid,
        "rotulo": "positivo",
        "reproduz": "true",
        "classe": CLASSE[d.codigo],
        "nome_esperado": nome,
        "origem": "atribuido",
        "lang": "rust",
        "path": alvo,
        "project_root": str(raiz_workspace(alvo, raiz)),
        "snap": f"snap/{eid}.rs",
        "snap_sha256": sha(texto),
        "mensagem": d.cabeca,
        "contexto": [],
        "codigo": d.codigo,
        "linha": d.linha,
        "coluna": d.coluna,
        "session": ch.sid,
        "projeto": ch.proj,
        "ts": ch.ts,
        "tool_use_id": ch.tid,
        **extras,
    }


def montar_positivo(oc: Ocorrencia, ctx: Ctx) -> tuple[Item | None, str]:
    d = oc.diag
    nome, motivo = nome_esperado(d)
    if nome is None:
        return None, motivo
    if not d.caminho:
        return None, "resultado sem localização (--> ausente: saída filtrada)"
    alvo, motivo = resolver(d.caminho, oc.chamada, ctx.uni.idx, ctx.cfg.raizes)
    if alvo is None:
        return None, motivo
    lt = ctx.rec.linha_tempo(alvo, oc.chamada.ts)
    if lt is None or lt.estados[-1] is None:
        return None, "arquivo não reconstruído"
    texto = lt.estados[-1]
    extras: dict = {}
    if motivo := sintaxe_invalida(texto):
        return None, motivo
    if ctx.cfg.confirmar:
        motivo, extras = confirmar(texto, d, nome)
        if motivo:
            return None, motivo
    linhas = texto.split("\n")
    linha_txt = linhas[d.linha - 1] if 1 <= d.linha <= len(linhas) else ""
    antes, estado, introduzido = _antes_do_positivo(lt, linha_txt)
    entrada = entrada_positiva(
        oc, alvo, nome, texto, {"introduzido": introduzido, **extras}
    )
    return Item(
        entrada, texto, antes, estado, (alvo, sha(texto), d.linha, d.codigo)
    ), ""


# --------------------------------------------------------------------------- negatives


@dataclasses.dataclass(frozen=True)
class Candidato:
    edicao: Edicao
    cargo: Chamada
    inv: Invocacao
    crate: Crate
    evidencia: str


def so_compila_em_teste(alvo: str, texto: str) -> bool:
    p = Path(alvo)
    return (
        p.stem == "tests"
        or p.stem.endswith("_tests")
        or p.parent.name in ("tests", "benches", "examples")
        or "#[cfg(test)]" in texto
        or "#[test]" in texto
    )


DECL_MOD = re.compile(
    r"(?:^|(?<=[\n;{}]))[ \t]*((?:#\[[^\]]*\]\s*)*)(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;"
)


def texto_do_pai(rec: Reconstrutor, pai: Path, ate: str) -> str | None:
    """A parent module's text at the instant; when its history cannot be rebuilt (an untracked patch broke the fold)
    the committed text and then today's file stand in — a `mod` line rarely moves."""
    texto = rec.texto_em(str(pai), ate)
    if texto is None:
        raiz = raiz_git(str(pai))
        if raiz is not None:
            texto = corpus_v0d.base_no_instante(raiz, str(pai.relative_to(raiz)), ate)
    if texto is None and pai.is_file():
        texto = pai.read_text(encoding="utf-8", errors="replace")
    return texto


def declaracao_do_modulo(
    alvo: str, ate: str, rec: Reconstrutor
) -> tuple[str, Path | None] | None:
    """(attributes on the `mod nome;` that declares this file, the declaring file; ('', None) for a target root);
    None = no parent declares it. A file cargo never reaches proves nothing when cargo is clean.
    `#[path = "x_tests.rs"] mod tests;` declares x_tests.rs."""
    p = Path(alvo)
    if p.name in RAIZES_DE_ALVO or p.parent.name in PASTAS_DE_ALVO:
        return "", None
    nome, pasta = (
        (p.parent.name, p.parent.parent) if p.name == "mod.rs" else (p.stem, p.parent)
    )
    pais = [
        pasta / "mod.rs",
        pasta / "lib.rs",
        pasta / "main.rs",
        pasta.parent / f"{pasta.name}.rs",
    ]
    if p.stem.endswith("_tests"):
        pais.append(pasta / f"{p.stem[: -len('_tests')]}.rs")
    for pai in pais:
        texto = texto_do_pai(rec, pai, ate)
        for m in DECL_MOD.finditer(texto or ""):
            if m.group(2) == nome or re.search(
                rf'path\s*=\s*"[^"]*{re.escape(p.name)}"', m.group(1)
            ):
                return m.group(1), pai
    return None


def cobre(c: Chamada, inv: Invocacao, r: Resumo, crate: Crate, ctx: Ctx) -> bool:
    """Did this cargo call compile the crate? The output says so when it lists it; a partial or silent output
    falls back to what the command, as written and run from its `cd`, compiles."""
    if (crate.nome, os.path.normpath(crate.dir)) in r.compilados:
        return True
    if r.compilados and inv.forma == "direto":
        return False  # the whole output is there and the crate is not in it
    cwd = Path(c.cwd) if c.cwd else None
    base = _expande(inv.base, cwd) if inv.base else cwd
    return base is not None and cobre_por_escopo(inv, base, crate, ctx.manifestos)


def proximo_cargo(
    e: Edicao, crate: Crate, ctx: Ctx
) -> tuple[tuple[Chamada, Invocacao, str] | None, str]:
    """((call, statement, evidence), motive): the first cargo call within 30 min that proves something about
    the edit's crate."""
    limite = (datetime.fromisoformat(e.ts.replace("Z", "+00:00")) + JANELA).strftime(
        "%Y-%m-%dT%H:%M:%S.999Z"
    )
    for c in ctx.uni.cargo_sessao.get(e.sid, []):
        r = ctx.uni.cargo_res.get(c.tid)
        if not e.ts < c.ts <= limite or r is None:
            continue
        for inv in c.invs:
            ev = evidencia(inv, r, c.tid in ctx.uni.erros, len(c.invs) == 1)
            if ev and cobre(c, inv, r, crate, ctx):
                if ev == "erro":
                    return None, "o cargo seguinte terminou com erro"
                return (c, inv, ev), ""
    return None, "nenhum cargo verificável, em 30 min, compilou o crate da edição"


def edicao_intermediaria(e: Edicao, cargo: Chamada, ctx: Ctx) -> bool:
    return any(
        e.ts < r["timestamp"] < cargo.ts
        and r["message"]["content"][0].get("id") != e.tid
        for r in ctx.uni.idx[e.caminho]
    )


def candidato(e: Edicao, ctx: Ctx) -> tuple[Candidato | None, str]:
    if not dentro(e.caminho, ctx.cfg.raizes):
        return None, "caminho fora do projeto"
    crate = crate_de(e.caminho, ctx.manifestos)
    if crate is None:
        return None, "crate não encontrado"
    achado, motivo = proximo_cargo(e, crate, ctx)
    if achado is None:
        return None, motivo
    cargo, inv, ev = achado
    if edicao_intermediaria(e, cargo, ctx):
        return None, "outra edição do mesmo arquivo antes do cargo"
    return Candidato(e, cargo, inv, crate, ev), ""


def candidatos_negativos(
    ctx: Ctx,
) -> tuple[dict[str, list[Candidato]], collections.Counter]:
    """The cheap filters, over EVERY `.rs` edit: no history is rebuilt here."""
    por_estrato: dict[str, list[Candidato]] = collections.defaultdict(list)
    motivos: collections.Counter = collections.Counter()
    for e in ctx.uni.edicoes:
        c, motivo = candidato(e, ctx)
        if c:
            por_estrato[e.proj].append(c)
        else:
            motivos[motivo] += 1
    return por_estrato, motivos


def ultimo_write(e: Edicao, uni: Universo) -> str:
    """The instant of the last full `Write` of the file up to the edit: from there the fold needs no earlier history,
    so a Bash patch before it cannot have left the rebuilt text behind."""
    return max(
        (
            r["timestamp"]
            for r in uni.idx[e.caminho]
            if r["message"]["content"][0].get("name") == "Write"
            and r["timestamp"] <= e.ts
        ),
        default="",
    )


def reescrito_por_bash(alvo: str, de: str, ate: str, uni: Universo) -> bool:
    """Did a Bash command between the base commit and the cargo call rewrite THIS file? A mention with a path must
    point at it; a bare name (`sed -i ... lib.rs`) counts when the command ran in the file's directory (common
    names) or anywhere in the same repository (distinctive names)."""
    p = Path(alvo)
    raiz = raiz_git(alvo)
    for ts, token, base in uni.bash_muta.get(p.name, []):
        if not de < ts < ate:
            continue
        if "/" in token:
            visto = Path(os.path.expanduser(token.replace("$HOME", "~")))
            if (visto.is_absolute() and visto == p) or alvo.endswith(
                "/" + token.lstrip("./")
            ):
                return True
        elif p.name not in NOMES_COMUNS:
            if raiz is not None and base and Path(base).is_relative_to(raiz):
                return True
        elif base and Path(base) == p.parent:
            return True
    return False


ONDE_FICA = {
    "test": "tests",
    "example": "examples",
    "examples": "examples",
    "bin": "bin",
    "bins": "bin",
}


def fora_de_outros_alvos(alvo: str, pai: Path | None) -> bool:
    """Is the file in the library tree — not tests/, benches/, examples/, bin/ nor a main.rs tree?"""
    p = Path(alvo)
    return (
        p.parent.name not in PASTAS_DE_ALVO
        and p.name != "main.rs"
        and (pai is None or pai.name != "main.rs")
    )


def alvo_coberto(inv: Invocacao, alvo: str, pai: Path | None, pede_teste: bool) -> bool:
    """With `--lib/--bin/--test/--example` cargo compiles only those targets (plus the library as their
    dependency, in normal mode). Does that include this file?"""
    if not inv.seletores:
        return True
    p = Path(alvo)
    na_lib = fora_de_outros_alvos(alvo, pai)
    for tipo, nome in inv.seletores:
        if tipo == "lib" and na_lib:
            return True
        if (
            tipo in ONDE_FICA
            and p.parent.name == ONDE_FICA[tipo]
            and (not nome or p.stem == nome)
        ):
            return True
        if tipo in ONDE_FICA and na_lib and not pede_teste:
            return True
    return False


def recusa_de_materializacao(
    c: Candidato, lt: LinhaTempo, depois: str, ctx: Ctx
) -> str:
    """Why a candidate whose history was rebuilt still cannot be a negative ('' = it can)."""
    e = c.edicao
    if reescrito_por_bash(
        e.caminho, max(lt.commit_ts, ultimo_write(e, ctx.uni)), c.cargo.ts, ctx.uni
    ):
        return (
            "arquivo reescrito por Bash (patch que a dobra não reproduz) antes do cargo"
        )
    decl = declaracao_do_modulo(e.caminho, c.cargo.ts, ctx.rec)
    if decl is None:
        return "arquivo fora da árvore de módulos (nenhum `mod` o declara)"
    attrs, pai = decl
    cfgs = [x.strip() for x in re.findall(r"cfg\(([^)]*)\)", attrs)]
    for x in cfgs:
        feat = re.fullmatch(r'feature\s*=\s*"([^"]+)"', x)
        if x != "test" and not (
            feat and feat.group(1) in features_ativas(c.crate, c.inv, ctx.manifestos)
        ):
            return "módulo sob `cfg` de feature/alvo que o cargo seguinte não liga"
    pede_teste = so_compila_em_teste(e.caminho, depois) or "test" in cfgs
    if pede_teste and not c.inv.testes:
        return "código só de teste e o cargo seguinte não compila testes"
    if not alvo_coberto(c.inv, e.caminho, pai, pede_teste):
        return "o cargo seguinte compila só um alvo (--lib/--bin/--test/--example) que não inclui o arquivo"
    return ""


def entrada_negativa(c: Candidato, depois: str) -> dict:
    e = c.edicao
    eid = ident("negativo", e.tid, e.caminho, e.ts)
    raiz = raiz_git(e.caminho) or Path(e.caminho).parent
    return {
        "id": eid,
        "rotulo": "negativo",
        "reproduz": None,
        "classe": None,
        "nome_esperado": None,
        "origem": "atribuido",
        "lang": "rust",
        "path": e.caminho,
        "project_root": str(raiz_workspace(e.caminho, raiz)),
        "snap": f"snap/{eid}.rs",
        "snap_sha256": sha(depois),
        "mensagem": "",
        "contexto": [],
        "session": e.sid,
        "projeto": e.proj,
        "ts": e.ts,
        "tool_use_id": e.tid,
        "cargo_tool_use_id": c.cargo.tid,
        "crate": c.crate.nome,
        "evidencia": c.evidencia,
    }


def materializar_negativo(c: Candidato, ctx: Ctx) -> tuple[Item | None, str]:
    e = c.edicao
    lt = ctx.rec.linha_tempo(e.caminho, e.ts)
    if lt is None or e.ts not in lt.ts:
        return None, "arquivo não reconstruído"
    i = lt.ts.index(e.ts)
    depois, antes = lt.estados[i + 1], lt.estados[i]
    if depois is None:
        return None, "arquivo não reconstruído"
    if depois == antes:
        return None, "edição sem efeito (negada ou nula)"
    motivo = recusa_de_materializacao(c, lt, depois, ctx) or sintaxe_invalida(depois)
    if motivo:
        return None, motivo
    if antes is not None:
        estado = "ok"
    else:
        estado = (
            "arquivo criado pela edição (sem disco)"
            if not lt.base_existe and i == 0
            else "estado anterior à edição não reconstruído"
        )
    return Item(entrada_negativa(c, depois), depois, antes, estado), ""


def sortear_negativos(
    por_estrato: dict[str, list[Candidato]], ctx: Ctx
) -> tuple[list[Item], dict, collections.Counter]:
    """Per stratum up to `cap`; then round-robin top-up until `n_min` (or exhaustion). Seeded, canonical order."""
    rng = random.Random(ctx.cfg.semente)
    filas: dict[str, collections.deque[Candidato]] = {}
    for estrato in sorted(por_estrato):
        fila = list(por_estrato[estrato])
        rng.shuffle(fila)
        filas[estrato] = collections.deque(fila)
    itens: dict[str, list[Item]] = {k: [] for k in filas}
    motivos: collections.Counter = collections.Counter()

    def tirar(estrato: str) -> bool:
        while filas[estrato]:
            item, motivo = materializar_negativo(filas[estrato].popleft(), ctx)
            if item:
                itens[estrato].append(item)
                return True
            motivos[motivo] += 1
        return False

    for estrato in filas:
        while len(itens[estrato]) < ctx.cfg.cap and tirar(estrato):
            pass
    while sum(map(len, itens.values())) < ctx.cfg.n_min:
        # Round-robin: EVERY stratum yields one per round. `any()` over a generator would stop at the first
        # stratum that yields and starve the others, changing the corpus.
        rodada = [tirar(k) for k in filas]
        if not any(rodada):
            break
    ordem = [
        i
        for k in sorted(itens)
        for i in sorted(itens[k], key=lambda i: i.entrada["id"])
    ]
    n = {
        k: {"disponiveis": len(por_estrato[k]), "sorteados": len(itens[k])}
        for k in sorted(itens)
    }
    return ordem, n, motivos


# --------------------------------------------------------------------------- build


def construir_positivos(ctx: Ctx) -> tuple[list[Item], collections.Counter, dict]:
    motivos: collections.Counter = collections.Counter()
    repetidos: set[tuple] = set()
    unicas: set[tuple] = set()
    itens: list[Item] = []
    ocs = sorted(
        ctx.uni.ocorrencias,
        key=lambda o: (
            o.chamada.ts,
            o.chamada.tid,
            o.diag.codigo,
            o.diag.caminho,
            o.diag.linha,
        ),
    )
    for oc in ocs:
        chave = (oc.chamada.tid, oc.diag.codigo, oc.diag.caminho, oc.diag.linha)
        if chave in unicas:
            motivos["mesma (chamada, código, caminho, linha)"] += 1
            continue
        unicas.add(chave)
        item, motivo = montar_positivo(oc, ctx)
        if item is None:
            motivos[motivo] += 1
        elif item.chave_repeticao in repetidos:
            motivos[
                "repetição: mesmo erro sobre o mesmo arquivo (nova rodada do cargo)"
            ] += 1
        else:
            repetidos.add(item.chave_repeticao)
            itens.append(item)
    return (
        itens,
        motivos,
        {"ocorrencias_brutas": len(ocs), "apos_dedupe_da_chamada": len(unicas)},
    )


def gravar(saida: Path, itens: list[Item]) -> None:
    """Writes the corpus; files an older build left behind are unlinked one by one (never a recursive delete)."""
    velho = saida / "entradas.jsonl"
    if velho.is_file():
        for linha in velho.read_text(encoding="utf-8").splitlines():
            e = json.loads(linha)
            (saida / e["snap"]).unlink(missing_ok=True)
            (saida / "estado_no_instante" / "antes" / f"{e['id']}.rs").unlink(
                missing_ok=True
            )
    (saida / "snap").mkdir(parents=True, exist_ok=True)
    (saida / "estado_no_instante" / "antes").mkdir(parents=True, exist_ok=True)
    manifest = {}
    for it in itens:
        e = it.entrada
        (saida / e["snap"]).write_text(it.snap, encoding="utf-8")
        disk = None
        if it.antes is not None:
            disk = f"estado_no_instante/antes/{e['id']}.rs"
            (saida / disk).write_text(it.antes, encoding="utf-8")
        manifest[e["id"]] = {
            "disk_file": disk,
            "overlays": [],
            "motivo": it.motivo_estado,
        }
    (saida / "entradas.jsonl").write_text(
        "".join(json.dumps(it.entrada, ensure_ascii=False) + "\n" for it in itens),
        encoding="utf-8",
    )
    (saida / "estado_no_instante" / "manifesto.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=1, sort_keys=True),
        encoding="utf-8",
    )


def contar(itens: list[Item], chave: str) -> dict:
    return dict(
        sorted(collections.Counter(str(i.entrada[chave]) for i in itens).items())
    )


def relatorio(
    cfg: Config, ctx: Ctx, pos: list[Item], neg: list[Item], det: tuple
) -> dict:
    mot_pos, stats_pos, mot_cand, n_neg, mot_neg = det
    return {
        "corte": cfg.corte,
        "semente": cfg.semente,
        "universo": dict(sorted(ctx.uni.stats.items())),
        "positivos": {
            "n": len(pos),
            "por_codigo": contar(pos, "codigo"),
            "por_classe": contar(pos, "classe"),
            "por_projeto_do_transcript": contar(pos, "projeto"),
            "por_raiz": contar(pos, "project_root"),
            "introduzido": contar(pos, "introduzido"),
            "com_disk_file": sum(i.antes is not None for i in pos),
            "descartados_por_motivo": dict(sorted(mot_pos.items())),
            **stats_pos,
        },
        "negativos": {
            "n": len(neg),
            "por_estrato": n_neg,
            "por_projeto_do_transcript": contar(neg, "projeto"),
            "por_evidencia": contar(neg, "evidencia"),
            "com_disk_file": sum(i.antes is not None for i in neg),
            "recusados_nos_filtros_baratos": dict(sorted(mot_cand.items())),
            "recusados_na_materializacao": dict(sorted(mot_neg.items())),
        },
    }


def construir(cfg: Config) -> dict:
    t0 = time.monotonic()
    uni = Universo(cfg.corte)
    for proj, raiz in cfg.transcripts:
        if not raiz.is_dir():
            raise SystemExit(
                f"universo vazio: {raiz} não existe (sandbox? rode nativo)"
            )
        uni.varrer(proj, raiz)
    if not uni.stats["resultados_bash"]:
        raise SystemExit(
            "universo vazio: nenhum resultado de Bash lido (sandbox? rode nativo)"
        )
    uni.fechar()
    ctx = Ctx(cfg, uni, Reconstrutor(uni.idx, uni.erros))
    positivos, mot_pos, stats_pos = construir_positivos(ctx)
    por_estrato, mot_cand = candidatos_negativos(ctx)
    negativos, n_neg, mot_neg = sortear_negativos(por_estrato, ctx)
    cfg.saida.mkdir(parents=True, exist_ok=True)
    gravar(cfg.saida, positivos + negativos)
    rel = relatorio(
        cfg, ctx, positivos, negativos, (mot_pos, stats_pos, mot_cand, n_neg, mot_neg)
    )
    rel["tempo_s"] = round(time.monotonic() - t0, 1)
    (cfg.saida / "relatorio.json").write_text(
        json.dumps(rel, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    return rel


# --------------------------------------------------------------------------- selftest


def _t(seg: int) -> str:
    """A synthetic instant far after any real commit."""
    base = datetime(2099, 1, 1, tzinfo=timezone.utc) + timedelta(seconds=seg)
    return base.strftime("%Y-%m-%dT%H:%M:%S.000Z")


def _rec(seg: int, cwd: str, bloco: dict) -> dict:
    return {"timestamp": _t(seg), "cwd": cwd, "message": {"content": [bloco]}}


def _edit(seg: int, cwd: str, tid: str, caminho: str, velho: str, novo: str) -> dict:
    entrada = {"file_path": caminho, "old_string": velho, "new_string": novo}
    return _rec(
        seg, cwd, {"type": "tool_use", "name": "Edit", "id": tid, "input": entrada}
    )


def _bash(seg: int, cwd: str, tid: str, comando: str) -> dict:
    uso = {"type": "tool_use", "name": "Bash", "id": tid, "input": {"command": comando}}
    return _rec(seg, cwd, uso)


def _resultado(seg: int, cwd: str, tid: str, texto: str, erro: bool = False) -> dict:
    return _rec(
        seg,
        cwd,
        {"type": "tool_result", "tool_use_id": tid, "content": texto, "is_error": erro},
    )


BASE_LIB = "pub mod outro;\npub fn usada() -> u32 {\n    1\n}\npub fn principal() -> u32 {\n    usada()\n}\n"
BASE_OUTRO = "pub fn dois() -> u32 {\n    2\n}\n"
LIMPO = "   Compiling cratex v0.1.0 ({dir})\n    Finished `dev` profile [unoptimized] target(s) in 0.4s\n"
ERRO_SELFTEST = (
    "   Compiling cratex v0.1.0 ({d})\n"
    "error[E0425]: cannot find function `inventada` in this scope\n"
    " --> src/lib.rs:6:5\n  |\n6 |     inventada(1)\n  |     ^^^^^^^^^ not found in this scope\n\n"
    "error: could not compile `cratex` (lib) due to 1 previous error\n"
)
# rustc says line 2 holds a different text than the rebuilt one -> «linha não bate»
ISCA_LINHA = (
    "error[E0425]: cannot find function `fantasma` in this scope\n"
    " --> src/lib.rs:2:8\n  |\n2 | pub fn fantasma() -> u32 {\n  |        ^^^^^^^^ not found\n"
)
# no snippet, and the name is not on line 3 -> «nome ausente na linha»
ISCA_NOME = (
    "error[E0425]: cannot find value `espectro` in this scope\n --> src/lib.rs:3:5\n"
)


def _roteiro_positivo(cwd: str, lib: str) -> list[dict]:
    return [
        _edit(10, cwd, "e1", lib, "    usada()", "    inventada(1)"),
        _bash(12, cwd, "b1", "cargo check"),
        _resultado(13, cwd, "b1", ERRO_SELFTEST.format(d=cwd), True),
        _bash(14, cwd, "b2", "cargo check 2>&1 | grep -A3 '^error'"),
        _resultado(15, cwd, "b2", ISCA_LINHA + ISCA_NOME),
    ]


def _par_cargo(
    seg: int, cwd: str, tid: str, comando: str, texto: str, erro: bool = False
) -> list[dict]:
    return [_bash(seg, cwd, tid, comando), _resultado(seg + 1, cwd, tid, texto, erro)]


def _roteiro_negativo(cwd: str, outro: str, solto: str, viva: str) -> list[dict]:
    limpo = LIMPO.format(dir=cwd)
    r = []
    # n1: clean cargo that lists the crate -> negative (evidence «visto»)
    r += [
        _edit(3000, cwd, "n1", outro, "    2", "    22"),
        *_par_cargo(3010, cwd, "c1", "cargo check", limpo),
    ]
    # n2: decoy — the next cargo failed
    r += [_edit(6000, cwd, "n2", outro, "    22", "    222")]
    r += _par_cargo(6010, cwd, "c2", "cargo check", ERRO_SELFTEST.format(d=cwd), True)
    # n3: decoy — the clean cargo came 45 min later
    r += [_edit(9000, cwd, "n3", outro, "    222", "    2222")]
    r += _par_cargo(9000 + 45 * 60, cwd, "c3", "cargo check", limpo)
    # n4a/n4b: two edits, one cargo — only the LAST was checked
    r += [
        _edit(14000, cwd, "n4a", outro, "    2222", "    4"),
        _edit(14010, cwd, "n4b", outro, "    4", "    44"),
    ]
    r += _par_cargo(14020, cwd, "c4", "cargo check", limpo)
    # n5: decoy — clean but for another package (-p), and silent about the crate
    r += [_edit(18000, cwd, "n5", outro, "    44", "    5")]
    r += _par_cargo(
        18010,
        cwd,
        "c5",
        "cargo check -p outro",
        "    Finished `dev` profile [unoptimized] in 0.1s\n",
    )
    # n6: decoy — a file no `mod` declares
    r += [
        _edit(22000, cwd, "n6", solto, "// x", "// y"),
        *_par_cargo(22010, cwd, "c6", "cargo check", limpo),
    ]
    # n7: bare `cargo check -q` that exited 0 and printed nothing -> negative (evidence «rc0»)
    r += [
        _edit(26000, cwd, "n7", outro, "    5", "    7"),
        *_par_cargo(26010, cwd, "c7", "cargo check -q", ""),
    ]
    # n8: decoy — a sed patch rewrote the file behind the fold's back, then a clean cargo
    r += [
        _bash(30000, cwd, "s8", "sed -i 's/a/b/' src/viva.rs"),
        _resultado(30001, cwd, "s8", ""),
    ]
    r += [
        _edit(30010, cwd, "n8", viva, "// v", "// w"),
        *_par_cargo(30020, cwd, "c8", "cargo check", limpo),
    ]
    # n9: decoy — output cut by `head`: proves nothing
    r += [
        _edit(34000, cwd, "n9", outro, "    7", "    9"),
        *_par_cargo(34010, cwd, "c9", "cargo check 2>&1 | head -3", ""),
    ]
    return r


def _transcript_de_teste(proj: Path) -> list[dict]:
    """One session: a name-class error with a rustc-printed snippet, two decoys that must NOT enter as
    positives, and negatives next to the decoys that must NOT enter as negatives."""
    cwd = str(proj)
    return _roteiro_positivo(cwd, str(proj / "src/lib.rs")) + _roteiro_negativo(
        cwd,
        str(proj / "src/outro.rs"),
        str(proj / "src/solto.rs"),
        str(proj / "src/viva.rs"),
    )


def _repo_de_teste(base: Path) -> Path:
    proj = base / "proj"
    (proj / "src").mkdir(parents=True)
    (proj / "Cargo.toml").write_text(
        '[package]\nname = "cratex"\nversion = "0.1.0"\n', encoding="utf-8"
    )
    (proj / "src/lib.rs").write_text(BASE_LIB + "pub mod viva;\n", encoding="utf-8")
    (proj / "src/outro.rs").write_text(BASE_OUTRO, encoding="utf-8")
    (proj / "src/solto.rs").write_text("// x\n", encoding="utf-8")
    (proj / "src/viva.rs").write_text("// v\n", encoding="utf-8")
    comandos = (
        ["init", "-q"],
        ["add", "-A"],
        ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "base"],
    )
    for args in comandos:
        subprocess.run(["git", "-C", str(proj), *args], check=True, capture_output=True)
    return proj


def config_de_teste(base: Path, saida: Path, confirmar: bool = True) -> Config:
    """Builds the synthetic repo + transcript under `base` and returns the Config that reads them."""
    proj = _repo_de_teste(base)
    pasta = base / "transcripts"
    pasta.mkdir()
    linhas = "".join(json.dumps(r) + "\n" for r in _transcript_de_teste(proj))
    (pasta / "s1.jsonl").write_text(linhas, encoding="utf-8")
    return Config(
        (("teste", pasta),),
        saida,
        (base,),
        corte="9999",
        n_min=1,
        cap=50,
        confirmar=confirmar,
    )


def cenario(base: Path, saida: Path, confirmar: bool) -> dict:
    """Runs the whole builder over the synthetic repo + transcript."""
    return construir(config_de_teste(base, saida, confirmar))


def falhas_do_cenario(saida: Path, rel: dict) -> list[str]:
    """Everything the selftest demands of ONE build; the list is empty when all hold."""
    es = [
        json.loads(ln)
        for ln in (saida / "entradas.jsonl").read_text(encoding="utf-8").splitlines()
    ]
    pos = [e for e in es if e["rotulo"] == "positivo"]
    neg = [e for e in es if e["rotulo"] == "negativo"]
    manifest = json.loads(
        (saida / "estado_no_instante/manifesto.json").read_text(encoding="utf-8")
    )
    falhas = []
    if [(e["nome_esperado"], e["classe"], e["reproduz"]) for e in pos] != [
        ("inventada", "nome", "true")
    ]:
        falhas.append(
            f"positivos esperados [inventada/nome/true], vieram {[e['nome_esperado'] for e in pos]}"
        )
    else:
        disk = manifest[pos[0]["id"]]["disk_file"]
        if disk is None or (saida / disk).read_text() != BASE_LIB + "pub mod viva;\n":
            falhas.append("disk_file do positivo não é o arquivo ANTES da edição")
        if "inventada(1)" not in (saida / pos[0]["snap"]).read_text():
            falhas.append("snapshot do positivo sem a edição")
    vindos = sorted((e["tool_use_id"], e["evidencia"]) for e in neg)
    if vindos != [("n1", "visto"), ("n4b", "visto"), ("n7", "rc0")]:
        falhas.append(
            f"negativos esperados [n1 visto, n4b visto, n7 rc0], vieram {vindos}"
        )
    if rel["positivos"]["n"] != len(pos):
        falhas.append("relatório não bate com as entradas")
    return falhas


def selftest() -> int:
    """Positive control (a synthetic rustc error becomes a positive), negative control (an edit followed by a clean
    cargo becomes a negative), decoys that must stay out, and the MUTANT: with the textual oracle off the decoys
    enter, so the selftest must FAIL — if it still passed, it would not be measuring the oracle."""
    with tempfile.TemporaryDirectory() as d1, tempfile.TemporaryDirectory() as d2:
        s1, s2 = Path(d1) / "saida", Path(d2) / "saida"
        normal = falhas_do_cenario(s1, cenario(Path(d1), s1, True))
        mutante = falhas_do_cenario(s2, cenario(Path(d2), s2, False))
    ok = not normal and bool(mutante)
    sys.stdout.write(
        f"selftest: {'ok' if ok else 'FALHOU'} (normal sem falhas {not normal} · mutante reprovado {bool(mutante)})\n"
    )
    for f in normal:
        sys.stdout.write(f"  normal: {f}\n")
    if not mutante:
        sys.stdout.write(
            "  mutante: desligar a confirmação textual NÃO reprovou o selftest\n"
        )
    return 0 if ok else 1


# --------------------------------------------------------------------------- main


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--saida", type=Path, default=VIVO)
    ap.add_argument("--corte", default=CORTE)
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    transcripts = tuple(
        (p, replay.PROJECTS / f"-home-gabrielgadea-projects-{p}") for p in PROJETOS
    )
    cfg = Config(transcripts, args.saida, (Path.home() / "projects",), corte=args.corte)
    sys.stdout.write(json.dumps(construir(cfg), ensure_ascii=False, indent=1) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
