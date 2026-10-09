"""V0a — inventory of EVERY error raised by a Bash run in Claude Code transcripts (Gabriel, 08/10/2026:
"a contagem deve considerar todos os erros, pois precisamos corrigir tudo").

An error counts only when it appears in the result of a Bash call (a Read/grep that echoes an old traceback is a
mention, not an event). Formats: Python traceback, pytest location line, rustc `error[E…]`, tsc `TS…`. Every
failed Bash result (is_error) that no format recognises is counted as "não classificado" — visible, never folded
into a class. Each error gets one bucket, by a positive predicate:
  atribuído      — a frame/diagnostic names a file the same transcript edited earlier (Edit/Write/MultiEdit)
  inline         — the deepest frame is <stdin>/<string> (code written inside the Bash or `touring run --code`)
  projeto        — the deepest frame is a project file not edited earlier in that transcript
  externo        — every frame is outside the projects (site-packages, stdlib, registry)

Raw rows (unique by identity, with occurrences) go to ~/.claude/touring/vgp/erros_inventario.jsonl — outside any
repository, since messages may carry case data. Only aggregates are printed.

Usage: python3 inventario_erros.py [--since 2026-09-17] [--selftest]
"""

import json
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

HOME = Path.home()
ROOTS = [HOME / ".claude/projects/-home-gabrielgadea-projects-analise",
         HOME / ".claude/projects/-home-gabrielgadea-projects-touring"]
OUT = HOME / ".claude/touring/vgp/erros_inventario.jsonl"
SINCE = sys.argv[sys.argv.index("--since") + 1] if "--since" in sys.argv else "2026-09-17"
WRITERS = ("Edit", "Write", "MultiEdit")
EXTERNAL = re.compile(r"site-packages|dist-packages|/usr/lib|/usr/local/lib|/\.venv/lib|/\.local/share/uv/|"
                      r"\.cargo/registry|/rustc/|/\.rustup/|node_modules")

TB_START = "Traceback (most recent call last):"
PY_FRAME = re.compile(r'^\s*File "([^"]+)", line (\d+)(?:, in (.+))?$')
PY_EXC = re.compile(r"^((?:[A-Za-z_][\w]*\.)*[A-Za-z_]\w*(?:Error|Exception|Exit|Interrupt|Warning|Iteration|Fault))(?::\s?(.*))?$")
PYTEST_LOC = re.compile(r"^([\w./-]+\.py):(\d+): ((?:[A-Za-z_]\w*\.)*[A-Za-z_]\w*(?:Error|Exception))$", re.M)
RS_ERR = re.compile(r"^error(?:\[(E\d{4})\])?: (.+)\n\s*--> ([^\s:]+):(\d+):\d+", re.M)
TS_ERR = re.compile(r"([\w./-]+\.(?:ts|tsx|js|jsx))[(:](\d+)[,:]\d+\)?:? -? ?error (TS\d+): (.+)")


def _norm(msg: str) -> str:
    """Message with volatile parts (numbers, hex addresses, quoted temp paths) folded, for identity."""
    msg = re.sub(r"0x[0-9a-f]+", "0x…", msg)
    return re.sub(r"\d+", "N", msg)[:200]


def _frame_line(line: str, frames: list) -> bool:
    """Consume a frame header or the source line Python prints under it; True when the line was one of them."""
    m = PY_FRAME.match(line)
    if m:
        frames.append((m.group(1), int(m.group(2)), m.group(3) or "", ""))
        return True
    if frames and not frames[-1][3] and line.startswith("    ") and line.strip():
        frames[-1] = (*frames[-1][:3], line.strip())
        return True
    return False


def _traceback(block: str) -> dict | None:
    frames: list = []
    for line in block.splitlines():
        if _frame_line(line, frames):
            continue
        e = PY_EXC.match(line.strip()) if not line.startswith((" ", "\t")) else None
        if e and frames:
            return {"lang": "python", "kind": e.group(1).split(".")[-1], "message": (e.group(2) or "").strip(),
                    "frames": frames}
    return None


def _python(text: str) -> list[dict]:
    return [tb for block in text.split(TB_START)[1:] if (tb := _traceback(block))]


def _pytest(text: str) -> list[dict]:
    return [{"lang": "python", "kind": m.group(3).split(".")[-1], "message": "(pytest)",
             "frames": [(m.group(1), int(m.group(2)), "")]} for m in PYTEST_LOC.finditer(text)]


