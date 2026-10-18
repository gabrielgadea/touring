"""Tests for snapshot_safety.py — the SNAPSHOT step of the Git Safety Protocol (REGRA #11), run on REAL git repos.

The script was born in a plan directory (10/10/2026), died with the /tmp it lived in, was recreated there on
17/10 and was never versioned or tested; `/checkpoint` depends on it, so it is promoted here with the invariant
that makes it a safety net instead of another way to lose work: the snapshot is ADDITIVE.

  · the worktree's files, the index file and HEAD are byte-identical after the run;
  · the new ref's tree holds the working tree as it was (tracked changes AND untracked files), minus .gitignore;
  · a ref that already exists is never overwritten, a clean tree makes no ref, a huge file refuses the snapshot;
  · nothing is stashed, pushed or branched into the current branch.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(os.environ.get("SNAPSHOT_SAFETY_SCRIPT") or Path(__file__).resolve().parent / "snapshot_safety.py")  # env: mutation runner
PREFIX = "safety/2026-10-18"
ENV = {**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t",
       "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"}


def _git(repo: Path, *args: str) -> str:
    done = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, env=ENV, check=True)
    return done.stdout.strip()


@pytest.fixture
def repo(tmp_path: Path) -> Path:
    root = tmp_path / "repo"
    root.mkdir()
    _git(root, "init", "-q", "-b", "main")
    (root / "tracked.txt").write_text("v1\n")
    (root / ".gitignore").write_text("ignored.log\n")
    _git(root, "add", "-A")
    _git(root, "commit", "-q", "-m", "init")
    return root


def _snapshot(repo: Path, name: str = "work", *flags: str, prefix: str = PREFIX) -> tuple[int, dict]:
    done = subprocess.run([sys.executable, str(SCRIPT), "--prefix", prefix, *flags, f"{name}={repo}"],
                          capture_output=True, text=True, env=ENV, check=False)
    lines = [line for line in done.stdout.splitlines() if line.strip()]
    return done.returncode, json.loads(lines[-1]) if lines else {}


def _digest(repo: Path) -> dict[str, str]:
    index = Path(_git(repo, "rev-parse", "--path-format=absolute", "--git-path", "index"))
    files = {str(p.relative_to(repo)): hashlib.sha256(p.read_bytes()).hexdigest()
             for p in sorted(repo.rglob("*")) if p.is_file() and ".git" not in p.relative_to(repo).parts}
    return {"head": _git(repo, "rev-parse", "HEAD"), "index": hashlib.sha256(index.read_bytes()).hexdigest(), **files}


def test_a_dirty_tree_is_captured_in_a_new_safety_ref_and_nothing_else_moves(repo: Path) -> None:
    (repo / "tracked.txt").write_text("v2 edited\n")
    (repo / "brand_new.py").write_text("print('untracked')\n")
    (repo / "ignored.log").write_text("noise\n")
    _git(repo, "add", "tracked.txt")  # a staged change must stay staged
    (repo / "tracked.txt").write_text("v3 edited again\n")
    before = _digest(repo)

    rc, out = _snapshot(repo)

    assert rc == 0 and out["status"] == "OK", out
    assert out["ref"] == f"refs/heads/{PREFIX}/work"
    assert _digest(repo) == before  # every file, the index and HEAD: byte-identical
    assert _git(repo, "show", f"{out['ref']}:tracked.txt") == "v3 edited again"
    assert _git(repo, "show", f"{out['ref']}:brand_new.py") == "print('untracked')"
    assert "ignored.log" not in _git(repo, "ls-tree", "-r", "--name-only", out["ref"])
    assert _git(repo, "rev-parse", f"{out['ref']}^") == before["head"]  # a child of HEAD
    assert _git(repo, "stash", "list") == ""
    assert _git(repo, "rev-parse", "--abbrev-ref", "HEAD") == "main"


def test_the_staged_state_survives(repo: Path) -> None:
    (repo / "tracked.txt").write_text("staged\n")
    _git(repo, "add", "tracked.txt")
    staged = _git(repo, "diff", "--cached", "--name-only")
    _snapshot(repo)
    assert _git(repo, "diff", "--cached", "--name-only") == staged == "tracked.txt"


def test_a_clean_tree_makes_no_ref(repo: Path) -> None:
    rc, out = _snapshot(repo)
    assert rc == 0 and out["status"] == "COBERTO-PELO-HEAD"
    assert _git(repo, "for-each-ref", f"refs/heads/{PREFIX}") == ""


def test_an_existing_ref_is_never_overwritten(repo: Path) -> None:
    (repo / "tracked.txt").write_text("first\n")
    rc, first = _snapshot(repo)
    assert rc == 0
    commit = _git(repo, "rev-parse", first["ref"])
    (repo / "tracked.txt").write_text("second\n")
    rc, again = _snapshot(repo)
    assert rc == 1 and again["status"] == "RECUSADO" and "já existe" in again["motivo"]
    assert _git(repo, "rev-parse", first["ref"]) == commit


def test_dry_run_measures_without_writing(repo: Path) -> None:
    (repo / "tracked.txt").write_text("edited\n")
    rc, out = _snapshot(repo, "work", "--dry-run")
    assert rc == 0 and out["status"] == "DRY-RUN" and out["caminhos_sobre_head"] == 1
    assert _git(repo, "for-each-ref", f"refs/heads/{PREFIX}") == ""


def test_a_file_above_the_ceiling_refuses_the_snapshot_and_names_it(repo: Path) -> None:
    (repo / "big.bin").write_bytes(b"x" * (2 << 20))
    rc, out = _snapshot(repo, "work", "--max-mb", "1")
    assert rc == 1 and out["status"] == "RECUSADO" and "big.bin" in out["acima_do_teto"][0]
    assert _git(repo, "for-each-ref", f"refs/heads/{PREFIX}") == ""


def test_the_prefix_must_live_under_safety(repo: Path) -> None:
    done = subprocess.run([sys.executable, str(SCRIPT), "--prefix", "feature/x", f"w={repo}"], capture_output=True,
                          text=True, env=ENV, check=False)
    assert done.returncode == 2 and "safety/" in done.stderr


def test_a_path_that_is_not_a_repository_is_an_error_line_not_a_traceback(tmp_path: Path) -> None:
    plain = tmp_path / "plain"
    plain.mkdir()
    rc, out = _snapshot(plain)
    assert rc == 1 and out["status"] == "ERRO"
