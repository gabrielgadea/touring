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
import time
from pathlib import Path

import pytest

HOOKS = Path(__file__).resolve().parent
sys.path.insert(0, str(HOOKS))
import loop_marker as lm  # noqa: E402
import loop_outer_arm as arm  # noqa: E402
import loop_outer_gate as gate  # noqa: E402
import loop_stop_guard as guard  # noqa: E402
import loop_snapshot as snapshot  # noqa: E402

ARM = HOOKS / "loop_outer_arm.py"
GATE = HOOKS / "loop_outer_gate.py"
GUARD = HOOKS / "loop_stop_guard.py"
RESUME = HOOKS / "loop_resume.py"


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


@pytest.fixture(autouse=True)
def ambiente_sem_kill_switch(monkeypatch):
    """A suíte não herda política do ambiente (painel cego 30/08/2026).

    Sob ``TOURING_WORK_OUTER_DISABLED=1`` ambiente — exatamente o env com que
    os agentes headless de ADW são spawnados por design — 6 testes que assumem
    arming falhavam (62/68, reproduzido pelas 3 lentes do painel). O default
    aqui é a env LIMPA; o teste do kill switch o liga explicitamente via
    ``monkeypatch.setenv``, que vence este delenv por ordem de aplicação."""
    monkeypatch.delenv("TOURING_WORK_OUTER_DISABLED", raising=False)


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
    # The manifest requires verdict.converged == true (the CCE contract) — an
    # empty ledger is precisely the "not yet dry" state and must NOT complete
    # the gate (this fixture once planted "{}" and three tests rotted green).
    (scope / ".touring-explore" / "t.ledger.json").write_text(
        '{"verdict": {"converged": true}}')


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


# ── the DEFAULT flow: substantive work arms even without a slash command ─────
# Gabriel, 2026-08-02: "o OUTER determinístico já é um exemplo de um
# procedimento que deve ser padrão". Precision is the constraint — the prose
# cases above must keep returning None.

@pytest.mark.parametrize("prompt", [
    "sim, corrija todas as falhas do CI",
    "faça o CI auto-sincronizar com o sync_metrics comparando LOC vivo",
    "certifique-se de que o modus operandi padrão seja o do loop",
    "refactor the daemon socket resolution and add tests",
])
def test_substantive_work_arms_default_flow(prompt):
    assert arm.detect_flow(prompt) == arm.DEFAULT_FLOW


@pytest.mark.parametrize("prompt", [
    "me explique como funciona o wiring do daemon por favor",  # question, no imperative
    "desative o block_git.sh",                                 # below the substance floor
    "o fix do audit ficou bom",                                # nouns, not commands
])
def test_default_flow_stays_out_of_conversation(prompt):
    assert arm.detect_flow(prompt) is None


def test_default_flow_kill_switch(monkeypatch):
    prompt = "corrija todas as falhas do CI agora"
    assert arm.detect_flow(prompt) == arm.DEFAULT_FLOW
    monkeypatch.setenv("TOURING_WORK_OUTER_DISABLED", "1")
    assert arm.detect_flow(prompt) is None


def test_default_flow_never_downgrades_an_invoked_flow(tmp_path):
    """A work prompt mid-flow must not swap a strict manifest for the loose one.

    `strategy-outer` still owes a strategy doc; letting `work-outer` overwrite
    the marker would silently forgive it.
    """
    run_arm("/loop-engineering plano grande", tmp_path)
    run_arm("corrija agora todas as falhas do CI", tmp_path)
    assert json.loads(marker_for(tmp_path).read_text())["flow"] == "strategy-outer"


# ── defect #4: N Claude Code sessions on ONE project must not share a loop ───
# Gabriel, 2026-08-02: "o daemon considera os loops como se fossem de uma única
# sessão". The marker was keyed by cwd alone, so B's Stop was held by A's unmet
# manifest, B free-rode on A's artifacts, and the continuation cap was shared.

def test_marker_path_separates_sessions_on_the_same_project(tmp_path, monkeypatch):
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-A")
    a = lm.marker_path(str(tmp_path))
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-B")
    b = lm.marker_path(str(tmp_path))
    assert a != b
    assert lm._key(str(tmp_path.resolve())) in a.name  # project dimension kept