def _rust(text: str) -> list[dict]:
    return [{"lang": "rust", "kind": m.group(1) or "error", "message": m.group(2),
             "frames": [(m.group(3), int(m.group(4)), "")]} for m in RS_ERR.finditer(text)]


def _ts(text: str) -> list[dict]:
    return [{"lang": "ts/js", "kind": m.group(3), "message": m.group(4),
             "frames": [(m.group(1), int(m.group(2)), "")]} for m in TS_ERR.finditer(text)]


PY_SYNTAX = re.compile(r'^\s*File "([^"]+)", line (\d+)\n(?:.*\n){0,3}?\s*\^+\s*\n(SyntaxError|IndentationError|TabError): (.+)', re.M)
# Failed Bash results with no compiler/interpreter diagnostic: each class has a POSITIVE predicate, in order.
RESIDUE = [
    ("hook/guard negou", re.compile(r"hook (error|blocked)|PreToolUse|blocked by|BLOCK(ED)?\b|NEGAD|denied by|guard|^\[G\d+[a-z]? ", re.I | re.M)),
    ("permissão do Claude Code negou", re.compile(r"^(Force operation|Force without confirmation|Recursive deletion|Local branch deletion|This command requires approval|This Bash command contains multiple operations)|destructive recursive delete", re.M)),
    ("entrada de ferramenta inválida", re.compile(r"InputValidationError|tool_use_error")),
    ("touring run falhou", re.compile(r"ERROR\S*\s+\S*touring\S*:\S*\s+run fail|\"success\":\s*false")),
    ("sintaxe do shell", re.compile(r"syntax error near unexpected token|unexpected EOF while looking for matching")),
    ("traceback cortado (sem a linha da exceção)", re.compile(r"Traceback \(most recent call last\):")),
    ("is_error com saída de sucesso (outro comando do pipeline falhou)", re.compile(r"^(ok|All checks passed!|=+ \d+ passed)", re.M)),
    ("pytest falhou (sumário)", re.compile(r"^(FAILED |ERROR )\S+|=+ .*\d+ (failed|error)", re.M)),
    ("cargo falhou (sem diagnóstico no recorte)", re.compile(r"error: (could not compile|test failed|aborting)|error\[E\d{4}\]")),
    ("timeout", re.compile(r"timed? ?out|Timeout|exit code 124\b|Exit code 124\b", re.I)),
    ("comando inexistente", re.compile(r"command not found|No such file or directory \(os error 2\)|not recognized")),
    ("arquivo/caminho inexistente", re.compile(r"No such file or directory|cannot access|does not exist", re.I)),
    ("jq/json", re.compile(r"jq: error|parse error|Expecting value|JSONDecodeError")),
    ("permissão/sandbox", re.compile(r"Permission denied|Operation not permitted|landlock|EACCES", re.I)),
    ("git", re.compile(r"^(fatal|error): .*(git|repository|pathspec|branch)|^fatal:", re.M)),
    ("CLI touring/uv/outros (error:)", re.compile(r"^(error|Error|ERROR)[:\s]", re.M)),
    ("saída vazia com exit≠0 (grep/test sem casamento)", re.compile(r"^\s*Exit code [1-9]\d*\s*$")),
]


def _syntax(text: str) -> list[dict]:
    return [{"lang": "python", "kind": m.group(3), "message": m.group(4),
             "frames": [(m.group(1), int(m.group(2)), "")]} for m in PY_SYNTAX.finditer(text)]


def residue_class(text: str) -> str:
    for name, pat in RESIDUE:
        if pat.search(text):
            return name
    lines = [ln.strip() for ln in text.splitlines() if ln.strip() and not re.fullmatch(r"Exit code \d+", ln.strip())]
    return "não classificado: " + _norm(lines[0] if lines else "(só Exit code)")[:70]


def parse_errors(text: str) -> list[dict]:
    return _python(text) + _syntax(text) + _pytest(text) + _rust(text) + _ts(text)


def _matches_edited(path: str, edited: set[str]) -> bool:
    # strip the `./` PREFIX only: `lstrip("./")` strips a character SET and ate the dot of `.cache/`, `.claude/`,
    # `.touring/` — a relative frame in a dot directory was attributed to nobody (08/10/2026)
    rel = re.sub(r"^(?:\./)+", "", path)
    return any(path == e or e.endswith("/" + rel) for e in edited)


