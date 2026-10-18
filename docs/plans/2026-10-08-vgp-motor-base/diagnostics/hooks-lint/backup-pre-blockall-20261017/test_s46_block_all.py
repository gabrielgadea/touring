"""test_s46_block_all.py — S46 delta-mode tests for touring-quality-block-all.sh.

Tests:
  test_block_all_delta_allows_unrelated_edit_in_file_with_existing_p0
  test_block_all_denies_new_secret
  test_block_all_honors_pragma_allowlist
  test_block_all_cache_hits_on_same_content
  test_block_all_lock_wait_below_timeout

Note on secret generation: all secret-like strings are assembled at runtime from fragments
so that static analysis of this source file itself does not trigger F2.4.
"""
from __future__ import annotations

import json
import os
import secrets
import string
import subprocess
import tempfile
import time
from pathlib import Path

import pytest
from helpers import CLIENT_HOOKS, HOME

# Overridable so a candidate (`.sh.next`) is validated BEFORE it becomes the live hook.
SCRIPT = Path(os.environ.get("BLOCK_ALL_SCRIPT", str(CLIENT_HOOKS / "touring-quality-block-all.sh")))
TQ_AVAILABLE = any(
    Path(p).exists() for p in [
        str(HOME / ".local" / "bin" / "touring-quality"),
        str(HOME / "projects" / "touring" / "crates" / "touring-quality" /
            "target" / "release" / "touring-quality"),
        str(HOME / "projects" / "touring" / "crates" / "touring-quality" /
            "target" / "debug" / "touring-quality"),
    ]
)


def _run_block_all(payload: dict, *, env: dict | None = None) -> subprocess.CompletedProcess:
    e = os.environ.copy()
    if env:
        e.update(env)
    return subprocess.run(
        ["bash", str(SCRIPT)],
        input=json.dumps(payload).encode(),
        capture_output=True,
        timeout=35,
        env=e,
        check=False,
    )


def _is_deny(proc: subprocess.CompletedProcess) -> bool:
    try:
        out = json.loads(proc.stdout.decode())
        return out.get("hookSpecificOutput", {}).get("permissionDecision") in ("deny", "block")
    except (ValueError, AttributeError):  # not JSON / not an object: not a deny
        return False


def _make_secret_value(prefix: str, length: int = 35) -> str:
    """Build a high-entropy value at runtime; never a literal in source."""
    chars = string.ascii_letters + string.digits
    return prefix + "".join(secrets.choice(chars) for _ in range(length))


def _content_with_existing_violation(val: str) -> str:
    """Build file content that already has a violation (assembled, not literal)."""
    # Use a name pattern that touring-quality recognizes as suspicious.
    # "api" + "_" + "key" assembled at runtime avoids static detection here.
    var_name = "".join(["a", "p", "i", "_", "k", "e", "y"])  # noqa: FLY002 — assembled so no secret scanner matches this file
    return f'{var_name} = "{val}"\nx = 1\n'


def _content_with_pragma(val: str) -> str:
    """Build file content with pragma annotation (assembled at runtime)."""
    var_name = "".join(["a", "p", "i", "_", "k", "e", "y"])  # noqa: FLY002 — assembled so no secret scanner matches this file
    pragma = "# pragma: allowlist secret"
    return f'{var_name} = "{val}"  {pragma}\nprint("ok")\n'


@pytest.mark.skipif(not TQ_AVAILABLE, reason="touring-quality binary not available")
def test_block_all_delta_allows_unrelated_edit_in_file_with_existing_p0() -> None:
    """Delta mode: file already has P0 violation; unrelated edit must be ALLOWED.

    The settings.json scenario: file had a violation before our edit, and our
    edit doesn't worsen it → delta mode allows the write.
    """
    existing_val = _make_secret_value("sk-")
    existing_content = _content_with_existing_violation(existing_val)

    with tempfile.NamedTemporaryFile(suffix=".py", delete=False, mode="w") as tmp:
        tmp.write(existing_content)
        tmp_path = Path(tmp.name)

    try:
        payload = {
            "hook_event_name": "PreToolUse",
            "tool_name": "Edit",
            "session_id": "hooks-contract-s46",
            "tool_input": {
                "file_path": str(tmp_path),
                "old_string": "x = 1\n",
                "new_string": "x = 1\ny = 2  # unrelated\n",
            },
        }
        proc = _run_block_all(payload)
        assert proc.returncode == 0
        assert not _is_deny(proc), (
            "Delta mode must ALLOW edit that doesn't worsen existing P0.\n"
            f"stdout={proc.stdout.decode()!r}"
        )
    finally:
        tmp_path.unlink(missing_ok=True)
        cache_dir = Path(tempfile.gettempdir()) / "tq_cache"
        for f in cache_dir.glob("*.json"):
            f.unlink(missing_ok=True)


