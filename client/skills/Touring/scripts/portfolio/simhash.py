#!/usr/bin/env python3
# #tags: kind:script purpose:measure domain:code-mode process:measure lang:python
"""simhash.py — Python port of the Rust ``code_simhash`` used by the ``touring run`` repeat gate.

The portfolio needs the SAME body-similarity signal on both sides: the executor gate (Rust,
``touring-server/src/tools/ctx_execute_tools.rs:502``) and the Write/Bash hook (Python). The Rust
function has no CLI exit, so this is a port, and a port is only acceptable with proven parity:
``test_simhash.py`` checks every vector in ``vetores_rust.json``, each produced by the production
binary itself (``gerar_vetores_rust.py`` runs the body through ``touring run --allow-repeat`` and
reads the ``code_simhash`` the binary journaled for that ``run_id``).

Faithfulness notes (each one is a place a naive port diverges):
  * lines split on ``\\n`` only (Rust ``str::lines``), never ``str.splitlines`` (which also splits on
    ``\\x0b``, ``\\x0c``, ``\\u2028`` …); a trailing ``\\r`` is removed by ``trim`` anyway;
  * a line is dropped when, trimmed, it is empty or starts with ``#`` or ``//``;
  * tokens come from ``split_inclusive`` on any char that is not alphanumeric or ``_``: the
    separator stays GLUED to the token before it (``foo(`` is one token), then each piece is
    trimmed and empty pieces dropped;
  * fewer than 3 tokens: plain FNV-1a over the concatenated token bytes (identity, not similarity);
  * otherwise: 64-bit SimHash over 3-token shingles, FNV-1a 64 over the UTF-8 bytes of the three
    tokens concatenated with no separator.

Usage:
    python3 simhash.py <file>            # prints the 16-hex simhash
    python3 simhash.py --hamming A B     # distance between two hex hashes
"""
from __future__ import annotations

import sys
from pathlib import Path

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
MASK64 = 0xFFFFFFFFFFFFFFFF
#: Distances calibrated on the 14 real recreation champions (positives <= 8, negatives >= 20).
TAU_FORTE = 10
TAU_FRACO = 20


def _palavra(ch: str) -> bool:
    """Rust ``c.is_alphanumeric() || c == '_'`` (parity checked by the vectors, incl. non-ASCII)."""
    return ch == "_" or ch.isalnum()


def _split_inclusive(linha: str) -> list[str]:
    """Rust ``split_inclusive(pred)``: each piece ends WITH the separator that closed it."""
    pedacos: list[str] = []
    inicio = 0
    for i, ch in enumerate(linha):
        if not _palavra(ch):
            pedacos.append(linha[inicio : i + 1])
            inicio = i + 1
    if inicio < len(linha):
        pedacos.append(linha[inicio:])
    return pedacos


def tokens(codigo: str) -> list[str]:
    """The normalized token stream the Rust function hashes."""
    saida: list[str] = []
    for bruta in codigo.split("\n"):
        linha = bruta.strip()
        if not linha or linha.startswith(("#", "//")):
            continue
        for pedaco in _split_inclusive(linha):
            t = pedaco.strip()
            if t:
                saida.append(t)
    return saida


def _fnv1a(dados: bytes, h: int = FNV_OFFSET) -> int:
    for b in dados:
        h = ((h ^ b) * FNV_PRIME) & MASK64
    return h


def simhash64(codigo: str) -> int:
    """Port of ``code_simhash``; returns the unsigned 64-bit value."""
    toks = tokens(codigo)
    if len(toks) < 3:
        h = FNV_OFFSET
        for t in toks:
            h = _fnv1a(t.encode("utf-8"), h)
        return h
    v = [0] * 64
    for i in range(len(toks) - 2):
        h = _fnv1a("".join(toks[i : i + 3]).encode("utf-8"))
        for bit in range(64):
            v[bit] += 1 if (h >> bit) & 1 else -1
    return sum(1 << bit for bit in range(64) if v[bit] > 0)


def hex64(valor: int) -> str:
    """The journal's encoding: 16 lowercase hex digits."""
    return f"{valor:016x}"


def hamming(a: int, b: int) -> int:
    """Bit distance between two simhashes."""
    return (a ^ b).bit_count()


def main(argv: list[str]) -> int:
    if len(argv) == 3 and argv[0] == "--hamming":
        print(hamming(int(argv[1], 16), int(argv[2], 16)))
        return 0
    if len(argv) != 1 or argv[0].startswith("-"):
        print("uso: simhash.py <arquivo> | simhash.py --hamming <hexA> <hexB>", file=sys.stderr)
        return 2
    print(hex64(simhash64(Path(argv[0]).read_text(encoding="utf-8", errors="replace"))))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
