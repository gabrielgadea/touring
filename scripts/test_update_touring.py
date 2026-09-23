#!/usr/bin/env python3
"""Guards for `scripts/update-touring` — the tool that performs every deploy.

Until 2026-08-18 this script existed ONLY at ~/.local/bin/update-touring: outside
version control, unreviewable, and unable to survive a new machine. It now lives
here and is reached on PATH through a symlink, the same shape already used for
the touring/touring-hook/touring-daemon binaries.

Two kinds of check, split by what each environment can actually see:

  · **Content invariants** run anywhere, CI included. They encode the two defects
    that versioning exposed: a default workspace pointing at the FROZEN pre-move
    root, and process checks broad enough to match other projects' daemons.
  · **The symlink check** needs the operator's machine, so it skips elsewhere
    rather than failing — a gate that cannot look must not block.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "update-touring"
INSTALLED = Path.home() / ".local" / "bin" / "update-touring"

#: Lines that are comments (or blank) carry no behaviour — the invariants below
#: are about what the script DOES, and several comments quote the old code on
#: purpose so the next reader knows what was wrong.
def executable_lines() -> list[tuple[int, str]]:
    out = []
    for n, raw in enumerate(SCRIPT.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if line and not line.startswith("#"):
            out.append((n, raw))
    return out


def test_the_script_is_versioned_here_and_executable():
    assert SCRIPT.is_file(), f"{SCRIPT} missing — the deploy tool must be in the repo"
    assert os.access(SCRIPT, os.X_OK), "the deploy tool must be executable"


def test_it_parses():
    proc = subprocess.run(["bash", "-n", str(SCRIPT)], capture_output=True, text=True)
    assert proc.returncode == 0, proc.stderr


@pytest.mark.skipif(shutil.which("shellcheck") is None, reason="shellcheck not installed")
def test_shellcheck_is_clean():
    proc = subprocess.run(
        ["shellcheck", "-S", "warning", str(SCRIPT)], capture_output=True, text=True
    )
    assert proc.returncode == 0, proc.stdout


def test_the_workspace_is_never_defaulted_to_the_frozen_root():
    """`~/.claude/rust` is the FROZEN pre-move tree (see CLAUDE.md).

    The old default was `${TOURING_WORKSPACE_ROOT:-${HOME}/.claude/rust}`, so from
    any shell that did not export that variable — an ordinary terminal — this
    script rebuilt and installed the frozen tree and said nothing. The workspace
    is now derived from the script's own location, which is exactly what living
    inside the workspace buys."""
    for n, line in executable_lines():
        assert ".claude/rust" not in line, (
            f"line {n} resolves the workspace to the frozen root: {line.strip()}"
        )
    body = SCRIPT.read_text(encoding="utf-8")
    assert "BASH_SOURCE" in body, "the workspace must be derived from the script's location"


def test_no_process_check_matches_another_project_s_daemon():
    """Per-project daemons are normal now; "any process named touring-daemon" is not
    a question with a useful answer.

    The broad pattern made three separate call sites wrong at once: the kill
    fallback would SIGTERM/SIGKILL other projects' daemons (the cascading kill
    REGRA #19 exists to prevent), `start_daemon` would skip starting the global
    daemon because a sibling was up, and `verify` would fail the run over an
    unrelated process's binary."""
    # `pgrep` was the observed offender, but `pkill`/`killall` and a
    # `ps | grep` pipeline are the SAME name-match mistake with a trigger
    # attached — a guard that names one spelling invites the next (T5,
    # cross-audit 2026-08-23).
    offenders = [
        (n, line.strip())
        for n, line in executable_lines()
        if re.search(r"\b(pgrep|pkill|killall)\b", line)
        or re.search(r"\bps\b.*\|\s*grep", line)
    ]
    assert offenders == [], (
        "process checks must resolve the owner of the global socket "
        f"(global_daemon_pid), not match a name: {offenders}"
    )


