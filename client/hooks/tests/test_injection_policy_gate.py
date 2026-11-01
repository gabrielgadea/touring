"""injection_policy_gate.py denies a hook that would inject context without the policy.

Origin 27/10/2026: `script_portfolio_gate.py` was created and registered with no
policy consult; the after-the-fact guard caught it. The gate judges the write.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

GATE = Path(__file__).resolve().parent.parent / "injection_policy_gate.py"

INJECTOR = 'import json\nprint(json.dumps({"hookSpecificOutput": {"additionalContext": "dica"}}))\n'
COMPLIANT = (
    "import json, sys\nimport injection_policy\n"
    'if not injection_policy.allows("plugin-hook"):\n    sys.exit(0)\n' + INJECTOR
)
SHELL_FAIL_CLOSED = (
    '#!/bin/bash\nif ! python3 "$HOME/.claude/hooks/injection_policy.py" --allows plugin-hook; then exit 0; fi\n'
    'printf \'{"hookSpecificOutput":{"additionalContext":"x"}}\'\n'
)
ENFORCER = 'import json\nprint(json.dumps({"hookSpecificOutput": {"permissionDecision": "deny"}}))\n'


def _run(home: Path, payload: dict, **env: str) -> subprocess.CompletedProcess[str]:
    full_env = {**os.environ, "HOME": str(home), **env}
    full_env.pop("TOURING_INJECTION_GATE_DISABLED", None) if "TOURING_INJECTION_GATE_DISABLED" not in env else None
    return subprocess.run([sys.executable, str(GATE)], input=json.dumps(payload), capture_output=True,
                          text=True, env=full_env, timeout=30, check=False)


def _write(path: Path, content: str) -> dict:
    return {"tool_name": "Write", "tool_input": {"file_path": str(path), "content": content}}


def _denied(res: subprocess.CompletedProcess[str]) -> bool:
    assert res.returncode == 0, res.stderr
    if not res.stdout.strip():
        return False
    return json.loads(res.stdout)["hookSpecificOutput"]["permissionDecision"] == "deny"


def _settings(home: Path, scripts: list[Path]) -> Path:
    path = home / ".claude" / "settings.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    hooks = [{"type": "command", "command": f"python3 {s}"} for s in scripts]
    path.write_text(json.dumps({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": hooks}]}}))
    return path


def test_new_injector_hook_without_policy_is_denied(tmp_path: Path) -> None:
    res = _run(tmp_path, _write(tmp_path / "hooks" / "novo_hook.py", INJECTOR))
    assert _denied(res)
    assert "allows(" in json.loads(res.stdout)["hookSpecificOutput"]["permissionDecisionReason"]


def test_injector_that_consults_the_policy_is_allowed(tmp_path: Path) -> None:
    assert not _denied(_run(tmp_path, _write(tmp_path / "hooks" / "novo_hook.py", COMPLIANT)))


def test_shell_fail_closed_consult_is_denied(tmp_path: Path) -> None:
    res = _run(tmp_path, _write(tmp_path / "hooks" / "novo.sh", SHELL_FAIL_CLOSED))
    assert _denied(res)
    assert "if ! python3" in json.loads(res.stdout)["hookSpecificOutput"]["permissionDecisionReason"]


def test_enforcement_only_hook_is_allowed(tmp_path: Path) -> None:
    assert not _denied(_run(tmp_path, _write(tmp_path / "hooks" / "bloqueio.py", ENFORCER)))


def test_non_hook_file_and_hook_tests_are_ignored(tmp_path: Path) -> None:
    assert not _denied(_run(tmp_path, _write(tmp_path / "src" / "lib.py", INJECTOR)))
    assert not _denied(_run(tmp_path, _write(tmp_path / "hooks" / "tests" / "test_x.py", INJECTOR)))


def test_edit_that_removes_the_consult_is_denied(tmp_path: Path) -> None:
    hook = tmp_path / "hooks" / "existente.py"
    hook.parent.mkdir(parents=True)
    hook.write_text(COMPLIANT)
    edit = {"tool_name": "Edit", "tool_input": {
        "file_path": str(hook),
        "old_string": 'if not injection_policy.allows("plugin-hook"):\n    sys.exit(0)\n',
        "new_string": "",
    }}
    assert _denied(_run(tmp_path, edit))


def test_registered_script_outside_hooks_dir_is_gated(tmp_path: Path) -> None:
    script = tmp_path / "scripts" / "aviso.py"
    _settings(tmp_path, [script])
    assert _denied(_run(tmp_path, _write(script, INJECTOR)))


def test_registering_an_uncovered_injector_in_settings_is_denied(tmp_path: Path) -> None:
    script = tmp_path / "scripts" / "aviso.py"
    script.parent.mkdir(parents=True)
    script.write_text(INJECTOR)
    settings = _settings(tmp_path, [])
    new = json.dumps({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
        {"type": "command", "command": f"python3 {script}"}]}]}})
    assert _denied(_run(tmp_path, _write(settings, new)))


def test_registering_a_compliant_hook_in_settings_is_allowed(tmp_path: Path) -> None:
    script = tmp_path / "scripts" / "aviso.py"
    script.parent.mkdir(parents=True)
    script.write_text(COMPLIANT)
    settings = _settings(tmp_path, [])
    new = json.dumps({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
        {"type": "command", "command": f"python3 {script}"}]}]}})
    assert not _denied(_run(tmp_path, _write(settings, new)))


def test_kill_switch_disables_the_gate(tmp_path: Path) -> None:
    res = _run(tmp_path, _write(tmp_path / "hooks" / "novo_hook.py", INJECTOR), TOURING_INJECTION_GATE_DISABLED="1")
    assert not _denied(res)


def test_malformed_payload_fails_open(tmp_path: Path) -> None:
    res = subprocess.run([sys.executable, str(GATE)], input="nao-e-json", capture_output=True, text=True,
                         env={**os.environ, "HOME": str(tmp_path)}, timeout=30, check=False)
    assert res.returncode == 0 and res.stdout.strip() == ""
