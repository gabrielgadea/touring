"""test_s41_s42_s43.py — S41 (doc-coverage-gate), S42 (check_context), S43 (telemetry_logger).

Each test follows RED-GREEN-REVERT cycle:
  RED: prove old behavior fails the contract (e.g., writes to stderr → model sees nothing).
  GREEN: new script emits additionalContext via JSON stdout.
"""
from __future__ import annotations

import json
import os
import subprocess
import tempfile
from pathlib import Path

import pytest
from helpers import CLIENT_HOOKS, HOME, run_hook_sh

GLOBAL_HOOKS = HOME / ".claude" / "hooks"

# ── S41: doc-coverage-gate.sh ─────────────────────────────────────────────────

def _make_rs_file(content: str) -> Path:
    """Create a .rs file under a fake crates/ path that the gate will inspect."""
    import tempfile
    tmp_dir = Path(tempfile.mkdtemp())
    crates_dir = tmp_dir / "crates" / "fake" / "src"
    crates_dir.mkdir(parents=True)
    rs_file = crates_dir / "lib.rs"
    rs_file.write_text(content)
    return rs_file


def test_doc_coverage_reads_stdin() -> None:
    """S41: script reads file_path from stdin JSON, not from $1."""
    rs_file = _make_rs_file(
        "pub fn a() {}\npub fn b() {}\npub fn c() {}\npub fn d() {}\n"
    )
    payload = {
        "hook_event_name": "PostToolUse",
        "tool_name": "Write",
        "session_id": "hooks-contract-s41",
        "tool_input": {"file_path": str(rs_file)},
    }
    script = CLIENT_HOOKS / "doc-coverage-gate.sh"
    result = run_hook_sh(script, payload)
    assert result.rc == 0, f"Script exited {result.rc}: {result.stderr_raw}"
    # The gate MUST emit via stdout JSON when docs are missing, not via stderr only.
    j = result.json
    if j is not None:
        hso = j.get("hookSpecificOutput", {})
        ctx = hso.get("additionalContext", "")
        assert ctx, "Expected additionalContext with doc-coverage warning"
        assert "doc-coverage" in ctx, f"Expected 'doc-coverage' in context, got: {ctx!r}"
    # Schema must be valid.
    assert result.schema_ok("PostToolUse"), f"Schema violation: {result.stdout_raw}"


def test_doc_coverage_rust_file_without_docs_emits_context() -> None:
    """S41: .rs file with >3 pub items and no /// docs → additionalContext emitted."""
    rs_file = _make_rs_file(
        "pub fn alpha() {}\npub fn beta() {}\npub fn gamma() {}\npub fn delta() {}\n"
    )
    payload = {
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        "session_id": "hooks-contract-s41",
        "tool_input": {"file_path": str(rs_file)},
    }
    result = run_hook_sh(CLIENT_HOOKS / "doc-coverage-gate.sh", payload)
    assert result.rc == 0
    assert result.json is not None, "Expected JSON output for undocumented .rs file"
    ctx = result.additional_context()
    assert "doc-coverage" in ctx.lower() or "pub" in ctx.lower()


def test_doc_coverage_empty_file_path_is_silent() -> None:
    """S41: empty or missing file_path → script exits 0 silently."""
    payload = {"hook_event_name": "PostToolUse", "tool_name": "Write",
               "session_id": "hooks-contract-s41", "tool_input": {"file_path": ""}}
    result = run_hook_sh(CLIENT_HOOKS / "doc-coverage-gate.sh", payload)
    assert result.rc == 0
    assert not result.is_deny()


def test_doc_coverage_non_rs_file_is_silent() -> None:
    """S41: non-.rs files are skipped."""
    with tempfile.NamedTemporaryFile(suffix=".py", delete=False) as tmp:
        tmp.write(b"def foo(): pass\n")
        tmp_path = Path(tmp.name)
    payload = {"hook_event_name": "PostToolUse", "tool_name": "Write",
               "session_id": "hooks-contract-s41", "tool_input": {"file_path": str(tmp_path)}}
    result = run_hook_sh(CLIENT_HOOKS / "doc-coverage-gate.sh", payload)
    assert result.rc == 0
    assert result.json is None or not result.additional_context()
    tmp_path.unlink(missing_ok=True)