def test_runtime_libs_are_the_ones_touring_update_links():
    """update-touring links the ONNX Runtime provider libraries next to the global
    daemon; `touring update` links PROJECT_RUNTIME_LIBS next to each project's. A
    name on one side only leaves that side's daemons embedding on CPU, with no
    error anywhere but a counter."""
    body = SCRIPT.read_text(encoding="utf-8")
    bash = re.search(r"^readonly RUNTIME_LIBS=\(([^)]*)\)", body, re.M)
    assert bash, "RUNTIME_LIBS array not found in update-touring"
    rust_src = (REPO / "crates/touring-server/src/cli/project_toolchain.rs").read_text(
        encoding="utf-8"
    )
    rust = re.search(r"PROJECT_RUNTIME_LIBS: &\[&str\] = &\[(.*?)\];", rust_src, re.S)
    assert rust, "PROJECT_RUNTIME_LIBS not found in project_toolchain.rs"
    rust_names = re.findall(r'"([^"]+)"', rust.group(1))
    assert rust_names, "PROJECT_RUNTIME_LIBS is empty"
    assert bash.group(1).split() == rust_names


def test_a_per_command_relaxation_never_reaches_the_launched_daemon():
    """`TOURING_CODE_MODE=native <cmd>` relaxa UM comando. O daemon que a herda
    desliga os gates de code mode para TODAS as sessões, porque a apresentação é
    resolvida na env de quem decide — e quem decide é o daemon.

    Medido em 22/09/2026: uma propagação disparada de um shell que carregava a var
    deixou a máquina inteira em `native`, com `doctor` verde e a prova
    comportamental reprovando 2 de 40 asserções sem nomear a causa. Este launcher
    é um dos dois sítios que sobem o daemon (o outro é o Rust); a lista tem de ser
    a MESMA nos dois, ou o defeito volta pelo lado que ninguém olhou."""
    body = SCRIPT.read_text(encoding="utf-8")
    launcher = re.search(r"launch_daemon_and_wait\(\) \{(.*?)\n\}", body, re.S)
    assert launcher, "launch_daemon_and_wait not found in update-touring"
    rust_src = (REPO / "crates/touring-foundation/src/daemon_spawn.rs").read_text(
        encoding="utf-8"
    )
    rust = re.search(r"PER_COMMAND_RELAXATIONS: &\[&str\] = &\[(.*?)\];", rust_src, re.S)
    assert rust, "PER_COMMAND_RELAXATIONS not found in daemon_spawn.rs"
    scrubbed = re.findall(r'"([^"]+)"', rust.group(1))
    assert scrubbed, "PER_COMMAND_RELAXATIONS is empty"
    for var in scrubbed:
        assert f"-u {var}" in launcher.group(1), (
            f"{var} must be unset when update-touring launches the daemon "
            "(the Rust spawn already scrubs it)"
        )

    deliberate = re.search(
        r'DAEMON_CODE_MODE_ENV: &str = "([^"]+)"', rust_src
    )
    assert deliberate, "DAEMON_CODE_MODE_ENV not found in daemon_spawn.rs"
    assert deliberate.group(1) in launcher.group(1), (
        "a intenção deliberada precisa da MESMA porta nos dois launchers"
    )


def test_no_uid_is_hardcoded_in_runtime_paths():
    """`/tmp/touring-daemon-1000.sock` only resolves on uid 1000 — on any other
    machine `global_daemon_pid` finds nothing and every dependent path (kill,
    cleanup, verify) silently acts on nothing. The uid must come from `id -u`
    (T1, cross-audit 2026-08-23)."""
    offenders = [
        (n, line.strip())
        for n, line in executable_lines()
        if re.search(r"touring-daemon-1000\.", line)
    ]
    assert offenders == [], f"uid hardcoded in runtime path: {offenders}"
    body = SCRIPT.read_text(encoding="utf-8")
    assert 'touring-daemon-$(id -u)' in body, (
        "socket/lock paths must derive the uid via $(id -u)"
    )


def test_the_owner_resolver_verifies_through_proc():
    """REGRA #19: a pid is only ours once /proc confirms what it is. A recycled
    pid from a stale file must never be mistaken for the daemon."""
    body = SCRIPT.read_text(encoding="utf-8")
    assert "global_daemon_pid()" in body, "the single owner-resolver must exist"
    resolver = body.split("global_daemon_pid()", 1)[1].split("\n}", 1)[0]
    assert "/proc/" in resolver, "the resolver must confirm the pid through /proc"
    assert "comm" in resolver, "the resolver must confirm the process identity"
    assert "DAEMON_SOCKET" in resolver, "the resolver must be scoped to the global socket"


