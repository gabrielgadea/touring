"""The shell guard of the injection switch silences a hook ONLY when the policy denies.

27/10/2026: `if ! python3 injection_policy.py --allows …` treated a missing or
broken policy (rc 2) as a denial, so check_context.sh went mute under any HOME
without the policy file and 27 tests went red. The guard now exits only on rc 1.
"""
from __future__ import annotations

from pathlib import Path

from test_status_monitor_quota import _arm_quota, _check

POLICY = Path(__file__).resolve().parent.parent / "injection_policy.py"


def _install_policy(home: Path, body: str | None = None) -> None:
    hooks = home / ".claude" / "hooks"
    hooks.mkdir(parents=True, exist_ok=True)
    target = hooks / "injection_policy.py"
    if body is None:
        target.symlink_to(POLICY)
    else:
        target.write_text(body)


def test_missing_policy_file_keeps_hook_output(tmp_path: Path) -> None:
    _arm_quota(tmp_path)
    res = _check(tmp_path)
    assert res.rc == 0 and res.json is not None, res.stderr_raw


def test_broken_policy_keeps_hook_output(tmp_path: Path) -> None:
    _install_policy(tmp_path, "import sys\nsys.exit(2)\n")
    _arm_quota(tmp_path)
    res = _check(tmp_path)
    assert res.rc == 0 and res.json is not None, res.stderr_raw


def test_denying_policy_silences_hook(tmp_path: Path) -> None:
    _install_policy(tmp_path)
    _arm_quota(tmp_path)
    res = _check(tmp_path, TOURING_HOOK_INJECTION="off")
    assert res.rc == 0 and res.stdout_raw.strip() == "", res.stdout_raw


def test_allowing_policy_keeps_hook_output(tmp_path: Path) -> None:
    _install_policy(tmp_path)
    _arm_quota(tmp_path)
    res = _check(tmp_path, TOURING_HOOK_INJECTION="full")
    assert res.rc == 0 and res.json is not None, res.stderr_raw