def bucket(frames: list, edited: set[str]) -> tuple[str, tuple | None]:
    """(bucket, the frame that decided it). The deepest frame decides inline/projeto; any edited frame → atribuído."""
    for f in reversed(frames):
        if _matches_edited(f[0], edited):
            return "atribuído", f
    own = [f for f in frames if not EXTERNAL.search(f[0])]
    if not own:
        return "externo", frames[-1] if frames else None
    deepest = own[-1]
    return ("inline" if deepest[0] in ("<stdin>", "<string>") else "projeto"), deepest


def _text(c: dict) -> str:
    body = c.get("content")
    return body if isinstance(body, str) else " ".join(x.get("text", "") for x in body or [] if isinstance(x, dict))


def unwrap_run(text: str) -> str:
    """A `touring run` result is a JSON envelope whose stderr/stdout carry the program's traceback with escaped
    newlines; append them decoded so the language parsers see the error the program raised."""
    start = text.find("{")
    if start < 0 or '"stderr"' not in text:
        return text
    try:
        env, _ = json.JSONDecoder().raw_decode(text[start:])
    except json.JSONDecodeError:
        return text
    if not isinstance(env, dict):
        return text
    return "\n".join([text, str(env.get("stderr") or ""), str(env.get("stdout") or "")])


class Inventory:
    def __init__(self):
        self.rows: dict[tuple, dict] = {}
        self.residue: Counter = Counter()
        self.bash_results = 0
        self.transcripts = 0
        self._cmds: dict = {}  # tool_use id → the Bash command that produced the result (the ORIGINAL invocation)

    def scan(self, f: Path, project: str) -> None:
        edited, names = set(), {}
        for line in f.open(encoding="utf-8", errors="replace"):
            if '"tool_use"' not in line and '"tool_result"' not in line:
                continue
            rec = json.loads(line)
            if rec.get("timestamp", "") < SINCE:
                continue
            for c in (rec.get("message") or {}).get("content") or []:
                if isinstance(c, dict):
                    self._one(c, rec, edited, names, project)

    def _one(self, c, rec, edited, names, project) -> None:
        if c.get("type") == "tool_use":
            names[c.get("id")] = c.get("name")
            self._cmds[c.get("id")] = str((c.get("input") or {}).get("command", ""))
            if c.get("name") in WRITERS:
                edited.add(str((c.get("input") or {}).get("file_path", "")))
        elif c.get("type") == "tool_result" and names.get(c.get("tool_use_id")) == "Bash":
            self.bash_results += 1
            errs = parse_errors(unwrap_run(_text(c)))
            if c.get("is_error") and not errs:
                self.residue[residue_class(_text(c))] += 1
            for e in errs:
                self._add(e, rec, edited, project, self._cmds.get(c.get("tool_use_id"), ""), c.get("tool_use_id", ""))

    def _add(self, e, rec, edited, project, command: str = "", tool_use_id: str = "") -> None:
        b, frame = bucket(e["frames"], edited)
        where = (frame[0], frame[1]) if frame else ("", 0)
        key = (project, e["lang"], e["kind"], _norm(e["message"]), where[0] if b != "inline" else "<inline>",
               where[1] if b in ("atribuído", "projeto") else 0)
        row = self.rows.setdefault(key, {"project": project, "lang": e["lang"], "kind": e["kind"],
                                         "message": e["message"][:300], "file": where[0], "line": where[1],
                                         "func": frame[2] if frame else "", "bucket": b, "occurrences": 0,
                                         "code": frame[3] if frame and len(frame) > 3 else "",
                                         # the ORIGINAL invocation, WHOLE (a [:600] cut fabricated "unbalanced
                                         # quotes" in 27 of the 98 V0c items, 08/10/2026) + its locator
                                         "command": command, "tool_use_id": tool_use_id,
                                         "first": rec.get("timestamp", ""), "session": rec.get("sessionId", "")})
        row["occurrences"] += 1
        row["last"] = rec.get("timestamp", "")


