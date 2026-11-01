"""Scaffolding shared by the `test_propagate_release_*` guards of `scripts/propagate-release.sh`.

The label, glob and gen-reference guards each read the script's executable lines and build the
same minimal fake release tree (a copy of the script plus a stubbed S1a gate). That code lived,
byte for byte, in all three; it lives here once.

Named without the `test_` prefix on purpose: the rite's own glob is `test_propagate_release*.py`,
and a helper that matched it would be collected as a rite test.
"""
from __future__ import annotations

import os
import shutil
import stat
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "propagate-release.sh"


def executable_lines() -> list[str]:
    """The script's non-blank, non-comment lines, stripped."""
    out = []
    for raw in SCRIPT.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            out.append(line)
    return out


def fake_tree_with_script(prefix: str) -> Path:
    """A scratch workspace whose `scripts/` holds an executable copy of `propagate-release.sh`."""
    tmp = Path(tempfile.mkdtemp(prefix=prefix))
    (tmp / "scripts").mkdir(parents=True)
    shutil.copy(SCRIPT, tmp / "scripts" / "propagate-release.sh")
    os.chmod(tmp / "scripts" / "propagate-release.sh", 0o755)
    return tmp


def stub_s1a_gate(tmp: Path) -> None:
    """Replace the S1a gate (r3, XA-13) by a stub that passes: the fake tree has no hw-slot."""
    s1a = tmp / "scripts" / "r3-s1a-gate"
    s1a.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    s1a.chmod(0o755)


def write_canonical_workspace_manifest(tmp: Path) -> None:
    """The minimal root `Cargo.toml` the script accepts as a canonical workspace."""
    (tmp / "Cargo.toml").write_text(
        '[workspace.package]\nversion = "0.0.0"\n[workspace]\nmembers = []\n',
        encoding="utf-8")


def write_fake_touring(tmp: Path, declared: str = "30.4.67") -> None:
    """A fake `target/release/touring` whose multi-line `--version` (on STDERR) declares ``declared``.

    The real `--version` is MULTI-LINE (version + sha + date + features): a one-line fixture let
    an awk-$NF parse read the features list as the version and kill a correct build.
    """
    bin_dir = tmp / "target" / "release"
    bin_dir.mkdir(parents=True)
    fake = bin_dir / "touring"
    fake.write_text(
        '#!/bin/sh\n'
        f'printf "touring {declared}\\n'
        'b8e2e7e68faf69285011c1ffd5a17579630d9f4f\\n'
        '2026-08-18\\n2026-09-24T04:12:23Z\\n'
        'analysis-gate,async-memory,console,default,otlp,tantivy-fts\\n" >&2\n',
        encoding="utf-8")
    fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
