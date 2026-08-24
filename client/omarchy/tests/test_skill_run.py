"""Tests for client/omarchy/bin/omarchy-skill-run."""

from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path

OMARCHY_DIR = Path(__file__).parent.parent
SCRIPT      = OMARCHY_DIR / "bin" / "omarchy-skill-run"
FAKE_BINS   = Path(__file__).parent / "bin"   # contains fake claude + herdr


def _env_with_fake_bins(**extra_env: str) -> dict[str, str]:
    """Return os.environ with tests/bin prepended to PATH."""
    env = os.environ.copy()
    env["PATH"] = str(FAKE_BINS) + os.pathsep + env.get("PATH", "")
    env.update(extra_env)
    return env


def _run(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(SCRIPT)] + args,
        capture_output=True,
        text=True,
        **kwargs,
    )


# ── --dry-run ────────────────────────────────────────────────────────────────

def test_dry_run_prints_claude_command() -> None:
    """--dry-run must print the claude invocation and exit 0 without executing."""
    result = _run(
        ["--dry-run", "TACO-cross-audit", "fable", "high", "acceptEdits", "/tmp"],
        env=_env_with_fake_bins(),
    )
    assert result.returncode == 0, f"Expected exit 0, got {result.returncode}\n{result.stderr}"
    output = result.stdout
    assert "claude" in output,                  f"'claude' not in output: {output!r}"
    assert "/TACO-cross-audit" in output,        f"'/TACO-cross-audit' not in output: {output!r}"
    assert "--model" in output,                  f"'--model' not in output: {output!r}"
    assert "--effort" in output,                 f"'--effort' not in output: {output!r}"
    assert "--permission-mode" in output,        f"'--permission-mode' not in output: {output!r}"


def test_dry_run_writes_no_log() -> None:
    """--dry-run must NOT write to the log file."""
    with tempfile.NamedTemporaryFile(suffix=".log", delete=False) as tf:
        log_path = tf.name
    try:
        Path(log_path).unlink()   # start with no file
        _run(
            ["--dry-run", "TACO-cross-audit", "fable", "high", "acceptEdits", "/tmp"],
            env=_env_with_fake_bins(OMARCHY_SKILL_RUN_LOG=log_path),
        )
        assert not Path(log_path).exists(), "Log file must not be created by --dry-run"
    finally:
        Path(log_path).unlink(missing_ok=True)


# ── bypassPermissions ────────────────────────────────────────────────────────

def test_bypass_permissions_refused() -> None:
    """bypassPermissions mode must be rejected with exit code 2."""
    result = _run(
        ["TACO-cross-audit", "fable", "high", "bypassPermissions", "/tmp"],
        env=_env_with_fake_bins(),
    )
    assert result.returncode == 2, \
        f"Expected exit 2 for bypassPermissions, got {result.returncode}\n{result.stderr}"
    assert "bypassPermissions" in result.stderr or "not allowed" in result.stderr, \
        f"Expected informative stderr: {result.stderr!r}"


# ── validation ───────────────────────────────────────────────────────────────

def test_invalid_model_exits_2() -> None:
    result = _run(["skill", "gpt4", "high", "auto", "/tmp"], env=_env_with_fake_bins())
    assert result.returncode == 2

def test_invalid_effort_exits_2() -> None:
    result = _run(["skill", "fable", "extreme", "auto", "/tmp"], env=_env_with_fake_bins())
    assert result.returncode == 2

def test_missing_cwd_exits_2() -> None:
    result = _run(
        ["skill", "fable", "high", "auto", "/nonexistent_dir_xyz_abc"],
        env=_env_with_fake_bins(),
    )
    assert result.returncode == 2

def test_too_few_args_exits_2() -> None:
    result = _run(["skill", "fable"], env=_env_with_fake_bins())
    assert result.returncode == 2


# ── direct run with fake claude ───────────────────────────────────────────────