def selftest() -> int:
    """Positive control per format, negatives for prose and an echo of a non-error."""
    py = 'Traceback (most recent call last):\n  File "/p/x.py", line 3, in f\n    a.b\nAttributeError: \'A\' object has no attribute \'b\'\n'
    inl = 'Traceback (most recent call last):\n  File "<stdin>", line 9, in <module>\nKeyError: \'c\'\n'
    rs = "error[E0609]: no field `zz` on type `S`\n  --> crates/a/src/lib.rs:12:7\n"
    ts = "src/app.tsx(4,9): error TS2339: Property 'q' does not exist on type 'P'.\n"
    pt = "tests/test_a.py:41: AssertionError\n"
    checks = {
        "python": [e["kind"] for e in parse_errors(py)] == ["AttributeError"],
        "inline": bucket(parse_errors(inl)[0]["frames"], set())[0] == "inline",
        "atribuído": bucket(parse_errors(py)[0]["frames"], {"/p/x.py"})[0] == "atribuído",
        "projeto": bucket(parse_errors(py)[0]["frames"], set())[0] == "projeto",
        "rust": [(e["kind"], e["frames"][0][1]) for e in parse_errors(rs)] == [("E0609", 12)],
        "ts": [e["kind"] for e in parse_errors(ts)] == ["TS2339"],
        # a relative frame inside a DOT directory (.cache/.claude/.touring) is the session's own file: `lstrip("./")`
        # ate the dot and attributed it to nobody (08/10/2026, Gabriel: "faça uma verificação em .cache/")
        "relativo em dir-ponto": bucket([(".cache/v/s.py", 3, "<module>", "x = y")], {"/p/.cache/v/s.py"})[0] == "atribuído",
        "sufixo sem fronteira": bucket([("a.py", 1, "f", "x")], {"/p/ba.py"})[0] != "atribuído",
        "pytest": [e["kind"] for e in parse_errors(pt)] == ["AssertionError"],
        "negativo prosa": parse_errors("the Error: word in prose, no traceback\nall good\n") == [],
        "syntax": [e["kind"] for e in parse_errors('  File "<stdin>", line 2\n    x = (\n        ^\nSyntaxError: \'(\' was never closed\n')] == ["SyntaxError"],
        "resíduo hook": residue_class("PreToolUse:Bash hook error: G9 nega sed -i") == "hook/guard negou",
        "touring run": [e["kind"] for e in parse_errors(unwrap_run("INFO x\n" + json.dumps({"stderr": inl, "success": False})))] == ["KeyError"],
        "resíduo vazio": residue_class("Exit code 1") == "saída vazia com exit≠0 (grep/test sem casamento)",
        "negativo externo": bucket([("/x/.venv/lib/python3.12/site-packages/m.py", 1, "")], set())[0] == "externo",
    }
    for k, v in checks.items():
        print(f"  {'ok ' if v else 'FALHOU'} {k}")
    return 0 if all(checks.values()) else 1


def main() -> int:
    if selftest():
        print("selftest FALHOU — inventário não roda com instrumento cego")
        return 2
    if "--selftest" in sys.argv:
        return 0
    inv, since_ts = Inventory(), datetime.fromisoformat(SINCE).timestamp()
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime >= since_ts:
                inv.transcripts += 1
                inv.scan(f, root.name.rsplit("-", 1)[-1])
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in inv.rows.values()), encoding="utf-8")
    report(inv)
    return 0


def report(inv: Inventory) -> None:
    rows = list(inv.rows.values())
    occ = sum(r["occurrences"] for r in rows)
    print(f"transcripts {inv.transcripts} · resultados de Bash {inv.bash_results} · erros {occ} ocorrências em "
          f"{len(rows)} distintos · Bash com is_error sem diagnóstico de linguagem: {sum(inv.residue.values())}")
    named = Counter({k: v for k, v in inv.residue.items() if not k.startswith("não classificado")})
    rest = Counter({k: v for k, v in inv.residue.items() if k.startswith("não classificado")})
    print("  resíduo por classe (predicado positivo):", named.most_common())
    print(f"  não classificado: {sum(rest.values())} em {len(rest)} primeiras linhas distintas; top:")
    for k, v in rest.most_common(15):
        print(f"     {v:4} {k[18:]}")
    by = Counter((r["project"], r["bucket"]) for r in rows)
    print("distintos por projeto × balde:", dict(sorted(by.items())))
    kinds = Counter((r["lang"], r["kind"]) for r in rows)
    print("classes mais frequentes (distintos):", kinds.most_common(25))
    print(f"inventário bruto: {OUT}")


if __name__ == "__main__":
    sys.exit(main())
