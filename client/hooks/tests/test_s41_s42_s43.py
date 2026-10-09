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

# 08/10/2026 (compactação aos 65%): the flag is PER SESSION under ~/.claude/touring/context_alert/
# (`<session>.pct`, written by status_monitor.sh). Until then it was one global file, so with three
# live sessions the first to cross the threshold lit the alert and any session's Stop hook consumed
# it. The warning is ONE checkpoint per compaction cycle (`<session>.delivered`), not a line per call.

def _alert_dir(home: Path) -> Path:
    d = home / ".claude" / "touring" / "context_alert"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _ctx_payload(session: str) -> dict[str, object]:
    return {"hook_event_name": "PostToolUse", "tool_name": "Bash", "session_id": session, "tool_input": {}}


def test_check_context_warns_once_per_cycle(tmp_path: Path) -> None:
    """S42: this session's flag → one checkpoint warning naming both numbers; the next call is silent."""
    (_alert_dir(tmp_path) / "sess-a.pct").write_text("57")
    first = run_hook_sh(CLIENT_HOOKS / "check_context.sh", _ctx_payload("sess-a"), home=tmp_path)
    assert first.rc == 0 and first.json is not None, f"stdout={first.stdout_raw!r} stderr={first.stderr_raw!r}"
    ctx = first.additional_context()
    assert "57%" in ctx and "65%" in ctx, ctx
    assert "/clear" in ctx, "the checkpoint must name the /clear route for a distinct next task"
    assert first.schema_ok("PostToolUse")
    second = run_hook_sh(CLIENT_HOOKS / "check_context.sh", _ctx_payload("sess-a"), home=tmp_path)
    assert second.rc == 0 and second.json is None, f"repeated warning: {second.stdout_raw!r}"


def test_check_context_ignores_another_sessions_flag(tmp_path: Path) -> None:
    """S42: a flag lit by session B never reaches session A (the global-file defect)."""
    (_alert_dir(tmp_path) / "sess-b.pct").write_text("60")
    result = run_hook_sh(CLIENT_HOOKS / "check_context.sh", _ctx_payload("sess-a"), home=tmp_path)
    assert result.rc == 0 and result.json is None, result.stdout_raw
    assert (_alert_dir(tmp_path) / "sess-b.pct").exists(), "session A must not consume B's flag"


def test_check_context_no_flag_is_silent(tmp_path: Path) -> None:
    """S42: without flag file, script produces no output."""
    result = run_hook_sh(CLIENT_HOOKS / "check_context.sh", _ctx_payload("sess-a"), home=tmp_path)
    assert result.rc == 0
    assert result.json is None, f"Unexpected output when no flag: {result.stdout_raw!r}"


def test_check_context_honours_autocompact_pct(tmp_path: Path) -> None:
    """S42: the warning quotes the threshold in force, not a constant."""
    (_alert_dir(tmp_path) / "sess-a.pct").write_text("62")
    result = run_hook_sh(CLIENT_HOOKS / "check_context.sh", _ctx_payload("sess-a"), home=tmp_path,
                         extra_env={"CLAUDE_AUTOCOMPACT_PCT_OVERRIDE": "72"})
    assert "72%" in result.additional_context(), result.stdout_raw


@pytest.mark.parametrize("registered", [False, True])
def test_check_context_defers_only_to_a_registered_project_monitor(tmp_path: Path, registered: bool) -> None:
    """S42: the project monitor FILE existing is not the monitor RUNNING (analise had it unregistered → no alert)."""
    project = tmp_path / "proj"
    (project / ".claude" / "hooks" / "validation").mkdir(parents=True)
    (project / ".claude" / "hooks" / "validation" / "context_monitor.py").write_text("# present\n")
    hooks = {"PostToolUse": [{"hooks": [{"type": "command", "command": "python3 .claude/hooks/validation/context_monitor.py"}]}]}
    (project / ".claude" / "settings.local.json").write_text(json.dumps({"hooks": hooks if registered else {}}))
    (_alert_dir(tmp_path) / "sess-a.pct").write_text("58")
    result = run_hook_sh(CLIENT_HOOKS / "check_context.sh", _ctx_payload("sess-a"), home=tmp_path,
                         extra_env={"CLAUDE_PROJECT_DIR": str(project)})
    assert result.rc == 0
    assert (result.json is None) is registered, f"registered={registered} stdout={result.stdout_raw!r}"


# ── status_monitor.sh: arms and re-arms the per-session flag ─────────────────

def _statusline(home: Path, session: str, tokens: int, extra_env: dict[str, str] | None = None) -> str:
    payload = {"session_id": session, "model": {"display_name": "Test", "id": "test"},
               "context_window": {"context_window_size": 1000000,
                                  "current_usage": {"input_tokens": tokens}}}
    env = {**os.environ, "HOME": str(home), **(extra_env or {})}
    proc = subprocess.run(["bash", str(CLIENT_HOOKS / "status_monitor.sh")], input=json.dumps(payload).encode(),
                          capture_output=True, timeout=15, env=env)
    assert proc.returncode == 0, proc.stderr
    return proc.stdout.decode()


def test_status_monitor_arms_flag_at_checkpoint(tmp_path: Path) -> None:
    """At 60% of the window (checkpoint 55 = 65 − 10) the session's flag holds the percentage."""
    _statusline(tmp_path, "sess-a", 600_000)
    flag = tmp_path / ".claude" / "touring" / "context_alert" / "sess-a.pct"
    assert flag.read_text().strip() == "60"
    assert not (tmp_path / ".claude" / "context_alert").exists(), "the global flag is retired"


def test_status_monitor_below_checkpoint_does_not_arm(tmp_path: Path) -> None:
    """Control: at 54% nothing is armed."""
    _statusline(tmp_path, "sess-a", 540_000)
    assert not (tmp_path / ".claude" / "touring" / "context_alert" / "sess-a.pct").exists()


def test_status_monitor_rearms_after_compaction(tmp_path: Path) -> None:
    """Back under the checkpoint (after a compaction) both flag and delivered marker go: next cycle warns again."""
    d = _alert_dir(tmp_path)
    (d / "sess-a.pct").write_text("60")
    (d / "sess-a.delivered").write_text("")
    _statusline(tmp_path, "sess-a", 200_000)
    assert not (d / "sess-a.pct").exists() and not (d / "sess-a.delivered").exists()


def test_status_monitor_checkpoint_follows_override(tmp_path: Path) -> None:
    """With autocompact at 80 the checkpoint is 70: 60% does not arm."""
    _statusline(tmp_path, "sess-a", 600_000, {"CLAUDE_AUTOCOMPACT_PCT_OVERRIDE": "80"})
    assert not (tmp_path / ".claude" / "touring" / "context_alert" / "sess-a.pct").exists()


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
