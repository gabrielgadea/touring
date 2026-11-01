"""Reconhecedor de `cargo` ESTENDIDO (HORUST, 10/10/2026).

O `analisar_cargo` do gerador (`corpus_rust_natural.py`) so entende prefixos simples antes do `cargo` (`VAR=x`, `timeout N`, `time`,
`nice -n N`, `env`, `hw-slot --class X --`). Medido sobre os transcripts de analise e touring (`diagnostico_reconhecimento.py`),
desde a semana 38 ele deixa de ver 47-48% dos comandos `cargo`: `hw-slot --class rust --wait N --`, `env -u VAR`, `taskset -c`, e o
mais comum, `hw-slot ... -- bash -c '... cargo ...'`. Cada chamada nao vista e um positivo, um negativo e uma prova de compilacao a
menos no universo.

Esta camada DESEMBRULHA o statement (tira os envoltorios que nao mudam o que o cargo faz e abre `bash -c '<script>'`
recursivamente, preservando os `cd` e os `&&`) e entrega o resultado ao `analisar_cargo` ORIGINAL, que continua sendo o unico que
entende o cargo. Nada do gerador e editado: `instalar()` troca `corpus_rust_natural.analisar_cargo` por esta versao (e
`restaurar()` desfaz). O que o original ja reconhecia continua igual; mencao a cargo (`echo 'cargo check'`) continua nao sendo cargo.
"""

from __future__ import annotations

import re
import shlex

import corpus_rust_natural as crn

_ORIGINAL = crn.analisar_cargo
_VAR = re.compile(r"[A-Za-z_]\w*=.*")
_DURACAO = re.compile(r"\d+(?:\.\d+)?[smhd]?")
_SHELLS = {"bash", "sh", "zsh", "dash"}


def _base(tok: str) -> str:
    return tok.rsplit("/", 1)[-1]


def _pular(tok: list[str], i: int) -> int:
    """Indice do primeiro token depois de UM envoltorio em `tok[i]`; i se `tok[i]` nao e envoltorio."""
    t, b = tok[i], _base(tok[i])
    if _VAR.fullmatch(t) or b in ("nohup", "time", "command", "exec", "stdbuf"):
        return i + 1
    if b == "env":
        j = i + 1
        while j < len(tok):
            if tok[j] in ("-u", "-C", "-S"):
                j += 2
            elif tok[j].startswith("-") or _VAR.fullmatch(tok[j]):
                j += 1
            else:
                break
        return j
    if b == "timeout":
        j = i + 1
        while j < len(tok) and tok[j].startswith("-"):
            j += 2 if tok[j] in ("-k", "-s") else 1
        return j + 1 if j < len(tok) and _DURACAO.fullmatch(tok[j]) else j
    if b in ("nice", "ionice"):
        j = i + 1
        while j < len(tok) and tok[j].startswith("-"):
            j += 2 if tok[j] in ("-n", "-c") else 1
        return j
    if b == "taskset":
        j = i + 1
        if j < len(tok) and tok[j] in ("-c", "--cpu-list"):
            return j + 2
        return j + 1 if j < len(tok) and not tok[j].startswith("-") else j
    if b == "hw-slot":
        return tok.index("--", i) + 1 if "--" in tok[i:] else len(tok)
    if b == "systemd-run":
        j = i + 1
        while j < len(tok) and _base(tok[j]) not in _SHELLS:
            j += 1
        return j
    return i


def _desembrulhar_stmt(stmt: str) -> str:
    """O statement sem os envoltorios; `bash -c 'script'` vira o proprio script (recursivo). Sem mudanca → o texto original."""
    try:
        tok = shlex.split(stmt)
    except ValueError:
        return stmt
    i = 0
    while i < len(tok):
        j = _pular(tok, i)
        if j == i:
            break
        i = j
    if i < len(tok) and _base(tok[i]) in _SHELLS:
        k = i + 1
        while k < len(tok) and tok[k].startswith("-") and "c" not in tok[k].lstrip("-"):
            k += 1
        if k + 1 < len(tok) and tok[k].startswith("-") and "c" in tok[k].lstrip("-"):
            return transformar(tok[k + 1])
    if i == 0:
        return stmt
    return " ".join(shlex.quote(x) if re.search(r"""[\s'"$`\\]""", x) else x for x in tok[i:])


def transformar(comando: str) -> str:
    """O comando com cada statement desembrulhado; os separadores (`&&`, `||`, `;`, quebra de linha) ficam como estavam."""
    return "".join(_desembrulhar_stmt(seg) + sep for seg, sep in crn.dividir(comando, ("&&", "||", ";", "\n")))


def analisar_cargo_estendido(comando: str):
    """As invocacoes de cargo que o comando RODA, vendo atraves dos envoltorios; nunca menos que o analisador original."""
    a = _ORIGINAL(comando)
    try:
        b = _ORIGINAL(transformar(comando))
    except Exception:  # noqa: BLE001 — qualquer surpresa de parse: fica com o original
        return a
    return b if len(b) >= len(a) else a


def instalar() -> None:
    crn.analisar_cargo = analisar_cargo_estendido


def restaurar() -> None:
    crn.analisar_cargo = _ORIGINAL