def test_session_b_never_sees_session_a_loop(tmp_path, monkeypatch):
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-A")
    lm.write_marker(task="task_A", scope=str(tmp_path), cwd=str(tmp_path),
                    status="active")
    assert lm.active_marker(str(tmp_path))[1]["task"] == "task_A"
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-B")
    assert lm.active_marker(str(tmp_path)) == (None, None)


def test_foreign_stamp_rejected_even_on_a_reached_path(tmp_path, monkeypatch):
    """Defense in depth: the stamp, not just the filename, decides ownership."""
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-A")
    path = lm.marker_path(str(tmp_path))
    lm.write_marker(task="task_A", scope=str(tmp_path), cwd=str(tmp_path))
    data = json.loads(path.read_text())
    data["session_id"] = "sess-OTHER"
    path.write_text(json.dumps(data))
    assert lm.active_marker(str(tmp_path)) == (None, None)


def test_unattributed_marker_is_claimed_once(tmp_path, monkeypatch):
    """Migration: a pre-session marker is adopted by the first evaluator, once."""
    legacy = lm.project_marker_path(str(tmp_path))
    legacy.parent.mkdir(parents=True, exist_ok=True)
    legacy.write_text(json.dumps({"task": "task_OLD", "scope": str(tmp_path),
                                  "cwd": str(tmp_path.resolve()), "status": "active",
                                  "created_at": time.time(), "updated_at": time.time()}))
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-A")
    path, data = lm.active_marker(str(tmp_path))
    assert data["task"] == "task_OLD" and data["session_id"] == "sess-A"
    assert path == lm.marker_path(str(tmp_path)) and not legacy.exists()
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-B")
    assert lm.active_marker(str(tmp_path)) == (None, None)  # claimed, not re-shared


def test_without_session_identity_project_scope_is_unchanged(tmp_path, monkeypatch):
    """No resolvable session ⇒ exactly the pre-2026-08-02 behavior."""
    monkeypatch.delenv("CLAUDE_CODE_SESSION_ID", raising=False)
    monkeypatch.delenv("TOURING_SESSION_ID", raising=False)
    assert lm.marker_path(str(tmp_path)) == lm.project_marker_path(str(tmp_path))
    lm.write_marker(task="task_X", scope=str(tmp_path), cwd=str(tmp_path))
    assert lm.active_marker(str(tmp_path))[1]["task"] == "task_X"


def test_stop_of_session_b_is_not_held_by_session_a(tmp_path, monkeypatch):
    """The end-to-end symptom: B's turn must end even while A's OUTER is unmet."""
    scope, bundle = tmp_path / "p", tmp_path / "b"
    scope.mkdir()
    bundle.mkdir()
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-A")
    lm.write_marker(task="OUTER", scope=str(scope), bundle=str(bundle),
                    cwd=str(scope), status="outer", flow="strategy-outer")

    def guard_stdout(session: str) -> str:
        monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", session)
        return subprocess.run([sys.executable, str(GUARD)], cwd=str(scope),
                              capture_output=True, text=True, timeout=60,
                              stdin=subprocess.DEVNULL).stdout

    assert guard_stdout("sess-B").strip() == ""          # B is free
    assert json.loads(guard_stdout("sess-A"))["decision"] == "block"  # A still owes


# ── recuperação de contexto: a chave do snapshot e o leitor (2026-08-02) ─────

def test_state_key_is_deterministic_across_processes(tmp_path):
    """The snapshot key must be reproducible by a LATER process, or it is lost.

    `abs(hash(cwd)) % 10**8` was not: Python randomizes str hashing per process
    (PYTHONHASHSEED), so three consecutive runs gave 86221029 / 375623 /
    47015023 for one cwd and every PreCompact record landed under a fresh,
    unlookupable key. Violated REGRA #17 (ids are derived, never emergent).
    """
    marker = {"status": "outer", "flow": "strategy-outer", "task": "OUTER",
              "cwd": str(tmp_path), "session_id": "sess-A"}
    code = (f"import sys; sys.path.insert(0, {str(HOOKS)!r});"
            f"from loop_marker import state_key; print(state_key({marker!r}))")
    keys = {subprocess.run([sys.executable, "-c", code], capture_output=True,
                           text=True, timeout=30).stdout.strip() for _ in range(3)}
    assert len(keys) == 1
    assert keys.pop().startswith("flow-state:strategy-outer:")


