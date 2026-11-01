"""Runtime state committed to the (public) touring repo: which files, how big, and what the SQLite ones hold.

Read-only: SQLite files are opened with mode=ro&immutable=1 (lesson sqlite-so-leitura-abre-readonly). Only the
schema (table names) and the page count are read: no query interpolates a name.
Usage: estado_versionado.py <repo>
"""
import re
import sqlite3
import subprocess
import sys
from pathlib import Path

repo = Path(sys.argv[1])
ls = subprocess.run(["git", "-C", str(repo), "ls-files", "-z"], capture_output=True, text=True, check=True).stdout.split("\0")
alvo = [f for f in ls if f and (re.search(r"\.(db|sqlite3?|rkyv|db-wal|db-shm)$", f) or "/.claude/touring/" in f or "/.claude/data/" in f)]
print(f"arquivos de estado versionados: {len(alvo)}")
segredo = re.compile(rb"(AIza[0-9A-Za-z_\-]{30,}|sk-[A-Za-z0-9]{20,}|ghp_[A-Za-z0-9]{30,}|AKIA[0-9A-Z]{16}|xox[bap]-[0-9A-Za-z-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY)")
for f in alvo:
    p = repo / f
    tam = p.stat().st_size if p.exists() else -1
    linha = f"  {f}  {tam / 1024:.0f} KB"
    if p.exists():
        bruto = p.read_bytes()
        achados = sorted({m.group(0)[:8].decode("latin-1") + "…" for m in segredo.finditer(bruto)})
        linha += f" · padrões de segredo: {len(achados)} {achados[:3]} · /home/gabrielgadea: {bruto.count(b'/home/gabrielgadea')}"
        if f.endswith(".db"):
            try:
                con = sqlite3.connect(f"file:{p}?mode=ro&immutable=1", uri=True)
                tabs = [r[0] for r in con.execute("SELECT name FROM sqlite_master WHERE type = 'table'")]
                paginas = con.execute("PRAGMA page_count").fetchone()[0]
                linha += f" · {paginas} páginas · tabelas: {tabs[:14]}"
                con.close()
            except sqlite3.DatabaseError as e:
                linha += f" · sqlite: {e}"
    print(linha)
log = subprocess.run(["git", "-C", str(repo), "log", "--format=%h %cs %s", "-3", "--", *alvo[:20]], capture_output=True, text=True, check=False).stdout
print("últimos commits que tocaram:", log.strip().replace("\n", " | ")[:400])
ensina = subprocess.run(["git", "-C", str(repo), "grep", "-l", "-E", r"touring web\b", "--", "client", "docs/*.md", "crates"],
                        capture_output=True, text=True, check=False).stdout.split()
print("arquivos que ensinam `touring web`:", ensina[:6])
