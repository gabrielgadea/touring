"""Trust boundary of the walk-up hook shim (scripts/hooks/touring-hook-shim.sh).

17/10/2026 (ECC analysis, C2): the user-level shim `~/.claude/hooks/touring-hook` walked up from the project
directory and exec'd the FIRST executable `.touring/bin/touring-hook` it met. Any repository carrying that file
ran its own code as the user on every hook event of a session opened inside it, around Claude Code's folder
trust. A per-project binary is now trusted only when it resolves INTO the user's installed toolchains
(`$TOURING_HOME/toolchains/`, which is what `touring update --project` links to), is owned by the user and is
not group/other-writable. Anything else is ignored (logged once) and resolution falls through.

Every case runs the shim in a private HOME, so no real binary is ever reached.
"""
from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
SHIM = Path(os.environ.get("TOURING_HOOK_SHIM_UNDER_TEST", str(REPO / "scripts/hooks/touring-hook-shim.sh")))


def _bin(path: Path, tag: str, mode: int = 0o755) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"#!/bin/sh\necho {tag}\n", encoding="utf-8")
    path.chmod(mode)
    return path


@pytest.fixture()
def mundo(tmp_path: Path) -> dict[str, Path]:
    home = tmp_path / "home"
    touring_home = home / ".touring"
    _bin(touring_home / "toolchains/9.9.9/bin/touring-hook", "TOOLCHAIN-9.9.9")
    _bin(touring_home / "toolchains/8.8.8/bin/touring-hook", "TOOLCHAIN-8.8.8")
    (touring_home / "default").write_text("9.9.9\n", encoding="utf-8")
    project = tmp_path / "proj"
    (project / "src").mkdir(parents=True)
    return {"tmp": tmp_path, "home": home, "touring_home": touring_home, "project": project}


def _run(m: dict[str, Path], start: Path | None = None, path_prefix: Path | None = None) -> subprocess.CompletedProcess:
    start = start or m["project"]
    env = {
        "HOME": str(m["home"]),
        "TOURING_HOME": str(m["touring_home"]),
        "CLAUDE_PROJECT_DIR": str(start),
        "PATH": (f"{path_prefix}:" if path_prefix else "") + "/usr/bin:/bin",
        "TOURING_HOOK_SHIM_TRACE": "1",
    }
    return subprocess.run(["bash", str(SHIM), "probe"], cwd=start, env=env, capture_output=True, text=True,
                          timeout=30, check=False)


def _link(m: dict[str, Path], version: str, where: Path | None = None) -> Path:
    dst = (where or m["project"]) / ".touring/bin/touring-hook"
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.symlink_to(m["touring_home"] / f"toolchains/{version}/bin/touring-hook")
    return dst


def test_a_project_pinned_to_an_installed_toolchain_runs_that_toolchain(mundo):
    _link(mundo, "8.8.8")  # the pin differs from the user default, so Layer 2 is what answered
    p = _run(mundo, mundo["project"] / "src")
    assert p.stdout.strip() == "TOOLCHAIN-8.8.8", p.stderr


def test_a_repository_provided_executable_is_never_run(mundo):
    _bin(mundo["project"] / ".touring/bin/touring-hook", "EVIL")
    p = _run(mundo)
    assert "EVIL" not in p.stdout
    assert p.stdout.strip() == "TOOLCHAIN-9.9.9", "falls through to the user default toolchain"
    log = mundo["home"] / ".claude/touring/logs/hook-shim-rejected.log"
    assert log.is_file() and ".touring/bin/touring-hook" in log.read_text(encoding="utf-8")


def test_a_symlink_pointing_outside_the_toolchains_is_rejected(mundo):
    evil = _bin(mundo["tmp"] / "elsewhere/touring-hook", "EVIL")
    dst = mundo["project"] / ".touring/bin/touring-hook"
    dst.parent.mkdir(parents=True)
    dst.symlink_to(evil)
    assert "EVIL" not in _run(mundo).stdout


def test_a_toolchains_lookalike_prefix_is_rejected(mundo):
    # `$TOURING_HOME/toolchains-evil/...` shares the string prefix but is not inside `toolchains/`.
    evil = _bin(mundo["touring_home"] / "toolchains-evil/bin/touring-hook", "EVIL")
    dst = mundo["project"] / ".touring/bin/touring-hook"
    dst.parent.mkdir(parents=True)
    dst.symlink_to(evil)
    assert "EVIL" not in _run(mundo).stdout


def test_a_group_or_world_writable_toolchain_binary_is_rejected(mundo):
    (mundo["touring_home"] / "toolchains/8.8.8/bin/touring-hook").chmod(0o777)
    _link(mundo, "8.8.8")
    assert _run(mundo).stdout.strip() == "TOOLCHAIN-9.9.9"


def test_a_toolchain_binary_owned_by_another_user_is_rejected(mundo):
    # Creating a file owned by another uid needs root, so a `stat` shim on PATH reports the pinned binary as
    # root-owned; every other call reaches the real stat. The check under test stays exactly as shipped.
    real = (mundo["touring_home"] / "toolchains/8.8.8/bin/touring-hook").resolve()
    fake = mundo["tmp"] / "fakebin"
    fake.mkdir()
    (fake / "stat").write_text(
        f'#!/bin/sh\nfor a in "$@"; do [ "$a" = "{real}" ] && {{ echo "0 -rwxr-xr-x"; exit 0; }}; done\n'
        'exec /usr/bin/stat "$@"\n',
        encoding="utf-8",
    )
    (fake / "stat").chmod(0o755)
    _link(mundo, "8.8.8")
    assert _run(mundo, path_prefix=fake).stdout.strip() == "TOOLCHAIN-9.9.9"


def test_an_untrusted_candidate_in_a_parent_does_not_shadow_a_trusted_one_above(mundo):
    outer = mundo["tmp"]
    _link(mundo, "8.8.8", where=outer)  # trusted pin at the outer level
    _bin(mundo["project"] / ".touring/bin/touring-hook", "EVIL")  # untrusted, nearer
    p = _run(mundo)
    assert "EVIL" not in p.stdout
    assert p.stdout.strip() == "TOOLCHAIN-8.8.8"


def test_no_candidate_keeps_the_user_default(mundo):
    assert _run(mundo).stdout.strip() == "TOOLCHAIN-9.9.9"
