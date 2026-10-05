#!/usr/bin/env python3
"""`touring <command> --help` answers with help and never runs the command. Measured by the executor.

A unit test can only check the dispatcher's list; whether a command RUNS when handed `--help` is a
property of the binary. The blind review of the 04/10/2026 fixes found `init-project --help` writing
`.touring/` into the cwd, and a census of every table command then found 55 of 141 that did not answer
with help: 19 ran (`exec` and its family, `context`, `route`, `eval`, `status`, `doctor`...), 6 would
have run against a live daemon (`kpi`, `mutation-test`...), 16 failed instead of helping. The fix
inverted the rule (`SELF_HELP` in `crates/touring-server/src/cli/common.rs`: every command not proven
to answer help itself is answered by the dispatcher). This re-runs that census against the release
binary, so a command added later, or a name put in `SELF_HELP` without evidence, fails here.

Each command runs with a scratch HOME and cwd and a daemon socket that does not exist. A command that
runs instead of helping writes into the scratch, which the test reads back. The masters forward
`--help` to a skill script, so the scratch HOME links the real `~/.claude/skills` (read only).
"""
from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
TABLE = REPO / "crates" / "touring-server" / "src" / "cli" / "command_table.rs"
BINARY = Path(os.environ.get("TOURING_HELP_BIN", REPO / "target" / "release" / "touring"))
SKILLS = Path.home() / ".claude" / "skills"
HELP = re.compile(r"^\s*usage:", re.I | re.M)


def _names() -> list[str]:
    names = re.findall(r'name:\s*"([^"]+)"', TABLE.read_text(encoding="utf-8"))
    assert len(names) > 100, f"the table parse found only {len(names)} commands — the regex no longer matches"
    return names


def _files(root: Path) -> list[str]:
    out = []
    for d, dirs, files in os.walk(root):  # does not follow the skills symlink
        out += [str(Path(d, f).relative_to(root)) for f in files]
    return sorted(out)


def _ask_for_help(name: str, scratch: Path) -> tuple[int, str, list[str]]:
    home, cwd = scratch / "home", scratch / "cwd"
    (home / ".claude").mkdir(parents=True)
    cwd.mkdir()
    if SKILLS.is_dir():
        (home / ".claude" / "skills").symlink_to(SKILLS)
    env = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": str(home),
        "TMPDIR": str(scratch),
        "NO_COLOR": "1",
        "TOURING_DAEMON_SOCKET": str(scratch / "no-daemon.sock"),
        "TOURING_DAEMON_SOCK": str(scratch / "no-daemon.sock"),
    }
    p = subprocess.run([str(BINARY), name, "--help"], cwd=cwd, env=env, capture_output=True,
                       text=True, timeout=30)
    written = [f"home/{f}" for f in _files(home)] + [f"cwd/{f}" for f in _files(cwd)]
    return p.returncode, (p.stdout or "") + (p.stderr or ""), written


@pytest.mark.skipif(not BINARY.is_file(), reason=f"release binary not built: {BINARY}")
def test_every_command_answers_help_and_runs_nothing(tmp_path: Path) -> None:
    offenders = []
    for name in _names():
        rc, out, written = _ask_for_help(name, tmp_path / name)
        helped = bool(HELP.search(out)) or out.lstrip().startswith(f"touring {name} —")
        if rc != 0 or not helped or written:
            first = next((l for l in out.splitlines() if l.strip()), "")[:90]
            offenders.append(f"{name}: rc={rc} help={helped} wrote={written[:3]} :: {first}")
    assert not offenders, (
        f"{len(offenders)} command(s) did not answer `--help` with help, or wrote while asked for it:\n"
        + "\n".join(offenders)
    )


def _self_help() -> list[str]:
    src = (REPO / "crates" / "touring-server" / "src" / "cli" / "common.rs").read_text(encoding="utf-8")
    block = re.search(r"pub const SELF_HELP: &\[&str\] = &\[(.*?)\];", src, re.S)
    assert block, "SELF_HELP not found in common.rs"
    return re.findall(r'"([^"]+)"', block.group(1))


def _subcommands(name: str, scratch: Path) -> list[str]:
    rc, out, _ = _ask(scratch, name, "--help")
    tail = out.split("Commands:", 1)[1] if "Commands:" in out else ""
    subs = []
    for line in tail.splitlines():
        if not line.strip():
            if subs:
                break
            continue
        m = re.match(r"\s{2,}([a-z][a-z0-9-]*)\s", line + " ")
        if not m:
            break
        if m.group(1) != "help":
            subs.append(m.group(1))
    return subs


def _ask(scratch: Path, *argv: str) -> tuple[int, str, str]:
    scratch.mkdir(parents=True, exist_ok=True)
    env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(scratch), "NO_COLOR": "1",
           "TOURING_DAEMON_SOCKET": str(scratch / "no-daemon.sock"),
           "TOURING_DAEMON_SOCK": str(scratch / "no-daemon.sock")}
    p = subprocess.run([str(BINARY), *argv], cwd=scratch, env=env, capture_output=True, text=True, timeout=30)
    return p.returncode, (p.stdout or "") + (p.stderr or ""), p.stderr or ""


MASTERS = {"adw", "blast", "explore", "factory", "guard", "health", "investigate", "map", "read", "scout"}


@pytest.mark.skipif(not BINARY.is_file(), reason=f"release binary not built: {BINARY}")
def test_the_global_json_flag_is_accepted_after_every_subcommand(tmp_path: Path) -> None:
    """`-j`/`--json` is a global flag (main.rs: "the handler re-parses globals from any position"), and 35 of the
    36 clap commands that rejected it at the top level also rejected it after the subcommand, where a user writes
    it: `touring decompose get <task> -j` exited 2 with "unexpected argument '-j'" (04/10/2026). Probed with
    `--help` after the flag, so nothing runs; a command that strips the flag prints its help."""
    offenders = []
    for name in _self_help():
        if name in MASTERS:  # forwarded to a script's argparse, which owns its flags
            continue
        targets = [[name, s] for s in _subcommands(name, tmp_path / name)] or [[name]]
        for t in targets:
            for flag in ("-j", "--json"):
                _, _, err = _ask(tmp_path / name, *t, flag, "--help")
                if "unexpected argument" in err:
                    offenders.append(f"{' '.join(t)} {flag}")
    assert not offenders, f"{len(offenders)} target(s) reject the global json flag:\n" + "\n".join(offenders[:40])


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-q"]))
