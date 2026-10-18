#!/usr/bin/env python3
"""Read-only row-count snapshot of the real Touring databases (E2EHERM proof).

Opens every DB with `file:...?mode=ro` (uri=True); never writes. Output is JSON:
{"<db path>": {"size": N, "mtime": T, "tables": {"<table>": rows}}}
`--find KEY` reports memory_entries rows whose key LIKE 'KEY%'.
"""
import glob
import json
import os
import re
import sqlite3
import sys

HOME = os.path.expanduser("~")
ROOTS = [
    f"{HOME}/.claude/touring",
    f"{HOME}/.claude/.claude/touring",
    f"{HOME}/.touring",
    "/home/gabrielgadea/projects/touring/.claude/touring",
    "/home/gabrielgadea/projects/touring/.touring",
    "/home/gabrielgadea/projects/touring-wt-e2eherm/.claude/touring",
    "/home/gabrielgadea/projects/touring-wt-e2eherm/.touring",
]
IDENT = re.compile(r"^[A-Za-z0-9_]+$")
COUNT_SQL = "select count(*) from @T"


def discover():
    seen = set()
    for root in ROOTS:
        for pattern in ("*.db", "*/*.db"):
            for p in sorted(glob.glob(os.path.join(root, pattern))):
                rp = os.path.realpath(p)
                if rp not in seen and os.path.isfile(rp):
                    seen.add(rp)
                    yield rp


def open_ro(path):
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True, timeout=5)


def snapshot():
    out = {}
    for path in discover():
        st = os.stat(path)
        entry = {"size": st.st_size, "mtime": int(st.st_mtime), "tables": {}}
        try:
            con = open_ro(path)
            names = [
                r[0]
                for r in con.execute(
                    "select name from sqlite_master where type='table' and name not like 'sqlite_%'"
                )
            ]
            for t in names:
                if not IDENT.match(t):
                    entry["tables"][t] = "SKIP:odd-identifier"
                    continue
                try:
                    entry["tables"][t] = con.execute(COUNT_SQL.replace("@T", t)).fetchone()[0]
                except sqlite3.Error as e:  # virtual tables without module, etc.
                    entry["tables"][t] = "ERR:" + type(e).__name__
            con.close()
        except sqlite3.Error as e:
            entry["error"] = type(e).__name__ + ": " + str(e)
        out[path] = entry
    return out


def find(key_prefix):
    hits = []
    for path in discover():
        try:
            con = open_ro(path)
            has = con.execute("select 1 from sqlite_master where name='memory_entries'").fetchone()
            if has:
                for row in con.execute(
                    "select key, tier from memory_entries where key like ?", (key_prefix + "%",)
                ):
                    hits.append({"db": path, "key": row[0], "tier": row[1]})
            con.close()
        except sqlite3.Error:
            pass
    return hits


def diff(path_a, path_b):
    """Row-count differences between two snapshots (identical => empty `changed`)."""
    with open(path_a) as fa, open(path_b) as fb:
        a, b = json.load(fa), json.load(fb)
    changed, same_tables, touched_files = [], 0, []
    for db in sorted(set(a) | set(b)):
        ta = a.get(db, {}).get("tables", {})
        tb = b.get(db, {}).get("tables", {})
        if db not in a or db not in b:
            changed.append({"db": db, "table": "(db)", "before": db in a, "after": db in b})
            continue
        for t in sorted(set(ta) | set(tb)):
            if ta.get(t) != tb.get(t):
                changed.append({"db": db, "table": t, "before": ta.get(t), "after": tb.get(t)})
            else:
                same_tables += 1
        if a[db]["mtime"] != b[db]["mtime"] or a[db]["size"] != b[db]["size"]:
            touched_files.append(db)
    return {
        "dbs": len(set(a) | set(b)),
        "tables_identical": same_tables,
        "tables_changed": changed,
        "files_with_new_mtime_or_size": touched_files,
    }


if __name__ == "__main__":
    if len(sys.argv) >= 4 and sys.argv[1] == "--diff":
        print(json.dumps(diff(sys.argv[2], sys.argv[3]), indent=1))
    elif len(sys.argv) >= 3 and sys.argv[1] == "--find":
        print(json.dumps(find(sys.argv[2]), indent=1))
    else:
        print(json.dumps(snapshot(), indent=1, sort_keys=True))