def test_direct_run_writes_log_exit_0() -> None:
    """With a fake claude that exits 0, the log line must contain exit=0."""
    with tempfile.NamedTemporaryFile(suffix=".log", delete=True) as tf:
        log_path = tf.name
    Path(log_path).unlink(missing_ok=True)   # ensure fresh
    try:
        result = _run(
            ["TACO-cross-audit", "fable", "high", "acceptEdits", "/tmp"],
            env=_env_with_fake_bins(OMARCHY_SKILL_RUN_LOG=log_path),
        )
        assert result.returncode == 0, \
            f"Expected exit 0 from fake claude, got {result.returncode}\n{result.stderr}"
        assert Path(log_path).exists(), "Log file was not created"
        log_line = Path(log_path).read_text()
        assert "exit=0"                  in log_line, f"exit=0 not in log: {log_line!r}"
        assert "skill=TACO-cross-audit"  in log_line, f"skill not in log: {log_line!r}"
        assert "model=fable"             in log_line, f"model not in log: {log_line!r}"
        assert "effort=high"             in log_line, f"effort not in log: {log_line!r}"
        assert "mode=acceptEdits"        in log_line, f"mode not in log: {log_line!r}"
        assert "secs="                   in log_line, f"secs= not in log: {log_line!r}"
        assert "via=direct"              in log_line, f"via=direct not in log: {log_line!r}"

        # `artifact=` must name a file that EXISTS and holds the run transcript.
        # Asserting only that the key appears is what let `artifact=-` pass for a
        # whole phase: the old runner guessed the artifact by scanning for the
        # newest file in ~/Work/artifacts, so a run that produced nothing simply
        # logged a dash — and the plan's S-5.2 test ("caminho de artefato
        # existente") had nothing to stand on.
        artifact = log_line.split("artifact=", 1)[1].strip()
        assert artifact != "-", f"artifact must not be '-': {log_line!r}"
        assert Path(artifact).is_file(), f"artifact does not exist: {artifact}"
        assert "claude" in Path(artifact).read_text(), \
            f"artifact does not hold the run transcript: {artifact}"
        Path(artifact).unlink(missing_ok=True)
    finally:
        Path(log_path).unlink(missing_ok=True)


# ── --herdr with fake herdr ───────────────────────────────────────────────────