def test_kill_is_never_applied_to_a_list_of_pids():
    """One pid, named, or none. `kill -9 ${pids}` over a pgrep result is precisely
    the cascading kill that took out other sessions' processes before."""
    for n, line in executable_lines():
        if re.search(r"\bkill\s+-", line):
            assert "${pid}" in line or '"${pid}"' in line, (
                f"line {n} kills something other than the resolved global pid: {line.strip()}"
            )


# ── the half only the operator's machine can answer ──────────────────────────


def test_the_daemon_is_launched_in_a_scope_of_its_own_with_a_direct_fallback():
    """Decision 2-A (14/09/2026): a process inherits its parent's cgroup, and
    systemd kills a unit's whole cgroup when the unit ends — a release run as a
    oneshot service left the daemon inside it. The launch goes through
    `systemd-run --user --scope` when the user manager answers, keeps the direct
    launch when it does not (or when the scope launch never brings the socket
    up), and honours the same opt-out as the Rust launcher."""
    body = SCRIPT.read_text(encoding="utf-8")
    start = body.index("start_daemon() {")
    fn = body[start : body.index("\n}\n", start)]
    code = "\n".join(l for l in fn.splitlines() if not l.strip().startswith("#"))
    assert "systemd-run --user --scope" in code, "the daemon must own a scope"
    assert '"${XDG_RUNTIME_DIR:-/nonexistent}/systemd/private"' in code, (
        "the scope route is taken only when the user manager is reachable"
    )
    assert "TOURING_DAEMON_SCOPE" in code, "same opt-out as touring_foundation::daemon_spawn"
    assert "--expand-environment=no" in code, "systemd-run must not rewrite $VAR in the argv"
    launcher = body[body.index("launch_daemon_and_wait() {") :]
    assert 'setsid "$@" env' in launcher, "the prefix runs inside the new session"


# ── start_daemon, executed (cross-audit 14/09/2026, C5) ──────────────────────
# The content test above survived a mutation that disabled the scope by default.
# These run the real `start_daemon` against stubs: a fake daemon that binds the
# socket, a fake `systemd-run` that records its argv and execs the rest, and a
# fake user-manager socket. Only the functions are taken from the script; its
# top level (argument parsing, the deploy itself) never runs.

DAEMON_STUB = """#!/usr/bin/env python3
import os, socket, sys, time
t = os.environ["UT_DIR"]
with open(os.path.join(t, "launches"), "a") as f:
    f.write(f"{os.getpid()}\\n")
if os.environ.get("UT_DAEMON_EXITS"):
    sys.exit(1)
time.sleep(float(os.environ.get("UT_BIND_DELAY", "0")))
path = os.path.join(t, "daemon.sock")
try:
    os.unlink(path)
except FileNotFoundError:
    pass
s = socket.socket(socket.AF_UNIX)
s.bind(path)
s.listen(8)
s.settimeout(0.2)
end = time.time() + 30
while time.time() < end and not os.path.exists(os.path.join(t, "stop")):
    try:
        s.accept()[0].close()
    except OSError:
        pass
"""

SYSTEMD_RUN_STUB = """#!/usr/bin/env bash
printf '%s\\n' "$*" >> "$UT_DIR/systemd-run.calls"
[[ -n "${UT_LAUNCHER_FAILS:-}" ]] && exit 1
while [[ $# -gt 0 && "$1" != "--" ]]; do shift; done
shift
exec "$@"
"""


def shell_function(name: str) -> str:
    body = SCRIPT.read_text(encoding="utf-8")
    start = body.index(f"{name}() {{")
    return body[start : body.index("\n}\n", start) + 3]


