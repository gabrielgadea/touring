"""V0b — which inventoried errors still LIVE in today's code (Gabriel, 08/10/2026: "precisamos corrigir tudo").

Reads ~/.claude/touring/vgp/erros_inventario.jsonl (inventario_erros.py) and gives every Python error that points at a
project file (buckets atribuído/projeto) exactly one status, each by a POSITIVE predicate — nothing becomes "fixed"
by elimination:
  vivo-estático      pyright on today's file reports the SAME name under the matching rule family
  vivo-sintaxe       `python -m py_compile` fails on today's file
  arquivo-removido   the file no longer exists
  linha-mudou        the source line the traceback printed no longer appears in the file
  teste-a-rerodar    AssertionError located in a test file — carries the pytest node id to rerun
  candidato          runtime error (KeyError, ValueError…) whose line is still intact — needs reproduction
  sem-evidência      none of the above could be decided (no code line, no static name) — listed, never dropped

Writes ~/.claude/touring/vgp/vivos.jsonl and prints aggregates. Usage: python3 vivos.py [--selftest]
"""

import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

HOME = Path.home()
VGP = HOME / ".claude/touring/vgp"
ROOT = {"analise": HOME / "projects/analise", "touring": HOME / "projects/touring"}
PYRIGHT = HOME / "projects/analise/.venv/bin/pyright"
# exception class → (regex extracting the name from the message, pyright rules that report that name)
STATIC = {
    "AttributeError": (r"has no attribute '(\w+)'", {"reportAttributeAccessIssue", "reportOptionalMemberAccess"}),
    "ImportError": (r"cannot import name '(\w+)'", {"reportAttributeAccessIssue", "reportPrivateImportUsage"}),
    "ModuleNotFoundError": (r"No module named '([\w.]+)'", {"reportMissingImports", "reportMissingModuleSource"}),
    "NameError": (r"name '(\w+)' is not defined", {"reportUndefinedVariable"}),
    "TypeError": (r"(?:unexpected keyword argument|required positional argument:?) '(\w+)'", {"reportCallIssue"}),
}
SYNTAX = {"SyntaxError", "IndentationError", "TabError"}


def project_root(path: Path) -> Path:
    """The project a file belongs to. A /tmp scratch file belongs to the session that wrote it — its scratchpad
    path names the project (`/tmp/claude-1000/-home-gabrielgadea-projects-analise/…`), and its imports resolve
    there (Gabriel, 08/10: what is written in /tmp is part of the work of every session)."""
    s = str(path)
    for name, root in ROOT.items():
        if s.startswith(str(root)) or f"-projects-{name}/" in s or s.endswith(f"-projects-{name}"):
            return root
    return ROOT["analise"]


def resolve(row: dict) -> Path:
    p = Path(row["file"])
    return p if p.is_absolute() else ROOT.get(row["project"], ROOT["analise"]) / p


def static_name(row: dict) -> str | None:
    spec = STATIC.get(row["kind"])
    m = re.search(spec[0], row["message"]) if spec else None
    return m.group(1).split(".")[0] if m else None


def pyright_diags(files: list[Path]) -> dict[str, list[tuple[str, str]]]:
    """{file: [(rule, message)]} for the given files, one pyright run per project root (its config applies)."""
    out: dict[str, list] = defaultdict(list)
    by_root = defaultdict(list)
    for f in files:
        by_root[project_root(f)].append(str(f))
    for root, group in by_root.items():
        for i in range(0, len(group), 200):
            p = subprocess.run([str(PYRIGHT), "--outputjson", *group[i:i + 200]], cwd=root, capture_output=True,
                               text=True, timeout=900)
            for d in json.loads(p.stdout or "{}").get("generalDiagnostics", []):
                out[d["file"]].append((d.get("rule") or "", d.get("message", "")))
    return out


def test_node(path: Path, line: int) -> str | None:
    """`file::func` of the test function enclosing `line` in today's file, if it is a test file."""
    if not path.name.startswith("test_"):
        return None
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()[:line]
    for text in reversed(lines):
        m = re.match(r"\s*def (test_\w+)", text)
        if m:
            return f"{path}::{m.group(1)}"
    return None


def _static_status(row: dict, path: Path, diags: dict) -> tuple[str, str] | None:
    """A verdict decidable without running the code: the name still flagged, or the file still not compiling."""
    name = static_name(row)
    if name and any(r in STATIC[row["kind"]][1] and f'"{name}"' in msg for r, msg in diags.get(str(path), [])):
        return "vivo-estático", name
    if row["kind"] in SYNTAX:
        bad = subprocess.run([sys.executable, "-m", "py_compile", str(path)], capture_output=True).returncode
        return ("vivo-sintaxe", "") if bad else ("linha-mudou", "compila hoje")
    return None


def classify(row: dict, path: Path, diags: dict) -> tuple[str, str]:
    if not path.exists():
        return "arquivo-removido", ""
    static = _static_status(row, path, diags)
    if static:
        return static
    code = row.get("code", "")
    body = {ln.strip() for ln in path.read_text(encoding="utf-8", errors="replace").splitlines()}
    if code and code not in body:
        return "linha-mudou", ""
    if row["kind"] == "AssertionError" and (node := test_node(path, row["line"])):
        return "teste-a-rerodar", node
    return ("candidato", "linha intacta") if code else ("sem-evidência", "sem linha de código no traceback")


