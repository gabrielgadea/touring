"""F4b — cost side of transcript-mined lessons: stock in memory.db and injected bytes.

Counts, over the main transcripts, every injected text block that carries the
`Past failure [` marker (cli_suggester.rs collect_memory_lessons output) and its bytes,
plus the stock of `outcome:%:transcript-%:failure` rows in the project's memory.db.
Aggregates only.
"""
import glob
import json
import os
import re
import sqlite3

ROOT = os.path.expanduser("~/.claude/projects/" + os.getcwd().replace("/", "-"))  # run from the project root
DB = os.path.join(os.getcwd(), ".claude", "touring", "memory.db")
MARK = "Past failure ["
LINE = re.compile(r"[^\n]*Past failure \[[^\n]*")


def strings(node):
    if isinstance(node, str):
        yield node
    elif isinstance(node, dict):
        for v in node.values():
            yield from strings(v)
    elif isinstance(node, list):
        for v in node:
            yield from strings(v)


def scan(path):
    lines = nbytes = 0
    keys = set()
    with open(path, encoding="utf-8", errors="ignore") as fh:
        for raw in fh:
            if MARK not in raw:
                continue
            try:
                rec = json.loads(raw)
            except ValueError:
                continue
            for s in strings(rec):
                for m in LINE.findall(s):
                    lines += 1
                    nbytes += len(m.encode())
                    k = re.search(r"Past failure \[([^\]]+)\]", m)
                    if k:
                        keys.add(k.group(1))
    return lines, nbytes, keys


def main():
    files = sorted(glob.glob(os.path.join(ROOT, "*.jsonl")))
    tot_l = tot_b = sess = 0
    allkeys = set()
    for f in files:
        n, b, k = scan(f)
        tot_l += n
        tot_b += b
        sess += n > 0
        allkeys |= k
    stock = None
    try:
        con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
        stock = con.execute(
            "select count(*) from memory_entries where key like 'outcome:%:transcript-%:failure'").fetchone()[0]
        con.close()
    except sqlite3.Error as e:
        stock = f"unavailable: {type(e).__name__}"
    print(json.dumps({
        "sessions": len(files), "sessions_with_past_failure_lines": sess,
        "past_failure_lines": tot_l, "past_failure_bytes": tot_b,
        "distinct_lesson_keys_injected": len(allkeys),
        "transcript_lessons_in_memory_db": stock,
    }))


if __name__ == "__main__":
    main()
