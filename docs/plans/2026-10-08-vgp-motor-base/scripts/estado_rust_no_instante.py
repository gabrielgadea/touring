#!/usr/bin/env python3
"""O estado do CRATE Rust NO INSTANTE de cada entrada do corpus Rust (HORUST, 10/10/2026) e a classe A.

Por que: no `corpus_rust_natural` os `overlays` do manifesto sao VAZIOS. O motor le entao o disco de HOJE para todo modulo que nao
e o proprio arquivo, e `crate::m::f` ou `use super::*` apontam para OUTRO modulo cujo estado no instante nao esta la. Em Rust o
fecho e o CRATE (`crate::` alcanca qualquer modulo), nao o que o arquivo importa.

Regra (pre-registrada antes de reconstruir): sobrepor TODO `.rs` da arvore de modulos do crate da entrada — a partir de
`src/lib.rs`, `src/main.rs`, `build.rs`, `src/bin/*`, `tests/*`, `examples/*`, `benches/*` e dos `path` do manifesto, seguindo
`mod x;` (`x.rs` ou `x/mod.rs`), `#[path = "..."]` e o aninhamento de `mod a { mod b; }` — no conteudo do INSTANTE: `git show
<rev>:<rel>` mais as edicoes da sessao ate ele (o `Reconstrutor` do gerador do corpus). Um modulo que nao reconstroi, ou que um
Bash reescreveu, deixa o crate INCOMPLETO, com o motivo nomeado; a entrada nunca recebe um overlay de hoje no lugar do instante.

Classe A (`define_no_escopo`): o snapshot reconstruido DEFINE o nome no escopo do uso, mas o rustc disse «cannot find». A
reconstrucao diverge do que o rustc compilou (provavel edicao via Bash que a dobra nao ve): o positivo e NAO REPRODUZIVEL.
O verificador e textual e SUBESTIMA de proposito (so conta o que certamente esta visivel): o que ele nao acusa e medido a olho.

Nada aqui roda o motor nem o juiz. A cola com o gerador (`ProvedorReconstrutor`) importa `corpus_v0d` so quando usada.
"""

from __future__ import annotations

import dataclasses
import hashlib
import os
import re
import sys
import tomllib
from collections import deque
from collections.abc import Callable, Iterable
from pathlib import Path
from typing import Protocol

sys.path.insert(0, str(Path(__file__).resolve().parent))

MOTIVO_INCOMPLETO = {
    "sem_raiz": "estado incompleto: crate sem raiz (lib.rs/main.rs) reconstruível no instante",
    "entrada_fora_da_arvore": "estado incompleto: arquivo da entrada fora da árvore de módulos reconstruída",
    "dobra_quebrada": "estado incompleto: módulo do crate com história irreconstruível no instante (a dobra não casa)",
    "mod_sem_arquivo": "estado incompleto: módulo declarado sem arquivo no instante (nem commit nem Write o criam)",
    "reescrito_por_bash": "estado incompleto: módulo do crate reescrito por Bash (patch que a dobra não reproduz)",
}
PRIORIDADE = ("sem_raiz", "entrada_fora_da_arvore", "dobra_quebrada", "mod_sem_arquivo", "reescrito_por_bash")


def sha(texto: str) -> str:
    return hashlib.sha256(texto.encode()).hexdigest()


# --------------------------------------------------------------------------- texto limpo


