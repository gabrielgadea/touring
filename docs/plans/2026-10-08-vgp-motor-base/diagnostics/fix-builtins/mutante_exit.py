#!/usr/bin/env python3
"""Remove exit/quit from the model, require the regression test to fail, restore bytes."""

import hashlib
import json
import os
import subprocess
from pathlib import Path

ROOT = Path("/home/gabrielgadea/projects/touring")
ALVO = ROOT / "crates/touring-code/src/vgp/lang/python/stdlib_model.rs"
TESTE = "site_builtins_exit_e_quit_nao_geram_falso_deny"
OUT = ROOT / "docs/plans/2026-10-08-vgp-motor-base/diagnostics/fix-builtins"


def sha() -> str:
    return hashlib.sha256(ALVO.read_bytes()).hexdigest()


def cargo(nome: str) -> tuple[int, str]:
    run = subprocess.run(
        ["cargo", "test", "-p", "touring-code", TESTE],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=600,
        check=False,
    )
    (OUT / nome).write_text(run.stdout + run.stderr, encoding="utf-8")
    return run.returncode, run.stdout + run.stderr


def main() -> int:
    original = ALVO.read_text(encoding="utf-8")
    sha_antes = sha()
    try:
        mutante = original.replace('    "exit",\n    "quit",\n', "")
        assert mutante != original
        ALVO.write_text(mutante, encoding="utf-8")
        rc, log = cargo("mutante_exit.log")
        morto = rc != 0 and "FAILED" in log and TESTE in log
    finally:
        ALVO.write_text(original, encoding="utf-8")
        os.utime(ALVO, None)
    assert sha() == sha_antes, "stdlib_model.rs não foi restaurado byte a byte"
    rc_verde, _ = cargo("green_depois_mutante.log")
    resultado = {"mutante_sem_exit_quit_rc": rc, "morto_por_assertion": morto, "green_depois_rc": rc_verde}
    (OUT / "mutante_exit.json").write_text(json.dumps(resultado, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(resultado))
    return 0 if morto and rc_verde == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
