#!/usr/bin/env python3
"""Guards for the `gen_reference.py --validate` check in gate 1/6 of
`scripts/propagate-release.sh` (2026-09-24, touring-36).

The defect it closes: the elite 06_documentation gate failed on reference
drift (hooks 239→241, modules 360→377) and NOTHING in the release rite
watched it — the coordinator only saw it by running `elite_aggregate
--check` by hand. A release that ships a stale reference documents commands
that already changed shape, and the rite that never checks it cannot be
trusted to catch the next drift.

Two layers, matching test_propagate_release_label.py:

  · **Content invariants** run anywhere: the call exists in the gates block
    and the refusal teaches the regeneration command.
  · **Behavioral** runs the REAL script against a fake tree: a stub
    `gen_reference.py` exiting 1 (drift) must die with the teaching message;
    exiting 0 (in sync) must let the gates pass. A mutation that removes the
    check makes the drift run pass — and the first test fails.
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

def test_the_gates_block_validates_the_generated_reference():
    body = "\n".join(executable_lines())
    assert "docs/gen_reference.py --validate" in body, \
        "gate 1/6 must call the generated-reference validation"


def test_the_refusal_teaches_the_regeneration_command():
    body = "\n".join(executable_lines())
    assert "python3 docs/gen_reference.py" in body, \
        "the die must teach the fix, not just name the failure"


# ── behavioral (fake tree) ───────────────────────────────────────────────────

def _fake_tree() -> Path:
    """The same minimal shape the label-guard tests use: a workspace the
    script accepts and a fake binary declaring the right label, so every
    OTHER gate passes and only the reference check is in play."""
    tmp = Path(tempfile.mkdtemp(prefix="genref-gate-"))
    (tmp / "scripts").mkdir(parents=True)
    shutil.copy(SCRIPT, tmp / "scripts" / "propagate-release.sh")
    os.chmod(tmp / "scripts" / "propagate-release.sh", 0o755)
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


def _stub_bin(tmp: Path, gen_rc: int) -> Path:
    """Stub every external the gates and the rest of the script call; the
    python3 dispatcher fails ONLY on gen_reference (gen_rc), passing the rest."""
    stub = tmp / "stubbin"
    stub.mkdir(exist_ok=True)
    (stub / "cargo").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    (stub / "update-touring").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    (stub / "touring").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    (stub / "python3").write_text(
        "#!/bin/sh\n"
        'case " $*" in\n'
        f"  *gen_reference.py*) exit {gen_rc} ;;\n"
        "  *) exit 0 ;;\n"
        "esac\n",
        encoding="utf-8")
    for f in stub.iterdir():
        f.chmod(f.stat().st_mode | stat.S_IEXEC)
    return stub


def _run(tmp: Path, gen_rc: int) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    env["PATH"] = f"{_stub_bin(tmp, gen_rc)}:{os.environ['PATH']}"
    return subprocess.run(
        ["bash", str(tmp / "scripts" / "propagate-release.sh"), "30.4.67",
         "--skip-build", "--skip-freeze", "--no-default"],
        capture_output=True, text=True, timeout=120, env=env,
    )


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_reference_drift_dies_teaching_the_regeneration():
    """Drift (validate exits 1) must fail the gates with the teaching message.
    Mutation-sensitive: with the check removed, this run PASSES — and this
    test fails."""
    tmp = _fake_tree()
    try:
        r = _run(tmp, gen_rc=1)
        out = r.stdout + r.stderr
        assert r.returncode != 0, f"drift must die, got rc={r.returncode}: {out[-300:]}"
        assert "python3 docs/gen_reference.py" in out, \
            f"the refusal must teach the regeneration command: {out[-300:]}"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@pytest.mark.skipif(shutil.which("bash") is None, reason="bash needed")
def test_an_in_sync_reference_passes_the_gates():
    tmp = _fake_tree()
    try:
        r = _run(tmp, gen_rc=0)
        out = r.stdout + r.stderr
        assert "gates OK" in out, f"the gates must pass in sync: {out[-300:]}"
        assert "drift" not in out.lower() or "gen_reference" not in out
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