def test_state_key_separates_sessions_and_tasks(tmp_path):
    a = {"status": "outer", "flow": "strategy-outer", "task": "OUTER",
         "cwd": str(tmp_path), "session_id": "sess-A"}
    b = dict(a, session_id="sess-B")
    assert lm.state_key(a) != lm.state_key(b)
    # an active loop keys on its (already unique) task id
    assert lm.state_key({"status": "active", "task": "task_42"}) == "loop-state:task_42"


def test_resume_reinjects_unmet_outer_then_goes_silent(tmp_path):
    """The reader that did not exist: state must come back after a compaction."""
    scope, bundle = tmp_path / "p", tmp_path / "b"
    scope.mkdir()
    bundle.mkdir()
    marker = write_outer_marker(scope, bundle)
    proc = subprocess.run([sys.executable, str(RESUME), "--marker", str(marker)],
                          input=json.dumps({"cwd": str(scope),
                                            "hook_event_name": "SessionStart"}),
                          capture_output=True, text=True, timeout=90)
    assert proc.returncode == 0
    ctx = json.loads(proc.stdout)["hookSpecificOutput"]
    assert ctx["hookEventName"] == "SessionStart"
    assert "explore-ledger" in ctx["additionalContext"]
    assert str(bundle) in ctx["additionalContext"]   # real value, no placeholder
    make_complete_artifacts(scope, bundle)
    done = subprocess.run([sys.executable, str(RESUME), "--marker", str(marker)],
                          input="{}", capture_output=True, text=True, timeout=90)
    assert done.returncode == 0 and done.stdout.strip() == ""  # nothing owed → quiet


def test_resume_emits_a_shape_the_harness_accepts_for_every_event(tmp_path):
    """Structural guard over ALL events, not just the one that broke.

    `hookSpecificOutput` is validated as a discriminated union on
    `hookEventName`, and PostCompact is not a member: echoing the payload's
    event name back made every post-compaction run die with
    `(root): Invalid input` — state computed, then discarded. The previous test
    asserted the emitted *shape* and so encoded the bug instead of catching it;
    a shape assertion only proves what we produced, never what is accepted.
    """
    import loop_resume as resume  # noqa: PLC0415 — hooks dir is on sys.path

    scope, bundle = tmp_path / "p", tmp_path / "b"
    scope.mkdir()
    bundle.mkdir()
    marker = write_outer_marker(scope, bundle)
    root_only_fields = {"continue", "suppressOutput", "stopReason", "decision",
                        "reason", "systemMessage", "terminalSequence",
                        "permissionDecision", "hookSpecificOutput"}
    for event in ["SessionStart", "PostCompact", "PreCompact", "Notification"]:
        proc = subprocess.run(
            [sys.executable, str(RESUME), "--marker", str(marker)],
            input=json.dumps({"cwd": str(scope), "hook_event_name": event}),
            capture_output=True, text=True, timeout=90)
        assert proc.returncode == 0, f"{event}: {proc.stderr}"
        out = json.loads(proc.stdout)
        assert set(out) <= root_only_fields, f"{event} emitted unknown root keys: {set(out)}"
        hso = out.get("hookSpecificOutput")
        if hso is not None:
            assert hso["hookEventName"] in resume.CONTEXT_EVENTS, (
                f"{event} has no additionalContext channel — the harness rejects it")
        else:
            # The state must still surface, never be silently dropped.
            assert str(bundle) in out["systemMessage"]


def test_completed_is_terminal_at_every_dag_reader(tmp_path):
    """C08 cross-caller: one vocabulary, four readers.

    `touring decompose` closes subtasks as "completed"; only `loop_converged.py`
    listed it. So on a DAG with 4 of 6 closed, the resume hook announced
    "6/6 subtasks pending" and told the next context to redo finished work.
    """
    subs = [{"subtask_id": "t::A", "status": "completed"},
            {"subtask_id": "t::B", "status": "done"},
            {"subtask_id": "t::C", "status": "finalized"},
            {"subtask_id": "t::D", "status": "pending"},
            {"subtask_id": "t::E", "status": "in_progress"}]
    assert lm.pending_subtask_ids(subs) == ["D", "E"]

    # No reader may re-introduce a private terminal set.
    readers = ["loop_resume.py", "loop_snapshot.py", "loop_stop_guard.py",
               "../loop_converged.py"]
    for name in readers:
        src = (HOOKS / name).read_text()
        assert "pending_subtask_ids" in src, f"{name} does not use the shared reader"
        assert '"done", "finalized"' not in src, f"{name} kept a private terminal set"


