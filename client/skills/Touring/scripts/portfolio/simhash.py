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

import re
import sys
from collections import Counter
from pathlib import Path

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
MASK64 = 0xFFFFFFFFFFFFFFFF
#: Distances calibrated on the 14 real recreation champions (positives <= 8, negatives >= 20).
TAU_FORTE = 10
TAU_FRACO = 20


#: Rust ``split_inclusive(|c| !(c.is_alphanumeric() || c == '_'))``: each piece ends WITH the separator
#: that closed it, plus the trailing run. In CPython's ``sre`` a ``\\w`` is exactly ``ch.isalnum() or
#: ch == "_"`` (checked over every code point by ``diagnostics/paridade_simhash_otimizado.py``).
_PEDACO = re.compile(r"\w*\W|\w+\Z")
#: For each byte value, the bit positions it sets - the vote counts bytes instead of looping 64 bits.
_BITS_DO_BYTE = tuple(tuple(b for b in range(8) if v >> b & 1) for v in range(256))


def tokens(codigo: str) -> list[str]:
    """The normalized token stream the Rust function hashes."""
    linhas = (bruta.strip() for bruta in codigo.split("\n"))
    # One findall over the kept lines: "\n" is a non-word char, so no piece crosses a line, and the "\n"
    # glued to a line's last piece is removed by the same strip that Rust's per-line split never needed.
    texto = "\n".join(linha for linha in linhas if linha and not linha.startswith(("#", "//")))
    return [t for pedaco in _PEDACO.findall(texto) if (t := pedaco.strip())]


def _fnv1a(dados: bytes, h: int = FNV_OFFSET) -> int:
    for b in dados:
        h = ((h ^ b) * FNV_PRIME) & MASK64
    return h


def _votar(hashes: list[int]) -> int:
    """Bit ``b`` is set when more hashes have it at 1 than at 0 (Rust: ``v[b] += ±1``, then ``v[b] > 0``).

    Counting per byte column turns 64 Python steps per hash into one C-level ``Counter`` pass per column.
    """
    buf = b"".join(h.to_bytes(8, "little") for h in hashes)
    uns = [0] * 64
    for coluna in range(8):
        for valor, n in Counter(buf[coluna::8]).items():
            for b in _BITS_DO_BYTE[valor]:
                uns[8 * coluna + b] += n
    total = len(hashes)
    return sum(1 << bit for bit in range(64) if 2 * uns[bit] > total)


def simhash64(codigo: str, *, distintos: bool = True) -> int:
    """Port of ``code_simhash``; returns the unsigned 64-bit value.

    ``distintos=True`` (default since 29/10/2026) is the Rust ``code_simhash_v2`` bit for bit: each DISTINCT
    shingle votes once. ``distintos=False`` is the Rust ``code_simhash`` (v1): every shingle votes as many
    times as it occurs. Measured 28/10/2026 on the frozen corpus: decorative lines (``print("─" * 60)``,
    ``====``) tokenize into dozens of identical shingles that outvote the rest of the program, so unrelated
    scripts collided at distance 0 under v1 (``capa_invariant.py`` × ``s64_warmer.py``, Jaccard 0.066).
    Both modes are checked against the binary by ``test_simhash.py`` (``vetores_rust.json``).
    """
    toks = tokens(codigo)
    if len(toks) < 3:
        h = FNV_OFFSET
        for t in toks:
            h = _fnv1a(t.encode("utf-8"), h)
        return h
    shingles = ["".join(toks[i : i + 3]) for i in range(len(toks) - 2)]
    vistos = {s: _fnv1a(s.encode("utf-8")) for s in set(shingles)}
    return _votar(list(vistos.values()) if distintos else [vistos[s] for s in shingles])


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
