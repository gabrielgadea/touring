"""V0c round 3 — reproduce each item's ORIGINAL invocation, whole, in isolation; the record decides, not the delegate.

Round 2 showed a verdict cannot rest on what a delegate says it ran: 33 items closed as "impossible to reproduce"
(scripts in /tmp removed, "needs a shell", "unbalanced quotes"), and one admitted writing in `comando` a command it
never executed. The quotes were MY defect (the inventory cut commands at 600 chars: 27 of 98 items). So the executor
is code, and the judge reads its record:

  1. full command — recovered from the session transcript: the Bash tool_use whose command starts with the stored
     prefix, preferring the one whose result carries the item's `first` timestamp. Two distinct survivors = ambiguous,
     reported, never resolved by picking the first. Its `cwd` comes from the same transcript record.
  2. /tmp files the session wrote and that no longer exist are rebuilt from its Write/Edit/MultiEdit calls up to
     `first` (Gabriel, 08/10: what is written in /tmp is part of the work) and copied to their original paths INSIDE
     the sandbox. An Edit whose old_string is missing marks the file incomplete.
  3. `bash cmd.sh` under bwrap: $HOME and /tmp are overlays whose writes vanish, network and pids are unshared and
     the daemon sockets are hidden — the replay writes nothing, kills nothing, calls no daemon outside itself.
  4. ~/.claude/touring/vgp/replay/<id>.json: command, cmd_sha256, cwd, rc, timed_out, wall_s, kind_in_output,
     token_in_output, tail, rebuilt, incomplete, unrebuilt, command_source.

Usage: python3 replay.py [--ids a,b] [--jobs 4] [--timeout 300] [--selftest]
"""

import concurrent.futures
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HOME = Path.home()
VGP = HOME / ".claude/touring/vgp"
OUT = VGP / "replay"
PROJECTS = HOME / ".claude/projects"
SOCKETS = [Path(f"/run/user/{os.getuid()}")]  # global daemon + CC session sockets: hidden inside the sandbox


def transcripts_of(session: str) -> list[Path]:
    return sorted(PROJECTS.glob(f"*/{session}.jsonl")) + sorted(PROJECTS.glob(f"*/{session}/**/*.jsonl"))


def _records(files: list[Path]):
    for f in files:
        for line in f.open(encoding="utf-8", errors="replace"):
            if '"tool_use"' in line or '"tool_result"' in line:
                yield json.loads(line)


def _contents(rec: dict) -> list[dict]:
    return [c for c in (rec.get("message") or {}).get("content") or [] if isinstance(c, dict)]


def _bash_uses(prefix: str, records: list[dict]) -> tuple[dict, dict]:
    """{tool_use id: (command, cwd)} of the Bash calls starting with `prefix`, and {tool_use id: result timestamp}."""
    uses, result_ts = {}, {}
    for rec in records:
        for c in _contents(rec):
            cmd = str((c.get("input") or {}).get("command", ""))
            if c.get("type") == "tool_use" and c.get("name") == "Bash" and prefix and cmd.startswith(prefix):
                uses[c.get("id")] = (cmd, rec.get("cwd", ""))
            elif c.get("type") == "tool_result":
                result_ts[c.get("tool_use_id")] = rec.get("timestamp", "")
    return uses, result_ts


def full_command(item: dict, records: list[dict]) -> tuple[str, str, str]:
    """(command, cwd, source). source ∈ inventário | transcript | ambíguo | não-achado."""
    prefix = item.get("command", "")
    uses, result_ts = _bash_uses(prefix, records)
    at_first = {u: v for u, v in uses.items() if result_ts.get(u) == item.get("first")}
    distinct = set((at_first or uses).values())
    if len(distinct) == 1:
        cmd, cwd = distinct.pop()
        return cmd, cwd, "inventário" if cmd == prefix else "transcript"
    return prefix, "", "ambíguo" if distinct else "não-achado"


def _apply_edit(text: str, old: str, new: str, replace_all: bool) -> str | None:
    if old not in text:
        return None
    return text.replace(old, new) if replace_all else text.replace(old, new, 1)


def _edits_of(c: dict) -> list[dict]:
    if c.get("name") not in ("Edit", "MultiEdit"):
        return []
    inp = c.get("input") or {}
    return inp.get("edits") or [inp]


SESSION_DIRS = re.compile(r"^/tmp/|^/home/[^/]+/projects/[^/]+/\.cache/")


def session_work(path: str) -> bool:
    """A file a session writes as part of its work and may lose (Gabriel, 08/10: /tmp and, likewise, the project's
    `.cache/`). The home's `~/.cache` is tool caches (uv, pip, hf) — external, never rebuilt."""
    return SESSION_DIRS.search(path) is not None


