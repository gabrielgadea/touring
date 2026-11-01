#!/usr/bin/env python3
"""The rite proof for the HEAD-mirror gate (30.4.69, touring-36).

The measured recurrence (25/09): a commit landed 3 mirror files with a STALE
manifest and every working-tree check passed — the `--check` reads the working
tree, where the updated manifest exists. These tests build a fake git repo per
case and run `check_mirror_at_head.py` against its HEAD: consistent → exit 0;
mirror file committed with an old manifest → exit 1 NAMING the file; a file
committed without a manifest entry → exit 1 naming it. Plus the D8 wiring
assertion: `propagate-release.sh` must invoke the checker (the declaration and
the executor reconciled by test, not good will).
"""

from __future__ import annotations

import hashlib
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
CHECKER = ROOT / "scripts" / "check_mirror_at_head.py"
PROPAGATE = ROOT / "scripts" / "propagate-release.sh"


def _git(repo: Path, *args: str) -> None:
    subprocess.run(
        ["git", "-c", "user.email=rite@local", "-c", "user.name=rite", *args],
        cwd=repo, check=True, capture_output=True,
    )


def _repo_with_mirror(tmp_path: Path, file_body: str) -> Path:
    repo = tmp_path / "repo"
    target = repo / "client" / "skills" / "x"
    target.mkdir(parents=True)
    (target / "SKILL.md").write_text(file_body, encoding="utf-8")
    _git(repo, "init", "-q")
    return repo


def _commit_with_manifest(repo: Path) -> None:
    blob = repo / "client" / "skills" / "x" / "SKILL.md"
    sha = hashlib.sha256(blob.read_bytes()).hexdigest()
    (repo / "client" / "MANIFEST.sha256").write_text(f"{sha}  skills/x/SKILL.md\n", encoding="utf-8")
    _git(repo, "add", "-A")
    _git(repo, "commit", "-q", "-m", "mirror")


def _check(repo: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(CHECKER)], cwd=repo, capture_output=True, text=True
    )


def test_head_consistent_passes(tmp_path):
    repo = _repo_with_mirror(tmp_path, "# skill v1\n")
    _commit_with_manifest(repo)
    proc = _check(repo)
    assert proc.returncode == 0, proc.stdout
    assert "íntegro" in proc.stdout


def test_mirror_committed_with_stale_manifest_dies_naming_the_file(tmp_path):
    """The 7789b9c0 case: the mirror file changed in HEAD, the manifest did not."""
    repo = _repo_with_mirror(tmp_path, "# skill v1\n")
    _commit_with_manifest(repo)
    blob = repo / "client" / "skills" / "x" / "SKILL.md"
    blob.write_text("# skill v2 — mudou\n", encoding="utf-8")
    _git(repo, "add", "-A")
    _git(repo, "commit", "-q", "-m", "espelho novo, manifesto velho")
    proc = _check(repo)
    assert proc.returncode == 1
    assert "skills/x/SKILL.md" in proc.stdout, "o arquivo divergente é nomeado"
    assert "manifesto velho" in proc.stdout


def test_file_committed_without_manifest_entry_dies_naming_it(tmp_path):
    repo = _repo_with_mirror(tmp_path, "# skill v1\n")
    _commit_with_manifest(repo)
    extra = repo / "client" / "skills" / "y"
    extra.mkdir(parents=True)
    (extra / "SKILL.md").write_text("# skill nova sem entrada\n", encoding="utf-8")
    _git(repo, "add", "-A")
    _git(repo, "commit", "-q", "-m", "arquivo sem manifesto")
    proc = _check(repo)
    assert proc.returncode == 1
    assert "skills/y/SKILL.md" in proc.stdout, "o arquivo sem entrada é nomeado"


def test_a_subtree_the_manifest_does_not_declare_is_not_flagged(tmp_path):
    """`client/omarchy/` exists at HEAD but is not the skills mirror — the
    sweep must not demand manifest coverage for it (90+ false positives on the
    real HEAD before the sweep was scoped to the manifest's declared dirs)."""
    repo = _repo_with_mirror(tmp_path, "# skill v1\n")
    other = repo / "client" / "omarchy" / "bin"
    other.mkdir(parents=True)
    (other / "tool.sh").write_text("#!/bin/sh\n", encoding="utf-8")
    _commit_with_manifest(repo)
    proc = _check(repo)
    assert proc.returncode == 0, proc.stdout


def test_the_rite_wires_the_checker_into_gate_1():
    """D8: a declaração (o gate) e o executor (este arquivo) reconciliados."""
    body = PROPAGATE.read_text(encoding="utf-8")
    assert "check_mirror_at_head.py" in body, (
        "o gate 1/6 do propagate-release.sh precisa invocar o verificador do HEAD"
    )


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
