"""test_s48_install.py — S48: every registered global script is a symlink into the repo.

RED: before S48 symlinks are created, test_every_registered_global_script_is_a_symlink_into_repo fails.
GREEN: after install_claude_hooks.py --apply and symlink creation, all pass.
"""
from __future__ import annotations

import json
import os
from pathlib import Path

import pytest

HOME = Path.home()
REPO_ROOT = Path(__file__).resolve().parents[3]
GLOBAL_HOOKS = HOME / ".claude" / "hooks"
CLIENT_HOOKS = REPO_ROOT / "client" / "hooks"
SETTINGS_PATH = HOME / ".claude" / "settings.json"

# Scripts that MUST be versioned in client/hooks/ (S48 minimum set).
REQUIRED_SCRIPTS = [
    "touring-quality-block-all.sh",
    "block_git.sh",
    "touring-process-guard.sh",
    "doc-coverage-gate.sh",
    "check_context.sh",
    "telemetry_logger.sh",
    "guard_settings.sh",
]


def _get_registered_global_shell_scripts() -> list[str]:
    """Return script paths registered in settings.json that live in ~/.claude/hooks/."""
    with open(SETTINGS_PATH) as f:
        settings = json.load(f)
    scripts = []
    for event, entries in settings.get("hooks", {}).items():
        for entry in entries:
            for h in entry.get("hooks", []):
                cmd = h.get("command", "")
                for part in cmd.split():
                    expanded = part.replace("$HOME", str(HOME)).replace('"', "").replace("'", "")
                    p = Path(expanded)
                    if str(GLOBAL_HOOKS) in str(p) and p.suffix in (".sh", ".py", ".cjs", ""):
                        scripts.append(str(p))
    return list(set(scripts))


def test_required_scripts_exist_in_client_hooks() -> None:
    """All S48-required scripts are present in client/hooks/."""
    missing = []
    for name in REQUIRED_SCRIPTS:
        target = CLIENT_HOOKS / name
        if not target.exists():
            missing.append(name)
    assert not missing, f"Missing in client/hooks/: {missing}"


def test_every_registered_global_script_is_a_symlink_into_repo() -> None:
    """Every .sh script registered in settings.json hooks that lives in ~/.claude/hooks/
    is a symlink pointing into ~/projects/touring/client/hooks/.

    RED before symlinks are created. GREEN after install.
    """
    registered = _get_registered_global_shell_scripts()
    sh_scripts = [p for p in registered if p.endswith(".sh")]
    if not sh_scripts:
        pytest.skip("No shell scripts registered in settings.json")

    not_symlinks = []
    wrong_target = []
    for script_path in sh_scripts:
        p = Path(script_path)
        if not p.exists():
            continue  # script missing entirely — tested elsewhere
        if not p.is_symlink():
            not_symlinks.append(str(p))
        else:
            resolved = p.resolve()
            if str(CLIENT_HOOKS) not in str(resolved):
                wrong_target.append(f"{p} → {resolved}")

    errors = []
    if not_symlinks:
        errors.append(f"Not symlinks: {not_symlinks}")
    if wrong_target:
        errors.append(f"Wrong symlink targets (not in client/hooks/): {wrong_target}")
    assert not errors, "\n".join(errors)


def test_client_hooks_scripts_are_executable() -> None:
    """All .sh files in client/hooks/ have execute permission."""
    sh_files = list(CLIENT_HOOKS.glob("*.sh"))
    assert sh_files, "No .sh files found in client/hooks/"
    non_exec = [str(f) for f in sh_files if not os.access(f, os.X_OK)]
    assert not non_exec, f"Not executable: {non_exec}"


def test_no_binary_or_daemon_in_client_hooks() -> None:
    """S48: touring-hook shim and touring-daemon binaries must NOT be in client/hooks/."""
    banned = ["touring-hook", "touring-daemon", "touring"]
    found = []
    for name in banned:
        p = CLIENT_HOOKS / name
        if p.exists() and not p.name.endswith(".sh") and not p.name.endswith(".py"):
            found.append(str(p))
    assert not found, f"Binary shims must not be in client/hooks/: {found}"