def limpar(texto: str) -> str:
    """Comentarios (de linha, de bloco aninhado) e o miolo de strings, raw strings e char literals viram espacos; os offsets e as
    quebras de linha ficam. Lifetimes (`'a`) ficam como estao."""
    saida = list(texto)
    n, i = len(texto), 0

    def branquear(a: int, b: int) -> None:
        for k in range(a, min(b, n)):
            if saida[k] != "\n":
                saida[k] = " "

    while i < n:
        c = texto[i]
        if c == "/" and texto.startswith("//", i):
            j = texto.find("\n", i)
            j = n if j < 0 else j
            branquear(i, j)
            i = j
        elif c == "/" and texto.startswith("/*", i):
            nivel, j = 1, i + 2
            while j < n and nivel:
                if texto.startswith("/*", j):
                    nivel, j = nivel + 1, j + 2
                elif texto.startswith("*/", j):
                    nivel, j = nivel - 1, j + 2
                else:
                    j += 1
            branquear(i, j)
            i = j
        elif c == "r" and (i == 0 or not (texto[i - 1].isalnum() or texto[i - 1] == "_")) and re.match(r'r#*"', texto[i:i + 40]):
            j = i + 1
            cercas = 0
            while texto[j] == "#":
                cercas, j = cercas + 1, j + 1
            fim = texto.find('"' + "#" * cercas, j + 1)
            fim = n if fim < 0 else fim
            branquear(j + 1, fim)
            i = fim + 1 + cercas
        elif c == '"':
            j = i + 1
            while j < n and texto[j] != '"':
                j += 2 if texto[j] == "\\" else 1
            branquear(i + 1, j)
            i = j + 1
        elif c == "'":
            if texto.startswith("'\\", i):
                j = texto.find("'", i + 3)
                j = n if j < 0 else j
                branquear(i + 1, j)
                i = j + 1
            elif i + 2 < n and texto[i + 2] == "'":
                branquear(i + 1, i + 2)
                i += 3
            else:
                i += 1  # lifetime
        else:
            i += 1
    return "".join(saida)


# --------------------------------------------------------------------------- declaracoes de modulo


@dataclasses.dataclass(frozen=True)
class DeclMod:
    nome: str
    externo: bool  # `mod x;` (True) | `mod x { ... }` (False)
    path_attr: str | None
    aninhamento: tuple[str, ...]  # nomes dos `mod a { }` inline que envolvem a declaracao
    attrs: str
    offset: int


_MOD = re.compile(
    r"(?P<attrs>(?:#\s*\[[^\]]*\]\s*)*)(?:pub(?:\s*\([^)]*\))?\s+)?\bmod\s+(?:r#)?(?P<nome>\w+)\s*(?P<fim>[;{])|(?P<abre>\{)|(?P<fecha>\})"
)
_PATH_ATTR = re.compile(r'\bpath\s*=\s*"([^"]*)"')


def declaracoes_mod(texto: str) -> list[DeclMod]:
    """Toda `mod x;` / `mod x { }` do arquivo (fora de comentarios e strings), com atributos e o aninhamento inline."""
    limpo = limpar(texto)
    pilha: list[str | None] = []
    saida: list[DeclMod] = []
    for m in _MOD.finditer(limpo):
        if m.group("abre"):
            pilha.append(None)
        elif m.group("fecha"):
            if pilha:
                pilha.pop()
        else:
            attrs = texto[m.start("attrs"):m.end("attrs")]
            p = _PATH_ATTR.search(attrs)
            saida.append(
                DeclMod(m.group("nome"), m.group("fim") == ";", p.group(1) if p else None,
                        tuple(x for x in pilha if x), re.sub(r"\s+", "", attrs), m.start())
            )
            if m.group("fim") == "{":
                pilha.append(m.group("nome"))
    return saida


def _candidatos(arquivo: Path, rootlike: bool, d: DeclMod) -> list[tuple[Path, bool]]:
    """Onde mora o arquivo de `mod x;` declarado em `arquivo`: [(caminho, o filho age como mod-rs)]. Arquivos de raiz de alvo e
    `mod.rs` guardam os filhos ao lado; `a.rs` os guarda em `a/`; o carregado por `#[path]` age como mod-rs."""
    base = arquivo.parent if rootlike else arquivo.parent / arquivo.stem
    for n in d.aninhamento:
        base = base / n
    if d.path_attr:
        alvo = (base if d.aninhamento else arquivo.parent) / d.path_attr
        return [(Path(os.path.normpath(alvo)), True)]
    return [(base / f"{d.nome}.rs", False), (base / d.nome / "mod.rs", True)]


# --------------------------------------------------------------------------- o crate


class Provedor(Protocol):
    def texto(self, p: Path) -> tuple[str | None, str]:
        """(conteudo no instante | None, motivo). motivo: '' ok | 'sem_origem' | 'dobra_quebrada' | 'reescrito_por_bash'."""

    def arquivos(self, d: Path) -> list[Path]:
        """Os `.rs` conhecidos sob `d` no instante (commit + criados pela sessao)."""


@dataclasses.dataclass(frozen=True)
class Falta:
    path: str
    motivo: str
    detalhe: str = ""