def test_resume_fails_open_without_marker_or_stdin(tmp_path):
    missing = subprocess.run([sys.executable, str(RESUME), "--marker",
                              str(tmp_path / "nope.json")],
                             input="", capture_output=True, text=True, timeout=60)
    assert missing.returncode == 0 and missing.stdout.strip() == ""
    garbage = subprocess.run([sys.executable, str(RESUME)], input="not json",
                             capture_output=True, text=True, timeout=60)
    assert garbage.returncode == 0


def test_default_flow_derives_a_stable_bundle(tmp_path):
    """The default flow must never leave `bundle` null — the manifest needs it."""
    (tmp_path / "docs").mkdir()
    run_arm("corrija todas as falhas do CI do projeto", tmp_path)
    first = json.loads(marker_for(tmp_path).read_text())["bundle"]
    assert first and "docs/plans" in first and first.endswith("-work-outer")
    run_arm("atualize também os testes desse modulo", tmp_path)
    assert json.loads(marker_for(tmp_path).read_text())["bundle"] == first


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


# ─────────────────────────────────────────────────────────────────────────────
# flow_armed_at — the artifact gate's mtime floor (20/08/2026)
# ─────────────────────────────────────────────────────────────────────────────
#
# The floor used to be `created_at`, which survives every re-arm. Measured on a
# live marker: created 18/08 09:45, still arming work 20/08 01:11 — a floor 39.4h
# old, accepting five explore ledgers from unrelated topics. The Stop hook ran 54
# times in one session and never blocked, because Law L3's "verdict by artifact"
# was being satisfied by somebody else's artifact.


def _armed(tmp_path, monkeypatch, flow, **extra):
    """write_marker into an isolated dir; return the persisted marker."""
    import json
    monkeypatch.setattr(lm, "MARKER_DIR", tmp_path)
    monkeypatch.setattr(lm, "marker_path",
                        lambda cwd, sess=None: tmp_path / "m.json")
    lm.write_marker("OUTER", "/x", cwd="/x", status="outer",
                             flow=flow, **extra)
    return json.loads((tmp_path / "m.json").read_text())


def test_flow_armed_at_is_stamped_on_a_new_marker(tmp_path, monkeypatch):
    assert _armed(tmp_path, monkeypatch, "work-outer")["flow_armed_at"] > 0


def test_re_arming_the_same_open_flow_keeps_the_floor(tmp_path, monkeypatch):
    """An open cycle must not push its own floor forward on every prompt —
    that would make the gate unsatisfiable in the opposite direction."""
    first = _armed(tmp_path, monkeypatch, "work-outer")["flow_armed_at"]
    time.sleep(0.02)
    assert _armed(tmp_path, monkeypatch, "work-outer")["flow_armed_at"] == first


def test_changing_flow_restamps_the_floor(tmp_path, monkeypatch):
    first = _armed(tmp_path, monkeypatch, "work-outer")["flow_armed_at"]
    time.sleep(0.02)
    assert _armed(tmp_path, monkeypatch, "strategy-outer")["flow_armed_at"] > first


def test_a_satisfied_manifest_starts_a_new_cycle(tmp_path, monkeypatch):
    """New work after the previous manifest was met deserves a fresh floor —
    otherwise the second goal inherits the first goal's artifacts."""
    import json
    first = _armed(tmp_path, monkeypatch, "strategy-outer")["flow_armed_at"]
    d = json.loads((tmp_path / "m.json").read_text())
    d["outer_complete"] = True
    (tmp_path / "m.json").write_text(json.dumps(d))
    time.sleep(0.02)
    assert _armed(tmp_path, monkeypatch, "strategy-outer")["flow_armed_at"] > first


