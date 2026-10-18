"""Mutants of the hook-shim trust check: each must be killed by scripts/test_touring_hook_shim.py.

Usage: mutantes_shim.py   (exit 0 only when every mutant is killed and the unmutated shim passes)
"""
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path("/home/gabrielgadea/projects/touring")
SHIM = REPO / "scripts/hooks/touring-hook-shim.sh"
TEST = REPO / "scripts/test_touring_hook_shim.py"
MUTANTES = {
    "sem checagem de prefixo": ('[[ -n "$real" && -n "$root" && "$real" == "$root"/* ]] || return 1', ": # M1"),
    "prefixo sem a barra": ('"$real" == "$root"/*', '"$real" == "$root"*'),
    "sem checagem de dono": ('[[ "$owner" == "$EUID" ]] || return 1', ": # M3"),
    "sem checagem de escrita": ('[[ "${perm:5:1}" != "w" && "${perm:8:1}" != "w" ]]', "true # M4"),
    "rejeitado encerra a busca": ('            note_rejected "$candidate"\n', '            note_rejected "$candidate"; return 1\n'),
    "candidato sem checagem": ("            if project_bin_trusted \"$candidate\"; then", "            if true; then"),
}


def roda(shim: Path) -> int:
    env = dict(os.environ, TOURING_HOOK_SHIM_UNDER_TEST=str(shim))
    return subprocess.run([sys.executable, "-m", "pytest", str(TEST), "-q", "-p", "no:cacheprovider"],
                          env=env, capture_output=True, text=True, timeout=120, check=False).returncode


def main() -> int:
    original = SHIM.read_text(encoding="utf-8")
    if roda(SHIM) != 0:
        print("ORIGINAL falha: nada a medir")
        return 2
    vivos = 0
    with tempfile.TemporaryDirectory() as d:
        for nome, (velho, novo) in MUTANTES.items():
            assert original.count(velho) == 1, f"{nome}: alvo não encontrado exatamente 1 vez"
            m = Path(d) / "shim.sh"
            m.write_text(original.replace(velho, novo), encoding="utf-8")
            rc = roda(m)
            morto = rc != 0
            vivos += not morto
            print(f"{'MORTO' if morto else 'VIVO '} {nome} (rc={rc})")
    print(f"{len(MUTANTES) - vivos}/{len(MUTANTES)} mortos")
    return 1 if vivos else 0


if __name__ == "__main__":
    sys.exit(main())