@dataclasses.dataclass(frozen=True)
class ArquivoCrate:
    path: Path
    conteudo: str
    profundidade: int


@dataclasses.dataclass
class EstadoCrate:
    crate_dir: str
    raizes: list[str]
    arquivos: list[ArquivoCrate]
    faltas: list[Falta]
    orfaos: list[str]


def raizes_do_manifesto(crate_dir: Path) -> list[Path]:
    """Os `path` que o Cargo.toml (de hoje) declara em [lib], [[bin]], [[test]], [[example]], [[bench]]."""
    try:
        m = tomllib.loads((Path(crate_dir) / "Cargo.toml").read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError):
        return []
    caminhos = []
    if isinstance(m.get("lib"), dict) and m["lib"].get("path"):
        caminhos.append(m["lib"]["path"])
    for chave in ("bin", "test", "example", "bench"):
        caminhos += [t["path"] for t in m.get(chave, []) if isinstance(t, dict) and t.get("path")]
    return sorted({Path(crate_dir) / p for p in caminhos})


def _raizes(crate_dir: Path, prov: Provedor, extras: Iterable[Path]) -> list[Path]:
    cand = [crate_dir / "src/lib.rs", crate_dir / "src/main.rs", crate_dir / "build.rs", *map(Path, extras)]
    for sub in ("src/bin", "tests", "examples", "benches"):
        pasta = crate_dir / sub
        for p in prov.arquivos(pasta):
            rel = p.relative_to(pasta)
            if p.suffix == ".rs" and (len(rel.parts) == 1 or (len(rel.parts) == 2 and rel.name == "main.rs")):
                cand.append(p)
    return list(dict.fromkeys(cand))


def estado_do_crate(crate_dir, prov: Provedor, proprio: str | None = None, extras_raiz: Iterable[Path] = ()) -> EstadoCrate:
    """A arvore de modulos do crate no instante do provedor: quais arquivos, com que conteudo, e o que faltou (nomeado)."""
    crate_dir = Path(crate_dir)
    faltas: list[Falta] = []
    raizes: list[Path] = []
    fila: deque[tuple[Path, str, bool, int]] = deque()
    agendado: set[Path] = set()
    for r in _raizes(crate_dir, prov, extras_raiz):
        t, motivo = prov.texto(r)
        if t is None:
            if motivo == "dobra_quebrada":
                faltas.append(Falta(str(r), "dobra_quebrada", "raiz"))
            continue
        if motivo:
            faltas.append(Falta(str(r), motivo, "raiz"))
            continue
        raizes.append(r)
        agendado.add(r)
        fila.append((r, t, True, 0))
    if not raizes and not faltas:
        faltas.append(Falta(str(crate_dir), "sem_raiz"))
    visto: dict[Path, ArquivoCrate] = {}
    while fila:
        arq, texto, rootlike, prof = fila.popleft()
        visto[arq] = ArquivoCrate(arq, texto, prof)
        for d in declaracoes_mod(texto):
            if not d.externo:
                continue
            achado, pior = None, ("sem_origem", None)
            cands = _candidatos(arq, rootlike, d)
            for caminho, filho_rootlike in cands:
                if caminho in agendado:
                    achado = "ja"
                    break
                t, motivo = prov.texto(caminho)
                if t is not None:
                    achado = (caminho, t, motivo, filho_rootlike)
                    break
                if motivo == "dobra_quebrada":
                    pior = ("dobra_quebrada", caminho)
            if achado == "ja":
                continue
            if achado is None:
                faltas.append(Falta(str(pior[1] or cands[0][0]), "dobra_quebrada" if pior[1] else "mod_sem_arquivo", f"declarado em {arq}"))
                continue
            caminho, t, motivo, filho_rootlike = achado
            agendado.add(caminho)
            if motivo:
                faltas.append(Falta(str(caminho), motivo, f"declarado em {arq}"))
                continue
            fila.append((caminho, t, filho_rootlike, prof + 1))
    conhecidos = {Path(f.path) for f in faltas}
    orfaos = sorted(
        str(p) for p in prov.arquivos(crate_dir)
        if p.suffix == ".rs" and p not in visto and p not in conhecidos and "target" not in p.relative_to(crate_dir).parts
    )
    if proprio is not None and Path(proprio) not in visto and Path(proprio) not in conhecidos:
        faltas.append(Falta(str(proprio), "entrada_fora_da_arvore"))
    arquivos = sorted(visto.values(), key=lambda a: (a.profundidade, str(a.path)))
    return EstadoCrate(str(crate_dir), [str(r) for r in raizes], arquivos, faltas, orfaos)


