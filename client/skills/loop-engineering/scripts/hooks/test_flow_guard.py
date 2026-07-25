#!/usr/bin/env python3
"""test_flow_guard.py — permanent regression suite for the flow-enforcement guard.

Covers the E1 acceptance battery (originally ad-hoc bash, cross-audit finding
F3: an unrecorded battery regresses silently) plus the cross-audit regressions:

  F1  prose mentioning a flow name must NEVER arm (only a start-anchored slash
      command or a <command-name> tag is an invocation)
  F2  a PreCompact snapshot of an OUTER marker uses a per-project
      ``flow-state:*`` key, never the colliding ``loop-state:OUTER``
  F4  the compliance log is trimmed past 2000 lines

Run: python3 -m pytest test_flow_guard.py -q   (from this directory)
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

HOOKS = Path(__file__).resolve().parent
sys.path.insert(0, str(HOOKS))
import loop_marker as lm  # noqa: E402
import loop_outer_arm as arm  # noqa: E402
import loop_outer_gate as gate  # noqa: E402
import loop_snapshot as snapshot  # noqa: E402

ARM = HOOKS / "loop_outer_arm.py"
GATE = HOOKS / "loop_outer_gate.py"
GUARD = HOOKS / "loop_stop_guard.py"


@pytest.fixture(autouse=True)
def isolated_marker_dir(tmp_path_factory, monkeypatch):
    """Route ALL marker writes (in-process AND subprocess) to a temp dir.

    Without this, every subprocess-driven test leaves an orphan
    ``active-<hash>.json`` in the PRODUCTION marker dir (~30 found on
    2026-07-24). The env var covers subprocesses; the setattr covers the
    modules already imported in this process."""
    d = tmp_path_factory.mktemp("markers")
    monkeypatch.setenv("LOOP_ENGINEERING_HOME", str(d))
    monkeypatch.setattr(lm, "MARKER_DIR", d)
    monkeypatch.setattr(gate, "MARKER_DIR", d)
    monkeypatch.setattr(gate, "COMPLIANCE_LOG", d / "compliance.jsonl")
    yield d


def run_arm(prompt: str, cwd: Path) -> subprocess.CompletedProcess:
    payload = json.dumps({"prompt": prompt, "cwd": str(cwd)})
    return subprocess.run([sys.executable, str(ARM)], input=payload,
                          capture_output=True, text=True, timeout=30)


def marker_for(cwd: Path) -> Path:
    return lm.marker_path(str(cwd))


def write_outer_marker(cwd: Path, bundle: Path | None, flow="strategy-outer") -> Path:
    lm.write_marker(task="OUTER", scope=str(cwd), bundle=str(bundle) if bundle else None,
                    cwd=str(cwd), status="outer", flow=flow)
    return marker_for(cwd)


def gate_report(marker: Path) -> tuple[int, dict]:
    proc = subprocess.run(
        [sys.executable, str(GATE), "--marker", str(marker), "--json", "--no-emit"],
        capture_output=True, text=True, timeout=30)
    return proc.returncode, json.loads(proc.stdout)


def make_complete_artifacts(scope: Path, bundle: Path) -> None:
    (bundle / "diagnostics").mkdir(parents=True, exist_ok=True)
    (bundle / "diagnostics" / "d.md").write_text("diag")
    (bundle / "strategy-2026-07-23-t.md").write_text("strategy")
    (scope / ".touring-explore").mkdir(parents=True, exist_ok=True)
    (scope / ".touring-explore" / "t.ledger.json").write_text("{}")


# ── detection (F1 regression: invocation forms only, prose never) ────────────

@pytest.mark.parametrize("prompt,flow", [
    ("/loop-engineering migrar o touring", "strategy-outer"),
    ("  /goal terminar tudo", "strategy-outer"),
    ("/TACO-cross-audit auditar", "cross-audit"),
    ("<command-message>x</command-message>\n<command-name>/TACO-cross-audit</command-name>", "cross-audit"),
    ("<command-name>loop-engineering</command-name>", "strategy-outer"),
])
def test_invocation_forms_detected(prompt, flow):
    assert arm.detect_flow(prompt) == flow


@pytest.mark.parametrize("prompt", [
    "o skill loop-engineering ficou ótimo, obrigado",
    "qual é o /goal disso tudo?",
    "sobre o TACO-cross-audit conversamos amanhã",
    "veja docs em skills/loop-engineering/SKILL.md",
    "",
])
def test_prose_never_arms(prompt):
    assert arm.detect_flow(prompt) is None


# ── arming (subprocess E2E, per-project markers in tmp cwds) ─────────────────

def test_arm_writes_outer_marker(tmp_path):
    proc = run_arm("/loop-engineering migrar touring", tmp_path)
    assert proc.returncode == 0
    ctx = json.loads(proc.stdout)["hookSpecificOutput"]["additionalContext"]
    assert "strategy-outer" in ctx
    data = json.loads(marker_for(tmp_path).read_text())
    assert (data["status"], data["flow"], data["task"]) == ("outer", "strategy-outer", "OUTER")


def test_arm_is_silent_noop_on_plain_prompt(tmp_path):
    proc = run_arm("como está o tempo hoje?", tmp_path)
    assert proc.returncode == 0 and proc.stdout.strip() == ""
    assert not marker_for(tmp_path).exists()


def test_arm_never_clobbers_active_loop(tmp_path):
    lm.write_marker(task="task_REAL", scope=str(tmp_path), cwd=str(tmp_path), status="active")
    run_arm("/loop-engineering de novo", tmp_path)
    assert json.loads(marker_for(tmp_path).read_text())["task"] == "task_REAL"


def test_arm_fails_open_on_garbage_stdin():
    proc = subprocess.run([sys.executable, str(ARM)], input="not json",
                          capture_output=True, text=True, timeout=30)
    assert proc.returncode == 0 and proc.stdout.strip() == ""


def test_arm_without_cwd_is_noop_never_env_fallback(tmp_path):
    """F5 regression: a payload without cwd must NOT arm the project pointed to
    by CLAUDE_PROJECT_DIR (that fallback overwrote a real marker during the
    2026-07-23 cross-audit)."""
    env = dict(os.environ, CLAUDE_PROJECT_DIR=str(tmp_path))
    payload = json.dumps({"prompt": "/loop-engineering sem cwd"})
    proc = subprocess.run([sys.executable, str(ARM)], input=payload, env=env,
                          capture_output=True, text=True, timeout=30)
    assert proc.returncode == 0 and proc.stdout.strip() == ""
    assert not marker_for(tmp_path).exists()


# ── the artifact gate ────────────────────────────────────────────────────────

def test_gate_incomplete_lists_missing_and_exits_1(tmp_path):
    marker = write_outer_marker(tmp_path / "p", tmp_path / "b")
    rc, report = gate_report(marker)
    assert rc == 1 and report["complete"] is False
    assert {m["id"] for m in report["missing"]} == {
        "diagnostic-okf", "explore-ledger", "strategy-doc"}


def test_gate_completes_with_artifacts(tmp_path):
    scope, bundle = tmp_path / "p", tmp_path / "b"
    marker = write_outer_marker(scope, bundle)
    make_complete_artifacts(scope, bundle)
    rc, report = gate_report(marker)
    assert (rc, report["complete"]) == (0, True) and report["missing"] == []


def test_gate_null_bundle_surfaces_bundle_artifacts_as_missing(tmp_path):
    marker = write_outer_marker(tmp_path / "p", None)
    rc, report = gate_report(marker)
    missing = {m["id"] for m in report["missing"]}
    assert rc == 1 and "diagnostic-okf" in missing and "strategy-doc" in missing


def test_gate_stale_artifact_does_not_count(tmp_path):
    scope, bundle = tmp_path / "p", tmp_path / "b"
    marker = write_outer_marker(scope, bundle)
    make_complete_artifacts(scope, bundle)
    old = 1_000_000.0  # far before the marker's created_at → outside the floor
    for p in [bundle / "diagnostics" / "d.md"]:
        os.utime(p, (old, old))
    rc, report = gate_report(marker)
    assert rc == 1 and "diagnostic-okf" in {m["id"] for m in report["missing"]}


def test_gate_fails_open_on_bad_manifests(tmp_path):
    marker = write_outer_marker(tmp_path / "p", tmp_path / "b")
    proc = subprocess.run(
        [sys.executable, str(GATE), "--marker", str(marker),
         "--manifests", "/dev/null", "--no-emit"],
        capture_output=True, text=True, timeout=30)
    assert proc.returncode == 0


# ── the Stop guard (block → continue → allow) ────────────────────────────────

def test_stop_guard_blocks_then_allows(tmp_path):
    scope, bundle = tmp_path / "p", tmp_path / "b"
    marker = write_outer_marker(scope, bundle)
    blocked = subprocess.run([sys.executable, str(GUARD), "--marker", str(marker)],
                             capture_output=True, text=True, timeout=60)
    decision = json.loads(blocked.stdout)
    assert decision["decision"] == "block" and "explore-ledger" in decision["reason"]
    assert json.loads(marker.read_text())["continuations"] == 1
    make_complete_artifacts(scope, bundle)
    allowed = subprocess.run([sys.executable, str(GUARD), "--marker", str(marker)],
                             capture_output=True, text=True, timeout=60)
    assert allowed.stdout.strip() == ""
    assert json.loads(marker.read_text())["outer_complete"] is True


def test_stop_guard_outer_cap_allows(tmp_path):
    marker = write_outer_marker(tmp_path / "p", tmp_path / "b")
    data = json.loads(marker.read_text())
    data["continuations"] = 99
    lm.save_marker(marker, data)
    proc = subprocess.run([sys.executable, str(GUARD), "--marker", str(marker)],
                          capture_output=True, text=True, timeout=60)
    assert proc.stdout.strip() == "" and "cap" in proc.stderr


# ── registration regression: hooks must run AS REGISTERED ────────────────────

def test_registered_hook_commands_are_runnable(tmp_path):
    """The 2026-07-23 incident: loop_outer_arm.py was registered as a direct
    path but never received +x (its chmod was inside a compound command a
    PreToolUse hook denied, and the rewritten command omitted it) → every
    UserPromptSubmit failed with Permission denied. This test runs each
    loop-engineering hook command EXACTLY as Claude Code does (/bin/sh -c,
    JSON on stdin) and requires the fail-open exit 0."""
    settings = json.loads((Path.home() / ".claude" / "settings.json").read_text())
    commands = [h["command"]
                for entries in settings.get("hooks", {}).values()
                for e in entries for h in e.get("hooks", [])
                if "loop-engineering" in h.get("command", "")]
    assert commands, "loop-engineering hooks must be registered"
    payload = json.dumps({"prompt": "ping", "cwd": str(tmp_path)})
    for cmd in commands:
        proc = subprocess.run(["/bin/sh", "-c", cmd], input=payload,
                              capture_output=True, text=True, timeout=60,
                              env=dict(os.environ, HOME=str(Path.home())))
        assert proc.returncode == 0, f"hook not runnable as registered: {cmd!r} → " \
                                     f"rc={proc.returncode} stderr={proc.stderr[:200]}"


# ── F2 regression: OUTER snapshot key is per-project flow-state ──────────────

def test_snapshot_outer_uses_flow_state_key(tmp_path, monkeypatch):
    marker = {"task": "OUTER", "status": "outer", "flow": "strategy-outer",
              "scope": str(tmp_path), "cwd": str(tmp_path), "bundle": None}
    calls = []

    def fake_run(cmd, **kwargs):
        calls.append(cmd)
        return subprocess.CompletedProcess(cmd, 0, stdout="{}", stderr="")

    monkeypatch.setattr(snapshot.subprocess, "run", fake_run)
    assert snapshot.snapshot_outer(marker, "2026-07-23T00:00:00") == 0
    stores = [c for c in calls if c[:3] == ["touring", "memory", "store"]]
    assert stores, "outer snapshot must persist a memory record"
    key = stores[0][3]
    assert key.startswith("flow-state:strategy-outer:") and "loop-state:OUTER" not in key


# ── F4 regression: compliance log is bounded ─────────────────────────────────

def test_compliance_log_trims(tmp_path, monkeypatch):
    log = tmp_path / "compliance.jsonl"
    log.write_text("{}\n" * 2500)
    monkeypatch.setattr(gate, "COMPLIANCE_LOG", log)
    gate._trim_log()
    assert len(log.read_text().splitlines()) == 1000
