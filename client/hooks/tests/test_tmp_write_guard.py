"""Tests for client/hooks/tmp_write_guard.py — run as a subprocess with a real payload, as Claude Code does."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

HOOK = Path(__file__).resolve().parents[1] / "tmp_write_guard.py"


@pytest.fixture
def tmp_path():
    """Overrides pytest's tmp_path: that one lives under /tmp, whose `tmp` component the guard itself judges."""
    base = Path(tempfile.mkdtemp(prefix="guard-test-", dir=Path.home()))
    yield base
    shutil.rmtree(base, ignore_errors=True)


@pytest.fixture
def env(tmp_path: Path) -> dict:
    """A fake temp root and a home inside tmp_path, so pytest's own /tmp never counts as ephemeral."""
    root = tmp_path / "fakeroot"
    root.mkdir()
    home = tmp_path / "home"
    home.mkdir()
    e = {k: v for k, v in os.environ.items() if not k.startswith("TMP_WRITE_GUARD")}
    e.update(
        HOME=str(home),
        TMP_WRITE_GUARD_ROOTS=str(root),
        TMP_WRITE_GUARD_DEST=str(home / "artefatos"),
    )
    return e


@pytest.fixture
def repo(tmp_path: Path) -> Path:
    r = tmp_path / "repo"
    (r / "src" / "cache").mkdir(parents=True)
    subprocess.run(["git", "init", "-q", str(r)], check=True)
    (r / ".gitignore").write_text(".cache/\nbuild/\n")
    return r


def run(env: dict, tool: str, tool_input: dict, cwd: Path) -> dict | None:
    payload = {
        "tool_name": tool,
        "tool_input": tool_input,
        "cwd": str(cwd),
        "session_id": "t",
    }
    p = subprocess.run(
        [sys.executable, str(HOOK)],
        input=json.dumps(payload),
        capture_output=True,
        text=True,
        env=env,
        timeout=10,
        check=False,
    )
    assert p.returncode == 0, p.stderr
    return json.loads(p.stdout) if p.stdout.strip() else None


def denied(out: dict | None) -> bool:
    return bool(out) and out["hookSpecificOutput"]["permissionDecision"] == "deny"


def bash(env, cmd, cwd):
    return run(env, "Bash", {"command": cmd}, cwd)


# ---------------------------------------------------------------- Write / Edit tools


@pytest.mark.parametrize(
    "tool,key",
    [
        ("Write", "file_path"),
        ("Edit", "file_path"),
        ("MultiEdit", "file_path"),
        ("NotebookEdit", "notebook_path"),
    ],
)
def test_file_tools_under_root_denied(env, tmp_path, tool, key):
    out = run(env, tool, {key: str(tmp_path / "fakeroot" / "relatorio.md")}, tmp_path)
    assert denied(out)
    reason = out["hookSpecificOutput"]["permissionDecisionReason"]
    assert (
        "artefatos" in reason and "relatorio.md" in reason
    )  # the route back carries the real file name


def test_write_durable_path_passes(env, tmp_path):
    assert (
        run(
            env,
            "Write",
            {"file_path": str(tmp_path / "home" / "docs" / "a.md")},
            tmp_path,
        )
        is None
    )


def test_versioned_cache_dir_in_repo_passes(env, repo):
    assert (
        run(env, "Write", {"file_path": str(repo / "src" / "cache" / "mod.rs")}, repo)
        is None
    )


def test_ignored_cache_dir_in_repo_denied(env, repo):
    assert denied(
        run(env, "Write", {"file_path": str(repo / ".cache" / "x.json")}, repo)
    )


@pytest.mark.parametrize("name", ["scratch", "scratchpad", "tmp", "TEMP", "cache"])
def test_ephemeral_component_outside_git_denied(env, tmp_path, name):
    assert denied(
        run(
            env,
            "Write",
            {"file_path": str(tmp_path / "home" / name / "f.txt")},
            tmp_path,
        )
    )


def test_relative_path_resolved_against_cwd(env, tmp_path):
    assert denied(run(env, "Write", {"file_path": "x.md"}, tmp_path / "fakeroot"))