def motivo_incompleto(estado: EstadoCrate) -> tuple[bool, str]:
    """(completo, motivo): o primeiro motivo por prioridade; `(True, 'ok')` quando nada faltou."""
    tem = {f.motivo for f in estado.faltas}
    for chave in PRIORIDADE:
        if chave in tem:
            return False, MOTIVO_INCOMPLETO[chave]
    return True, "ok"


def overlays_do_crate(estado: EstadoCrate, proprio: str | None) -> list[tuple[Path, str]]:
    """Todo modulo do crate no instante, menos o proprio arquivo da entrada (o `--content-file` ja e ele)."""
    return [(a.path, a.conteudo) for a in estado.arquivos if str(a.path) != str(proprio)]


def materializar(estado: EstadoCrate, proprio: str | None, destino: Path, eid: str) -> list[dict]:
    """Grava os overlays em `<destino>/modulos/<eid>/<n>.rs` no formato do manifesto: [{path, file, sha256}]."""
    out = []
    for n, (p, conteudo) in enumerate(overlays_do_crate(estado, proprio)):
        arq = destino / "modulos" / eid / f"{n}.rs"
        arq.parent.mkdir(parents=True, exist_ok=True)
        arq.write_text(conteudo, encoding="utf-8")
        out.append({"path": str(p), "file": str(arq.relative_to(destino.parent)), "sha256": sha(conteudo)})
    return out


# --------------------------------------------------------------------------- cola com o Reconstrutor do gerador


class ProvedorReconstrutor:
    """O provedor do estado: `Reconstrutor` (commit anterior a `ts` + dobra das edicoes da sessao) e a listagem de `git ls-tree`.
    `suspeito(caminho, linha_do_tempo) -> bool` marca o arquivo que um Bash reescreveu (a dobra nao o reproduz)."""

    def __init__(self, rec, ts: str, suspeito: Callable[[str, object], bool] | None = None) -> None:
        self.rec, self.ts, self.suspeito = rec, ts, suspeito
        self._rev: dict[str, str] = {}
        self._ls: dict[tuple[str, str], list[str]] = {}

    def texto(self, p: Path) -> tuple[str | None, str]:
        lt = self.rec.linha_tempo(str(p), self.ts)
        if lt is None:
            return None, "sem_origem"
        t = lt.estados[-1]
        if t is None:
            registrado = any(r["timestamp"] <= self.ts for r in self.rec.idx.get(str(p), []))
            return None, ("dobra_quebrada" if lt.base_existe or registrado else "sem_origem")
        if self.suspeito is not None and self.suspeito(str(p), lt):
            return t, "reescrito_por_bash"
        return t, ""

    def arquivos(self, d: Path) -> list[Path]:
        import corpus_v0d  # o construtor do corpus e a fonte unica do git por instante

        d = Path(d)
        raiz = next((a for a in (d, *d.parents) if (a / ".git").exists()), None)
        achados: set[Path] = set()
        if raiz is not None:
            if str(raiz) not in self._rev:
                self._rev[str(raiz)] = corpus_v0d._git(raiz, "rev-list", "-1", f"--before={self.ts}", "HEAD").stdout.strip()
            rev = self._rev[str(raiz)]
            rel = str(d.relative_to(raiz)) or "."
            if rev and (str(raiz), rel) not in self._ls:
                p = corpus_v0d._git(raiz, "ls-tree", "-r", "--name-only", rev, "--", rel)
                self._ls[(str(raiz), rel)] = p.stdout.splitlines() if p.returncode == 0 else []
            achados |= {raiz / x for x in self._ls.get((str(raiz), rel), []) if x.endswith(".rs")}
        for k, regs in self.rec.idx.items():
            if k.endswith(".rs") and d in Path(k).parents and any(r["timestamp"] <= self.ts for r in regs):
                achados.add(Path(k))
        return sorted(achados)


# --------------------------------------------------------------------------- classe A: o snapshot DEFINE o nome no escopo do uso

