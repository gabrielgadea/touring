#!/usr/bin/env python3
"""tmp_write_guard.py — PreToolUse(Write|Edit|MultiEdit|NotebookEdit|Bash): nothing the model writes lands in an
ephemeral directory.

Source of truth: client/hooks/tmp_write_guard.py. ~/.claude/hooks/tmp_write_guard.py is a symlink to it.
Tests: client/hooks/tests/test_tmp_write_guard.py.

WHY (Gabriel, 10/11/2026): "tenho perdido arquivos importantes de trabalhos importantes". Deliverables, logs of long
jobs and verdicts were written to /tmp, the session scratchpad (/tmp/claude-<uid>/…) or a cache dir, and vanished on
reboot, tmpfs cleanup or end of session (memory `veredito-no-scratchpad-morre-com-a-sessao`).

WHAT IS EPHEMERAL
  * anything under a temp ROOT: /tmp, /var/tmp, /dev/shm, /run/user/<uid>, ~/.cache, $TMPDIR — always;
  * anything with a path COMPONENT named tmp|temp|.tmp|.temp|cache|.cache|caches|scratch|.scratch|scratchpad
    (case-insensitive) — unless the path is inside a git work tree AND not git-ignored. A versioned `src/cache/`
    is source code and passes; an ignored `.cache/` or a `scratch/` outside any repo is denied.

WHAT IS JUDGED (the target of a write, never a read or a delete)
  Write|Edit|MultiEdit   tool_input.file_path          NotebookEdit   tool_input.notebook_path
  Bash   redirections (> >> >| &> &>> <>), tee, cp/mv/install/ln/rsync/scp destination (or -t DIR), touch, mkdir,
         truncate, dd of=, sed/perl -i, curl -o, wget -O/-P, -o/--output of compilers & converters, git clone /
         git worktree add destination, unzip -d — also inside bash -c, eval, $(…), heredoc-to-shell (via
         _shell_mask.scan). `cd DIR` earlier in the same command moves the base of relative paths.

ESCAPES
  Bash, per command: prefix `TMP_WRITE_OK=1 <command>` — for content that is genuinely disposable. Logged.
  Human config (settings.json env, invisible to the model's tool calls):
    TMP_WRITE_GUARD_DISABLED=1      kill switch
    TMP_WRITE_GUARD_ALLOW=p1:p2     path prefixes always allowed
    TMP_WRITE_GUARD_DEST=dir        durable base suggested in the deny (default ~/claude-artefatos)
    TMP_WRITE_GUARD_ROOTS=r1:r2     replaces the root list (test seam)

KNOWN LIMIT (declared): textual. A program that builds the path at run time (python -c "open('/tmp/x','w')", a
script file, a variable holding /tmp) is not seen; neither is a write done inside `touring run`'s sandbox.

FAIL-OPEN: any unexpected error exits 0 in silence — the guard never blocks on its own failure.
Every deny and every escape is appended to ~/.claude/touring/tmp_write_guard/events.jsonl.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
import time
from pathlib import Path

HOME = Path.home()
EPHEMERAL_NAMES = {
    "tmp",
    "temp",
    ".tmp",
    ".temp",
    "cache",
    ".cache",
    "caches",
    "scratch",
    ".scratch",
    "scratchpad",
}
ESCAPE = "TMP_WRITE_OK"
LOG = HOME / ".claude" / "touring" / "tmp_write_guard" / "events.jsonl"

SEPARATORS = {";", "&&", "||", "|", "|&", "&", "(", ")", "\n", ";;"}
REDIRECTS = {">", ">>", ">|", "&>", "&>>", "<>"}
WRAPPERS = {
    "sudo",
    "env",
    "nice",
    "nohup",
    "time",
    "command",
    "exec",
    "builtin",
    "stdbuf",
    "ionice",
    "chrt",
}
COPY_LIKE = {"cp", "mv", "install", "ln", "rsync", "scp"}
ALL_ARGS = {"touch", "mkdir", "truncate"}
OUTPUT_FLAG = {
    "curl": ("-o", "--output"),
    "wget": ("-O", "--output-document", "-P", "--directory-prefix"),
    "unzip": ("-d",),
    "sort": ("-o", "--output"),
    "pandoc": ("-o", "--output"),
    "gcc": ("-o",),
    "g++": ("-o",),
    "cc": ("-o",),
    "clang": ("-o",),
    "rustc": ("-o",),
    "go": ("-o",),
    "yt-dlp": ("-o", "--output", "-P", "--paths"),
    "markitdown": ("-o", "--output"),
    "pdftotext": (),
    "convert": (),
    "magick": (),
}
LAST_ARG_WRITERS = {"pdftotext", "convert", "magick", "ffmpeg"}
HEREDOC = re.compile(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2")


# ---------------------------------------------------------------- classification


def _roots() -> list[Path]:
    override = os.environ.get("TMP_WRITE_GUARD_ROOTS")
    if override is not None:
        return [Path(p) for p in override.split(":") if p]
    roots = [
        "/tmp",
        "/var/tmp",
        "/dev/shm",
        f"/run/user/{os.getuid()}",
        str(HOME / ".cache"),
    ]
    tmpdir = os.environ.get("TMPDIR", "")
    if tmpdir and tmpdir != "/":
        roots.append(tmpdir)
    return [Path(r) for r in roots]


def _under(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def _existing_ancestor(path: Path) -> Path | None:
    for p in [path, *path.parents]:
        if p.is_dir():
            return p
    return None


def _git_versioned(path: Path) -> bool:
    """True when `path` sits in a git work tree and is not ignored there (it will survive)."""
    anchor = _existing_ancestor(path.parent)
    if anchor is None:
        return False
    try:
        r = subprocess.run(
            ["git", "-C", str(anchor), "check-ignore", "-q", "--no-index", str(path)],
            capture_output=True,
            timeout=2,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return False
    return r.returncode == 1  # 0 = ignored, 128 = not a repo


def ephemeral_reason(raw: str, cwd: str) -> str | None:
    """Why `raw` (as written in the tool call) is an ephemeral location, or None when it is durable."""
    if not raw or (raw.startswith("/dev/") and not raw.startswith("/dev/shm")):
        return None
    p = Path(os.path.expanduser(raw))
    if not p.is_absolute():
        p = Path(cwd) / p
    candidates = {Path(os.path.normpath(p))}
    try:
        candidates.add(Path(os.path.realpath(p)))
    except OSError:
        pass
    for allowed in filter(None, os.environ.get("TMP_WRITE_GUARD_ALLOW", "").split(":")):
        if any(_under(c, Path(os.path.expanduser(allowed))) for c in candidates):
            return None
    for c in candidates:
        for root in _roots():
            if _under(c, root):
                return f"raiz temporária {root}"
    for c in candidates:
        hit = next((part for part in c.parts if part.lower() in EPHEMERAL_NAMES), None)
        if hit and not _git_versioned(c):
            return f"diretório '{hit}' fora do git ou ignorado por ele"
    return None


# ---------------------------------------------------------------- bash parsing


def strip_heredoc_bodies(cmd: str) -> str:
    out, pending = [], []
    for line in cmd.split("\n"):
        if pending:
            delim, dash = pending[0]
            if (line.lstrip("\t") if dash else line) == delim:
                pending.pop(0)
            continue
        out.append(line)
        pending.extend((m.group(3), bool(m.group(1))) for m in HEREDOC.finditer(line))
    return "\n".join(out)


def tokenize(cmd: str) -> list[str]:
    lex = shlex.shlex(cmd, posix=True, punctuation_chars="();<>|&\n")
    lex.whitespace = " \t\r"
    lex.whitespace_split = True
    lex.commenters = ""
    return list(lex)


def segments(tokens: list[str]) -> list[list[str]]:
    segs, cur = [], []
    for t in tokens:
        if t in SEPARATORS:
            if cur:
                segs.append(cur)
            cur = []
        else:
            cur.append(t)
    if cur:
        segs.append(cur)
    return segs


def _positional(args: list[str]) -> list[str]:
    out, end = [], False
    for a in args:
        if not end and a == "--":
            end = True
        elif end or not a.startswith("-") or a == "-":
            out.append(a)
    return out


def _flag_values(args: list[str], flags: tuple[str, ...]) -> list[str]:
    vals = []
    for i, a in enumerate(args):
        for f in flags:
            if a == f and i + 1 < len(args):
                vals.append(args[i + 1])
            elif f.startswith("--") and a.startswith(f + "="):
                vals.append(a[len(f) + 1 :])
            elif (
                not f.startswith("--")
                and a.startswith(f)
                and len(a) > len(f)
                and a[1] == f[1]
            ):
                vals.append(a[len(f) :])
    return vals


ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
NUMERIC_SIZE = re.compile(r"[+-]?\d+[KMGkmg]?")


def _split_redirects(seg: list[str]) -> tuple[list[str], list[str]]:
    """(the words without redirections, the redirection targets), wherever the operators sit."""
    args: list[str] = []
    targets: list[str] = []
    i = 0
    while i < len(seg):
        t = seg[i]
        fd_prefixed = t.isdigit() and i + 1 < len(seg) and seg[i + 1] in REDIRECTS
        if t in REDIRECTS or fd_prefixed:
            i += 1 if fd_prefixed else 0
            nxt = seg[i + 1] if i + 1 < len(seg) else ""
            if nxt and not nxt.startswith("&") and nxt not in {"(", "<", ">"}:
                targets.append(nxt)
            i += 2
        elif t in {"<", "<<", "<<<", ">&", "<&"}:
            i += 2
        else:
            args.append(t)
            i += 1
    return args, targets


def _strip_prefix(args: list[str]) -> tuple[list[str], bool]:
    """Drop leading NAME=val assignments and wrappers (env, nice, timeout N, …); report the escape token."""
    escaped = False

    def eat_assignments(require_assign: bool) -> None:
        nonlocal escaped
        while args and (
            ASSIGNMENT.match(args[0])
            or (not require_assign and args[0].startswith("-"))
        ):
            escaped |= args.pop(0).split("=", 1)[0] == ESCAPE

    args = list(args)
    eat_assignments(True)
    while args and os.path.basename(args[0]) in WRAPPERS | {"timeout"}:
        name = os.path.basename(args.pop(0))
        eat_assignments(False)
        if name == "timeout" and args:
            args.pop(0)
    return args, escaped


def _in_place(name: str, rest: list[str]) -> bool:
    return name in {"sed", "perl"} and any(
        a.startswith(("-i", "--in-place"))
        or (name == "perl" and re.match(r"^-\w*i", a))
        for a in rest
    )


def _command_targets(name: str, rest: list[str]) -> list[str]:
    """Write targets carried by the command's own arguments."""
    pos = _positional(rest)
    targets: list[str] = []
    if name == "tee":
        targets += pos
    elif name in COPY_LIKE:
        targets += _flag_values(rest, ("-t", "--target-directory")) or (
            pos[-1:] if len(pos) >= 2 else []
        )
    elif name in ALL_ARGS:
        targets += [p for p in pos if not NUMERIC_SIZE.fullmatch(p)]
    elif name == "dd":
        targets += [a[3:] for a in rest if a.startswith("of=")]
    elif _in_place(name, rest):
        targets += [p for p in pos if "/" in p or "." in p]
    elif name == "git":
        targets += _git_targets(pos)
    targets += _flag_values(rest, OUTPUT_FLAG.get(name, ()))
    if name in LAST_ARG_WRITERS and len(pos) >= 2:
        targets += pos[-1:]
    return targets