def test_created_at_never_moves(tmp_path, monkeypatch):
    """`created_at` keeps meaning 'when this marker appeared' — the new field is
    additive, and TTL/session logic that reads it is untouched."""
    first = _armed(tmp_path, monkeypatch, "work-outer")["created_at"]
    time.sleep(0.02)
    _armed(tmp_path, monkeypatch, "strategy-outer")
    import json
    assert json.loads((tmp_path / "m.json").read_text())["created_at"] == first


def test_gate_prefers_flow_armed_at_over_created_at():
    """The gate must read the new floor; falling back to `created_at` only for
    markers written before this field existed."""
    src = Path(__file__).with_name("loop_outer_gate.py").read_text()
    assert 'marker.get("flow_armed_at")' in src, (
        "the artifact gate must floor on when THIS flow started, not on when the "
        "marker was first created")


class _proc:
    """Minimal CompletedProcess stand-in for the gate subprocess."""

    def __init__(self, stdout: str):
        self.stdout = stdout
        self.stderr = ""
        self.returncode = 0


def _capture(fn) -> str:
    """Run fn, returning whatever it printed to stdout."""
    import io
    import contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        fn()
    return buf.getvalue()


# ─────────────────────────────────────────────────────────────────────────────
# OUTER→INNER handoff — the gate that was missing entirely (20/08/2026)
# ─────────────────────────────────────────────────────────────────────────────
#
# The guard verified the OUTER artifacts and then went quiet forever. Step 11
# ("register the DAG, flip the marker to active") was prose: nothing made it
# happen, so the substantive work — every phase of it — was gated by nothing.
# Observed live: OUTER complete, DAG at subtask_count 0, every later turn allowed.


def test_first_completion_is_the_human_gate(tmp_path, monkeypatch):
    """Stopping right after the OUTER completes is legitimate — that IS step 9."""
    marker = {"status": "outer", "task": "OUTER", "flow": "strategy-outer",
              "continuations": 0}
    monkeypatch.setattr(guard, "save_marker", lambda *a, **k: None)
    monkeypatch.setattr(guard.subprocess, "run", lambda *a, **k: _proc(
        '{"applicable": true, "complete": true, "flow": "strategy-outer"}'))
    out = _capture(lambda: guard.outer_phase_gate(tmp_path / "m.json", marker))
    assert out.strip() == "", "the human gate must not be blocked"
    assert marker["outer_complete"] is True


def test_later_stops_demand_the_dag_handoff(tmp_path, monkeypatch):
    """Once the human has answered, a marker still parked on the placeholder task
    means no phase is being gated — that must block, with the exact next step."""
    marker = {"status": "outer", "task": "OUTER", "flow": "strategy-outer",
              "continuations": 0, "outer_complete": True, "bundle": "docs/plans/x"}
    monkeypatch.setattr(guard, "save_marker", lambda *a, **k: None)
    monkeypatch.setattr(guard.subprocess, "run", lambda *a, **k: _proc(
        '{"applicable": true, "complete": true, "flow": "strategy-outer", '
        '"max_continuations": 5}'))
    out = _capture(lambda: guard.outer_phase_gate(tmp_path / "m.json", marker))
    assert '"decision": "block"' in out
    assert "never entered the INNER" in out
    assert "loop_marker.py write --task" in out, "must carry the exact next command"


def test_a_real_dag_ends_the_handoff_demand(tmp_path, monkeypatch):
    """After step 11 the task is a real id — the handoff is done, nothing owed."""
    marker = {"status": "outer", "task": "task_123", "flow": "strategy-outer",
              "continuations": 0, "outer_complete": True}
    monkeypatch.setattr(guard, "save_marker", lambda *a, **k: None)
    monkeypatch.setattr(guard.subprocess, "run", lambda *a, **k: _proc(
        '{"applicable": true, "complete": true, "flow": "strategy-outer"}'))
    assert _capture(lambda: guard.outer_phase_gate(tmp_path / "m.json", marker)).strip() == ""


def test_handoff_demand_respects_the_cap(tmp_path, monkeypatch):
    """A guard with no cap is a session that cannot be ended — fail-open at the cap."""
    marker = {"status": "outer", "task": "OUTER", "flow": "strategy-outer",
              "continuations": 99, "outer_complete": True}
    monkeypatch.setattr(guard, "save_marker", lambda *a, **k: None)
    monkeypatch.setattr(guard.subprocess, "run", lambda *a, **k: _proc(
        '{"applicable": true, "complete": true, "flow": "strategy-outer", '
        '"max_continuations": 5}'))
    assert _capture(lambda: guard.outer_phase_gate(tmp_path / "m.json", marker)).strip() == ""