PALAVRAS = frozenset({"mut", "ref", "box", "_", "if", "in", "let", "move", "as", "self", "Self", "crate", "super", "true", "false"})
_LIMITE = r"(?:^|[;{}]|\])"
_ITEM = re.compile(
    _LIMITE + r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:(?:unsafe|async|const|extern(?:\s*\"[^\"]*\")?|default)\s+)*"
    r"(?:fn|struct|enum|union|trait|type|static|mod|const)\s+(?:r#)?(\w+)",
    re.MULTILINE,
)
_MACRO = re.compile(_LIMITE + r"\s*(?:#\s*\[[^\]]*\]\s*)*macro_rules!\s*(\w+)", re.MULTILINE)
_USE = re.compile(_LIMITE + r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?use\s+", re.MULTILINE)
_EXTERN = re.compile(r"\bextern\s+crate\s+(\w+)(?:\s+as\s+(\w+))?\s*;")
_LET = re.compile(r"\blet\s+([^=;]*?)\s*(?:=|;|\belse\b)")


@dataclasses.dataclass
class Bloco:
    inicio: int  # offset da `{`
    fim: int  # offset da `}` casada
    cab_inicio: int
    cabeca: str
    pai: int  # indice do bloco que o envolve (-1 = raiz do arquivo)


def _blocos(limpo: str) -> list[Bloco]:
    """Os blocos `{ }` do arquivo, menos os grupos de `use a::{b, c}` (que nao sao escopo)."""
    blocos: list[Bloco] = []
    pilha: list[tuple[int, bool]] = []  # (indice no `blocos` ou -1 para grupo de use, e_grupo_de_use)
    limite = 0
    for i, ch in enumerate(limpo):
        if ch == "{":
            cab = limpo[limite:i]
            em_use = (pilha and pilha[-1][1]) or re.match(r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?use\b", cab) is not None
            if em_use:
                pilha.append((-1, True))
            else:
                pai = next((k for k, _ in reversed(pilha) if k >= 0), -1)
                blocos.append(Bloco(i, -1, limite, cab, pai))
                pilha.append((len(blocos) - 1, False))
            limite = i + 1
        elif ch == "}":
            if pilha:
                k, _ = pilha.pop()
                if k >= 0:
                    blocos[k].fim = i
            limite = i + 1
        elif ch == ";":
            limite = i + 1
    for b in blocos:
        if b.fim < 0:
            b.fim = len(limpo)
    return blocos


def _tipo(cabeca: str) -> str:
    c = re.sub(r"^(?:\s*#\s*\[[^\]]*\])*\s*", "", cabeca).strip()
    c = re.sub(r"^(?:pub(?:\s*\([^)]*\))?\s+)?", "", c)
    if re.match(r"mod\s+\w+\s*$", c):
        return "mod"
    if re.match(r"(?:unsafe\s+|async\s+|const\s+|extern(?:\s*\"[^\"]*\")?\s+)*fn\b", c):
        return "fn"
    if re.match(r"(?:unsafe\s+)?(?:impl|trait)\b", c):
        return "impl"
    if re.match(r"(?:struct|enum|union)\b", c) or re.match(r"extern\b", c):
        return "tipo"
    return "outro"


def _offset(texto: str, linha: int, coluna: int) -> int | None:
    linhas = texto.split("\n")
    if not (1 <= linha <= len(linhas)):
        return None
    return sum(len(x) + 1 for x in linhas[: linha - 1]) + max(0, coluna - 1)


def _dentro(blocos: list[Bloco], off: int) -> list[int]:
    """Indices dos blocos que envolvem `off`, do mais externo ao mais interno."""
    return [k for k, b in enumerate(blocos) if b.inicio < off < b.fim]


def _dono(blocos: list[Bloco], off: int) -> int:
    """O bloco mais interno que contem `off` (-1 = raiz do arquivo)."""
    d = _dentro(blocos, off)
    return d[-1] if d else -1


def _topo(s: str, sep: str) -> list[str]:
    """Divide `s` em `sep` ignorando o que esta dentro de (), [], {}, <>."""
    partes, prof, atual = [], 0, []
    for ch in s:
        if ch in "([{<":
            prof += 1
        elif ch in ")]}>":
            prof -= 1 if ch != ">" or prof > 0 else 0
        if ch == sep and prof == 0:
            partes.append("".join(atual))
            atual = []
        else:
            atual.append(ch)
    partes.append("".join(atual))
    return partes


def _nomes_do_padrao(p: str) -> set[str]:
    """Identificadores que um padrao liga: os que nao sao construtor (`Some(`, `Foo {`, `A::B`), palavra-chave nem literal."""
    out = set()
    for m in re.finditer(r"(?<![\w'])(?:r#)?([A-Za-z_]\w*)", p):
        depois = p[m.end():].lstrip()
        if m.group(1) in PALAVRAS or depois.startswith(("(", "::", "{")):
            continue
        out.add(m.group(1))
    return out


def _nomes_do_use(arvore: str) -> set[str]:
    toks = re.findall(r"[A-Za-z_]\w*|::|\{|\}|,|\*", arvore)
    pos = 0
    out: set[str] = set()

    def elemento(pref: str | None) -> None:
        nonlocal pos
        ultimo = None
        while pos < len(toks):
            t = toks[pos]
            if t == "::":
                pos += 1
            elif t == "{":
                pos += 1
                while True:
                    elemento(ultimo if ultimo else pref)
                    if pos < len(toks) and toks[pos] == ",":
                        pos += 1
                        continue
                    break
                if pos < len(toks) and toks[pos] == "}":
                    pos += 1
                return
            elif t in (",", "}"):
                break
            elif t == "as" and ultimo is not None and pos + 1 < len(toks):
                if toks[pos + 1] != "_":
                    out.add(toks[pos + 1])
                pos += 2
                return
            else:
                ultimo = "*" if t == "*" else t
                pos += 1
        if ultimo and ultimo != "*":
            out.add(pref if ultimo == "self" and pref else ultimo)

    elemento(None)
    return out


def _usos(limpo: str) -> list[tuple[int, int, set[str]]]:
    """(offset do `use`, offset do `;`, nomes que o `use` traz) de cada `use` e `extern crate`."""
    out = []
    for m in _USE.finditer(limpo):
        i, prof = m.end(), 0
        while i < len(limpo) and not (limpo[i] == ";" and prof == 0):
            prof += (limpo[i] == "{") - (limpo[i] == "}")
            i += 1
        out.append((m.start(), i, _nomes_do_use(limpo[m.end():i])))
    for m in _EXTERN.finditer(limpo):
        out.append((m.start(), m.end(), {m.group(2) or m.group(1)}))
    return out


def _fim_do_let(limpo: str, a: int) -> int:
    prof = 0
    for i in range(a, len(limpo)):
        ch = limpo[i]
        if ch in "([{":
            prof += 1
        elif ch in ")]}":
            if prof == 0:
                return i
            prof -= 1
        elif ch == ";" and prof == 0:
            return i
    return len(limpo)


def _generico(cabeca: str) -> set[str]:
    m = re.search(r"\b(?:fn|impl|struct|enum|union|trait|type)\s*(?:\w+\s*)?<", cabeca)
    if not m:
        return set()
    i, prof = m.end(), 1
    j = i
    while j < len(cabeca) and prof:
        prof += (cabeca[j] == "<") - (cabeca[j] == ">")
        j += 1
    out = set()
    for p in _topo(cabeca[i:j - 1], ","):
        p = p.strip()
        if not p or p.startswith("'"):
            continue
        mm = re.match(r"(?:const\s+)?(\w+)", p)
        if mm:
            out.add(mm.group(1))
    return out


def _parametros(cabeca: str) -> set[str]:
    m = re.search(r"\bfn\s+\w+\s*", cabeca)
    if not m:
        return set()
    i = m.end()
    if i < len(cabeca) and cabeca[i] == "<":
        prof = 0
        while i < len(cabeca):
            prof += (cabeca[i] == "<") - (cabeca[i] == ">")
            i += 1
            if prof == 0:
                break
    while i < len(cabeca) and cabeca[i] != "(":
        i += 1
    prof, j = 0, i
    while j < len(cabeca):
        prof += (cabeca[j] == "(") - (cabeca[j] == ")")
        j += 1
        if prof == 0:
            break
    out: set[str] = set()
    for p in _topo(cabeca[i + 1:j - 1], ","):
        out |= _nomes_do_padrao(_topo(p, ":")[0])
    return out


def _bindings_da_cabeca(cabeca: str) -> set[str]:
    """Nomes que a CABECA de um bloco liga para dentro dele: `if let`, `while let`, `for`, closure `|a|` e braco de `match`."""
    c = cabeca.strip().lstrip(",").strip()
    out: set[str] = set()
    m = re.search(r"\blet\s+(.*?)\s*=(?!=)", c, re.DOTALL)
    if m and re.search(r"\b(?:if|while)\s+(?:.*&&\s*)?let\b", c):
        out |= _nomes_do_padrao(m.group(1))
    m = re.match(r"(?:'\w+:\s*)?for\s+(.*?)\s+in\b", c, re.DOTALL)
    if m:
        out |= _nomes_do_padrao(m.group(1))
    m = re.search(r"\|([^|]*)\|\s*(?:->\s*[^{]*)?$", c)
    if m:
        for p in _topo(m.group(1), ","):
            out |= _nomes_do_padrao(_topo(p, ":")[0])
    if c.endswith("=>"):
        padrao = _topo(c[:-2], ",")[-1]
        padrao = re.split(r"\s+if\s+", padrao)[0]
        out |= _nomes_do_padrao(padrao)
    return out


def define_no_escopo(texto: str, linha: int, coluna: int, nome: str) -> str | None:
    """Por que o NOME esta definido no escopo do uso em (linha, coluna) do snapshot, ou None. Conta so o certamente visivel: item ou
    `use` no modulo do uso (ou local de um bloco que o envolve), parametro, `let` anterior, binding de `if let`/`for`/`match`/closure
    e generico. Glob (`use super::*`), item de outro modulo, de `impl` ou de outra funcao NAO contam; o uso QUALIFICADO por caminho
    (`crate::m::nome`, `Tipo::nome`) tambem nao: o nome e procurado no modulo nomeado, nao no escopo do uso."""
    off = _offset(texto, linha, coluna)
    if off is None or texto[:off].rstrip().endswith("::"):
        return None  # uso qualificado (`crate::m::nome`): o rustc procura em `m`, nao no escopo do uso
    limpo = limpar(texto)
    blocos = _blocos(limpo)
    cadeia = _dentro(blocos, off)
    modulo = next((k for k in reversed(cadeia) if _tipo(blocos[k].cabeca) == "mod"), -1)
    corrente = cadeia[cadeia.index(modulo) + 1:] if modulo >= 0 else cadeia
    visiveis = {modulo} | {k for k in corrente if _tipo(blocos[k].cabeca) in ("fn", "outro")}

    def ln(o: int) -> int:
        return limpo.count("\n", 0, o) + 1

    for rx in (_ITEM, _MACRO):
        for m in rx.finditer(limpo):
            if m.group(1) == nome and _dono(blocos, m.start(1)) in visiveis:
                local = _dono(blocos, m.start(1)) != modulo
                return f"item{' local' if local else ''} `{nome}` declarado {'no bloco' if local else 'no módulo'} do uso (L{ln(m.start(1))})"
    for ini, fim, nomes in _usos(limpo):
        if nome in nomes and _dono(blocos, ini) in visiveis:
            return f"use que importa `{nome}` no escopo do uso (L{ln(ini)})"
    for k in corrente:
        b = blocos[k]
        if _tipo(b.cabeca) == "fn" and nome in _parametros(b.cabeca):
            return f"parâmetro `{nome}` da fn que envolve o uso (L{ln(b.inicio)})"
    for m in _LET.finditer(limpo):
        padrao = _topo(m.group(1), ":")[0] if ":" in m.group(1) else m.group(1)
        if nome in _nomes_do_padrao(padrao) and _dono(blocos, m.start()) in (set(corrente) | {modulo}) and _fim_do_let(limpo, m.end()) < off:
            return f"let `{nome}` anterior no bloco do uso (L{ln(m.start())})"
    for k in corrente:
        if nome in _bindings_da_cabeca(blocos[k].cabeca):
            return f"binding `{nome}` do padrão que envolve o uso (L{ln(blocos[k].inicio)})"
    cabecas = [blocos[k].cabeca for k in cadeia] + [b.cabeca for b in blocos if b.cab_inicio <= off < b.inicio]
    for c in cabecas:
        if nome in _generico(c):
            return f"parâmetro genérico `{nome}` da assinatura que envolve o uso"
    return None