HEREDOC = re.compile(r"cat\s+(>>?)\s*\"?([^\s\"<;|&]+)\"?\s*<<-?\s*['\"]?(\w+)['\"]?[^\n]*\n(.*?)\n\3\s*$", re.S | re.M)
ASSIGN = re.compile(r"(?:^|[;&\n]\s*)(?:export\s+)?([A-Za-z_]\w*)=([^\s;&|`$()]+)")


def _apply_bash_heredocs(command: str, files: dict[str, str]) -> None:
    """`S=/tmp/…; cat > $S/x.py <<'EOF' … EOF` — the other way sessions write scratch files (17 of the 98 replays
    lacked a /tmp file created like this, 08/10/2026). Only literal assignments are expanded; anything fancier stays
    unrebuilt and is reported as such."""
    env = dict(ASSIGN.findall(command))
    for op, target, _tag, body in HEREDOC.findall(command):
        path = re.sub(r"\$\{?([A-Za-z_]\w*)\}?", lambda m: env.get(m.group(1), m.group(0)), target)
        if session_work(path) and "$" not in path and not Path(path).exists():
            files[path] = (files.get(path, "") if op == ">>" else "") + body + "\n"


def _fold(c: dict, files: dict[str, str], broken: set[str]) -> None:
    if c.get("type") == "tool_use" and c.get("name") == "Bash":
        _apply_bash_heredocs(str((c.get("input") or {}).get("command", "")), files)
    else:
        _apply_tool(c, files, broken)


def _apply_tool(c: dict, files: dict[str, str], broken: set[str]) -> None:
    """Fold one Write/Edit/MultiEdit on a vanished /tmp file into `files`; a missing old_string marks it broken."""
    path = str((c.get("input") or {}).get("file_path", ""))
    if c.get("type") != "tool_use" or not session_work(path) or Path(path).exists():
        return
    if c.get("name") == "Write":
        files[path] = str(c["input"].get("content", ""))
        broken.discard(path)
        return
    for e in _edits_of(c):
        done = _apply_edit(files.get(path, ""), e.get("old_string", ""), e.get("new_string", ""),
                           bool(e.get("replace_all")))
        if done is None:
            broken.add(path)
        else:
            files[path] = done


def rebuild_tmp(records: list[dict], until: str) -> tuple[dict[str, str], set[str]]:
    """Final state at `until` of every /tmp file the session wrote and that is gone today; + the incomplete ones."""
    # a call whose result is an error (hook deny, "String to replace not found") never changed the file: applying it
    # anyway desynchronised 52 of 72 rebuilds (08/10/2026 — a G3-denied Edit, then its retry no longer matched)
    failed = {c.get("tool_use_id") for rec in records for c in _contents(rec)
              if c.get("type") == "tool_result" and c.get("is_error")}
    files: dict[str, str] = {}
    broken: set[str] = set()
    for rec in records:
        if rec.get("timestamp", "") > until:
            break
        for c in _contents(rec):
            if c.get("type") != "tool_use" or c.get("id") not in failed:
                _fold(c, files, broken)
    return files, broken


