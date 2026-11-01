"""The schema fallback in helpers.py — the validator a CI runner actually uses.

On the operator's machine helpers.py imports the analise contract, so the fallback never runs
there; on a runner it is the only validator. It once iterated its argument as a dict and
crashed on the HookResult every caller passes (26/09/2026). Exercised directly here, with the
shapes the callers use.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from helpers import HookResult, _fallback_schema_violations  # noqa: E402


def _result(stdout: str) -> HookResult:
    proc = subprocess.CompletedProcess(args=["x"], returncode=0, stdout=stdout.encode(), stderr=b"")
    return HookResult(script=Path("x"), payload={}, proc=proc)


def test_valid_output_has_no_violation() -> None:
    ok = '{"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": "x"}}'
    assert _fallback_schema_violations("PostToolUse", _result(ok)) == []


def test_silent_hook_is_valid() -> None:
    assert _fallback_schema_violations("PostToolUse", _result("")) == []


def test_unknown_fields_are_reported() -> None:
    bad_top = _result('{"prompt": "x"}')
    assert _fallback_schema_violations("UserPromptSubmit", bad_top) != []
    bad_hso = _result('{"hookSpecificOutput": {"hookEventName": "Stop", "permissionDecision": "deny"}}')
    assert _fallback_schema_violations("Stop", bad_hso) != []


def test_a_bare_dict_is_still_accepted() -> None:
    assert _fallback_schema_violations("PostToolUse", {"decision": "block", "reason": "x"}) == []
