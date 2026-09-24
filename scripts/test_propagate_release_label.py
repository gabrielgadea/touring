#!/usr/bin/env python3
"""Guards for the LABEL GUARD in `scripts/propagate-release.sh` (2026-09-24).

The defect class it closes (the `propagacao-rotulo-nao-prova-build` /
`bump-de-versao-antes-do-build` family): the 30.4.66 toolchain was installed
with a binary that DECLARES itself 30.4.65, because the workspace Cargo.toml
was never bumped — and `.touring/bin/touring --version` is a project's local
truth, so tomorrow's diagnostic would conclude the release never arrived.

Two layers, matching test_update_touring.py:

  · **Content invariants** run anywhere: the guard exists, reads `--version`
    with `2>&1` (never `2>/dev/null` — stderr is where the string LIVES), and
    the refusal teaches the bump.
  · **Behavioral** runs the REAL script against a fake tree: a binary
    declaring the wrong version must die with the teaching message; the right
    version must pass the guard. Skips (not fails) where a stub chain cannot
    be assembled — a gate that cannot look must not block.
"""

from __future__ import annotations

import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "propagate-release.sh"


def executable_lines() -> list[str]:
    out = []
    for raw in SCRIPT.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            out.append(line)
    return out


# ── content invariants ───────────────────────────────────────────────────────

def test_the_guard_exists_and_refuses_a_wrong_label():
    body = "\n".join(executable_lines())
    assert "BUILT_VERSION" in body, "the guard computes the built label"
    assert "rótulo divergente" in body, "the refusal is named"
    assert '!= "$VERSION"' in body, "it compares built vs target"


def test_the_guard_reads_stderr_never_deletes_it():
    body = "\n".join(executable_lines())
    assert '--version 2>&1' in body, "--version writes to STDERR; 2>&1 is mandatory"
    assert "--version 2>/dev/null" not in body, "2>/dev/null would blank the label and pass blind"


def test_the_refusal_teaches_the_bump():
    body = "\n".join(executable_lines())
    assert "Cargo.toml" in body and "bump" in body, "the message names the remedy, not just the failure"


# ── behavioral (fake tree) ───────────────────────────────────────────────────

def _fake_tree(declared: str) -> Path:
    tmp = Path(tempfile.mkdtemp(prefix="label-guard-"))
    (tmp / "scripts").mkdir(parents=True)
    shutil.copy(SCRIPT, tmp / "scripts" / "propagate-release.sh")
    os.chmod(tmp / "scripts" / "propagate-release.sh", 0o755)
    # The script refuses a non-canonical WORKSPACE before the guard runs.
    (tmp / "Cargo.toml").write_text('[workspace.package]\nversion = "0.0.0"\n[workspace]\nmembers = []\n',
                                    encoding="utf-8")
    bin_dir = tmp / "target" / "release"
    bin_dir.mkdir(parents=True)
    fake = bin_dir / "touring"
    # The REAL `--version` is MULTI-LINE (version + sha + date + features) —
    # a one-line fixture let an awk-$NF parse read the features list as the
    # version and kill a correct build (the production condition the audit
    # doctrine demands the fixture carry).
    fake.write_text(
        '#!/bin/sh\n'
        f'printf "touring {declared}\\n'
        'b8e2e7e68faf69285011c1ffd5a17579630d9f4f\\n'
        '2026-08-18\\n2026-09-24T04:12:23Z\\n'
        'analysis-gate,async-memory,console,default,otlp,tantivy-fts\\n" >&2\n',
        encoding="utf-8")
    fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
    return tmp


def _stub_bin(tmp: Path) -> Path:
    stub = tmp / "stubbin"
    stub.mkdir(exist_ok=True)
    (stub / "update-touring").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    (stub / "touring").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    for f in stub.iterdir():
        f.chmod(f.stat().st_mode | stat.S_IEXEC)
    return stub