def _declared_timeout(command: str, cap: int) -> int:
    nums = [int(n) for n in re.findall(r"\btimeout\s+(\d+)", command)]
    nums += [int(n) // 1000 for n in re.findall(r"--timeout-ms\s+(\d+)", command)]
    return min(max(nums + [60]) + 30, cap)


def _bwrap(workdir: Path, home_sockets: list[Path]) -> list[str]:
    # the overlay's writes land in tmpfs = RAM, charged to the writer's cgroup: a 6 GiB scope caps a runaway writer
    # (probe 08/10: a 64 MiB scope killed a 200 MiB tmpfs write, rc 137)
    scope = ["systemd-run", "--user", "--scope", "-q", "-p", "MemoryMax=6G", "-p", "MemorySwapMax=0"]
    args = scope + ["bwrap", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc",
            "--overlay-src", str(HOME), "--tmp-overlay", str(HOME), "--overlay-src", "/tmp", "--tmp-overlay", "/tmp",
            "--unshare-net", "--unshare-pid", "--die-with-parent", "--new-session"]
    for s in SOCKETS:
        args += ["--tmpfs", str(s)]
    for s in home_sockets:
        args += ["--ro-bind", "/dev/null", str(s)]
    return args + ["bash", str(workdir / "prelude.sh")]


def _write_workdir(work: Path, command: str, cwd: str, files: dict[str, str]) -> Path:
    (work / "files").mkdir(parents=True, exist_ok=True)
    lines = ["set +e"]
    for n, (path, text) in enumerate(sorted(files.items())):
        (work / "files" / str(n)).write_text(text, encoding="utf-8")
        lines.append(f"mkdir -p '{Path(path).parent}' && cp '{work}/files/{n}' '{path}'")
    (work / "cmd.sh").write_text(command + "\n", encoding="utf-8")
    lines.append(f"cd '{cwd}' 2>/dev/null || cd '{HOME}/projects/analise'")
    lines.append(f"exec bash '{work}/cmd.sh'")
    (work / "prelude.sh").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return work


ANSI = re.compile(r"\x1b\[[0-9;]*m")


def frame_in_output(item: dict, out: str) -> bool:
    """The STRONG predicate: the replay's traceback names the item's own file AND line. The class alone is weak —
    of 32 replays showing it (08/10/2026) only 14 had the frame; a script may print the class or the sandbox raise it."""
    name, line = Path(item.get("path", "")).name, item.get("line")
    if not name or not line:
        return False
    return re.search(rf'{re.escape(name)}", line {line}\b', out) is not None or f"{name}:{line}" in out


def _verdict_fields(item: dict, out: str) -> dict:
    out = ANSI.sub("", out)
    tokens = re.findall(r"'([^']{4,})'", item.get("message", ""))
    return {"kind_in_output": bool(item.get("kind")) and item["kind"] in out,
            "token_in_output": any(t in out for t in tokens), "frame_in_output": frame_in_output(item, out),
            "tail": out[-3000:]}


_EXT = r"\.(?:py|sh|json|md|txt)"


def _referenced(command: str, cwd: str) -> set[str]:
    """Session-work files the command names: absolute /tmp or project .cache paths, and a RELATIVE `.cache/…`
    resolved against the session's cwd (`python3 .cache/x.py` run from the project root)."""
    refs = {r for r in re.findall(rf"(/[^\s'\";|&)]+{_EXT})", command) if session_work(r)}
    base = Path(cwd or HOME / "projects/analise")
    return refs | {str(base / r) for r in re.findall(rf"(?<![\w/])(\.cache/[^\s'\";|&)]+{_EXT})", command)}


def run_isolated(work: Path, command: str, cwd: str, files: dict[str, str], cap: int) -> tuple[int | None, bool, str]:
    """(rc, timed_out, output) of `command` run from `cwd` under bwrap, with `files` (path → content) laid over their
    real paths inside the sandbox; every write vanishes. Public for V0d: the corpus builder and its judge overlay a
    SNAPSHOT at a project path exactly as /tmp rebuilds are laid (V0d-spec.md)."""
    _write_workdir(work, command, cwd or str(HOME / "projects/analise"), files)
    sockets = sorted(HOME.glob("projects/*/.touring/daemon.sock"))
    try:
        p = subprocess.run(_bwrap(work, sockets), capture_output=True, text=True, timeout=_declared_timeout(command, cap))
        return p.returncode, False, p.stdout + p.stderr
    except subprocess.TimeoutExpired as e:  # its captured streams may be bytes even with text=True
        return None, True, "".join(s.decode(errors="replace") if isinstance(s, bytes) else s
                                   for s in (e.stdout, e.stderr) if s)


def replay(item: dict, cap: int) -> dict:
    records = list(_records(transcripts_of(item.get("session", ""))))
    command, cwd, source = full_command(item, records)
    files, broken = rebuild_tmp(records, item.get("first", ""))
    refs = _referenced(command, cwd)
    unrebuilt = sorted(r for r in refs if not Path(r).exists() and r not in files)
    t0 = time.monotonic()
    rc, timed_out, out = run_isolated(OUT / item["id"], command, cwd, files, cap)
    rec = {"id": item["id"], "command": command, "cmd_sha256": hashlib.sha256(command.encode()).hexdigest(),
           "cwd": cwd, "command_source": source, "rc": rc, "timed_out": timed_out,
           "wall_s": round(time.monotonic() - t0, 1), "rebuilt": sorted(files), "incomplete": sorted(broken),
           "unrebuilt": unrebuilt, "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"), **_verdict_fields(item, out)}
    (OUT / f"{item['id']}.json").write_text(json.dumps(rec, ensure_ascii=False, indent=1), encoding="utf-8")
    return rec


