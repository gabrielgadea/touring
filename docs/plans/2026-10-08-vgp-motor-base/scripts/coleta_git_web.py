"""Estrato `git` do holdout web: pares (blob antes, blob depois) de commits reais (onda 2, tarefa B).

SO COMANDOS DE LEITURA: `git log`, `git show <sha>:<path>`, `git diff-tree`. Nada de checkout, nada
de worktree, nada de escrita no repositorio.

Regras da coordenadora (09/10):
- cada edicao e o par (blob antes, blob depois) de um arquivo .ts/.tsx/.js/.mjs/.html;
- deduplicar os repositorios pela IDENTIDADE do repo (os `touring-wt-*` contam como um so) e as
  edicoes por (caminho, sha do blob depois);
- excluir node_modules, dist, build, vendor, *.min.js, lockfiles, gerado e terceiro com
  cabecalho de licenca alheia;
- corte pela data do commit <= 2026-10-09T23:00:00Z;
- teto de concentracao de 3% por arquivo e por commit;
- sobreposicao zero com `corpus_semeado/` por sha de conteudo.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
from collections import Counter
from pathlib import Path

PROJECTS = Path.home() / "projects"
SEMEADO = Path.home() / ".claude/touring/vgp/corpus_semeado"
CORTE_EPOCH = 1791416400  # 2026-10-09T23:00:00Z
EXT = {".ts", ".tsx", ".js", ".mjs", ".html"}
EXCLUIR_PATH = (
    "node_modules/",
    "/dist/",
    "/build/",
    "/target/",
    "/vendor/",
    "/.next/",
    "/coverage/",
    "/third_party/",
)
EXCLUIR_NOME = (
    ".min.js",
    ".lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    ".d.ts",
)
LICENCA_ALHEIA = re.compile(
    r"@author\s|@license\s|Copyright\s*\(c\)|SPDX-License-Identifier|"
    r"Permission is hereby granted|All rights reserved",
    re.IGNORECASE,
)
GERADO = re.compile(
    r"auto-?generated|@generated|DO NOT EDIT|@ts-nocheck", re.IGNORECASE
)


def git(repo: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        capture_output=True,
        text=True,
        timeout=300,
        check=False,
    ).stdout


def identidade(repo: Path) -> str:
    """A identidade do repo: os `touring-wt-*` apontam para o MESMO repositorio."""
    real = git(repo, "rev-parse", "--path-format=absolute", "--git-common-dir").strip()
    if real:
        return os.path.realpath(real)
    return str(repo.resolve())


def shas_do_semeado() -> set:
    """Todo conteudo que ja esta no corpus semeado (blob depois = base de um semeado)."""
    shas = set()
    if not SEMEADO.is_dir():
        return shas
    for lang in sorted(os.listdir(SEMEADO)):
        d = SEMEADO / lang
        ej = d / "entradas.jsonl"
        if not ej.is_file():
            continue
        with open(ej, encoding="utf-8", errors="replace") as f:
            for ln in f:
                if not ln.strip():
                    continue
                try:
                    r = json.loads(ln)
                except json.JSONDecodeError:
                    continue
                for chave in ("snap",):
                    snap = d / str(r.get(chave) or "")
                    if snap.is_file():
                        shas.add(hashlib.sha256(snap.read_bytes()).hexdigest())
                orig = str(r.get("snap") or "")
                if orig.endswith(".orig") or ".orig." in orig:
                    p = d / orig
                    if p.is_file():
                        shas.add(hashlib.sha256(p.read_bytes()).hexdigest())
    return shas


def elegivel(caminho: str) -> tuple:
    """(bool, motivo) -- o arquivo presta? (o motivo e o nome da regra, nao uma prosa)"""
    if os.path.splitext(caminho)[1] not in EXT:
        return False, "ext_fora"
    for frag in EXCLUIR_PATH:
        if frag in "/" + caminho:
            return False, "diretorio_excluido"
    for nome in EXCLUIR_NOME:
        if caminho.endswith(nome):
            return False, "nome_excluido"
    return True, ""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-commits", type=int, default=400, help="por repositorio")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument(
        "--saida", type=Path, default=Path.home() / ".cache/vgp-semeado/git_pairs.json"
    )
    a = ap.parse_args()

    stats = Counter()
    vistos_repo: set = set()
    pares: list = []
    descartes = Counter()

    for repo in sorted(PROJECTS.iterdir() if PROJECTS.is_dir() else []):
        # worktree tem `.git` como ARQUIVO, repo tem como diretorio: os dois contam.
        if not repo.is_dir() or not (repo / ".git").exists():
            continue
        ident = identidade(repo)
        if ident in vistos_repo:
            stats["repo_duplicado"] += 1
            continue
        vistos_repo.add(ident)
        stats["repos"] += 1

        # `--raw` e nao ambiguo: `:100644 100644 <blob_antes> <blob_depois> M\t<caminho>`.
        # `--name-only` nao separa o commit do arquivo (medido: as linhas vem coladas), o que
        # fez a 1a versao devolver 0 pares sem erro.
        log = git(
            repo,
            "log",
            f"--max-count={a.max_commits}",
            f"--until=@{CORTE_EPOCH}",
            "--diff-filter=M",
            "--raw",
            "--no-abbrev",
            "--pretty=format:@@@%H",
        )
        commits = []
        atual = None
        for ln in log.splitlines():
            if ln.startswith("@@@"):
                atual = ln[3:].strip()
                continue
            if not atual or not ln.startswith(":"):
                continue
            # :<modo_antes> <modo_depois> <sha_antes> <sha_depois> <status>\t<caminho>
            corpo, _, caminho = ln.partition("\t")
            partes = corpo.split()
            if len(partes) < 5 or partes[4] != "M":
                continue
            caminho = caminho.strip()
            ok, motivo = elegivel(caminho)
            if not ok:
                descartes[motivo] += 1
                continue
            commits.append((atual, caminho))

        n_commit = 0
        for csha, caminho in commits:
            pai = git(repo, "rev-parse", f"{csha}^").strip()
            if not pai:
                descartes["commit_sem_pai"] += 1
                continue
            depois = git(repo, "show", f"{csha}:{caminho}")
            antes = git(repo, "show", f"{pai}:{caminho}")
            if not depois:
                descartes["blob_depois_ilegivel"] += 1
                continue
            if not antes:
                descartes["sem_blob_antes"] += 1
                continue
            if GERADO.search(antes[:400]) or GERADO.search(depois[:400]):
                descartes["parece_gerado"] += 1
                continue
            if LICENCA_ALHEIA.search(antes[:400]) or LICENCA_ALHEIA.search(
                depois[:400]
            ):
                descartes["cabecalho_de_terceiro"] += 1
                continue
            pares.append(
                {
                    "repo": str(repo),
                    "identidade_repo": ident,
                    "commit": csha,
                    "caminho": caminho,
                    "antes": antes,
                    "depois": depois,
                }
            )
            n_commit += 1
        stats["pares"] += n_commit

    # dedup por (caminho, sha do blob depois) e por identidade do repo
    vistos: dict = {}
    for p in pares:
        chave = (
            p["identidade_repo"],
            p["caminho"],
            hashlib.sha256(p["depois"].encode("utf-8", "replace")).hexdigest(),
        )
        vistos.setdefault(chave, p)
    if len(vistos) < len(pares):
        descartes["duplicata_repo_caminho_sha"] += len(pares) - len(vistos)
    unicos = list(vistos.values())

    # sobreposicao com o semeado, por sha de CONTEUDO
    shas_sem = shas_do_semeado()
    sem_overlap = []
    for p in unicos:
        if (
            hashlib.sha256(p["depois"].encode("utf-8", "replace")).hexdigest()
            in shas_sem
        ):
            descartes["sha_ja_no_semeado"] += 1
            continue
        sem_overlap.append(p)

    print("=== estrato git ===")
    for k, v in sorted(stats.items()):
        print(f"  {k:24s} {v}")
    print(
        f"  pares coletados: {len(pares)} | unicos: {len(unicos)} | "
        f"apos sobreposicao: {len(sem_overlap)}"
    )
    print("  descartes:")
    for k, v in sorted(descartes.items()):
        print(f"     {k:32s} {v}")
    por_lang = Counter(os.path.splitext(p["caminho"])[1] for p in sem_overlap)
    print(f"  por extensao: {dict(por_lang)}")

    if not a.dry_run:
        a.saida.parent.mkdir(parents=True, exist_ok=True)
        a.saida.write_text(
            json.dumps(
                {
                    "stats": dict(stats),
                    "descartes": dict(descartes),
                    "n_sobreposicao": len(sem_overlap),
                    "pares": sem_overlap,
                },
                ensure_ascii=False,
            ),
            encoding="utf-8",
        )
        print(f"  escrito: {a.saida} ({a.saida.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