def run_start_daemon(tmp_path: Path, stale_socket: bool = False, **env: str):
    import socket as _socket
    import time as _time

    home = tmp_path / "home"
    (home / ".local" / "bin").mkdir(parents=True)
    daemon = home / ".local" / "bin" / "touring-daemon"
    daemon.write_text(DAEMON_STUB)
    daemon.chmod(0o755)
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    stub = bin_dir / "systemd-run"
    stub.write_text(SYSTEMD_RUN_STUB)
    stub.chmod(0o755)
    xdg = tmp_path / "xdg"
    (xdg / "systemd").mkdir(parents=True)
    manager = _socket.socket(_socket.AF_UNIX)
    manager.bind(str(xdg / "systemd" / "private"))
    if stale_socket:
        dead = _socket.socket(_socket.AF_UNIX)
        dead.bind(str(tmp_path / "daemon.sock"))
        dead.close()
    harness = "\n".join(
        [
            "set -euo pipefail",
            f'DAEMON_SOCKET="{tmp_path}/daemon.sock"',
            f'LOG_FILE="{tmp_path}/update.log"',
            f'DAEMON_STDERR="{tmp_path}/stderr.log"',
            f'DAEMON_CRASH_LOG="{tmp_path}/crash.jsonl"',
            "DAEMON_STDERR_MAX_BYTES=10485760",
            f'log() {{ echo "[log] $*" >> "{tmp_path}/out.log"; }}',
            f'err() {{ echo "[err] $*" >> "{tmp_path}/out.log"; }}',
            "global_daemon_pid() { return 1; }",
            shell_function("socket_accepts"),
            shell_function("process_running"),
            shell_function("launch_daemon_and_wait"),
            shell_function("start_daemon"),
            "start_daemon",
        ]
    )
    child_env = {
        **{k: v for k, v in os.environ.items() if not k.startswith("TOURING_")},
        "HOME": str(home),
        "PATH": f"{bin_dir}:{os.environ['PATH']}",
        "XDG_RUNTIME_DIR": str(xdg),
        "UT_DIR": str(tmp_path),
        **env,
    }
    started = _time.monotonic()
    try:
        proc = subprocess.run(
            ["bash", "-c", harness], env=child_env, capture_output=True, text=True, timeout=60
        )
    finally:
        (tmp_path / "stop").touch()
        manager.close()
    elapsed = _time.monotonic() - started
    read = lambda name: (
        (tmp_path / name).read_text().splitlines() if (tmp_path / name).exists() else []
    )
    return proc.returncode, read("systemd-run.calls"), read("launches"), read("out.log"), elapsed


@pytest.mark.skipif(shutil.which("setsid") is None, reason="needs util-linux setsid")
def test_start_daemon_launches_once_through_the_scope(tmp_path):
    rc, calls, launches, log, _ = run_start_daemon(tmp_path)
    assert rc == 0, log
    assert len(calls) == 1 and "--scope" in calls[0] and "--expand-environment=no" in calls[0], calls
    assert len(launches) == 1, (launches, log)


@pytest.mark.skipif(shutil.which("setsid") is None, reason="needs util-linux setsid")
def test_a_failed_scope_launcher_falls_back_to_one_direct_launch(tmp_path):
    rc, calls, launches, log, _ = run_start_daemon(tmp_path, UT_LAUNCHER_FAILS="1")
    assert rc == 0, log
    assert len(calls) == 1, calls
    assert len(launches) == 1, "only the direct launch reached the daemon"
    assert any("retrying the direct launch" in line for line in log), log


@pytest.mark.skipif(shutil.which("setsid") is None, reason="needs util-linux setsid")
def test_a_slow_daemon_behind_a_stale_socket_is_never_launched_twice(tmp_path):
    """C4: a leftover socket FILE used to read as ready at once, and a boot past
    5 s triggered a second, direct launch that could win the flock outside the
    scope. Readiness is a connection now, and a live launch is never doubled."""
    rc, calls, launches, log, elapsed = run_start_daemon(
        tmp_path, stale_socket=True, UT_BIND_DELAY="8"
    )
    assert rc == 4, log
    assert elapsed >= 4.5, "the stale socket file must not count as ready"
    assert len(calls) == 1 and len(launches) == 1, (calls, launches, log)
    assert any("not starting a second one" in line for line in log), log