def test_symlink_into_root_denied(env, tmp_path):
    link = tmp_path / "home" / "link"
    link.symlink_to(tmp_path / "fakeroot")
    assert denied(run(env, "Write", {"file_path": str(link / "f.md")}, tmp_path))


def test_allow_prefix_and_kill_switch(env, tmp_path):
    target = {"file_path": str(tmp_path / "fakeroot" / "f.md")}
    assert (
        run(
            {**env, "TMP_WRITE_GUARD_ALLOW": str(tmp_path / "fakeroot")},
            "Write",
            target,
            tmp_path,
        )
        is None
    )
    assert (
        run({**env, "TMP_WRITE_GUARD_DISABLED": "1"}, "Write", target, tmp_path) is None
    )


def test_similar_names_are_not_components(env, tmp_path):
    for p in ["home/cache-design.md", "home/tmp-tools/a.py", "home/my_scratch/a.py"]:
        assert run(env, "Write", {"file_path": str(tmp_path / p)}, tmp_path) is None, p


# ---------------------------------------------------------------- Bash


@pytest.mark.parametrize(
    "cmd",
    [
        "echo hi > {R}/a.log",
        "echo hi >> {R}/a.log",
        "cargo test 2> {R}/err.log",
        "make &> {R}/build.log",
        'echo hi > "{R}/com espaço.md"',
        "ls | tee {R}/out.txt",
        "tee -a {R}/out.txt < in",
        "cp relatorio.md {R}/",
        "cp -t {R} a b",
        "mv a.md {R}/b.md",
        "rsync -a dir/ {R}/dir/",
        "mkdir -p {R}/work",
        "touch {R}/x",
        "dd if=/dev/zero of={R}/img bs=1M count=1",
        "curl -sL https://example.com -o {R}/page.html",
        "curl -sL --output={R}/page.html https://example.com",
        "wget -O {R}/f https://example.com",
        "sed -i 's/a/b/' {R}/notes.md",
        "git clone https://example.com/r.git {R}/r",
        "cd {R} && echo x > out.txt",
        "bash -c 'echo x > {R}/a'",
        "timeout 60 nice cp a {R}/b",
        "cat > {R}/plano.md <<'EOF'\nconteudo\nEOF",
        "true; echo ok > {R}/a",
    ],
)
def test_bash_writes_to_root_denied(env, tmp_path, cmd):
    assert denied(bash(env, cmd.format(R=tmp_path / "fakeroot"), tmp_path)), cmd


@pytest.mark.parametrize(
    "cmd",
    [
        "cat {R}/a.log",
        "grep foo {R}/a.log > out.txt",
        "cp {R}/a.log ./a.log",
        "rm -rf {R}/old",
        'echo "grave > {R}/x"',
        "cat > notes.md <<'EOF'\nexemplo: echo x > {R}/y\nEOF",
        "ls 2>/dev/null",
        "cmd 2>&1 | head",
        "diff <(sort a) <(sort b) > out.txt",
    ],
)
def test_bash_reads_and_durable_writes_pass(env, tmp_path, cmd):
    assert bash(env, cmd.format(R=tmp_path / "fakeroot"), tmp_path) is None, cmd


def test_bash_escape_token_passes_and_is_logged(env, tmp_path):
    cmd = f"TMP_WRITE_OK=1 cargo test > {tmp_path / 'fakeroot'}/run.log"
    assert bash(env, cmd, tmp_path) is None
    log = Path(env["HOME"]) / ".claude" / "touring" / "tmp_write_guard" / "events.jsonl"
    events = [json.loads(line) for line in log.read_text().splitlines()]
    assert events[-1]["decision"] == "escape"


def test_bash_deny_lists_every_target(env, tmp_path):
    r = tmp_path / "fakeroot"
    out = bash(env, f"echo a > {r}/1; echo b > {r}/2", tmp_path)
    assert "+1 outro(s)" in out["hookSpecificOutput"]["permissionDecisionReason"]


def test_malformed_payload_fails_open(env, tmp_path):
    p = subprocess.run(
        [sys.executable, str(HOOK)],
        input="not json",
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )
    assert p.returncode == 0 and p.stdout == ""


def test_unbalanced_quote_fails_open(env, tmp_path):
    assert bash(env, "echo 'unterminated", tmp_path) is None