def test_herdr_records_four_calls() -> None:
    """--herdr must invoke: workspace create, agent start, agent prompt, agent read.

    The existing tests/bin/herdr mock records calls to HERDR_MOCK_LOG.
    We use a temp path to keep test isolation.
    """
    with tempfile.NamedTemporaryFile(suffix=".log", delete=False) as tf:
        herdr_log = tf.name   # HERDR_MOCK_LOG for the pre-existing mock
    with tempfile.NamedTemporaryFile(suffix=".log", delete=False) as tf:
        skill_log = tf.name
    Path(herdr_log).unlink(missing_ok=True)
    Path(skill_log).unlink(missing_ok=True)
    try:
        result = _run(
            ["--herdr", "TACO-cross-audit", "fable", "high", "acceptEdits", "/tmp"],
            env=_env_with_fake_bins(
                HERDR_MOCK_LOG=herdr_log,       # convention used by tests/bin/herdr
                OMARCHY_SKILL_RUN_LOG=skill_log,
            ),
        )
        assert result.returncode == 0, \
            f"Expected exit 0 for --herdr, got {result.returncode}\n{result.stderr}"

        assert Path(herdr_log).exists(), "Herdr call log was not created"
        calls = Path(herdr_log).read_text().strip().splitlines()
        # 5, not 4: `pane read` is the readiness probe that waits for the shell
        # prompt before `agent start` (see wait_for_shell_prompt in the runner).
        assert len(calls) == 5, f"Expected 5 herdr calls, got {len(calls)}: {calls}"
        assert any("pane read" in c for c in calls), \
            f"'pane read' readiness probe not in herdr calls: {calls}"
        assert any("workspace create" in c for c in calls), \
            f"'workspace create' not in herdr calls: {calls}"
        agent_start = next((c for c in calls if "agent start" in c), None)
        assert agent_start, f"'agent start' not in herdr calls: {calls}"
        # model/effort/permission-mode MUST reach the agent through the `--`
        # passthrough. Without it herdr launches a bare `claude`: the three
        # arguments the caller validated are dropped in silence, and the
        # bypassPermissions refusal becomes decorative on this path.
        assert "--pane w1:p1" in agent_start, \
            f"pane id from the create response not used: {agent_start!r}"
        for expected in ("--model fable", "--effort high", "--permission-mode acceptEdits"):
            assert expected in agent_start, \
                f"{expected!r} not passed through to the agent: {agent_start!r}"
        assert any("agent prompt"     in c for c in calls), \
            f"'agent prompt' not in herdr calls: {calls}"
        assert any("agent read"       in c for c in calls), \
            f"'agent read' not in herdr calls: {calls}"

        # `exit` is ALWAYS numeric — it is the status of the dispatch, and `via`
        # is what says the run is asynchronous. The literal `exit=herdr` this
        # replaces broke the field's own contract and, worse, turned the P5 gate
        # red on success: validate_p5.sh asserts `tail -1 … | grep exit=0`, so
        # using the herdr button made the S-5.2 check fail. Measured 2026-08-23.
        assert Path(skill_log).exists(), "Skill run log was not created for --herdr"
        log_line = Path(skill_log).read_text()
        assert "exit=0"    in log_line, f"exit=0 not in log: {log_line!r}"
        assert "via=herdr" in log_line, f"via=herdr not in log: {log_line!r}"
        assert "exit=herdr" not in log_line, \
            f"exit= must be numeric, never the literal 'herdr': {log_line!r}"

        artifact = log_line.split("artifact=", 1)[1].strip()
        assert Path(artifact).is_file(), f"herdr artifact does not exist: {artifact}"
        assert "mock pane output" in Path(artifact).read_text(), \
            f"herdr artifact does not hold the pane snapshot: {artifact}"
        Path(artifact).unlink(missing_ok=True)
    finally:
        Path(herdr_log).unlink(missing_ok=True)
        Path(skill_log).unlink(missing_ok=True)


def test_herdr_dry_run_prints_three_commands() -> None:
    """--herdr --dry-run must print all three herdr commands without executing."""
    result = _run(
        ["--herdr", "--dry-run", "TACO-cross-audit", "fable", "high", "acceptEdits", "/tmp"],
        env=_env_with_fake_bins(),
    )
    assert result.returncode == 0
    output = result.stdout
    assert "workspace create" in output, f"workspace create not in: {output!r}"
    assert "agent start"      in output, f"agent start not in: {output!r}"
    assert "agent prompt"     in output, f"agent prompt not in: {output!r}"
    assert "agent read"       in output, f"agent read not in: {output!r}"
    assert "--permission-mode acceptEdits" in output, \
        f"permission mode not carried into the herdr dry-run: {output!r}"


# ── herdr agent-name derivation ───────────────────────────────────────────────

def test_herdr_agent_name_is_lowercased_from_the_skill() -> None:
    """herdr rejects the skill name verbatim; the runner must derive a legal one.

    herdr 0.8.0: "agent name must start with a lowercase letter and contain only
    lowercase letters, digits, '-' or '_' (1-32 characters)". Passing
    `TACO-cross-audit` straight through returns `invalid_agent_name` and the
    dispatch dies at `agent start` — which is exactly what happened the first
    time the --herdr button was ever executed, on 2026-08-23. The WORKSPACE keeps
    the human-readable label; only the agent name is normalised.
    """
    result = _run(
        ["--herdr", "--dry-run", "TACO-cross-audit", "fable", "high", "acceptEdits", "/tmp"],
        env=_env_with_fake_bins(),
    )
    assert result.returncode == 0
    out = result.stdout
    assert "agent start 'taco-cross-audit'" in out, f"agent name not normalised: {out!r}"
    assert "agent prompt 'taco-cross-audit'" in out, f"prompt uses the wrong name: {out!r}"
    assert "agent read 'taco-cross-audit'" in out, f"read uses the wrong name: {out!r}"
    # The workspace label stays human-readable.
    assert "--label 'TACO-cross-audit'" in out, f"workspace label was mangled: {out!r}"