# pytest exit codes: 0 passed · 1 a test failed · 2 interrupted · 3 internal error · 4 usage error · 5 none collected
PYTEST_RC = {0: "teste-passa-hoje", 1: "vivo-teste", 4: "teste-não-coletado", 5: "teste-não-coletado"}
_COV_FLAGS: dict[str, tuple[str, ...]] = {}


def coverage_off_flags(python: str) -> tuple[str, ...]:
    """`--no-cov` when that interpreter has pytest-cov, else nothing. `-p no:pytest_cov` was WRONG: it unloads the
    plugin while a project's `addopts = --cov=…` still passes the flag, and pytest rejects the whole call (rc 4 —
    found by the MiniMax in packages/antt-process-analysis, 08/10/2026). `--no-cov` keeps the plugin and turns it off."""
    if python not in _COV_FLAGS:
        has = subprocess.run([python, "-c", "import pytest_cov"], capture_output=True).returncode == 0
        _COV_FLAGS[python] = ("--no-cov",) if has else ()
    return _COV_FLAGS[python]


def rerun_test(node: str) -> tuple[str, str]:
    """Run one test function from its project root, with that project's interpreter; the exit code decides.
    `-k <func>` (not the node id) also finds tests nested in classes; the coverage plugin is off, because a
    project-wide fail-under turns a single passing test into rc 1."""
    path, func = node.split("::")
    root = project_root(Path(path))
    python = root / ".venv/bin/python3" if (root / ".venv/bin/python3").exists() else Path(sys.executable)
    cmd = [str(python), "-m", "pytest", "-q", "-p", "no:cacheprovider", *coverage_off_flags(str(python)), path, "-k", func]
    try:
        p = subprocess.run(cmd, cwd=root, capture_output=True, text=True, timeout=180)
    except subprocess.TimeoutExpired:
        return "teste-timeout", "180 s"
    tail = (p.stdout + p.stderr).strip().splitlines()[-1:] or [""]
    return PYTEST_RC.get(p.returncode, "teste-erro-de-execução"), f"rc={p.returncode} {node} · {tail[0][:120]}"


def selftest() -> int:
    """Positive control: an invented attribute in a real file is vivo-estático; negative: a real one is not."""
    probe = VGP / "selftest_probe.py"
    probe.write_text("from pathlib import Path\n\n\ndef f(p: Path) -> str:\n    return p.campo_inventado\n")
    row = {"project": "analise", "kind": "AttributeError", "message": "'Path' object has no attribute "
           "'campo_inventado'", "file": str(probe), "line": 5, "code": "return p.campo_inventado"}
    neg = {**row, "message": "'Path' object has no attribute 'name'", "code": "return p.name"}
    diags = pyright_diags([probe])
    ok = classify(row, probe, diags)[0] == "vivo-estático" and classify(neg, probe, diags)[0] != "vivo-estático"
    print(f"selftest: {'ok' if ok else 'FALHOU'}")
    return 0 if ok else 1


def _status_row(row: dict, path: Path, diags: dict) -> dict:
    status, detail = classify(row, path, diags)
    if status == "teste-a-rerodar" and "--rerodar-testes" in sys.argv:
        status, detail = rerun_test(detail)
    return {**row, "status": status, "detail": detail, "path": str(path)}


def main() -> int:
    if selftest():
        return 2
    if "--selftest" in sys.argv:  # only the controls: the full scan below REWRITES vivos.jsonl (it did, twice, 08/10)
        return 0
    return scan()


def scan() -> int:
    """Classify every attributable Python error of the inventory and rewrite vivos.jsonl."""
    rows = [json.loads(ln) for ln in (VGP / "erros_inventario.jsonl").open(encoding="utf-8")]
    py = [r for r in rows if r["lang"] == "python" and r["bucket"] in ("atribuído", "projeto")]
    paths = {id(r): resolve(r) for r in py}
    diags = pyright_diags(sorted({p for p in paths.values() if p.exists() and p.suffix == ".py"}))
    out = [_status_row(r, paths[id(r)], diags) for r in py]
    (VGP / "vivos.jsonl").write_text("".join(json.dumps(o, ensure_ascii=False) + "\n" for o in out), encoding="utf-8")
    report(out, paths, diags)
    return 0


def report(out: list[dict], paths: dict, diags: dict) -> None:
    print(f"erros Python em arquivo do projeto: {len(out)} distintos · arquivos existentes: "
          f"{len({p for p in paths.values() if p.exists()})} · com diagnóstico do pyright: {len(diags)}")
    print("status:", Counter(o["status"] for o in out).most_common())
    print("status em /tmp (trabalho de sessão):", Counter(o["status"] for o in out if o["path"].startswith("/tmp/")).most_common())
    # the project's .cache/ is session work too (Gabriel, 08/10); the home's ~/.cache is tool caches, never here
    print("status em <projeto>/.cache (trabalho de sessão):",
          Counter(o["status"] for o in out if re.search(r"/projects/[^/]+/\.cache/", o["path"])).most_common())
    print("vivo-estático por classe:", Counter(o["kind"] for o in out if o["status"] == "vivo-estático").most_common())
    print("candidato por classe:", Counter(o["kind"] for o in out if o["status"] == "candidato").most_common(8))
    print(f"detalhe: {VGP / 'vivos.jsonl'}")


if __name__ == "__main__":
    sys.exit(main())
