"""Mutantes da coordenadora (independentes dos do subagente) contra o selftest do shell_inline.

Cada mutante substitui UM trecho que ocorre exatamente uma vez; a cópia roda como subprocesso numa pasta
temporária no scratchpad. Morto = selftest com rc != 0. Uso: python3 mutar_shell_inline.py
"""

from __future__ import annotations

import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path

ALVO = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/shell_inline.py"
SCRATCH = Path(__file__).parent

MUTANTES = {
    "M1 opção curta com valor nunca toma a próxima": (
        "return 1 if k + 1 < len(v) else 2",
        "return 1",
    ),
    "M2 continuação no fim nunca engole o delimitador": (
        "engoliu = i + 2 == len(corpo)",
        "engoliu = False",
    ),
    "M3 heredoc sem aspas só desescapa o $": ('nxt in "$`\\\\"', 'nxt in "$"'),
    "M4 aninhado não incrementa a profundidade": ("prof=a.prof + 1", "prof=a.prof"),
    "M5 -o do shell não consome o valor": ('i += 2 if v[-1] in "oO" else 1', "i += 1"),
    "M6 bash -c com expansão aceito como programa": ("None if w.expande else w.valor", "w.valor"),
    "M7 -- do embrulho não é consumido": ('if v == "--":\n            return n + 1', 'if v == "--":\n            return n'),
}


def rodar(texto: str) -> tuple[int, str]:
    with tempfile.TemporaryDirectory(dir=SCRATCH) as d:
        p = Path(d) / "shell_inline.py"
        p.write_text(texto)
        r = subprocess.run([sys.executable, str(p), "--selftest"], capture_output=True, text=True, timeout=300)
        ult = (r.stdout + r.stderr).strip().splitlines()
        return r.returncode, ult[-1][:120] if ult else ""


def main() -> int:
    orig = ALVO.read_text()
    h = hashlib.sha256(orig.encode()).hexdigest()[:16]
    rc0, msg0 = rodar(orig)
    print(f"alvo sha {h} · controle (sem mutação): rc {rc0} · {msg0}")
    if rc0 != 0:
        return 2
    vivos = 0
    for nome, (de, para) in MUTANTES.items():
        n = orig.count(de)
        if n != 1:
            print(f"  ÂNCORA {n}x: {nome}")
            vivos += 1
            continue
        rc, msg = rodar(orig.replace(de, para))
        morto = rc != 0
        vivos += not morto
        print(f"  {'MORTO' if morto else 'VIVO '}  {nome}  (rc {rc} · {msg})")
    assert hashlib.sha256(ALVO.read_text().encode()).hexdigest()[:16] == h, "alvo mudou durante a mutação"
    print(f"vivos: {vivos}/{len(MUTANTES)}")
    return 1 if vivos else 0


if __name__ == "__main__":
    raise SystemExit(main())
