#!/usr/bin/env python3
"""Gera `frag.patch` (contra o ESPELHO = crates/ do int97) e PROVA que ele reconstrói a árvore do FRAG.

  1. `diff -ruN` int97/crates -> frag/crates, com os cabeçalhos reescritos para `a/crates/…` e `b/crates/…`
     (aplicável com `patch -p1` / `git apply` na raiz do int97);
  2. `patch --dry-run` na base pristina (touring-wt-frag-base) deve passar limpo;
  3. aplica numa CÓPIA reflink da base e compara o sha_tree de `crates/` com o do FRAG (igual = reconstrói).

Uso: faz_patch.py <saida.patch>     (nada nos três worktrees é alterado: a prova roda numa cópia nova em /tmp)
"""

import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path

PROJ = Path("/home/gabrielgadea/projects")
INT97 = PROJ / "touring-wt-int97"
FRAG = PROJ / "touring-wt-frag"
BASE = PROJ / "touring-wt-frag-base"


def sha_tree(raiz: Path) -> str:
    h = hashlib.sha256()
    for f in sorted(p for p in raiz.rglob("*") if p.is_file()):
        h.update(str(f.relative_to(raiz)).encode() + hashlib.sha256(f.read_bytes()).digest())
    return h.hexdigest()[:16]


def gerar() -> str:
    r = subprocess.run(
        ["diff", "-ruN", "-x", "target", "touring-wt-int97/crates", "touring-wt-frag/crates"],
        cwd=PROJ, capture_output=True, text=True, check=False,
    )
    if r.returncode not in (0, 1):
        raise SystemExit(f"diff falhou: {r.stderr[:300]}")
    linhas = []
    for linha in r.stdout.splitlines(keepends=True):
        if linha.startswith(("diff ", "--- ", "+++ ")):
            linha = linha.replace("touring-wt-int97/", "a/", 1).replace("touring-wt-frag/", "b/", 1)
        linhas.append(linha)
    return "".join(linhas)


def main(saida: Path) -> int:
    patch = gerar()
    saida.write_text(patch)
    arquivos = sorted({l.split()[1][2:] for l in patch.splitlines() if l.startswith("+++ ")})
    print(f"patch: {len(patch.splitlines())} linhas, {len(arquivos)} arquivos:")
    for a in arquivos:
        print("  ", a)
    seco = subprocess.run(["patch", "-p1", "--dry-run", "-d", str(BASE), "-i", str(saida)], capture_output=True, text=True)
    print("dry-run na base pristina:", "LIMPO" if seco.returncode == 0 else f"FALHOU\n{seco.stdout}{seco.stderr}")
    if seco.returncode != 0:
        return 1
    # a prova roda numa cópia NOVA (nome com carimbo) em /tmp; nada é apagado: a cópia fica para autópsia
    copia = Path(tempfile.mkdtemp(prefix="frag-patch-prova-", dir="/tmp")) / "base"
    copia.mkdir()
    subprocess.run(["cp", "-a", "--reflink=auto", str(BASE / "crates"), str(copia / "crates")], check=True)
    ap = subprocess.run(["patch", "-p1", "-d", str(copia), "-i", str(saida)], capture_output=True, text=True)
    if ap.returncode != 0:
        print("aplicação real falhou:", ap.stdout, ap.stderr)
        return 1
    base_depois, frag = sha_tree(copia / "crates"), sha_tree(FRAG / "crates")
    print(f"sha_tree crates: base+patch={base_depois} frag={frag} {'IGUAL' if base_depois == frag else 'DIVERGE'}")
    print(f"sha_tree crates: int97(espelho)={sha_tree(INT97 / 'crates')} base_pristina={sha_tree(BASE / 'crates')}")
    print(f"cópia da prova: {copia}")
    return 0 if base_depois == frag else 1


if __name__ == "__main__":
    sys.exit(main(Path(sys.argv[1])))