def _git_targets(pos: list[str]) -> list[str]:
    if pos[:1] == ["clone"] and len(pos) >= 3:
        return pos[-1:]
    if pos[:2] == ["worktree", "add"]:
        return pos[2:3]
    return []


def segment_targets(seg: list[str]) -> tuple[list[str], bool, str | None]:
    """(write targets, escape token present, new cwd if the segment is `cd DIR`)."""
    args, targets = _split_redirects(seg)
    args, escaped = _strip_prefix(args)
    if not args:
        return targets, escaped, None
    name, rest = os.path.basename(args[0]), args[1:]
    if name == "cd":
        pos = _positional(rest)
        return targets, escaped, (pos[0] if pos else str(HOME))
    return targets + _command_targets(name, rest), escaped, None


def bash_targets(cmd: str, cwd: str) -> tuple[list[tuple[str, str]], bool]:
    """[(target, base cwd)], escape present. Nested code (bash -c, eval, $(…)) comes from _shell_mask.scan."""
    texts = [strip_heredoc_bodies(cmd)]
    try:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import _shell_mask  # optional; absent or broken means fail-open

        texts += list(_shell_mask.scan(cmd).views)[1:]
    except Exception:  # noqa: BLE001, S110 — nested views are a bonus, never a reason to fail
        pass
    found: list[tuple[str, str]] = []
    escaped = False
    for text in texts:
        try:
            toks = tokenize(text)
        except ValueError:
            continue
        base = cwd
        for seg in segments(toks):
            tg, esc, newcwd = segment_targets(seg)
            escaped |= esc
            found += [(t, base) for t in tg]
            if newcwd is not None:
                nc = Path(os.path.expanduser(newcwd))
                base = str(nc if nc.is_absolute() else Path(base) / nc)
    return found, escaped