def test_placeholder_detector_distinguishes_real_task_ids():
    assert guard._still_awaiting_dag({"status": "outer", "task": "OUTER"})
    assert guard._still_awaiting_dag({"status": "outer", "task": ""})
    assert not guard._still_awaiting_dag({"status": "outer", "task": "task_9"})
    assert not guard._still_awaiting_dag({"status": "active", "task": "OUTER"})


# ─────────────────────────────────────────────────────────────────────────────
# The timeout defect — a gate slower than its own timeout is not a gate
# ─────────────────────────────────────────────────────────────────────────────
#
# Measured 20/08/2026: the Stop hook was registered with timeout=20 while the
# convergence gate it ran took 24.0s. Claude Code killed it on EVERY Stop, before
# it could print a verdict — so the loop never once held a turn, while the guard
# looked perfectly healthy when run by hand (which has no timeout). The blocking
# decision, meanwhile, is settled by a 0.00s RPC: a non-empty `ready` set means
# `dag_done` is unmet, and no other clause can rescue it.


def _stop_hook_entries() -> list[dict]:
    settings = json.loads((Path.home() / ".claude" / "settings.json").read_text())
    return [h for e in settings.get("hooks", {}).get("Stop", [])
            for h in e.get("hooks", []) if "loop_stop_guard.py" in h.get("command", "")]


def test_registered_stop_timeout_dominates_the_guard_budget():
    """The invariant that was violated: whatever timeout the guard is REGISTERED
    with must exceed the worst case the guard itself allows its subprocesses."""
    entries = _stop_hook_entries()
    assert entries, "loop_stop_guard.py must be registered as a Stop hook"
    for h in entries:
        assert h.get("timeout", 0) >= guard.SELF_BUDGET_SECONDS, (
            f"Stop hook registered with timeout={h.get('timeout')} < the guard's own "
            f"budget of {guard.SELF_BUDGET_SECONDS}s — it would be killed before deciding")


def test_registered_stop_command_is_invoked_through_python3():
    """Same lesson as the 2026-07-23 +x incident: a bare path depends on a mode
    bit that nothing in the repo guarantees."""
    for h in _stop_hook_entries():
        assert h["command"].startswith("python3 "), \
            f"Stop hook must be registered as `python3 <path>`, got {h['command']!r}"


def _live_marker(tmp_path, **extra) -> Path:
    path = tmp_path / "m.json"
    marker = {"task": "task_1", "status": "active", "scope": str(tmp_path),
              "cwd": str(tmp_path), "bundle": None, "continuations": 0}
    marker.update(extra)
    path.write_text(json.dumps(marker))
    return path


def test_ready_subtasks_block_without_running_the_expensive_gate(tmp_path, monkeypatch):
    """The fix: `ready` non-empty settles it, so the 24s gate must not be run."""
    path = _live_marker(tmp_path)
    monkeypatch.setattr(guard, "dag_pending", lambda t: (True, 2))
    monkeypatch.setattr(guard, "dag_ready", lambda t: ["task_1::W1", "task_1::W2"])

    def _forbidden(*a, **k):
        raise AssertionError("run_converged must not run while a subtask is ready")

    monkeypatch.setattr(guard, "run_converged", _forbidden)
    out = _capture(lambda: guard.main(["--marker", str(path)]))
    payload = json.loads(out)
    assert payload["decision"] == "block"
    assert "W1" in payload["reason"] and "W2" in payload["reason"]
    assert "dag_done" in payload["reason"]
    assert json.loads(path.read_text())["continuations"] == 1


def test_drained_dag_still_consults_the_full_gate(tmp_path, monkeypatch):
    """The gate keeps the converged half — it is affordable exactly there."""
    path = _live_marker(tmp_path)
    seen = []
    monkeypatch.setattr(guard, "dag_pending", lambda t: (True, 0))
    monkeypatch.setattr(guard, "dag_ready", lambda t: [])
    monkeypatch.setattr(guard, "run_converged",
                        lambda t, m: (seen.append(t), (0, {}))[1])
    out = _capture(lambda: guard.main(["--marker", str(path)]))
    assert out.strip() == "", "a converged loop must allow the stop"
    assert seen == ["task_1"], "the full gate must still decide convergence"
    assert not path.exists(), "convergence archives the marker"