def _run(tmp: Path, target: str) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    env["PATH"] = f"{_stub_bin(tmp)}:{os.environ['PATH']}"
    return subprocess.run(
        ["bash", str(tmp / "scripts" / "propagate-release.sh"), target,
         "--skip-gates", "--skip-freeze", "--no-default"],
        capture_output=True, text=True, timeout=60, env=env,
    )


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_wrong_label_dies_teaching_the_bump():
    tmp = _fake_tree("30.4.65")
    try:
        r = _run(tmp, "30.4.66")
        out = r.stdout + r.stderr
        assert r.returncode == 1, f"wrong label must die, got rc={r.returncode}: {out[-400:]}"
        assert "rótulo divergente" in out
        assert "30.4.65" in out and "30.4.66" in out
        assert "bump" in out
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_the_right_label_passes_the_guard():
    tmp = _fake_tree("30.4.66")
    try:
        r = _run(tmp, "30.4.66")
        out = r.stdout + r.stderr
        assert "rótulo divergente" not in out, f"the guard must pass: {out[-400:]}"
        assert "rótulo OK" in out
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_skip_build_does_not_bypass_the_label_guard():
    """D8 in miniature (coordinator, 2026-09-24): --skip-build installs the
    target/release binary WITHOUT rebuilding — if the guard depended on the
    build, the flag would be the label's bypass. It must die on a wrong label
    there too. Mutation: with the guard gated on SKIP_BUILD=0, this run
    installs the wrong-labeled binary without a word (rc≠1, no refusal)."""
    tmp = _fake_tree("30.4.65")
    try:
        env = dict(os.environ)
        env["PATH"] = f"{_stub_bin(tmp)}:{os.environ['PATH']}"
        r = subprocess.run(
            ["bash", str(tmp / "scripts" / "propagate-release.sh"), "30.4.66",
             "--skip-gates", "--skip-build", "--skip-freeze", "--no-default"],
            capture_output=True, text=True, timeout=60, env=env)
        out = r.stdout + r.stderr
        assert r.returncode == 1, f"--skip-build must NOT bypass the guard: rc={r.returncode}: {out[-300:]}"
        assert "rótulo divergente" in out
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def _fake_inheriting_crate(tmp: Path, pkg: str = "touring-cli") -> None:
    """A workspace member that INHERITS the label — the lock guard only judges
    these (independent crates like touring-analysis 0.3.3 stay out by design)."""
    d = tmp / "crates" / pkg
    d.mkdir(parents=True)
    (d / "Cargo.toml").write_text(
        f'[package]\nname = "{pkg}"\nversion.workspace = true\n',
        encoding="utf-8")


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_a_stale_cargo_lock_also_dies():
    """The lock is the same lie one directory down (the 4 previous bumps
    carried it): an INHERITING member off-label must refuse the install."""
    tmp = _fake_tree("30.4.66")
    try:
        _fake_inheriting_crate(tmp)
        (tmp / "Cargo.lock").write_text(
            '[[package]]\nname = "touring-cli"\nversion = "30.4.65"\n\n'
            '[[package]]\nname = "color"\nversion = "0.3.3"\n\n'
            '[[package]]\nname = "touring-analysis"\nversion = "0.3.3"\n',
            encoding="utf-8")
        r = _run(tmp, "30.4.66")
        out = r.stdout + r.stderr
        assert r.returncode == 1, f"a stale inheriting entry must die, got rc={r.returncode}: {out[-400:]}"
        assert "Cargo.lock fora do rótulo" in out
        # and the fix is carried: a lock at the target version passes — and an
        # independent 0.3.3 crate never counts as a lie (3rd stop, 2026-09-24).
        (tmp / "Cargo.lock").write_text(
            '[[package]]\nname = "touring-cli"\nversion = "30.4.66"\n\n'
            '[[package]]\nname = "touring-analysis"\nversion = "0.3.3"\n',
            encoding="utf-8")
        r2 = _run(tmp, "30.4.66")
        assert "Cargo.lock fora do rótulo" not in (r2.stdout + r2.stderr)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# ── real repo (coordinator's step 4, 2026-09-24) ─────────────────────────────

def _workspace_version() -> str:
    """The CURRENT label, derived from the single source of truth — never
    hardcoded. The 30.4.67 propagation bumped the workspace and two tests here
    died still expecting 30.4.66 (2026-09-24, REGRA #21): a version literal in
    a test goes stale the day the release it guards actually ships."""
    text = (REPO / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'\[workspace\.package\][^\[]*?version = "([^"]+)"', text, re.S)
    assert m, "the workspace version must be derivable from Cargo.toml"
    return m.group(1)


def _guard_alone() -> subprocess.CompletedProcess:
    # --skip-gates é obrigatório: o passo de gates roda cargo, que REGRAVA o
    # Cargo.lock — e apagaria a mutação sob teste antes da guarda ler (a
    # mutação morria na sala de espera, não na guarda).
    return subprocess.run(
        ["bash", str(SCRIPT), _workspace_version(),
         "--skip-gates", "--skip-build", "--check-label-only"],
        capture_output=True, text=True, timeout=120, cwd=REPO)


@pytest.mark.skipif(
    not (REPO / "Cargo.lock").is_file() or not (REPO / "target" / "release" / "touring").is_file(),
    reason="needs the real workspace + a built binary")
def test_the_guard_passes_against_the_real_repo():
    """The guard alone against the real workspace, at the CURRENT label —
    whatever version the last propagation left built and locked."""
    r = _guard_alone()
    out = r.stdout + r.stderr
    assert r.returncode == 0, f"the real repo must pass: {out[-400:]}"
    assert "rótulo verificado" in out


@pytest.mark.skipif(
    not (REPO / "Cargo.lock").is_file() or not (REPO / "target" / "release" / "touring").is_file(),
    reason="needs the real workspace + a built binary")
def test_a_mutated_inheriting_lock_entry_dies():
    """Mutation proof: one inheriting member off-label in the real lock → the
    guard dies; the file is restored afterwards and passes again."""
    lock = REPO / "Cargo.lock"
    original = lock.read_text(encoding="utf-8")
    try:
        mutated = re.sub(
            r'(name = "touring-cli"\nversion = ")' + re.escape(_workspace_version()) + '(")',
            r"\g<1>30.4.65\g<2>", original, count=1)
        assert mutated != original, "the mutation must hit the inheriting member"
        lock.write_text(mutated, encoding="utf-8")
        r = _guard_alone()
        out = r.stdout + r.stderr
        assert r.returncode == 1, f"a mutated inheriting entry must die: {out[-300:]}"
        assert "Cargo.lock fora do rótulo" in out
    finally:
        lock.write_text(original, encoding="utf-8")
    r2 = _guard_alone()
    assert r2.returncode == 0, "the restored lock passes again"


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
