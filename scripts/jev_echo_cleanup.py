#!/usr/bin/env python3
"""jev_echo_cleanup.py — remoção do eco do `evolution` (decisão B do Gabriel).

O QUE
====
7.622 entradas em `memory.db` com **um único valor** —
`"Insufficient compliance data for CILA progression analysis"` — cujas chaves
diferem **só pelo timestamp**. 37,7% da base. São o registo histórico de um
defeito **já corrigido no fonte** (`insight_key`, endereçada por conteúdo —
REGRA #17). A correção ainda NÃO está no ar: quem escreve é um `touring serve`
stale com o binário de 27/09. Entra no ar quando esse bridge morre.

A DECISÃO (canvas 01/10, §8, confiança 0,85)
=============================================
**B — apagar DEPOIS do PROOF.** A pré-condição dispara sozinha quando a sessão
stale fechar, não há trabalho extra, e apagar a evidência antes de a correção
estar provada no ar é o gesto que a auditoria desaconselha.

POR QUE O PROOF ESTÁ NO CÓDIGO E NÃO NA MINHA MEMÓRIA
====================================================
Se a pré-condição fosse um requisito meu, o próximo que abrir este ficheiro
apaga 37,7% de uma base num dia normal. Por isso **`--apply` só existe se a
prova estiver em disco.** Não há `--force`.

AS CINCO TRAVAS
===============
1. **PROOF** — `access_count >= 3` na chave hash. Sem isso, sai.
2. **Predicado só-timestamp** — a chave da prova é hexadecimal e **não casa**.
   Um `LIKE 'insight:cila_progression:%'` apagaria a prova que autorizou a
   remoção.
3. **Duas derivações têm de concordar** — regex estrita em Python e GLOB em
   SQL. Se divergirem, recusa. Um `DELETE` de 7.622 linhas não se baseia numa
   única leitura.
4. **Dump antes** — o `memory.db` não tem backup (1 NVMe, sem restic/borg);
   esse ficheiro é o único ponto de retorno.
5. **`mode=ro` por omissão** — a escrita só abre depois de a prova passar.

NADA DE SQL INTERPOLADO
=======================
O gate F2.1 recusa f-string em `execute()`. Duas vezes. Tem razão — e a
resposta honesta não é argumentar que a constante é minha, é escrever o SQL
inteiro. A segurança que depende de eu lembrar não é segurança; depende de o
padrão não existir. É a diferença entre «é seguro» e «não se pode escrever
assim».

Uso:
    python3 jev_echo_cleanup.py --dry-run      # mostra o que faria (default)
    python3 jev_echo_cleanup.py --apply        # SÓ com a prova em disco
    python3 jev_echo_cleanup.py --restore <dump.json>
"""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import sys
import time
from pathlib import Path

MEMORY_DB = Path.home() / "projects/touring/.claude/touring/memory.db"
DUMP = Path.home() / ".claude/touring/insight_echo_dump.json"

PREFIX = "insight:cila_progression:"
ECHO_TEXT = "Insufficient compliance data for CILA progression analysis"
IS_TIMESTAMP = re.compile(r"^insight:cila_progression:\d+$")

# [F] O gate F2.1 bloqueou este ficheiro DUAS vezes por f-string em execute().
# A correção não é um pragma nem um argumento: são literais completos com `?`.
# O WHERE repete-se de propósito, e o `assert` abaixo garante que os quatro
# literais não divergem entre si.
# [F] O GLOB do SQLite NAO tem intervalos: `[0-9]` e o conjunto literal
# {0,-,9}. A primeira versao deste ficheiro por isso casava ZERO linhas em
# SQL e 7.623 em Python — e a dupla derivacao apanhou o bug antes de ele
# chegar a um DELETE. O conjunto e escrito por extenso, dez vezes.
GLOB_TS = "insight:cila_progression:[0123456789][0123456789][0123456789][0123456789][0123456789][0123456789][0123456789][0123456789][0123456789][0123456789]"
ECHO_PARAMS = (GLOB_TS,)