# ── S42: check_context.sh ─────────────────────────────────────────────────────

# Every S42/S43 test runs the script against a SCRATCH home: they used to write 92 into the
# operator's real ~/.claude/context_alert, delete it, and append synthetic entries to the real
# telemetry — with the telemetry assertions under `if file.exists()`, able to pass on nothing.

def test_check_context_output_is_visible(tmp_path: Path) -> None:
    """S42: when flag file exists, output is JSON on stdout, not just stderr."""
    (tmp_path / ".claude").mkdir()
    (tmp_path / ".claude" / "context_alert").write_text("92")
    payload = {"hook_event_name": "PostToolUse", "tool_name": "Bash",
               "session_id": "hooks-contract-s42", "tool_input": {}}
    result = run_hook_sh(CLIENT_HOOKS / "check_context.sh", payload, home=tmp_path)
    assert result.rc == 0
    # Must emit JSON stdout (visible to model).
    assert result.json is not None, (
        f"Expected JSON stdout from check_context.sh when flag set.\n"
        f"stdout={result.stdout_raw!r}\nstderr={result.stderr_raw!r}"
    )
    ctx = result.additional_context()
    assert "92%" in ctx or "contexto" in ctx.lower(), f"Expected context percentage in output: {ctx!r}"
    assert result.schema_ok("PostToolUse")


def test_check_context_no_flag_is_silent(tmp_path: Path) -> None:
    """S42: without flag file, script produces no output."""
    payload = {"hook_event_name": "PostToolUse", "tool_name": "Bash",
               "session_id": "hooks-contract-s42", "tool_input": {}}
    result = run_hook_sh(CLIENT_HOOKS / "check_context.sh", payload, home=tmp_path)
    assert result.rc == 0
    assert result.json is None, f"Unexpected output when no flag: {result.stdout_raw!r}"


# ── S43: telemetry_logger.sh ──────────────────────────────────────────────────

def _last_telemetry_entry(home: Path) -> dict[str, object]:
    telemetry_file = home / ".claude" / "touring-telemetry.ndjson"
    assert telemetry_file.exists(), "the logger wrote no telemetry file"
    last_line = telemetry_file.read_text().strip().splitlines()[-1]
    try:
        return json.loads(last_line)
    except json.JSONDecodeError as e:
        pytest.fail(f"Telemetry entry is not valid JSON: {last_line!r} ({e})")


def test_telemetry_logger_single_process_valid_ndjson(tmp_path: Path) -> None:
    """S43: logger uses single jq call, produces valid NDJSON."""
    (tmp_path / ".claude").mkdir()
    payload = {
        "hook_event_name": "PostToolUse",
        "tool_name": "Write",
        "session_id": "hooks-contract-s43-test",
        "tool_input": {},
    }
    result = run_hook_sh(CLIENT_HOOKS / "telemetry_logger.sh", payload, home=tmp_path)
    assert result.rc == 0, f"Script failed: {result.stderr_raw}"
    entry = _last_telemetry_entry(tmp_path)
    assert "ts" in entry, f"Missing 'ts' field: {entry}"
    assert entry.get("tool") == "Write", f"Expected tool=Write, got {entry.get('tool')!r}"
    assert entry.get("session") == "hooks-contract-s43-test"


def test_telemetry_logger_invalid_input_produces_fallback(tmp_path: Path) -> None:
    """S43: malformed stdin (not JSON) still produces a valid NDJSON entry."""
    (tmp_path / ".claude").mkdir()
    proc = subprocess.run(
        ["bash", str(CLIENT_HOOKS / "telemetry_logger.sh")],
        input=b"not json at all",
        capture_output=True,
        timeout=5,
        env={**os.environ, "HOME": str(tmp_path)},
    )
    assert proc.returncode == 0
    assert "ts" in _last_telemetry_entry(tmp_path)  # fallback entry has at minimum ts/epoch