@pytest.mark.skipif(shutil.which("setsid") is None, reason="needs util-linux setsid")
@pytest.mark.parametrize("value", ["0", "false", "off"])
def test_the_scope_opt_out_accepts_what_the_rust_launcher_accepts(tmp_path, value):
    rc, calls, launches, log, _ = run_start_daemon(tmp_path, TOURING_DAEMON_SCOPE=value)
    assert rc == 0, log
    assert calls == [], f"TOURING_DAEMON_SCOPE={value} must skip systemd-run"
    assert len(launches) == 1, log


def test_the_installed_tool_is_a_symlink_to_this_file():
    """Drift is prevented structurally: editing the installed path edits the repo
    file, because they are the same inode. Replacing the symlink with a copy
    would silently restore the old situation, so it is asserted.

    Strict mode (`UPDATE_TOURING_REQUIRE_SYMLINK=1`, exported by the
    propagate-release gate 1/6): a missing installed tool FAILS instead of
    skipping — the release path is about to invoke `update-touring` from the
    PATH, so "absent here" means "step 2/6 will run something unreviewed";
    a silent skip is exactly the hole the gate exists to close (T4,
    cross-audit 2026-08-23)."""
    if not INSTALLED.exists():
        if os.environ.get("UPDATE_TOURING_REQUIRE_SYMLINK") == "1":
            pytest.fail(
                f"{INSTALLED} absent under UPDATE_TOURING_REQUIRE_SYMLINK=1 — the "
                f"release gate needs the real symlink. Restore with: "
                f"ln -sfn {SCRIPT} {INSTALLED}"
            )
        pytest.skip(f"{INSTALLED} absent — needs the operator's machine")
    assert INSTALLED.is_symlink(), (
        f"{INSTALLED} is a regular file again — the deploy tool has drifted out of "
        f"the repo. Restore with: ln -sfn {SCRIPT} {INSTALLED}"
    )
    assert INSTALLED.resolve() == SCRIPT.resolve(), (
        f"{INSTALLED} points at {INSTALLED.resolve()}, not {SCRIPT}"
    )


def test_the_installed_tool_still_runs():
    if not INSTALLED.exists():
        pytest.skip(f"{INSTALLED} absent — needs the operator's machine")
    proc = subprocess.run(
        ["bash", "-n", str(INSTALLED)], capture_output=True, text=True
    )
    assert proc.returncode == 0, proc.stderr


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v", "--tb=short", "-p", "no:cacheprovider"]))


def run_verify_or_restart(tmp_path: Path, verdicts: list[int]):
    """Drive the real `verify_or_restart` with a scripted `verify_health`.

    Each call to the stub consumes the next exit code of `verdicts`, and every
    self-heal is recorded, so the test reads what the loop DID, not what it logs.
    """
    (tmp_path / "verdicts").write_text("\n".join(str(v) for v in verdicts) + "\n")
    harness = "\n".join(
        [
            "set -euo pipefail",
            f'UT="{tmp_path}"',
            "log() { :; }",
            "err() { :; }",
            "sleep() { :; }",
            'verify_health() { local v; v="$(head -1 "$UT/verdicts")";'
            ' sed -i 1d "$UT/verdicts"; return "$v"; }',
            'force_daemon_restart() { echo restart >> "$UT/restarts"; }',
            shell_function("verify_or_restart"),
            "verify_or_restart",
        ]
    )
    proc = subprocess.run(["bash", "-c", harness], capture_output=True, text=True, timeout=30)
    restarts = (tmp_path / "restarts").read_text().splitlines() if (tmp_path / "restarts").exists() else []
    return proc.returncode, len(restarts), proc.stderr


def test_a_stale_daemon_after_the_restart_is_healed_once():
    """14/09/2026: `local rc=$?` after an `if` with no `else` reads the status of
    the `if` itself, which is 0. The verify printed "running deleted binary",
    skipped the self-heal it exists for and let the deploy finish with exit 0."""
    import tempfile

    with tempfile.TemporaryDirectory() as d:
        rc, restarts, stderr = run_verify_or_restart(Path(d), [4, 0])
    assert (rc, restarts) == (0, 1), stderr


def test_a_daemon_that_stays_stale_fails_the_deploy():
    import tempfile

    with tempfile.TemporaryDirectory() as d:
        rc, restarts, stderr = run_verify_or_restart(Path(d), [4, 4])
    assert (rc, restarts) == (4, 1), stderr


