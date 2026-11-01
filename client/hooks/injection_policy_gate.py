#!/usr/bin/env python3
"""PreToolUse gate: deny writing or registering a context-injecting hook that skips the injection policy.

The context-injection switch (`TOURING_HOOK_INJECTION`) is only as good as its
coverage: every hook that writes `additionalContext`/`systemMessage` must consult
`injection_policy.allows(...)`. `scripts/guard_injection_policy.py` audits that
AFTER the fact; on 27/10/2026 a new hook (`script_portfolio_gate.py`) was created
and registered without the consult and was only caught by a later guard run.
This gate moves the same predicate (`guard_injection_policy.uncovered_injector`,
one source — rule D8) to the moment of the write:

* Write/Edit/MultiEdit of a hook script (a `.py`/`.sh` under a `hooks/` directory,
  or any script `~/.claude/settings.json` registers) whose RESULTING text injects
  context without consulting the policy → deny, with the fix in the reason;
* Write/Edit of `~/.claude/settings.json` that ADDS a hook command whose script
  injects without the consult → deny.

Fail-open on its own errors (a gate never takes the session down) and on a
missing guard module. Human kill switch: `TOURING_INJECTION_GATE_DISABLED=1`.
"""
from __future__ import annotations

import importlib.util
import json
import os
import sys
from pathlib import Path
from types import ModuleType

KILL_SWITCH = "TOURING_INJECTION_GATE_DISABLED"
WRITE_TOOLS = frozenset({"Write", "Edit", "MultiEdit"})
NOT_HOOKS = frozenset({"injection_policy.py", "injection_dedup.py", "conftest.py", "helpers.py"})
GUARD = Path(__file__).resolve().parents[2] / "scripts" / "guard_injection_policy.py"

PY_FIX = (
    "    _ip = pathlib.Path.home() / '.claude/hooks/injection_policy.py'\n"
    "    if _ip.exists():\n"
    "        _s = importlib.util.spec_from_file_location('injection_policy', _ip)\n"
    "        _m = importlib.util.module_from_spec(_s); _s.loader.exec_module(_m)\n"
    "        if not _m.allows('<família>'):\n"
    "            sys.exit(0)"
)
SH_FIX = (
    'python3 "$HOME/.claude/hooks/injection_policy.py" --allows <família> >/dev/null 2>&1 '
    "&& _ip_rc=0 || _ip_rc=$?\n"
    'if [[ "${_ip_rc}" -eq 1 ]]; then exit 0; fi'
)


def load_guard() -> ModuleType | None:
    """The guard module that owns the predicate, or None (gate fails open)."""
    if not GUARD.is_file():
        return None
    spec = importlib.util.spec_from_file_location("guard_injection_policy", GUARD)
    if spec is None or spec.loader is None:
        return None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def proposed_text(tool: str, tool_input: dict, path: Path) -> str | None:
    """The file's text after the write, or None when it cannot be derived."""
    if tool == "Write":
        content = tool_input.get("content")
        return content if isinstance(content, str) else None
    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return None
    edits = tool_input.get("edits") if tool == "MultiEdit" else [tool_input]
    for edit in edits or []:
        old, new = edit.get("old_string"), edit.get("new_string")
        if not isinstance(old, str) or not isinstance(new, str) or old not in text:
            return None  # the Edit tool itself will refuse it
        text = text.replace(old, new) if edit.get("replace_all") else text.replace(old, new, 1)
    return text


def registered_scripts(guard: ModuleType, settings_text: str) -> set[str]:
    """Real paths of the scripts a settings.json text registers as hooks."""
    try:
        data = json.loads(settings_text)
    except ValueError:
        return set()
    out: set[str] = set()
    for entries in (data.get("hooks") or {}).values():
        for entry in entries or []:
            for hook in entry.get("hooks") or []:
                script = guard.script_path(str(hook.get("command") or ""))
                if script:
                    out.add(os.path.realpath(script))
    return out


def is_hook_script(path: Path, registered: set[str]) -> bool:
    """A hook script: registered in settings.json, or a .py/.sh under a hooks/ dir (tests excluded)."""
    real = os.path.realpath(path)
    if path.name in NOT_HOOKS or path.name.startswith("test_") or "/tests/" in real:
        return False
    if real in registered:
        return True
    return path.suffix in (".py", ".sh") and ("/hooks/" in real or "/hooks/" in str(path))


def deny(reason: str) -> None:
    """Emit the PreToolUse deny envelope."""
    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",
        "permissionDecisionReason": reason,
    }}, ensure_ascii=False))


def reason_for(target: str, why: str) -> str:
    """The deny text: invariant, exact next step, what is forbidden (deny rubric)."""
    return (
        f"[injection-policy-gate] {target}: {why}. Invariante: todo hook que escreve contexto "
        "obedece ao interruptor TOURING_HOOK_INJECTION (guarda: scripts/guard_injection_policy.py). "
        "Próximo passo: antes de emitir o contexto, consulte a política "
        "(família em client/hooks/injection_policy.py; aviso de plugin = 'plugin-hook').\n"
        f"Python:\n{PY_FIX}\nShell:\n{SH_FIX}\n"
        "Proibido: `if ! python3 … --allows` (cala o hook quando a política falta) e isentar-se "
        "sem motivo. Hook só de enforcement (deny/block, sem contexto) não precisa da consulta; "
        "isenção legítima vai em EXEMPT de scripts/guard_injection_policy.py, com o motivo."
    )


def check_settings(guard: ModuleType, path: Path, new_text: str) -> str | None:
    """Deny reason when the new settings.json registers an uncovered injector."""
    try:
        old_text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        old_text = "{}"
    for script in sorted(registered_scripts(guard, new_text) - registered_scripts(guard, old_text)):
        try:
            body = Path(script).read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue  # not written yet: the write of the script itself is gated
        why = guard.uncovered_injector(Path(script).name, body)
        if why:
            return reason_for(f"registro de {script} no settings.json", why)
    return None


def evaluate(payload: dict, guard: ModuleType) -> str | None:
    """The deny reason for this PreToolUse payload, or None to allow."""
    tool = payload.get("tool_name")
    tool_input = payload.get("tool_input") or {}
    target = tool_input.get("file_path") or ""
    if tool not in WRITE_TOOLS or not target:
        return None
    path = Path(target)
    new_text = proposed_text(tool, tool_input, path)
    if new_text is None:
        return None
    settings = Path(guard.SETTINGS)
    if os.path.realpath(path) == os.path.realpath(settings):
        return check_settings(guard, path, new_text)
    try:
        current_settings = settings.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        current_settings = "{}"
    if not is_hook_script(path, registered_scripts(guard, current_settings)):
        return None
    why = guard.uncovered_injector(path.name, new_text)
    return reason_for(str(path), why) if why else None


def main() -> int:
    """Read the PreToolUse payload from stdin; deny or stay silent. Always exit 0."""
    if os.environ.get(KILL_SWITCH, "0") not in ("", "0"):
        return 0
    try:
        payload = json.load(sys.stdin)
        if not isinstance(payload, dict):
            return 0
        guard = load_guard()
        if guard is None:
            print(f"[injection-policy-gate] guard ausente em {GUARD}; gate inativo", file=sys.stderr)
            return 0
        reason = evaluate(payload, guard)
        if reason:
            deny(reason)
    except Exception as exc:  # noqa: BLE001 — a gate never fails the tool call; stderr keeps it loud
        print(f"[injection-policy-gate] erro interno, liberado: {type(exc).__name__}: {exc}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
