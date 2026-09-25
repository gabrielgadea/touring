#!/usr/bin/env python3
"""Guards for the GLOB over `scripts/test_propagate_release*.py` in gate 1/6
of `scripts/propagate-release.sh` (2026-09-24, touring-36).

The defect it closes: the 30.4.67 bump broke `test_propagate_release_label.py`
(a stale version literal) and the rite never saw it, because gate 1/6 ran
exactly two NAMED test files. A list ages; a glob does not. Now every rite
test joins the gate by being named, never by being remembered — and an EMPTY
glob fails the gate, because a gate that looks and finds nothing is not
green, it is blind.

Meta by design: this file's own name matches the glob, so the rule protects
itself on every future propagation.

  · **Content invariants**: the glob exists in the gates block and the
    empty-glob refusal exists and names the blindness.
  · **Behavioral** (fake tree): with NO matching file the script dies with
    the empty-glob message; with one present, the gates proceed. A mutation
    that removes the block makes the empty case pass — and the test fails.
"""

from __future__ import annotations

import os
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

def test_the_gates_block_globs_the_rite_tests():
    body = "\n".join(executable_lines())
    assert "test_propagate_release*.py" in body, \
        "gate 1/6 must glob the rite tests, not list them"


def test_an_empty_glob_fails_the_gate_and_says_so():
    body = "\n".join(executable_lines())
    assert "nullglob" in body, "without nullglob an empty glob is a literal string, not zero files"
    assert "nenhum scripts/test_propagate_release*.py encontrado" in body, \
        "the empty-glob refusal must exist and say the gate is blind"


# ── behavioral (fake tree) ───────────────────────────────────────────────────

def _fake_tree(with_rite_test: bool) -> Path:
    tmp = Path(tempfile.mkdtemp(prefix="rite-glob-"))
    (tmp / "scripts").mkdir(parents=True)
    shutil.copy(SCRIPT, tmp / "scripts" / "propagate-release.sh")
    os.chmod(tmp / "scripts" / "propagate-release.sh", 0o755)
    if with_rite_test:
        (tmp / "scripts" / "test_propagate_release_dummy.py").write_text(
            "def test_dummy():\n    assert True\n", encoding="utf-8")
    (tmp / "Cargo.toml").write_text(
        '[workspace.package]\nversion = "0.0.0"\n[workspace]\nmembers = []\n',
        encoding="utf-8")
    bin_dir = tmp / "target" / "release"
    bin_dir.mkdir(parents=True)
    fake = bin_dir / "touring"
    fake.write_text(
        '#!/bin/sh\n'
        'printf "touring 30.4.67\\n'
        'b8e2e7e68faf69285011c1ffd5a17579630d9f4f\\n'
        '2026-08-18\\n2026-09-24T04:12:23Z\\n'
        'analysis-gate,async-memory,console,default,otlp,tantivy-fts\\n" >&2\n',
        encoding="utf-8")
    fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
    return tmp


def _stub_bin(tmp: Path) -> Path:
    stub = tmp / "stubbin"
    stub.mkdir(exist_ok=True)
    for name in ("cargo", "update-touring", "touring", "python3"):
        (stub / name).write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    for f in stub.iterdir():
        f.chmod(f.stat().st_mode | stat.S_IEXEC)
    return stub


def _run(tmp: Path) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    env["PATH"] = f"{_stub_bin(tmp)}:{os.environ['PATH']}"
    return subprocess.run(
        ["bash", str(tmp / "scripts" / "propagate-release.sh"), "30.4.67",
         "--skip-build", "--skip-freeze", "--no-default"],
        capture_output=True, text=True, timeout=120, env=env,
    )


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_an_empty_glob_dies_naming_the_blind_gate():
    """No test_propagate_release*.py in the tree → the gate must fail, and say
    it is blind. Mutation-sensitive: with the block removed, this run passes."""
    tmp = _fake_tree(with_rite_test=False)
    try:
        r = _run(tmp)
        out = r.stdout + r.stderr
        assert r.returncode != 0, f"an empty glob must die, got rc={r.returncode}: {out[-300:]}"
        assert "nenhum scripts/test_propagate_release*.py encontrado" in out, out[-300:]
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_a_present_rite_test_lets_the_gates_pass():
    tmp = _fake_tree(with_rite_test=True)
    try:
        r = _run(tmp)
        out = r.stdout + r.stderr
        assert "gates OK" in out, f"the gates must pass with a rite test present: {out[-300:]}"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))


# ── the two-phase fix (24/09/2026, gate 1/6 of 30.4.68) ─────────────────────
# The glob ran the binary-dependent tests at gate 1, BEFORE the build — and
# bump → gate 1 → build means the binary is always OLD there. The fix marks
# them `post_build`: gate 1 runs `-m "not post_build"`, and after the build +
# label check the rite runs `-m post_build`, where skipping is not allowed.


def test_the_script_runs_two_phases_with_the_marker():
    body = "\n".join(executable_lines())
    assert '-m "not post_build"' in body, "gate 1 must exclude the binary-dependent tests"
    assert "-m post_build" in body, "the post-build phase must run exactly them"
    assert "testes pós-build do rito OK" in body, "the phase exists and is named"


def test_the_label_tests_dependent_on_the_binary_are_marked():
    src = (REPO / "scripts" / "test_propagate_release_label.py").read_text(encoding="utf-8")
    assert src.count("@pytest.mark.post_build") == 2, \
        "the two binary-dependent tests must carry the marker"


def test_gate1_selection_excludes_and_post_build_selection_exercises():
    """The mechanism, self-contained: a failing `post_build` test is SKIPPED
    by the gate-1 selection (passes) and RUN by the post-build selection
    (fails) — the exact defect the glob created, inverted by the marker."""
    import subprocess
    import tempfile

    tmp = Path(tempfile.mkdtemp(prefix="postbuild-mech-"))
    test_file = tmp / "test_mech.py"
    test_file.write_text(
        "import pytest\n\n"
        "@pytest.mark.post_build\n"
        "def test_needs_the_built_binary():\n"
        "    assert False, 'this runs only in the post_build phase'\n\n"
        "def test_binary_independent():\n"
        "    assert True\n",
        encoding="utf-8",
    )
    gate1 = subprocess.run(
        [sys.executable, "-m", "pytest", str(test_file), "-q", "-m", "not post_build"],
        capture_output=True, text=True, timeout=120, cwd=tmp,
    )
    assert gate1.returncode == 0, (
        f"gate 1 (not post_build) must skip the binary-dependent test: {gate1.stdout[-300:]}"
    )
    post = subprocess.run(
        [sys.executable, "-m", "pytest", str(test_file), "-q", "-m", "post_build"],
        capture_output=True, text=True, timeout=120, cwd=tmp,
    )
    assert post.returncode != 0, (
        "the post_build selection must RUN the marked test (and here it fails, "
        "exactly like at gate 1 with an old binary)"
    )


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_the_post_build_phase_passes_in_the_full_script():
    """The fake tree with the right label: gate 1 excludes the marked tests,
    and the post-build phase runs them and passes."""
    tmp = _fake_tree(with_rite_test=True)
    try:
        r = _run(tmp)
        out = r.stdout + r.stderr
        assert "testes pós-build do rito OK" in out, (
            f"the post-build phase must run and pass: {out[-400:]}"
        )
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