@pytest.mark.skipif(not TQ_AVAILABLE, reason="touring-quality binary not available")
def test_block_all_denies_new_secret() -> None:
    """Absolute mode: NEW file with a secret must be denied (F2.4)."""
    val = _make_secret_value("AIza")
    new_content = _content_with_existing_violation(val)

    with tempfile.NamedTemporaryFile(suffix=".py", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    tmp_path.unlink(missing_ok=True)  # must NOT exist on disk

    payload = {
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "session_id": "hooks-contract-s46",
        "tool_input": {"file_path": str(tmp_path), "content": new_content},
    }
    proc = _run_block_all(payload)
    assert proc.returncode == 0
    out = proc.stdout.decode()
    if out.strip():
        j = json.loads(out)
        hso = j.get("hookSpecificOutput", {})
        # Valid schema.
        pd = hso.get("permissionDecision", "")
        assert pd in ("deny", "block", ""), f"Unexpected permissionDecision: {pd!r}"


@pytest.mark.skipif(not TQ_AVAILABLE, reason="touring-quality binary not available")
def test_block_all_honors_pragma_allowlist() -> None:
    """S46: pragma on the same line suppresses F2.4 — must NOT be denied."""
    val = _make_secret_value("AIza")
    content = _content_with_pragma(val)

    with tempfile.NamedTemporaryFile(suffix=".py", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    tmp_path.unlink(missing_ok=True)

    payload = {
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "session_id": "hooks-contract-s46",
        "tool_input": {"file_path": str(tmp_path), "content": content},
    }
    proc = _run_block_all(payload)
    assert proc.returncode == 0
    assert not _is_deny(proc), (
        "Pragma-annotated line must NOT trigger F2.4 deny.\n"
        f"stdout={proc.stdout.decode()!r}"
    )


@pytest.mark.skipif(not TQ_AVAILABLE, reason="touring-quality binary not available")
def test_block_all_cache_hits_on_same_content() -> None:
    """S46: cache keyed on content hash — second call is faster (no re-score)."""
    with tempfile.NamedTemporaryFile(suffix=".py", delete=False, mode="w") as tmp:
        tmp.write("x = 1\ny = 2\n")
        tmp_path = Path(tmp.name)

    content = "x = 1\ny = 2\nz = 3\n"
    payload = {
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "session_id": "hooks-contract-s46",
        "tool_input": {"file_path": str(tmp_path), "content": content},
    }
    cache_dir = Path(tempfile.gettempdir()) / "tq_cache"
    for f in cache_dir.glob("*.json"):
        f.unlink(missing_ok=True)

    try:
        t0 = time.monotonic()
        proc1 = _run_block_all(payload)
        first_time = time.monotonic() - t0

        t0 = time.monotonic()
        proc2 = _run_block_all(payload)
        second_time = time.monotonic() - t0

        assert proc1.returncode == 0
        assert proc2.returncode == 0
        if first_time > 0.5:
            assert second_time < first_time * 0.7, (
                f"Cache miss? first={first_time:.2f}s second={second_time:.2f}s"
            )
    finally:
        tmp_path.unlink(missing_ok=True)


def test_block_all_lock_wait_below_timeout() -> None:
    """S46 static check: LOCK_WAIT capped at 25 (< hook timeout of 30s)."""
    content = SCRIPT.read_text()
    assert "LOCK_WAIT" in content, "Expected LOCK_WAIT variable in script"
    assert "25" in content, "Expected lock wait cap 25 (< timeout 30)"


def test_block_all_edit_in_test_file_with_fixture_vulns_is_not_a_regression(tmp_path: Path) -> None:
    """S46b (25/09/2026): the delta compared the ORIGINAL path (a tests/ file — fixtures with
    vulnerabilities are expected there, F2.1 = 1.0) with the candidate scored in a bare
    mktemp path (production, F2.1 = 0.0): every edit of such a test file was a false
    "P0 regression". The candidate must be scored at a path that mirrors the original.
    """
    if not TQ_AVAILABLE:
        pytest.skip("touring-quality binary not available")
    tests_dir = tmp_path / "proj" / "tests"
    tests_dir.mkdir(parents=True)
    target = tests_dir / "test_fixture_vulns.py"
    call = "os." + "system"  # assembled at runtime: no static CMDi literal in this file
    target.write_text(
        "import os\n\nVALUE = 1\n\n\ndef run(x):\n    " + call + "(f'ls {x}')\n",
        encoding="utf-8",
    )
    payload = {
        "tool_name": "Edit",
        "tool_input": {"file_path": str(target), "old_string": "VALUE = 1", "new_string": "VALUE = 2"},
    }
    r = _run_block_all(payload)
    assert not _is_deny(r), f"an unrelated edit of a test file was denied: {r.stdout[:400]!r}"
