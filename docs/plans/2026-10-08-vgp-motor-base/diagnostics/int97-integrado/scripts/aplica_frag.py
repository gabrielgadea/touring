#!/usr/bin/env python3
"""Apply FRAG (frag-base af515c6e → touring-wt-frag) onto touring-wt-int97 (+P0B +roteamento), 17/10/2026.

Per file, 3-way merge: base = frag-base, ours = int97, theirs = frag. The simulation found exactly ONE conflict,
the `use super::{…}` line of vgp/sombra.rs (P0B imports `Alvo`, FRAG swaps `check` for `check_em` and imports
`Contexto`). That block is resolved here only if it is byte-for-byte the expected one; any other conflict, or
any difference in the set of files, stops the script — a resolution never applies blind.

Resolution: the module body takes what it uses (Alvo from P0B; Contexto/check_em from FRAG). The three P0B tests
call `check(…)` (= `check_em(…, Contexto::Escrita)`, vgp/mod.rs) through `use super::*`; a top-level `check` import
would be unused outside cfg(test) (clippy -D warnings), so the tests module imports it.

Usage: aplica_frag.py --dry-run <scratch-dir> | --apply <backup-dir>
"""
import os
import subprocess
import sys
import tempfile
from pathlib import Path

P = Path("/home/gabrielgadea/projects")
F = P / "touring-wt-frag"
# APLICA_FRAG_ALVO: another worktree holding the SAME int97 content (the P0B-d base, 17/10) — default the int97 itself
I = Path(os.environ.get("APLICA_FRAG_ALVO", str(P / "touring-wt-int97")))
BASE = "af515c6e"
SOMBRA = "crates/touring-code/src/vgp/sombra.rs"
EXPECTED_FILES = 15
CONFLITO = (
    "<<<<<<< int97\n"
    "use super::{Alvo, CheckInput, CheckOutput, Finding, check};\n"
    "=======\n"
    "use super::{CheckInput, CheckOutput, Contexto, Finding, check_em};\n"
    ">>>>>>> frag\n"
)
RESOLVIDO = "use super::{Alvo, CheckInput, CheckOutput, Contexto, Finding, check_em};\n"
TESTS_OLD = "mod tests {\n    use super::*;\n"
TESTS_NEW = (
    "mod tests {\n"
    "    use super::*;\n"
    "    // The P0B tests ask the write-time verdict; since FRAG the module body only calls `check_em`.\n"
    "    use crate::vgp::check;\n"
)


def git(*a, env=None, check=True):
    r = subprocess.run(["git", *a], cwd=F, capture_output=True, env=env, check=False)
    if check and r.returncode != 0:
        sys.exit(f"git {a}: {r.stderr.decode()[:300]}")
    return r


def merge(rel: str) -> tuple[str, bytes | None]:
    theirs = (F / rel).read_bytes() if (F / rel).exists() else None
    b = git("show", f"{BASE}:{rel}", check=False)
    base = b.stdout if b.returncode == 0 else b""
    ours = (I / rel).read_bytes() if (I / rel).exists() else None
    if theirs is None:
        return ("DELETE", None) if ours == base else ("ABORTA: frag apaga um arquivo que o int97 mudou", None)
    if ours is None:
        return ("NOVO", theirs) if not base else ("ABORTA: ausente no int97 mas presente na base", None)
    if ours == base or ours == theirs:
        return ("copia", theirs)
    with tempfile.TemporaryDirectory() as t:
        o, bb, th = Path(t, "o"), Path(t, "b"), Path(t, "t")
        o.write_bytes(ours)
        bb.write_bytes(base)
        th.write_bytes(theirs)
        rc = subprocess.run(["git", "merge-file", "-L", "int97", "-L", "frag-base", "-L", "frag", str(o), str(bb), str(th)],
                            check=False).returncode
        out = o.read_bytes()
    if rc == 0:
        return ("merge", out)
    if rel != SOMBRA or rc != 1:
        return (f"ABORTA: conflito inesperado ({rc}) em {rel}", None)
    txt = out.decode("utf-8")
    if txt.count(CONFLITO) != 1 or txt.count("<<<<<<< ") != 1 or txt.count(TESTS_OLD) != 1:
        return ("ABORTA: o conflito do sombra.rs não é o bloco esperado", None)
    txt = txt.replace(CONFLITO, RESOLVIDO).replace(TESTS_OLD, TESTS_NEW)
    return ("merge+resolucao", txt.encode("utf-8"))


def main() -> int:
    modo, alvo = sys.argv[1], Path(sys.argv[2])
    with tempfile.TemporaryDirectory() as t:
        env = {**os.environ, "GIT_INDEX_FILE": f"{t}/idx"}
        git("read-tree", BASE, env=env)
        git("add", "-A", env=env)
        lines = git("diff", "--cached", "--name-status", BASE, env=env).stdout.decode().splitlines()
    rels = [ln.split("\t")[-1] for ln in lines if ln.strip()]
    if len(rels) != EXPECTED_FILES:
        sys.exit(f"ABORTA: o FRAG tem {len(rels)} caminhos, a simulação viu {EXPECTED_FILES} — rever antes de aplicar")
    plano = [(rel, *merge(rel)) for rel in rels]
    for rel, acao, _ in plano:
        print(f"  {acao:18s} {rel}")
    if any(a.startswith("ABORTA") for _, a, _ in plano):
        return 2
    for rel, acao, conteudo in plano:
        if conteudo is not None and b"<<<<<<< " in conteudo:
            sys.exit(f"ABORTA: marcador de conflito restou em {rel}")
    for rel, acao, conteudo in plano:
        if modo == "--dry-run":
            dst = alvo / rel
        else:
            src = I / rel
            if src.exists():
                (alvo / rel).parent.mkdir(parents=True, exist_ok=True)
                (alvo / rel).write_bytes(src.read_bytes())
            dst = src
        if conteudo is None:
            if modo == "--apply" and dst.exists():
                dst.unlink()
            continue
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(conteudo)
    print(f"{'SIMULADO em' if modo == '--dry-run' else 'APLICADO; backup em'} {alvo} · {len(plano)} arquivos")
    return 0


if __name__ == "__main__":
    sys.exit(main())