def test_a_healthy_daemon_is_never_restarted():
    import tempfile

    with tempfile.TemporaryDirectory() as d:
        rc, restarts, stderr = run_verify_or_restart(Path(d), [0])
    assert (rc, restarts) == (0, 0), stderr


@pytest.fixture
def two_daemons(tmp_path):
    """A draining predecessor and its successor, both named `touring-daemon`.

    The kernel names a process after the file it executed, so a copy of `sleep`
    under that name gives `/proc/<pid>/comm` the value the resolver checks.
    """
    import socket as _socket
    import time as _time

    fake = tmp_path / "touring-daemon"
    shutil.copy(shutil.which("sleep"), fake)
    old = subprocess.Popen(["sleep", "60"], executable=str(fake))
    _time.sleep(0.05)
    new = subprocess.Popen(["sleep", "60"], executable=str(fake))
    _time.sleep(0.05)
    intruder = subprocess.Popen(["sleep", "60"])
    listener = _socket.socket(_socket.AF_UNIX)
    listener.bind(str(tmp_path / "daemon.sock"))
    try:
        yield tmp_path, old.pid, new.pid, intruder.pid
    finally:
        listener.close()
        for p in (old, new, intruder):
            p.terminate()
            p.wait()


def resolve_global_pid(tmp_path: Path, lsof_pids: list[int], registry: dict | None) -> str:
    import json as _json

    bin_dir = tmp_path / "bin"
    bin_dir.mkdir(exist_ok=True)
    lsof = bin_dir / "lsof"
    lsof.write_text("#!/bin/sh\n" + "".join(f"echo {p}\n" for p in lsof_pids))
    lsof.chmod(0o755)
    registry_dir = tmp_path / "registry"
    registry_dir.mkdir(exist_ok=True)
    for entry in registry_dir.glob("*.json"):
        entry.unlink()
    if registry is not None:
        (registry_dir / "0000abcd.json").write_text(_json.dumps(registry))
    harness = "\n".join(
        [
            "set -euo pipefail",
            f'DAEMON_SOCKET="{tmp_path}/daemon.sock"',
            f'DAEMON_REGISTRY_DIR="{registry_dir}"',
            shell_function("global_daemon_pid"),
            "global_daemon_pid",
        ]
    )
    env = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}"}
    proc = subprocess.run(["bash", "-c", harness], env=env, capture_output=True, text=True, timeout=30)
    return proc.stdout.strip()


def test_the_resolver_names_the_registered_owner_not_the_draining_predecessor(two_daemons):
    """14/09/2026: right after `daemon-ctl restart` the predecessor was still
    flushing KPIs with its listener open, `lsof | head -1` returned its lower pid,
    and the verify reported the successor's fresh deploy as a deleted binary."""
    tmp_path, old, new, _ = two_daemons
    sock = str(tmp_path / "daemon.sock")
    # The registered owner wins even when a later process also holds the socket.
    assert resolve_global_pid(tmp_path, [old, new], {"pid": old, "socket": sock}) == str(old)


def test_a_registered_pid_that_no_longer_holds_the_socket_is_not_the_owner(two_daemons):
    """Cross-audit 14/09/2026 (R2-11): a stale entry whose pid was recycled by
    another `touring-daemon` passed the `comm` test and named the wrong process."""
    tmp_path, old, new, _ = two_daemons
    sock = str(tmp_path / "daemon.sock")
    assert resolve_global_pid(tmp_path, [new], {"pid": old, "socket": sock}) == str(new)


def test_without_a_registry_the_newest_socket_holder_wins(two_daemons):
    tmp_path, old, new, intruder = two_daemons
    assert resolve_global_pid(tmp_path, [old, new, intruder], None) == str(new)


def test_a_registry_entry_for_another_socket_or_process_is_ignored(two_daemons):
    tmp_path, old, new, _ = two_daemons
    other = {"pid": old, "socket": "/elsewhere/.touring/daemon.sock"}
    assert resolve_global_pid(tmp_path, [old, new], other) == str(new)
    not_a_daemon = {"pid": os.getpid(), "socket": str(tmp_path / "daemon.sock")}
    assert resolve_global_pid(tmp_path, [old, new], not_a_daemon) == str(new)