def _cache_parity_ok() -> bool:
    """The project's `.cache/` is session work like /tmp (Gabriel, 08/10); the home's `~/.cache/` is not."""
    proj = str(HOME / "projects/analise/.cache/vgp-st-gone/h.py")
    files: dict[str, str] = {}
    _apply_bash_heredocs(f"cat > {proj} <<'EOF'\nprint(2)\nEOF", files)
    return (session_work(proj) and not session_work(str(HOME / ".cache/uv/x.py")) and files == {proj: "print(2)\n"}
            and _referenced("python3 .cache/v/x.py", "/p/analise") == {"/p/analise/.cache/v/x.py"})


def selftest() -> int:
    """Positive: a replayed ValueError is seen and its write never reaches the real disk; a /tmp script rebuilt from
    a Write+Edit runs with the EDITED body. Negative: a clean command shows no kind."""
    with tempfile.TemporaryDirectory(dir=HOME / ".claude/touring") as d:
        probe = Path(d) / "escrita_que_nao_pode_vazar.txt"
        item = {"id": "selftest-ve", "kind": "ValueError", "message": "x 'sonda-xyz'", "session": "nenhuma",
                "first": "9", "command": f"echo vazou > {probe}; python3 -c \"raise ValueError('sonda-xyz')\""}
        r1 = replay(item, 60)
        ok1 = r1["kind_in_output"] and r1["token_in_output"] and not probe.exists()
        clean = replay({**item, "id": "selftest-ok", "command": "true"}, 60)
        tmp = "/tmp/vgp-selftest-gone/s.py"
        recs = [{"timestamp": "1", "message": {"content": [{"type": "tool_use", "name": "Write",
                 "input": {"file_path": tmp, "content": "raise KeyError('antes')\n"}}]}},
                {"timestamp": "2", "message": {"content": [{"type": "tool_use", "name": "Edit",
                 "input": {"file_path": tmp, "old_string": "antes", "new_string": "depois-editado"}}]}},
                {"timestamp": "2", "message": {"content": [{"type": "tool_use", "name": "Edit", "id": "negada",
                 "input": {"file_path": tmp, "old_string": "depois-editado", "new_string": "NEGADA-APLICADA"}}]}},
                {"timestamp": "2", "message": {"content": [{"type": "tool_result", "tool_use_id": "negada",
                 "is_error": True, "content": "PreToolUse:Edit hook error: negado"}]}}]
        files, broken = rebuild_tmp(recs, "3")
        ok2 = files.get(tmp) == "raise KeyError('depois-editado')\n" and not broken
    heredoc: dict[str, str] = {}
    _apply_bash_heredocs("S=/tmp/vgp-st-gone; cat > $S/h.py <<'EOF'\nprint(1)\nEOF\necho fim", heredoc)
    ok3 = heredoc == {"/tmp/vgp-st-gone/h.py": "print(1)\n"}
    ok4 = _cache_parity_ok()
    ok = all((ok1, ok2, ok3, ok4, not clean["kind_in_output"]))
    print(f"selftest: {'ok' if ok else 'FALHOU'} (isolamento+classe {ok1} · reconstrução {ok2} · heredoc {ok3} · "
          f".cache do projeto {ok4} · controle negativo {not clean['kind_in_output']})")
    return 0 if ok else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    args = dict(zip(sys.argv[1::2], sys.argv[2::2]))
    itens = [json.loads(ln) for ln in (VGP / "v0c_itens.jsonl").open(encoding="utf-8")]
    if "--ids" in args:
        itens = [i for i in itens if i["id"] in set(args["--ids"].split(","))]
    OUT.mkdir(parents=True, exist_ok=True)
    cap, jobs = int(args.get("--timeout", 300)), int(args.get("--jobs", 4))
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        recs = list(pool.map(lambda i: replay(i, cap), itens))
    print(json.dumps(summary(recs), ensure_ascii=False))
    return 0


def summary(recs: list[dict]) -> dict:
    """Every record lands in exactly one row of `por desfecho`; the other counters overlap and are labelled so."""
    def outcome(r: dict) -> str:
        if r["command_source"] in ("ambíguo", "não-achado"):
            return "comando ambíguo/não-achado"
        if r["timed_out"]:
            return "timeout"
        if r["kind_in_output"]:
            return "reproduz (classe na saída)"
        return "rc 0 sem a classe" if r["rc"] == 0 else "falhou por outro motivo"
    by = {}
    for r in recs:
        by[outcome(r)] = by.get(outcome(r), 0) + 1
    return {"itens": len(recs), "por desfecho": by, "(sobreposto) /tmp reconstruído": sum(bool(r["rebuilt"]) for r in recs),
            "(sobreposto) /tmp não reconstruído": sum(bool(r["unrebuilt"]) for r in recs),
            "(sobreposto) reconstrução incompleta": sum(bool(r["incomplete"]) for r in recs)}


if __name__ == "__main__":
    sys.exit(main())
