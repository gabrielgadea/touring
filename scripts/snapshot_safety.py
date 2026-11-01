#!/usr/bin/env python3
"""Snapshot aditivo de worktrees git — passo SNAPSHOT do Git Safety Protocol v2.

Recriado em 17/10/2026: o original de 10/10 vivia no scratchpad e morreu com o /tmp no boot.

Para cada worktree: copia o índice dela para um arquivo temporário, roda ``git add -A`` na
cópia, ``write-tree``, ``commit-tree -p HEAD`` e ``update-ref`` de uma ref ``safety/`` NOVA.
O índice, o HEAD, as branches e os arquivos da worktree nunca são tocados; ref existente nunca
é sobrescrita; nada é empurrado. Uma segunda captura do zero precisa dar a mesma árvore — é o
que prova que a ref guarda a worktree byte a byte (menos o que o .gitignore exclui).

Uso:
    snapshot_safety.py --prefix safety/2026-10-17 NOME=CAMINHO [NOME=CAMINHO ...] [--dry-run]
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

MAX_MB_DEFAULT = 50
TRAILER = "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
PASSA = ("OK", "DRY-RUN", "COBERTO-PELO-HEAD")


class SnapshotError(RuntimeError):
    pass


def git(wt: str, *args: str, index: str | None = None, check: bool = True) -> str:
    env = dict(os.environ)
    if index is not None:
        env["GIT_INDEX_FILE"] = index
    r = subprocess.run(["git", "-C", wt, *args], capture_output=True, text=True, env=env, check=False)
    if check and r.returncode != 0:
        raise SnapshotError(f"git {' '.join(args)} (rc={r.returncode}): {r.stderr.strip()}")
    return r.stdout.strip()


def _sha(path: str) -> str | None:
    if not os.path.exists(path):
        return None
    with open(path, "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()


def capture_tree(wt: str, max_bytes: int) -> tuple[str, list[str]]:
    """(árvore, acima_do_teto) da worktree como ``git add -A`` a vê, num índice descartável."""
    real_index = git(wt, "rev-parse", "--path-format=absolute", "--git-path", "index")
    with tempfile.TemporaryDirectory(prefix="snapshot-safety-") as td:
        idx = os.path.join(td, "index")
        if os.path.exists(real_index):
            shutil.copyfile(real_index, idx)
        else:
            git(wt, "read-tree", "HEAD", index=idx)
        listed = git(wt, "ls-files", "-z", "--others", "--modified", "--exclude-standard", index=idx)
        oversized = []
        for rel in filter(None, listed.split("\0")):
            p = os.path.join(wt, rel)
            if os.path.isfile(p) and not os.path.islink(p) and os.path.getsize(p) > max_bytes:
                oversized.append(f"{rel} ({os.path.getsize(p) >> 20} MiB)")
        if oversized:
            return "", oversized
        git(wt, "add", "-A", index=idx)
        return git(wt, "write-tree", index=idx), []


def snapshot(name: str, wt: str, ref: str, max_bytes: int, dry_run: bool) -> dict:
    out: dict = {"nome": name, "worktree": wt}
    head = git(wt, "rev-parse", "HEAD")
    head_tree = git(wt, "rev-parse", "HEAD^{tree}")
    index_path = git(wt, "rev-parse", "--path-format=absolute", "--git-path", "index")
    index_before = _sha(index_path)
    tree, oversized = capture_tree(wt, max_bytes)
    if oversized:
        return {**out, "status": "RECUSADO", "motivo": "arquivo acima do teto", "acima_do_teto": oversized}
    changed = git(wt, "diff-tree", "-r", "--no-renames", "--name-status", head_tree, tree).splitlines()
    out.update(
        head=head[:8],
        caminhos_sobre_head=len(changed),
        por_status=dict(collections.Counter(line.split("\t", 1)[0] for line in changed)),
    )
    if git(wt, "rev-parse", "--verify", "-q", ref, check=False):
        return {**out, "status": "RECUSADO", "motivo": f"{ref} já existe"}
    if not changed:
        return {**out, "status": "COBERTO-PELO-HEAD"}
    if dry_run:
        return {**out, "status": "DRY-RUN", "ref": ref}
    msg = (
        f"safety: snapshot {name} ({len(changed)} caminhos sobre {head[:8]})\n\n"
        "Aditivo: índice temporário + commit-tree + update-ref; sem push, sem tocar branch.\n\n"
        f"{TRAILER}\n"
    )
    commit = git(wt, "commit-tree", tree, "-p", head, "-m", msg)
    git(wt, "update-ref", "-m", "snapshot_safety", ref, commit, "")
    again, _ = capture_tree(wt, max_bytes)
    checks = {
        "ref_aponta_o_commit": git(wt, "rev-parse", ref) == commit,
        "recaptura_igual": again == tree,
        "indice_intocado": _sha(index_path) == index_before,
        "head_intocado": git(wt, "rev-parse", "HEAD") == head,
    }
    return {**out, "status": "OK" if all(checks.values()) else "DIVERGIU", "ref": ref, "commit": commit[:8], **checks}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Snapshot aditivo de worktrees em refs safety/.")
    ap.add_argument("--prefix", required=True, help="prefixo da ref, ex.: safety/2026-10-17")
    ap.add_argument("--max-mb", type=int, default=MAX_MB_DEFAULT, help="teto por arquivo (MiB)")
    ap.add_argument("--dry-run", action="store_true", help="mede e mostra, não grava ref")
    ap.add_argument("trees", nargs="+", metavar="NOME=CAMINHO")
    a = ap.parse_args(argv)
    if not a.prefix.startswith("safety/"):
        ap.error("o prefixo precisa começar com safety/")
    rc = 0
    for spec in a.trees:
        name, _, path = spec.partition("=")
        if not name or not path:
            ap.error(f"esperado NOME=CAMINHO, veio {spec!r}")
        ref = f"refs/heads/{a.prefix}/{name}"
        try:
            res = snapshot(name, os.path.abspath(path), ref, a.max_mb << 20, a.dry_run)
        except SnapshotError as e:
            res = {"nome": name, "status": "ERRO", "motivo": str(e)}
        print(json.dumps(res, ensure_ascii=False))
        if res["status"] not in PASSA:
            rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main())
