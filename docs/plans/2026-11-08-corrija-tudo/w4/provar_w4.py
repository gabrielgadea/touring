#!/usr/bin/env python3
"""W4 proof: the fixes of the corrija-tudo round, asked of the INSTALLED 30.4.105 toolchain.

Each check prints one JSON line {check, ok, detail}; exit 0 only when every check is ok.
Paths under $HOME are printed as ~ (public repository).
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HOME = str(Path.home())
BIN = Path(HOME) / ".touring/toolchains/30.4.105/bin"
TOURING = str(BIN / "touring")
HOOK = str(BIN / "touring-hook")
J17 = Path(os.environ.get("J17_CORPUS", "/tmp/vgp-j17/j17_hibrido_20261101c"))
results = []


def report(check: str, ok: bool, detail) -> None:
    line = {"check": check, "ok": ok, "detail": detail}
    results.append(line)
    print(json.dumps(line, ensure_ascii=False).replace(HOME, "~"))


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=300, **kw)


def last_json(text: str):
    for chunk in reversed(text.strip().splitlines()):
        try:
            return json.loads(chunk)
        except ValueError:
            continue
    try:
        return json.loads(text)
    except ValueError:
        return None


def verdict_of(out: dict | None) -> str:
    if not isinstance(out, dict):
        return "unparsed"
    return str(out.get("verdict") or out.get("decision") or out.get("status") or "?")


def j17_case() -> None:
    entry = next(
        json.loads(line)
        for line in open(J17 / "entradas.jsonl")
        if '"b81d9fa3c3e5"' in line
    )
    state = json.loads((J17 / "estado_no_instante/manifesto.json").read_text()).get(entry["id"], {})
    cmd = [TOURING, "vgp", "check", "--path", entry["path"], "--content-file", str(J17 / entry["snap"]),
           "--project-root", entry["project_root"], "--lang", "python", "--camadas", "1,2", "-j"]
    if state.get("disk_file"):
        cmd += ["--disk-file", str(J17 / state["disk_file"])]
    for o in state.get("overlays", []):
        cmd += ["--overlay", f'{o["path"]}={J17 / o["file"]}']
    r = run(cmd)
    out = last_json(r.stdout)
    v = verdict_of(out)
    report("j17_b81d9fa3c3e5_not_deny", r.returncode in (0, 1, 2) and v.lower() != "deny",
           {"rc": r.returncode, "verdict": v})


def json_loadz() -> None:
    # Stdlib attributes are layer 2 (pyright), which needs a project venv with the langserver:
    # the analise project has one (it is where J17 ran). The virtual path is never written.
    root = os.environ.get("W4_PYRIGHT_ROOT", f"{HOME}/projects/analise")
    with tempfile.TemporaryDirectory() as tmp:
        f = Path(tmp) / "proposta.py"
        f.write_text("import json\n\njson.loadz('{}')\n")
        r = run([TOURING, "vgp", "check", "--path", f"{root}/backend/w4_prova_mod.py", "--content-file", str(f),
                 "--project-root", root, "--lang", "python", "--camadas", "1,2", "-j"])
        out = last_json(r.stdout)
        v = verdict_of(out)
        report("json_loadz_deny", v.lower() == "deny", {"rc": r.returncode, "verdict": v,
                                                       "layer": (out or {}).get("layer")})


def finalize() -> None:
    cwd = os.environ.get("W4_PROJECT", f"{HOME}/projects/touring")
    r = run([TOURING, "decompose", "finalize", "task_w4_prova_inexistente_0", "-j"], cwd=cwd)
    out = last_json(r.stdout) or {}
    report("finalize_unknown_task_is_error", bool(out.get("error")) and out.get("archived") is False,
           {"rc": r.returncode, "error": out.get("error"), "archived": out.get("archived")})

    c = run([TOURING, "decompose", "create", "feature", "W4 prova finalize recusa plano aberto", "-j"], cwd=cwd)
    created = last_json(c.stdout) or {}
    task = created.get("task_id") or created.get("id")
    if not task:
        report("finalize_open_plan_refused", False, {"create_rc": c.returncode, "create": str(created)[:200]})
        return
    run([TOURING, "decompose", "add", task, "w4-aberta", "subtask left open on purpose", "-j"], cwd=cwd)
    g = last_json(run([TOURING, "decompose", "get", task, "-j"], cwd=cwd).stdout) or {}
    open_subs = [s["subtask_id"] for s in g.get("subtasks", []) if s.get("status") not in ("completed", "done")]
    f = last_json(run([TOURING, "decompose", "finalize", task, "-j"], cwd=cwd).stdout) or {}
    refused = bool(f.get("error")) and f.get("archived") is False and bool(f.get("blocking"))
    report("finalize_open_plan_refused", refused and bool(open_subs),
           {"open_subtasks": len(open_subs), "error": f.get("error"), "blocking": len(f.get("blocking") or [])})
    # cleanup: the probe plan leaves no open work behind
    for s in open_subs:
        run([TOURING, "decompose", "update", task, s, "--status", "cancelled", "-j"], cwd=cwd)
    done = last_json(run([TOURING, "decompose", "finalize", task, "-j"], cwd=cwd).stdout) or {}
    report("finalize_after_closing_archives", done.get("archived") is True or done.get("ready") is True,
           {"archived": done.get("archived"), "ready": done.get("ready"), "error": done.get("error")})


def pre_bash(name: str, command: str) -> None:
    payload = {"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": command},
               "session_id": "w4-prova", "cwd": tempfile.gettempdir()}
    r = run([HOOK, "pre-bash"], input=json.dumps(payload))
    text = r.stdout + r.stderr
    denied = r.returncode == 2 or '"deny"' in text or '"block"' in text.lower()
    report(name, denied, {"rc": r.returncode, "find_delete_named": "find -delete" in text})


j17_case()
json_loadz()
finalize()
pre_bash("pre_bash_find_delete_denied", "find . -name '*.go' -delete")
pre_bash("pre_bash_padded_find_delete_denied", "find . " + "a " * 1_000_000 + "-name '*.go' -delete")
ok = all(r["ok"] for r in results)
print(json.dumps({"all_ok": ok, "passed": sum(r["ok"] for r in results), "total": len(results)}))
sys.exit(0 if ok else 1)