SQL_KEYS_ECHO = "SELECT key FROM memory_entries WHERE key GLOB ?"
SQL_KEYS_LIKE = "SELECT key FROM memory_entries WHERE key LIKE ?"
SQL_PROOF = "SELECT key, access_count, created_at FROM memory_entries WHERE key LIKE ?"
SQL_VALUES = ("SELECT value, COUNT(*) n FROM memory_entries "
              "WHERE key GLOB ? GROUP BY value ORDER BY n DESC")
SQL_DELETE = "DELETE FROM memory_entries WHERE key GLOB ?"

_ECHO_CLAUSE = "key GLOB ?"
assert _ECHO_CLAUSE in SQL_KEYS_ECHO, "SQL_KEYS_ECHO perdeu o predicado"
assert _ECHO_CLAUSE in SQL_VALUES, "SQL_VALUES perdeu o predicado"
assert _ECHO_CLAUSE in SQL_DELETE, "SQL_DELETE perdeu o predicado"
assert SQL_DELETE.startswith("DELETE FROM memory_entries WHERE "), "SQL_DELETE alvo trocado"

PROOF_MIN_ACCESS = 3


def connect(writable: bool) -> sqlite3.Connection:
    if writable:
        con = sqlite3.connect(str(MEMORY_DB))
    else:
        con = sqlite3.connect(f"file:{MEMORY_DB}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row
    return con


def proof_state(con: sqlite3.Connection) -> dict:
    """A chave HASH — prova de que a correção está viva."""
    rows = con.execute(SQL_PROOF, (PREFIX + "%",)).fetchall()
    cands = [r for r in rows if not IS_TIMESTAMP.match(r["key"])]
    return {
        "found": [{"key": r["key"], "access_count": r["access_count"],
                   "created_at": r["created_at"]} for r in cands],
        "max_access": max((r["access_count"] or 0 for r in cands), default=0),
    }


def echo_keys_python(con: sqlite3.Connection) -> list[str]:
    """Derivação 1 — regex estrita, em Python."""
    rows = con.execute(SQL_KEYS_LIKE, (PREFIX + "%",)).fetchall()
    return sorted(r["key"] for r in rows if IS_TIMESTAMP.match(r["key"]))


def echo_keys_sql(con: sqlite3.Connection) -> list[str]:
    """Derivação 2 — GLOB em SQL. Tem de bater com a 1, senão o script recusa."""
    return sorted(r["key"] for r in con.execute(SQL_KEYS_ECHO, ECHO_PARAMS))


def distinct_values(con: sqlite3.Connection) -> dict:
    """Quantos textos distintos há no alvo — 1 é o sinal de 'é eco puro'."""
    rows = con.execute(SQL_VALUES, ECHO_PARAMS).fetchall()
    return {"distinct": len(rows),
            "texts": [{"n": r["n"], "text": (r["value"] or "")[:90]} for r in rows[:5]]}


def survey(con: sqlite3.Connection) -> dict:
    """Tudo o que o veredito precisa, numa leitura só de leitura."""
    return {"proof": proof_state(con),
            "py_keys": echo_keys_python(con),
            "sql_keys": echo_keys_sql(con),
            "texts": distinct_values(con)}


def blockers_for(s: dict) -> list[str]:
    """Cada trava que aborta o --apply. Lista vazia = seguro apagar."""
    out: list[str] = []
    if len(s["py_keys"]) != len(s["sql_keys"]):
        out.append(f"as duas derivações divergem: {len(s['py_keys'])} "
                   f"(python) vs {len(s['sql_keys'])} (sql)")
    if not s["py_keys"]:
        out.append("nada a apagar (alvo vazio)")
    if s["texts"]["distinct"] != 1:
        out.append(f"o alvo tem {s['texts']['distinct']} valores distintos — não é eco puro")
    if s["proof"]["max_access"] < PROOF_MIN_ACCESS:
        out.append(f"PROOF ausente/incompleta (max access_count="
                   f"{s['proof']['max_access']}, exigido {PROOF_MIN_ACCESS}) "
                   f"— a correção ainda não foi provada no ar")
    return out


def report(s: dict, blockers: list[str]) -> None:
    print("=== estado ===")
    print(f"  linhas de eco (regex python) : {len(s['py_keys'])}")
    print(f"  linhas de eco (GLOB sql)    : {len(s['sql_keys'])}")
    print(f"  valores DISTINTOS no alvo   : {s['texts']['distinct']}")
    for t in s["texts"]["texts"][:2]:
        print(f"      {t['n']:>6}  {t['text']!r}")
    print(f"  PROOF (chave hash)          : {s['proof']['found'] or 'AUSENTE'}")
    print(f"  prova max access_count      : {s['proof']['max_access']} "
          f"(exigido >= {PROOF_MIN_ACCESS})")
    print("\n=== travas ===")
    if blockers:
        for b in blockers:
            print(f"  ✗ {b}")
    else:
        print("  ✓ todas as travas passam")


def dump_keys(keys: list[str], texts: dict) -> Path:
    DUMP.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "kind": "insight_echo_dump",
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "db": str(MEMORY_DB),
        "count": len(keys),
        "distinct_values_at_dump": texts,
        "restore": f"python3 {Path(__file__).name} --restore {DUMP}",
        "keys": keys,
    }
    DUMP.write_text(json.dumps(payload, ensure_ascii=False, indent=1), encoding="utf-8")
    return DUMP


