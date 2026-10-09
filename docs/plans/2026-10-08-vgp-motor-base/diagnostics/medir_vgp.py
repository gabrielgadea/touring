"""How often each language is EDITED, and how often an invented-API error follows, in analise transcripts.

Denominator = Edit/Write/MultiEdit calls (how often a rule about that language is needed), never file counts.
An error is ATTRIBUTED when the file its traceback/diagnostic names was edited earlier by the same transcript;
the rest is shown as "não atribuído" — never folded into the attributed count.

Usage: python3 medir_vgp.py [--since 2026-09-17]
"""

import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(sys.argv[sys.argv.index("--root") + 1]) if "--root" in sys.argv else (
    Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise")
SINCE = sys.argv[sys.argv.index("--since") + 1] if "--since" in sys.argv else "2026-09-17"
LANG = {".py": "python", ".rs": "rust", ".ts": "ts/js", ".tsx": "ts/js", ".js": "ts/js", ".jsx": "ts/js",
        ".sh": "bash", ".md": "markdown", ".json": "config", ".toml": "config", ".yaml": "config", ".yml": "config"}
# (language, class, pattern, group holding the file the diagnostic points at — None when it must be found nearby)
ERRORS = [
    ("python", "atributo", re.compile(r"AttributeError: .*has no attribute")),
    ("python", "kwarg/assinatura", re.compile(r"TypeError: .*(unexpected keyword argument|required positional argument|takes \d+ positional)")),
    ("python", "import de nome", re.compile(r"ImportError: cannot import name")),
    ("python", "nome indefinido", re.compile(r"NameError: name '")),
    ("rust", "campo/método", re.compile(r"error\[E0(609|599|560|026)\]")),
    ("rust", "assinatura", re.compile(r"error\[E0(061|308)\]")),
    ("rust", "import/nome", re.compile(r"error\[E0(432|425|412|433)\]")),
    ("ts/js", "propriedade/membro", re.compile(r"error TS(2339|2305|2551|2724)")),
    ("ts/js", "assinatura", re.compile(r"error TS(2345|2554|2353)")),
]
PY_FILE = re.compile(r'File "([^"]+\.py)", line \d+')
RS_FILE = re.compile(r"--> ([^\s:]+\.rs):\d+")
TS_FILE = re.compile(r"([\w./-]+\.(?:ts|tsx|js|jsx))[(:]\d+")
DENY = "python_pre_gate: the edit ADDS code that cannot run"
INLINE = re.compile(r'File "<(stdin|string)>", line \d+')


def _content_text(c):
    body = c.get("content")
    if isinstance(body, str):
        return body
    return " ".join(x.get("text", "") for x in body or [] if isinstance(x, dict))


def _files_named(text):
    return set(PY_FILE.findall(text)) | set(RS_FILE.findall(text)) | set(TS_FILE.findall(text))


def _attributed(named, edited):
    # a path-SEGMENT boundary and the `./` PREFIX only (08/10/2026): `e.endswith(n.lstrip("./"))` matched `a.py`
    # against `…/ba.py`, and lstrip strips a character SET (it ate the dot of `.cache/`, `.claude/`)
    def rel(n):
        return re.sub(r"^(?:\./)+", "", n)
    return any(n == e or e.endswith("/" + rel(n)) for n in named for e in edited)


def scan(f, st):
    edited, names = set(), {}
    for line in f.open(encoding="utf-8", errors="replace"):
        if '"tool_use"' not in line and '"tool_result"' not in line:
            continue
        rec = json.loads(line)
        if rec.get("timestamp", "") < SINCE:
            continue
        for c in (rec.get("message") or {}).get("content") or []:
            if isinstance(c, dict):
                _one(c, edited, names, st)


def _one(c, edited, names, st):
    """A result counts only by the tool that produced it: an error is a Bash run that raised, a deny is an
    Edit/Write refused — a Read or grep that merely ECHOES either string is a mention, not an event."""
    if c.get("type") == "tool_use":
        names[c.get("id")] = c.get("name")
        if c.get("name") in ("Edit", "Write", "MultiEdit"):
            path = str((c.get("input") or {}).get("file_path", ""))
            st["edits"][LANG.get(Path(path).suffix, "outro")] += 1
            edited.add(path)
    elif c.get("type") == "tool_result":
        _result(c, names.get(c.get("tool_use_id")), edited, st)


def _result(c, tool, edited, st):
    text = _content_text(c)
    st["results"][tool] += 1
    if tool in ("Edit", "Write", "MultiEdit"):
        st["denies"] += DENY in text
        return
    if tool == "Bash":
        for lang, cls, pat in ERRORS:
            if pat.search(text):
                key = ("atribuído" if _attributed(_files_named(text), edited)
                       else "inline" if INLINE.search(text) else "não atribuído")
                st["errors"][(lang, cls)][key] += 1


def main():
    # heterogeneous record (counters + ints): a bare `dict` keeps pyright from inferring the union of value types
    # and flagging every `+= 1` / `.values()` (9 diagnostics, pre-existing, cleared 08/10/2026)
    st: dict = {"edits": Counter(), "errors": defaultdict(Counter), "denies": 0, "files": 0, "results": Counter()}
    for f in ROOT.rglob("*.jsonl"):
        if f.stat().st_mtime < __import__("datetime").datetime.fromisoformat(SINCE).timestamp():
            continue
        st["files"] += 1
        scan(f, st)
    total = sum(st["edits"].values())
    print(f"transcripts {st['files']} desde {SINCE} · edições {total}")
    for lang, n in st["edits"].most_common():
        print(f"  {lang:9} {n:6}  {n / total:6.1%}")
    print(f"controle: resultados por ferramenta {dict(st['results'].most_common(5))}")
    print(f"negações do python_pre_gate (só em resultado de Edit/Write/MultiEdit): {st['denies']}")
    print("erros de API inventada (só em resultado de Bash) — atribuído = arquivo editado antes na mesma sessão")
    for (lang, cls), cnt in sorted(st["errors"].items()):
        print(f"  {lang:6} {cls:20} atribuído {cnt['atribuído']:5} · inline {cnt['inline']:5}"
              f" · não atribuído {cnt['não atribuído']:5}")


if __name__ == "__main__":
    main()
