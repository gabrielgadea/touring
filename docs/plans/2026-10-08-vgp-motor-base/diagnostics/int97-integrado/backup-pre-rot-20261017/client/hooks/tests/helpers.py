"""helpers.py — shared utilities for client/hooks tests (importable as regular module).

Imports the contract schema from analise/tests/hooks_contract/_contract.py so the same
schema_violations() function governs both the analise and the global hook suites.

Session ids start with "hooks-contract-" to be excluded from liveness detectors.
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any


HOME = Path.home()
REPO_ROOT = Path(__file__).resolve().parents[3]  # ~/projects/touring
GLOBAL_HOOKS = HOME / ".claude" / "hooks"
CLIENT_HOOKS = REPO_ROOT / "client" / "hooks"

def _fallback_schema_violations(event: str, result: Any) -> list[str]:
    """Minimal schema validator (replicates _contract.py TOP_LEVEL_FIELDS logic).

    Takes what `HookResult.schema_ok` passes — the HookResult itself, read through its
    parsed `.json` — or a bare dict. The first version iterated its argument as a dict and
    crashed on the HookResult wherever the analise contract was absent: every CI runner.
    """
    data = getattr(result, "json", result)
    if not data:
        return []  # no stdout JSON is a valid (silent) hook output
    violations: list[str] = []
    allowed_top = {
        "continue", "suppressOutput", "stopReason", "decision",
        "systemMessage", "terminalSequence", "reason", "hookSpecificOutput",
    }
    for k in data:
        if k not in allowed_top:
            violations.append(f"unknown top-level field: {k!r}")
    hso = data.get("hookSpecificOutput", {})
    if hso:
        specific: dict[str, set[str]] = {
            "PreToolUse": {"permissionDecision", "permissionDecisionReason", "updatedInput", "additionalContext"},
            "PostToolUse": {"additionalContext", "classifierContext", "updatedToolOutput", "updatedMCPToolOutput"},
            "SessionStart": {"additionalContext", "initialUserMessage", "sessionTitle", "watchPaths", "reloadSkills"},
            "Stop": {"additionalContext"},
            "FileChanged": {"watchPaths"},
            "PermissionRequest": {"decision"},
        }
        allowed_hso = specific.get(event, set()) | {"hookEventName"}
        for k in hso:
            if k not in allowed_hso:
                violations.append(f"hookSpecificOutput.{k!r} not valid for event {event!r}")
    return violations


# The analise project's contract governs both suites when it is present; otherwise — and
# when importing it fails — the fallback above. (Before, an import failure left NO
# schema_violations defined: the "fall through" comment pointed at an `else` never taken.)
schema_violations = _fallback_schema_violations
_ANALISE_CONTRACT = HOME / "projects" / "analise" / "tests" / "hooks_contract" / "_contract.py"
if _ANALISE_CONTRACT.exists():
    import importlib.util
    _mod_name = "analise_hooks_contract"
    spec = importlib.util.spec_from_file_location(_mod_name, _ANALISE_CONTRACT)
    _contract_mod = importlib.util.module_from_spec(spec)  # type: ignore[arg-type]
    sys.modules[_mod_name] = _contract_mod  # register before exec so @dataclass works
    try:
        spec.loader.exec_module(_contract_mod)  # type: ignore[union-attr]
        schema_violations = _contract_mod.schema_violations
    except Exception:
        del sys.modules[_mod_name]


def run_hook(script: Path, payload: dict[str, Any], *, timeout: int = 15) -> "HookResult":
    """Run a hook script with the given payload dict, return HookResult."""
    payload_bytes = json.dumps(payload).encode()
    result = subprocess.run(
        [str(script)],
        input=payload_bytes,
        capture_output=True,
        timeout=timeout,
    )
    return HookResult(script=script, payload=payload, proc=result)


def run_hook_sh(
    script: Path,
    payload: dict[str, Any],
    *,
    args: list[str] | None = None,
    timeout: int = 15,
    home: Path | None = None,
    extra_env: dict[str, str] | None = None,
) -> "HookResult":
    """Run a shell hook with optional extra args.

    `home` runs the script against a scratch HOME: the hooks derive their state files from
    ${HOME}/.claude, and the tests used to write the operator's real telemetry and delete
    the real context_alert flag. `extra_env` overlays variables the hook reads (for example
    CLAUDE_PROJECT_DIR or CLAUDE_AUTOCOMPACT_PCT_OVERRIDE) without touching the caller's env.
    """
    import os

    cmd = ["bash", str(script)] + (args or [])
    payload_bytes = json.dumps(payload).encode()
    env = None
    if home is not None or extra_env:
        env = {**os.environ, **({"HOME": str(home)} if home is not None else {}), **(extra_env or {})}
    result = subprocess.run(cmd, input=payload_bytes, capture_output=True, timeout=timeout, env=env)
    return HookResult(script=script, payload=payload, proc=result)


class HookResult:
    def __init__(self, script: Path, payload: dict, proc: subprocess.CompletedProcess) -> None:
        self.script = script
        self.payload = payload
        self.proc = proc
        self.rc = proc.returncode
        self.stdout_raw = proc.stdout.decode("utf-8", errors="replace")
        self.stderr_raw = proc.stderr.decode("utf-8", errors="replace")
        self._json: dict | None = None

    @property
    def json(self) -> dict | None:
        if self._json is None and self.stdout_raw.strip():
            try:
                self._json = json.loads(self.stdout_raw.strip())
            except json.JSONDecodeError:
                pass
        return self._json

    def is_deny(self) -> bool:
        j = self.json or {}
        hso = j.get("hookSpecificOutput", {})
        pd = hso.get("permissionDecision", "")
        return pd in ("deny", "block")

    def is_allow(self) -> bool:
        return not self.is_deny() and self.rc == 0

    def additional_context(self) -> str:
        j = self.json or {}
        return j.get("hookSpecificOutput", {}).get("additionalContext", "")

    # ── _contract.py compatibility aliases ──────────────────────────────────────
    # _contract.py's HookResult has .parsed (dict|None), .stdout (bytes),
    # .stderr (bytes), .ms (float). Add properties so schema_violations(event, self) works.
    @property
    def parsed(self) -> dict | None:
        """Alias for .json — field name expected by _contract.py."""
        return self.json

    @property
    def stdout(self) -> bytes:
        """Raw stdout bytes — field name expected by _contract.py."""
        return self.proc.stdout

    @property
    def stderr(self) -> bytes:
        """Raw stderr bytes — field name expected by _contract.py."""
        return self.proc.stderr

    @property
    def ms(self) -> float:
        """Elapsed ms placeholder (not measured here; satisfies _contract.py typing)."""
        return 0.0

    def schema_ok(self, event: str) -> bool:
        j = self.json
        if j is None:
            return True  # no output = allow, valid
        return len(schema_violations(event, self)) == 0
