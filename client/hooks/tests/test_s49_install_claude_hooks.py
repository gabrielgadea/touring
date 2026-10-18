"""test_s49_install_claude_hooks.py — S49: the global hooks section managed from a versioned source.

Every behavioural test runs the REAL script against scratch files (`--settings`, `--source`):
nothing here reads or writes ~/.claude/settings.json or the versioned source. The previous
version of this file checked `if` at the GROUP level (the field lives on each HOOK, so the
lint never fired), "simulated" --apply instead of running it, and accepted either exit code
from --lint-only — three passes nothing could fail (26/09/2026).
"""

from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[3]
INSTALL_SCRIPT = REPO_ROOT / "scripts" / "install_claude_hooks.py"
HOOKS_SETTINGS = REPO_ROOT / "client" / "claude" / "hooks.settings.json"
TOURING_PROJECT_HOOKS = REPO_ROOT / "client" / "claude" / "touring-project.hooks.settings.json"


def _run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(INSTALL_SCRIPT), *args], capture_output=True, text=True, timeout=30, check=False
    )


def _hook(command: str, *, timeout: int | None = 5, if_: str | None = None) -> dict[str, Any]:
    hook: dict[str, Any] = {"type": "command", "command": command}
    if if_ is not None:
        hook["if"] = if_
    if timeout is not None:
        hook["timeout"] = timeout
    return hook


def _write(path: Path, data: Any) -> Path:
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return path


def _lint(tmp_path: Path, hooks: dict[str, Any]) -> list[str]:
    src = _write(tmp_path / "hooks.settings.json", hooks)
    proc = _run("--lint-only", "--source", str(src), "--json")
    return [f["rule"] for f in json.loads(proc.stdout)["findings"]]


# ── lint: each rule fires on the REAL shape and stays quiet on a clean one ──────────────


def test_lint_sees_if_alternation_on_the_hook(tmp_path: Path) -> None:
    """L1 on the shape the live settings use: `if` inside the hook, with `|`."""
    bad = {"PreToolUse": [{"matcher": "Edit", "hooks": [_hook("x", if_="Edit(*.rs|*.py)")]}]}
    assert "L1" in _lint(tmp_path, bad)
    ok = {"PreToolUse": [{"matcher": "Edit", "hooks": [_hook("x", if_="Edit(*.rs)")]}]}
    assert _lint(tmp_path, ok) == []


def test_lint_sees_if_on_an_event_that_ignores_it(tmp_path: Path) -> None:
    """L2: outside the five tool events, `if` makes the hook never run (Claude Code docs)."""
    bad = {"SessionStart": [{"hooks": [_hook("x", if_="Bash(git *)")]}]}
    assert "L2" in _lint(tmp_path, bad)


def test_no_project_script_registered_globally(tmp_path: Path) -> None:
    """L3 for ANY project, not a hard-coded one."""
    for command in ("python3 $HOME/projects/analise/scripts/x.py", "~/projects/konverter/bin/hook.sh"):
        assert "L3" in _lint(tmp_path, {"PostToolUse": [{"matcher": "Bash", "hooks": [_hook(command)]}]}), command
    assert _lint(tmp_path, {"PostToolUse": [{"matcher": "Bash", "hooks": [_hook("$HOME/.claude/hooks/x")]}]}) == []


def test_lint_requires_a_timeout(tmp_path: Path) -> None:
    assert "L4" in _lint(tmp_path, {"Stop": [{"hooks": [_hook("x", timeout=None)]}]})


# ── check / apply against scratch files ────────────────────────────────────────────────


def _pair(tmp_path: Path, source_hooks: dict[str, Any], live_hooks: dict[str, Any]) -> tuple[Path, Path]:
    settings = _write(tmp_path / "settings.json", {"model": "x", "hooks": live_hooks, "env": {"K": "v"}})
    source = _write(tmp_path / "hooks.settings.json", source_hooks)
    return settings, source


CLEAN = {"Stop": [{"hooks": [_hook("$HOME/.claude/hooks/a")]}]}


def test_check_detects_manual_drift(tmp_path: Path) -> None:
    settings, source = _pair(tmp_path, CLEAN, CLEAN)
    assert _run("--check", "--settings", str(settings), "--source", str(source)).returncode == 0
    drifted = {"Stop": [{"hooks": [_hook("$HOME/.claude/hooks/a", timeout=9)]}]}
    settings, source = _pair(tmp_path, CLEAN, drifted)
    assert _run("--check", "--settings", str(settings), "--source", str(source)).returncode == 1


