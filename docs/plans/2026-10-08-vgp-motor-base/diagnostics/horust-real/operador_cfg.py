"""HORUST-REAL / estrato cfg — operador `cfg_fn`: move uma função livre para `#[cfg(test)]` e deixa a ÚNICA chamada viva no alvo não-teste.

A mutação é uma linha a mais: `#[cfg(test)]` logo acima do `fn`. No alvo de teste a função continua existindo; no alvo normal o nome some
e o rustc responde E0425 na linha da chamada. É a mesma estrutura do caso natural `gates_limpo` (o nome existe no instante, sob cfg de
teste, e o alvo normal rejeita o uso), em arquivo real e com o rustc como oráculo. Nada aqui roda cargo: a validade é do oráculo.

Sítio = função livre na coluna 0, antes do primeiro `#[cfg(test)]`, com exatamente duas ocorrências do nome no arquivo mascarado (a definição
e UMA chamada livre viva, fora do corpo da própria função), sem o nome em outro arquivo do pacote e sem atributo que mude o item.
"""

from __future__ import annotations

import dataclasses
import re

import operadores_rust as op

CODIGO_CFG = ("cfg", "E0425")
OPERADOR = "cfg_fn"
LINHA_CFG = "#[cfg(test)]"

_DEF = re.compile(r"^(?:pub(?:\([a-z]+\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([a-z_][a-z0-9_]{2,})\b")
_ATRIBUTO_INOFENSIVO = re.compile(r"^\s*#\[\s*(?:inline|must_use|allow|expect|deprecated|doc|track_caller|cold)\b")


@dataclasses.dataclass(frozen=True)
class SitioCfg:
    nome: str
    linha_def: int  # 1-based, linha do `fn`
    linha_uso: int  # 1-based, no texto ORIGINAL


def linha_do_erro(s: SitioCfg) -> int:
    """A linha da chamada no texto MUTADO: a linha inserida empurra tudo o que vem a partir da definição."""
    return s.linha_uso + 1 if s.linha_uso > s.linha_def else s.linha_uso


def linha_do_uso(linha_erro: int, linha_def: int) -> int:
    """Inversa de `linha_do_erro`: a linha da chamada no texto ORIGINAL, dada a linha do erro e a da definição."""
    return linha_erro - 1 if linha_erro > linha_def else linha_erro


def _atributos_ok(i: int, linhas: list[str], originais: list[str]) -> bool:
    """Os atributos imediatamente acima do `fn` (comentários são transparentes) só podem ser os que não mudam o item."""
    j = i - 1
    while j >= 0:
        if originais[j].strip() and not linhas[j].strip():  # comentário (mascarado em branco)
            j -= 1
        elif linhas[j].lstrip().startswith("#["):
            if not _ATRIBUTO_INOFENSIVO.match(linhas[j]):
                return False
            j -= 1
        else:
            break
    return True


def _corpo_fim(linhas: list[str], i: int) -> int | None:
    """Linha (0-based) do `}` que fecha o corpo do `fn` que começa na linha i."""
    for j in range(i, len(linhas)):
        col = linhas[j].find("{")
        if col >= 0:
            fim = op._fecha_bloco(linhas, j, col)
            return fim[0] if fim else None
    return None


def sitios_cfg(texto: str, em_outros_arquivos: frozenset[str] = frozenset()) -> list[SitioCfg]:
    mascarado = op.mascarar(texto)
    linhas, originais = mascarado.split("\n"), texto.split("\n")
    mortas = op._linhas_mortas(mascarado, linhas)
    saida: list[SitioCfg] = []
    for i, ln in enumerate(linhas):
        m = _DEF.match(ln)
        if not m or i in mortas or m.group(1) in op.KEYWORDS or m.group(1) in em_outros_arquivos or not _atributos_ok(i, linhas, originais):
            continue
        nome = m.group(1)
        ocorrencias = [mascarado.count("\n", 0, o.start()) for o in re.finditer(rf"(?<![\w]){re.escape(nome)}(?![\w])", mascarado)]
        usos = [j for j in ocorrencias if j != i]
        fim = _corpo_fim(linhas, i)
        if len(ocorrencias) != 2 or len(usos) != 1 or fim is None:
            continue
        j = usos[0]
        chamada = re.search(rf"(?<![\w.:$']){re.escape(nome)}\s*\(", linhas[j])
        if j in mortas or i <= j <= fim or not chamada:
            continue
        saida.append(SitioCfg(nome, i + 1, j + 1))
    return saida


def mutar_cfg(texto: str, s: SitioCfg) -> str:
    linhas = texto.split("\n")
    m = _DEF.match(linhas[s.linha_def - 1]) if s.linha_def <= len(linhas) else None
    if not m or m.group(1) != s.nome:
        raise ValueError(f"o sítio cfg_fn@{s.linha_def} não bate com o texto: {s.nome!r}")
    return "\n".join([*linhas[: s.linha_def - 1], LINHA_CFG, *linhas[s.linha_def - 1 :]])