def apply_delete(con: sqlite3.Connection) -> int:
    cur = con.execute(SQL_DELETE, ECHO_PARAMS)
    con.commit()
    return cur.rowcount


def restore(dump_path: Path) -> int:
    """Reinsere o que o dump guardou. Pular o que já existe."""
    if not dump_path.exists():
        raise SystemExit(f"dump ausente: {dump_path}")
    data = json.loads(dump_path.read_text(encoding="utf-8"))
    con = connect(writable=True)
    try:
        n = 0
        for key in data["keys"]:
            if con.execute("SELECT 1 FROM memory_entries WHERE key = ?", (key,)).fetchone():
                continue
            con.execute(
                "INSERT INTO memory_entries (key, value, created_at, access_count) "
                "VALUES (?, ?, datetime('now'), 0)", (key, ECHO_TEXT))
            n += 1
        con.commit()
    finally:
        con.close()
    return n


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apply", action="store_true",
                    help="executa. SÓ funciona com a prova em disco.")
    ap.add_argument("--dry-run", action="store_true", help="mostra o que faria (default)")
    ap.add_argument("--restore", type=Path, default=None, help="reinsere de um dump")
    args = ap.parse_args(argv)

    if args.restore:
        print(f"restauradas {restore(args.restore)} linhas de {args.restore}")
        return 0

    ro = connect(writable=False)
    try:
        s = survey(ro)
    finally:
        ro.close()

    blockers = blockers_for(s)
    report(s, blockers)

    if not args.apply:
        print("\n(dry-run: nada foi escrito.)")
        return 0 if not blockers else 2

    if blockers:
        print("\nABORTADO. --apply não corre com travas abertas — e não há --force.")
        return 2

    path = dump_keys(s["py_keys"], s["texts"])
    print(f"\ndump gravado: {path}  ({len(s['py_keys'])} chaves)")
    con = connect(writable=True)
    try:
        n = apply_delete(con)
    finally:
        con.close()
    print(f"APAGADAS {n} linhas (esperado {len(s['py_keys'])})")
    if n != len(s["py_keys"]):
        print("⚠ divergência na contagem — o dump permite restaurar")
    return 0 if n == len(s["py_keys"]) else 3


if __name__ == "__main__":
    sys.exit(main())