def test_undeterminable_ready_falls_back_to_the_full_gate(tmp_path, monkeypatch):
    """Fail-open on the cheap query means the OLD path, never a silent allow."""
    path = _live_marker(tmp_path)
    monkeypatch.setattr(guard, "dag_pending", lambda t: (True, 3))
    monkeypatch.setattr(guard, "dag_ready", lambda t: None)
    monkeypatch.setattr(guard, "run_converged",
                        lambda t, m: (1, {"unmet": ["cargo_green"],
                                          "next_action": "fix the build"}))
    payload = json.loads(_capture(lambda: guard.main(["--marker", str(path)])))
    assert payload["decision"] == "block"
    assert "cargo_green" in payload["reason"] and "fix the build" in payload["reason"]


def test_dag_ready_parses_the_real_envelope():
    """Shape lock against the live `touring decompose ready` payload."""
    envelope = json.dumps({"ready_subtasks": [
        {"subtask_id": "task_9::W1", "status": "pending"},
        {"subtask_id": "task_9::W2", "status": "pending"}], "task_id": "task_9"})

    class _P:
        stdout, stderr, returncode = envelope, "", 0

    import loop_stop_guard as g
    orig = g.subprocess.run
    try:
        g.subprocess.run = lambda *a, **k: _P()
        assert g.dag_ready("task_9") == ["task_9::W1", "task_9::W2"]
        g.subprocess.run = lambda *a, **k: (_ for _ in ()).throw(OSError("daemon down"))
        assert g.dag_ready("task_9") is None
    finally:
        g.subprocess.run = orig


# ── W6 S-6.2 (plano code-mode-total): rajada sem programa bloqueia o turno ────

import loop_stop_guard as g_burst


def _transcript_sintetico(tmp_path, n_bash, n_run):
    """Um turno: 1 prompt genuíno do usuário + N tool_use Bash."""
    linhas = [json.dumps({"type": "user",
                          "message": {"role": "user", "content": "faça a coisa"}})]
    for i in range(n_bash):
        cmd = "touring run --lang python --file x.py" if i < n_run else f"rg -n 'p{i}' f.rs"
        linhas.append(json.dumps({
            "type": "assistant",
            "message": {"content": [
                {"type": "tool_use", "name": "Bash", "input": {"command": cmd}}]}}))
    tp = tmp_path / "transcript.jsonl"
    tp.write_text("\n".join(linhas))
    return tp


def test_rajada_25_bash_sem_run_bloqueia_uma_vez(tmp_path, monkeypatch):
    monkeypatch.delenv("TOURING_WORK_OUTER_DISABLED", raising=False)
    tp = _transcript_sintetico(tmp_path, 25, 0)
    razao = g_burst.code_mode_burst_block({"transcript_path": str(tp)})
    assert razao and "25 Bash" in razao and "rg" in razao
    # 2º Stop do MESMO turno: passa (sentinela) — nunca loop de block
    assert g_burst.code_mode_burst_block({"transcript_path": str(tp)}) is None


def test_rajada_com_um_touring_run_passa(tmp_path, monkeypatch):
    monkeypatch.delenv("TOURING_WORK_OUTER_DISABLED", raising=False)
    tp = _transcript_sintetico(tmp_path, 25, 1)
    assert g_burst.code_mode_burst_block({"transcript_path": str(tp)}) is None


def test_rajada_abaixo_do_piso_passa(tmp_path, monkeypatch):
    monkeypatch.delenv("TOURING_WORK_OUTER_DISABLED", raising=False)
    tp = _transcript_sintetico(tmp_path, 19, 0)
    assert g_burst.code_mode_burst_block({"transcript_path": str(tp)}) is None


def test_kill_switch_desliga_o_gate_de_rajada(tmp_path, monkeypatch):
    monkeypatch.setenv("TOURING_WORK_OUTER_DISABLED", "1")
    tp = _transcript_sintetico(tmp_path, 25, 0)
    assert g_burst.code_mode_burst_block({"transcript_path": str(tp)}) is None