def test_apply_writes_the_source_and_preserves_everything_else(tmp_path: Path) -> None:
    new = {"Stop": [{"hooks": [_hook("$HOME/.claude/hooks/b")]}]}
    settings, source = _pair(tmp_path, new, CLEAN)
    os.chmod(settings, 0o600)
    inode = settings.stat().st_ino
    proc = _run("--apply", "--settings", str(settings), "--source", str(source))
    assert proc.returncode == 0, proc.stderr
    data = json.loads(settings.read_text(encoding="utf-8"))
    assert data["hooks"] == new
    assert (data["model"], data["env"]) == ("x", {"K": "v"})
    assert stat.S_IMODE(settings.stat().st_mode) == 0o600, "the file mode must survive"
    assert settings.stat().st_ino != inode, "written by atomic rename, never in place"
    assert not list(tmp_path.glob("*.next")), "no staging file left behind"


def test_apply_refuses_a_source_that_adds_a_lint_finding(tmp_path: Path) -> None:
    """Pre-existing debt does not block (S31 is its own phase); a NEW finding does."""
    worse = {"Stop": [{"hooks": [_hook("$HOME/.claude/hooks/a"), _hook("$HOME/.claude/hooks/c", timeout=None)]}]}
    settings, source = _pair(tmp_path, worse, CLEAN)
    before = settings.read_text(encoding="utf-8")
    proc = _run("--apply", "--settings", str(settings), "--source", str(source))
    assert proc.returncode == 1
    assert "L4" in proc.stderr
    assert settings.read_text(encoding="utf-8") == before, "a refused apply must not touch the file"
    # Control: the same debt already present in the live file does not block.
    settings, source = _pair(tmp_path, worse, worse)
    assert _run("--apply", "--settings", str(settings), "--source", str(source)).returncode == 0


def test_apply_refuses_an_empty_source(tmp_path: Path) -> None:
    """An empty source is a lint-clean way to delete every global hook — refused, file untouched."""
    settings, source = _pair(tmp_path, {}, CLEAN)
    before = settings.read_text(encoding="utf-8")
    proc = _run("--apply", "--settings", str(settings), "--source", str(source))
    assert proc.returncode == 1
    assert settings.read_text(encoding="utf-8") == before


# ── the versioned source itself ────────────────────────────────────────────────────────


def test_versioned_source_is_valid() -> None:
    hooks = json.loads(HOOKS_SETTINGS.read_text(encoding="utf-8"))
    assert isinstance(hooks, dict)
    assert hooks, "empty source: an --apply would wipe every global hook"


def test_if_fields_are_single_permission_rule() -> None:
    """S31 (27/09/2026): the six `if` with `|` never matched, so four handlers never ran; each is now
    one entry per extension. Was a strict xfail while the dead hooks stayed in the source."""
    proc = _run("--lint-only", "--source", str(HOOKS_SETTINGS), "--json")
    rules = [f["rule"] for f in json.loads(proc.stdout)["findings"]]
    assert "L1" not in rules and "L2" not in rules, rules


def test_touring_project_source_lints_clean() -> None:
    """D5-A: the Rust API-contract prompt/agent hooks left the global settings for the touring project's
    (`.claude/` is git-ignored there, so this versioned copy is what `--apply --settings` installs)."""
    proc = _run("--lint-only", "--source", str(TOURING_PROJECT_HOOKS), "--json")
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert json.loads(proc.stdout)["findings"] == []


def test_versioned_source_has_no_lint_finding() -> None:
    """Every rule stays clean in the global source — no backlog exception left after S31."""
    proc = _run("--lint-only", "--source", str(HOOKS_SETTINGS), "--json")
    findings = json.loads(proc.stdout)["findings"]
    assert findings == [], findings


def test_cli_suggest_runs_for_the_agent_tool() -> None:
    """17/10/2026: the sub-agent tool is `Agent` (`Task` is the legacy name). The group that ran
    `cli-suggest` was keyed on `Task` only, and the hook payload carries `tool_name: "Agent"` (the
    model gate keys on it), so no delegation ever got its nudge — 0 emissions in 227 transcripts."""
    hooks = json.loads(HOOKS_SETTINGS.read_text(encoding="utf-8"))
    covered = {
        name
        for group in hooks["PreToolUse"]
        if any("touring-hook cli-suggest" in h.get("command", "") for h in group.get("hooks", []))
        for name in group.get("matcher", "").split("|")
    }
    assert "Agent" in covered, f"cli-suggest never runs for the Agent tool; covered: {sorted(covered)}"