def test_herdr_agent_name_never_starts_with_a_non_letter() -> None:
    """A skill starting with a digit or punctuation still yields a legal name.

    Two different repairs, both legal: a leading `_`/`-` is simply stripped (the
    remainder already starts with a letter), while a leading digit cannot be
    stripped without changing the name, so it gets a `skill-` prefix.
    """
    for skill, expected in (("9lives", "skill-9lives"), ("_hidden", "hidden"), ("--x", "x")):
        result = _run(
            ["--herdr", "--dry-run", skill, "fable", "high", "acceptEdits", "/tmp"],
            env=_env_with_fake_bins(),
        )
        assert result.returncode == 0, result.stderr
        assert f"agent start '{expected}'" in result.stdout, \
            f"{skill!r} -> expected {expected!r}, got: {result.stdout!r}"


def test_herdr_uses_the_pane_id_from_the_create_response() -> None:
    """The pane id is read from `workspace create`, never assumed to be w1:p1.

    Regression, 2026-08-23: the runner hardcoded `--pane w1:p1`. That holds only
    for the FIRST workspace of a herdr server; the second real dispatch created
    `w2:p1` and herdr answered `agent_pane_not_found`.
    """
    with tempfile.NamedTemporaryFile(suffix=".log", delete=False) as tf:
        herdr_log = tf.name
    with tempfile.NamedTemporaryFile(suffix=".log", delete=False) as tf:
        skill_log = tf.name
    Path(herdr_log).unlink(missing_ok=True)
    Path(skill_log).unlink(missing_ok=True)
    try:
        result = _run(
            ["--herdr", "TACO-cross-audit", "fable", "high", "acceptEdits", "/tmp"],
            env=_env_with_fake_bins(
                HERDR_MOCK_LOG=herdr_log,
                OMARCHY_SKILL_RUN_LOG=skill_log,
                HERDR_MOCK_PANE="w7:p3",
            ),
        )
        assert result.returncode == 0, result.stderr
        calls = Path(herdr_log).read_text()
        assert "--pane w7:p3" in calls, f"pane id not taken from the response: {calls!r}"
        assert "--pane w1:p1" not in calls, f"hardcoded pane id resurfaced: {calls!r}"
        artifact = Path(skill_log).read_text().split("artifact=", 1)[1].strip()
        Path(artifact).unlink(missing_ok=True)
    finally:
        Path(herdr_log).unlink(missing_ok=True)
        Path(skill_log).unlink(missing_ok=True)


def test_herdr_fails_closed_when_the_create_response_has_no_pane() -> None:
    """No pane id in the response must fail loudly, never fall back to a guess."""
    with tempfile.NamedTemporaryFile(suffix=".log", delete=False) as tf:
        skill_log = tf.name
    Path(skill_log).unlink(missing_ok=True)
    try:
        result = _run(
            ["--herdr", "TACO-cross-audit", "fable", "high", "acceptEdits", "/tmp"],
            env=_env_with_fake_bins(
                OMARCHY_SKILL_RUN_LOG=skill_log,
                HERDR_MOCK_PANE="none",
            ),
        )
        assert result.returncode != 0, "a create response with no pane must not succeed"
        assert "pane_id" in result.stderr, f"expected an explicit diagnosis: {result.stderr!r}"
        log_line = Path(skill_log).read_text()
        assert "exit=0" not in log_line, f"failure logged as success: {log_line!r}"
        artifact = log_line.split("artifact=", 1)[1].strip()
        Path(artifact).unlink(missing_ok=True)
    finally:
        Path(skill_log).unlink(missing_ok=True)