# ---------------------------------------------------------------- decision


def durable_suggestion(target: str, cwd: str) -> str:
    base = Path(
        os.path.expanduser(os.environ.get("TMP_WRITE_GUARD_DEST", "~/claude-artefatos"))
    )
    project = Path(cwd).name or "geral"
    try:
        top = subprocess.run(
            ["git", "-C", cwd, "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            timeout=2,
            check=False,
        ).stdout.strip()
        project = Path(top).name or project
    except (OSError, subprocess.SubprocessError):
        pass
    return str(base / project / (Path(target).name or "saida"))


def log(event: dict) -> None:
    try:
        LOG.parent.mkdir(parents=True, exist_ok=True)
        with LOG.open("a", encoding="utf-8") as fh:
            fh.write(
                json.dumps(
                    {"ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"), **event},
                    ensure_ascii=False,
                )
                + "\n"
            )
    except OSError:
        pass


def deny(reason: str) -> None:
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            },
            ensure_ascii=False,
        )
    )


def main() -> int:
    if os.environ.get("TMP_WRITE_GUARD_DISABLED") == "1":
        return 0
    payload = json.load(sys.stdin)
    tool = payload.get("tool_name", "")
    ti = payload.get("tool_input") or {}
    cwd = payload.get("cwd") or os.getcwd()
    session = payload.get("session_id", "")

    if tool in {"Write", "Edit", "MultiEdit", "NotebookEdit"}:
        target = (
            ti.get("notebook_path" if tool == "NotebookEdit" else "file_path") or ""
        )
        why = ephemeral_reason(target, cwd)
        if why:
            dest = durable_suggestion(target, cwd)
            log(
                {
                    "tool": tool,
                    "target": target,
                    "why": why,
                    "decision": "deny",
                    "session": session,
                }
            )
            deny(
                f"[tmp-write-guard] `{target}` está em área efêmera ({why}): arquivos ali somem em reboot, limpeza "
                f"ou fim de sessão. Rota: grave em `{dest}` (durável) ou num caminho versionado do projeto. "
                f"{tool} não tem atalho — o escape `{ESCAPE}=1` existe só no Bash, para conteúdo descartável."
            )
        return 0

    if tool == "Bash":
        cmd = ti.get("command") or ""
        hits = []
        targets, escaped = bash_targets(cmd, cwd)
        for target, base in targets:
            why = ephemeral_reason(target, base)
            if why:
                hits.append((target, why))
        if not hits:
            return 0
        if escaped:
            log(
                {
                    "tool": tool,
                    "targets": [h[0] for h in hits],
                    "decision": "escape",
                    "session": session,
                }
            )
            return 0
        target, why = hits[0]
        dest = durable_suggestion(target, cwd)
        others = (
            f" (+{len(hits) - 1} outro(s): {', '.join(h[0] for h in hits[1:4])})"
            if len(hits) > 1
            else ""
        )
        log(
            {
                "tool": tool,
                "targets": [h[0] for h in hits],
                "why": why,
                "decision": "deny",
                "session": session,
            }
        )
        deny(
            f"[tmp-write-guard] o comando grava em `{target}`{others}, área efêmera ({why}): arquivos ali somem em "
            f"reboot, limpeza ou fim de sessão. Rota: troque o destino por `{dest}` (durável) ou um caminho "
            f"versionado do projeto. Conteúdo descartável de verdade (ex.: saída que você lê e joga fora): "
            f"`{ESCAPE}=1 <o mesmo comando>` — fica registrado."
        )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:  # noqa: BLE001 — fail-open
        sys.exit(0)
