#!/usr/bin/env python3
"""Tests for adw.py — F0 acceptance criteria + unit coverage.

Acceptance (plan 2026-07-19, F0):
  A1 hello-factory runs end-to-end with no orchestrating LLM (mock agent).
  A2 kill -9 mid-run → `adw run --resume-run` resumes from the exact node.
  A3 `adw test` passes with mocked agents (edge test).
  A4 Class-D synthetic: agent narrates success, gate exits non-zero → runner FAIL
     + class_d_divergence (Law L3: verdict ≠ narrative).
"""

import json
import os
import re
import tomllib
from pathlib import Path

import pytest

import adw


@pytest.fixture(autouse=True)
def _no_activity(monkeypatch):
    """Silence the best-effort activity mirror during tests."""
    monkeypatch.setattr(adw, "activity_append", lambda *a, **k: None)


@pytest.fixture()
def root(tmp_path: Path) -> Path:
    (tmp_path / ".touring" / "adw").mkdir(parents=True)
    return tmp_path


def write_spec(root: Path, name: str, body: str) -> None:
    (adw.adw_dir(root) / f"{name}.toml").write_text(body, encoding="utf-8")


def write_recording(root: Path, spec_name: str, node: str, result: str,
                    exit_code: int = 0, session_id: str = "sess-1") -> None:
    rec_dir = adw.adw_dir(root) / f"{spec_name}.recordings"
    rec_dir.mkdir(parents=True, exist_ok=True)
    (rec_dir / f"{node}.json").write_text(json.dumps(
        {"result": result, "exit_code": exit_code, "session_id": session_id}),
        encoding="utf-8")


# ── spec + lint ───────────────────────────────────────────────────────────────


def test_load_spec_and_lint_ok(root):
    write_spec(root, "ok", """
[adw]
name = "ok"
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "__end__"
""")
    spec = adw.load_spec(root, "ok")
    errors, warnings = adw.lint_spec(spec)
    assert errors == []
    assert warnings == []


def test_lint_unknown_edge_and_bad_loop(root):
    write_spec(root, "bad", """
[adw]
name = "bad"
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "ghost"
[node.l]
type = "loop"
body = "nope"
""")
    errors, warnings = adw.lint_spec(adw.load_spec(root, "bad"))
    assert any("unknown node `ghost`" in e for e in errors)
    assert any("body → unknown node `nope`" in e for e in errors)
    assert any("max_iters" in e for e in errors)
    assert any("unreachable" in w for w in warnings)  # l is an orphan


def test_lint_cycle_without_exit(root):
    write_spec(root, "cyc", """
[adw]
name = "cyc"
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "b"
on_fail = "b"
[node.b]
type = "code"
command = ["true"]
idempotent = true
on_pass = "a"
on_fail = "a"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "cyc"))
    assert any("cycle without exit" in e for e in errors)


def test_lint_budget_verify(root):
    write_spec(root, "bud", """
[adw]
name = "bud"
entry = "a"
budget_tokens = 100
[node.a]
type = "agent"
driver = "mock"
budget_tokens = 60
on_pass = "b"
[node.b]
type = "agent"
driver = "mock"
budget_tokens = 60
on_pass = "__end__"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "bud"))
    assert any("budget-verify" in e for e in errors)


def test_lint_missing_command_is_error(root):
    write_spec(root, "nc", """
[adw]
name = "nc"
entry = "a"
[node.a]
type = "gate"
on_pass = "__end__"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "nc"))
    assert any("missing command" in e for e in errors)


# ── results store + templates (Law L4) ────────────────────────────────────────


def test_store_result_summary_inline_first(tmp_path):
    small = adw.store_result(tmp_path, "n#0", "short output")
    assert small == {"summary": "short output", "omitted_bytes": 0, "full_ref": ""}
    big = adw.store_result(tmp_path, "n#1", "x" * (adw.SUMMARY_LIMIT + 500))
    assert big["omitted_bytes"] == 500
    assert Path(big["full_ref"]).is_file()
    assert len(big["summary"].encode()) == adw.SUMMARY_LIMIT


def test_render_template_nodes_and_vars():
    results = {"scout": {"summary": "FOUND 3 issues", "omitted_bytes": 0, "full_ref": ""}}
    text = adw.render_template(
        "Fix: {{nodes.scout.summary}} in {{vars.target}}", results, {"target": "src/x.rs"})
    assert text == "Fix: FOUND 3 issues in src/x.rs"


# ── A1: hello-factory end-to-end (mock agent, feedback loop, no LLM) ──────────

HELLO_FACTORY = """
[adw]
name = "hello-factory"
description = "build agent → gate → fail feeds back → pass → report"
entry = "build"
[node.build]
type = "agent"
driver = "mock"
prompt = "build it"
on_pass = "clippy_gate"
on_fail = "__fail__"
[node.clippy_gate]
type = "gate"
command = ["bash", "-c", "test -f {sentinel} || {{ touch {sentinel}; exit 1; }}"]
idempotent = true
on_pass = "report"
on_fail = "build"
[node.report]
type = "code"
command = ["bash", "-c", "echo report: gate summary was: {{{{nodes.clippy_gate.summary}}}}"]
idempotent = true
on_pass = "__end__"
"""


def test_hello_factory_e2e_with_feedback_loop(root):
    sentinel = root / "gate.ok"
    write_spec(root, "hello-factory", HELLO_FACTORY.format(sentinel=sentinel))
    write_recording(root, "hello-factory", "build", "build finished — success ✅")
    spec = adw.load_spec(root, "hello-factory")
    outcome = adw.execute(spec, root)
    assert outcome.status == "completed"
    nodes_run = [s["node"] for s in outcome.steps]
    # gate failed once → back to build → gate passes → report
    assert nodes_run == ["build", "clippy_gate", "build", "clippy_gate", "report"]
    run_path = adw.runs_dir(root) / outcome.run_id
    events = [e["event"] for e in adw.Journal(run_path).events]
    assert events[0] == "run_started" and events[-1] == "run_finished"
    # results store artifacts exist for every executed node
    assert (run_path / "build#0.json").is_file()
    assert (run_path / "clippy_gate#1.json").is_file()


# ── A4: Class-D synthetic (Law L3) ────────────────────────────────────────────


def test_class_d_divergence_detected(root):
    write_spec(root, "classd", """
[adw]
name = "classd"
entry = "agentnode"
[node.agentnode]
type = "agent"
driver = "mock"
on_pass = "gate"
[node.gate]
type = "gate"
command = ["bash", "-c", "exit 1"]
idempotent = true
on_pass = "__end__"
on_fail = "__fail__"
""")
    write_recording(root, "classd", "agentnode", "All done! Tests pass — SUCCESS")
    outcome = adw.execute(adw.load_spec(root, "classd"), root)
    assert outcome.status == "failed"
    assert outcome.class_d is True
    run_path = adw.runs_dir(root) / outcome.run_id
    assert any(e["event"] == "class_d_divergence" for e in adw.Journal(run_path).events)


# ── A2: kill -9 mid-run → resume from the exact node ──────────────────────────


def test_resume_after_kill_replays_completed_and_reruns_interrupted(root):
    write_spec(root, "killed", """
[adw]
name = "killed"
entry = "one"
[node.one]
type = "code"
command = ["bash", "-c", "echo one"]
idempotent = true
on_pass = "two"
[node.two]
type = "code"
command = ["bash", "-c", "echo two"]
idempotent = true
on_pass = "__end__"
""")
    spec = adw.load_spec(root, "killed")
    # Simulate a run killed -9 mid-node-two: journal has node one completed and
    # node two started but never completed (fsync'd journal survives the kill).
    run_id = "killed-424242"
    run_path = adw.runs_dir(root) / run_id
    run_path.mkdir(parents=True)
    journal = adw.Journal(run_path)
    journal.append("run_started", adw="killed", run_id=run_id)
    journal.append("node_started", node="one", exec_key="one#0", type="code")
    adw.store_result(run_path, "one#0", "one\n")
    journal.append("node_completed", node="one", exec_key="one#0", exit_code=0,
                   verdict="pass", next="two", session_id=None, class_d=False)
    journal.append("node_started", node="two", exec_key="two#0", type="code")
    # kill -9 here — no node_completed for two#0

    outcome = adw.execute(spec, root, resume_run=run_id)
    assert outcome.status == "completed"
    assert outcome.steps[0] == {"node": "one", "exec_key": "one#0", "replayed": True}
    rerun = [s for s in outcome.steps if s["node"] == "two"]
    assert rerun and rerun[0].get("replayed") is None  # two was re-executed live
    assert rerun[0]["verdict"] == "pass"


# ── waiting_human: durable pause + approve on resume ──────────────────────────


def test_human_node_pauses_and_approves_on_resume(root, capsys):
    write_spec(root, "hum", """
[adw]
name = "hum"
entry = "gatekeeper"
[node.gatekeeper]
type = "human"
message = "approve deploy?"
on_pass = "deploy"
[node.deploy]
type = "code"
command = ["bash", "-c", "echo deployed"]
idempotent = true
on_pass = "__end__"
""")
    spec = adw.load_spec(root, "hum")
    paused = adw.execute(spec, root)
    assert paused.status == "waiting_human"
    resumed = adw.execute(spec, root, resume_run=paused.run_id, approve={"gatekeeper"})
    assert resumed.status == "completed"
    assert [s["node"] for s in resumed.steps] == ["gatekeeper", "deploy"]


# ── loop node: runner owns termination (Law L2) ───────────────────────────────


def test_loop_node_dry_convergence(root):
    counter = root / "count.txt"
    counter.write_text("3 2 0 0 5")  # findings per round; dry_rounds=2 stops at 4th
    write_spec(root, "lp", """
[adw]
name = "lp"
entry = "explore_loop"
[node.explore_loop]
type = "loop"
body = "probe"
max_iters = 10
dry_rounds = 2
on_dry = "after"
[node.probe]
type = "code"
command = ["bash", "-c", "n=$(cut -d' ' -f1 %s); sed -i 's/^[0-9]* //' %s; echo NEW_FINDINGS=$n"]
idempotent = true
[node.after]
type = "code"
command = ["bash", "-c", "echo converged"]
idempotent = true
on_pass = "__end__"
""" % (counter, counter))
    outcome = adw.execute(adw.load_spec(root, "lp"), root)
    assert outcome.status == "completed"
    probes = [s for s in outcome.steps if s["node"] == "probe"]
    assert len(probes) == 4  # 3,2 findings; then two dry rounds → on_dry (5 never read)
    assert outcome.steps[-1]["node"] == "after"


def test_loop_absent_marker_never_counts_as_dry(root):
    """FAIL-CLOSED: a silent body exhausts max_iters instead of faking dryness.

    Regression for 2026-08-02: reading an absent `NEW_FINDINGS=` as zero made
    silence indistinguishable from a dry round. `touring explore` emitted no
    marker (and the specs piped it through `tail -c 1800`), so strategy-loop
    stopped after exactly dry_rounds=2 iterations reporting convergence, while
    its own CCE ledger recorded 30 and 4 new findings in those two rounds.
    """
    write_spec(root, "silent", """
[adw]
name = "silent"
entry = "explore_loop"
[node.explore_loop]
type = "loop"
body = "probe"
max_iters = 3
dry_rounds = 2
on_dry = "faked"
on_pass = "honest"
[node.probe]
type = "code"
command = ["bash", "-c", "echo 'plenty found, but the marker is missing'"]
idempotent = true
[node.faked]
type = "code"
command = ["bash", "-c", "echo dry-claimed-without-evidence"]
idempotent = true
on_pass = "__end__"
[node.honest]
type = "code"
command = ["bash", "-c", "echo budget-exhausted"]
idempotent = true
on_pass = "__end__"
""")
    outcome = adw.execute(adw.load_spec(root, "silent"), root)
    probes = [s for s in outcome.steps if s["node"] == "probe"]
    assert len(probes) == 3               # ran the full budget, never short-circuited
    assert outcome.steps[-1]["node"] == "honest"
    rounds = [s for s in outcome.steps if "dry_signal" in s]
    assert rounds and all(r["dry_signal"] == "absent" for r in rounds)
    assert all(r["new_findings"] is None for r in rounds)  # unknown, not zero


# ── A3: `adw test` (mocked agents, edge test) + CLI surface ───────────────────


def test_cmd_test_runs_mocked(root, capsys):
    write_spec(root, "et", """
[adw]
name = "et"
entry = "agentnode"
[node.agentnode]
type = "agent"
driver = "claude"
prompt = "would call claude for real"
on_pass = "check"
[node.check]
type = "code"
command = ["bash", "-c", "echo ok"]
idempotent = true
on_pass = "__end__"
""")
    write_recording(root, "et", "agentnode", "mocked agent output")
    rc = adw.cmd_test(root, "et")
    out = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert out["status"] == "completed" and out["mocked"] is True


def test_from_template_generates_valid_spec(root, capsys):
    rc = adw.cmd_from_template(root, "neww")
    assert rc == 0
    spec = adw.load_spec(root, "neww")
    errors, _ = adw.lint_spec(spec)
    assert errors == []
    assert {n.type for n in spec.nodes.values()} == {"agent", "gate"}


# ── F3: central library (templates + tiers.toml) ──────────────────────────────


def test_from_template_prefers_library(root, tmp_path, monkeypatch, capsys):
    lib = tmp_path / "lib"
    lib.mkdir()
    (lib / "mytpl.toml").write_text("""
[adw]
name = "mytpl"
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "__end__"
""", encoding="utf-8")
    monkeypatch.setenv("TOURING_ADW_LIBRARY", str(lib))
    rc = adw.cmd_from_template(root, "mytpl")
    out = json.loads(capsys.readouterr().out)
    assert rc == 0 and out["source"] == "library"
    errors, _ = adw.lint_spec(adw.load_spec(root, "mytpl"))
    assert errors == []


def test_tier_models_library_override(tmp_path, monkeypatch):
    lib = tmp_path / "lib"
    lib.mkdir()
    (lib / "tiers.toml").write_text('[tiers]\nsota = "my-custom-model"\n', encoding="utf-8")
    monkeypatch.setenv("TOURING_ADW_LIBRARY", str(lib))
    mapping = adw.tier_models()
    assert mapping["sota"] == "my-custom-model"
    assert mapping["light"] == "haiku"  # built-in fallback survives


REAL_LIBRARY = Path.home() / ".claude" / "skills" / "Touring" / "adw-library"
LIBRARY_SPECS = sorted(p.stem for p in REAL_LIBRARY.glob("*.toml") if p.stem != "tiers")


@pytest.mark.skipif(not LIBRARY_SPECS, reason="no real adw-library present")
@pytest.mark.parametrize("template", LIBRARY_SPECS)
def test_real_library_spec_lints_clean(root, monkeypatch, template, capsys):
    """F3 acceptance: every shipped library spec instantiates and lints with 0 errors."""
    monkeypatch.setenv("TOURING_ADW_LIBRARY", str(REAL_LIBRARY))
    assert adw.cmd_from_template(root, template) == 0
    capsys.readouterr()
    spec = adw.load_spec(root, template)
    errors, _ = adw.lint_spec(spec)
    assert errors == [], f"library spec `{template}` has lint errors: {errors}"


def test_real_library_has_the_six_f3_adws():
    expected = {"explore-plan", "bugfix", "feature", "chore", "hotfix", "audit"}
    assert expected <= set(LIBRARY_SPECS), f"missing: {expected - set(LIBRARY_SPECS)}"


# ── Law L2 retry shutoff: a broken gate must not re-bill agents forever ───────


def test_feedback_loop_hits_retry_limit(root):
    """Regression for the 17-invocation chore incident: gate always fails →
    the run fails loud after max_retries feedback re-entries."""
    write_spec(root, "loopy", """
[adw]
name = "loopy"
entry = "agentnode"
[node.agentnode]
type = "agent"
driver = "mock"
on_pass = "gate"
[node.gate]
type = "gate"
command = ["bash", "-c", "exit 1"]
idempotent = true
max_retries = 2
on_pass = "__end__"
on_fail = "agentnode"
""")
    write_recording(root, "loopy", "agentnode", "done, success!")
    outcome = adw.execute(adw.load_spec(root, "loopy"), root)
    assert outcome.status == "failed"
    agent_runs = [s for s in outcome.steps if s["node"] == "agentnode"]
    gate_runs = [s for s in outcome.steps if s["node"] == "gate"]
    assert len(gate_runs) == 3  # initial + 2 retries, then shutoff
    assert len(agent_runs) == 3  # never a 4th billing
    run_path = adw.runs_dir(root) / outcome.run_id
    assert any(e["event"] == "retry_limit_exceeded" for e in adw.Journal(run_path).events)


# ── F5a: ZTE conformal bypass of the human gate (fail-closed) ─────────────────

ZTE_SPEC = """
[adw]
name = "zted"
entry = "work"
[node.work]
type = "code"
command = ["bash", "-c", "echo worked"]
idempotent = true
on_pass = "gatekeeper"
[node.gatekeeper]
type = "human"
zte = true
zte_warmup = 2
message = "approve?"
on_pass = "__end__"
"""


def _complete_run(root, name, n):
    """Seed n prior completed runs of spec `name` for warm-up evidence."""
    for i in range(n):
        run_path = adw.runs_dir(root) / f"{name}-seed{i}"
        run_path.mkdir(parents=True, exist_ok=True)
        journal = adw.Journal(run_path)
        journal.append("run_started", adw=name, run_id=run_path.name)
        journal.append("run_finished", status="completed", class_d=False)


def test_zte_bypass_granted_with_history_and_conformal_in(root, monkeypatch):
    write_spec(root, "zted", ZTE_SPEC)
    _complete_run(root, "zted", 2)
    monkeypatch.setattr(adw, "conformal_in", lambda c: True)
    outcome = adw.execute(adw.load_spec(root, "zted"), root)
    assert outcome.status == "completed"
    run_path = adw.runs_dir(root) / outcome.run_id
    bypass = [e for e in adw.Journal(run_path).events if e["event"] == "zte_bypass"]
    assert bypass and bypass[0]["conformal"] == "IN" and bypass[0]["prior_completed_runs"] == 2


def test_zte_falls_closed_without_warmup(root, monkeypatch):
    write_spec(root, "zted", ZTE_SPEC)
    _complete_run(root, "zted", 1)  # below zte_warmup = 2
    monkeypatch.setattr(adw, "conformal_in", lambda c: True)
    outcome = adw.execute(adw.load_spec(root, "zted"), root)
    assert outcome.status == "waiting_human"


def test_zte_falls_closed_on_conformal_out(root, monkeypatch):
    write_spec(root, "zted", ZTE_SPEC)
    _complete_run(root, "zted", 5)
    monkeypatch.setattr(adw, "conformal_in", lambda c: False)
    outcome = adw.execute(adw.load_spec(root, "zted"), root)
    assert outcome.status == "waiting_human"


def test_run_confidence_degrades_with_failures_and_class_d(tmp_path):
    ctx = adw._RunCtx(spec=None, journal=None, run_path=tmp_path, results={},
                      variables={}, approve=set(), force_mock=True, record=False,
                      run_id="r", outcome=adw.RunOutcome(status="failed", run_id="r"),
                      visits={}, completed={})
    assert adw.run_confidence(ctx) == 1.0
    ctx.fail_counts = {"gate": 2, "agent": 1}
    assert adw.run_confidence(ctx) == pytest.approx(0.7)
    ctx.outcome.class_d = True
    assert adw.run_confidence(ctx) == 0.0  # narrative divergence disqualifies


def test_human_gate_without_zte_never_bypasses(root, monkeypatch):
    write_spec(root, "plain", """
[adw]
name = "plain"
entry = "gatekeeper"
[node.gatekeeper]
type = "human"
message = "approve?"
on_pass = "__end__"
""")
    monkeypatch.setattr(adw, "conformal_in", lambda c: True)
    _complete_run(root, "plain", 10)
    outcome = adw.execute(adw.load_spec(root, "plain"), root)
    assert outcome.status == "waiting_human"  # zte is strictly opt-in


# ── F5b: racing — first-to-pass wins, losers canceled, winner-only merge ──────


def test_race_first_to_pass_wins_and_merges(root, capsys):
    (root / "base.txt").write_text("base", encoding="utf-8")
    write_spec(root, "racer", """
[adw]
name = "racer"
entry = "sprint"
[node.sprint]
type = "code"
command = ["bash", "-c", "sleep 0.{{vars.lane}}5; if [ '{{vars.lane}}' = '1' ]; then sleep 3; fi; echo win-{{vars.lane}} > result.txt; echo done"]
timeout_ms = 30000
idempotent = true
on_pass = "__end__"
""")
    rc = adw.cmd_race(root, "racer", 2, {})
    report = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert report["winner"] == 0  # lane 0 finishes first; lane 1 sleeps 3s more
    assert "result.txt" in report["merged_files"]
    assert (root / "result.txt").read_text().strip() == "win-0"
    canceled = [entry for entry in report["lanes"] if entry["exit"] == "canceled"]
    assert canceled and canceled[0]["lane"] == 1  # the loser was canceled, not awaited


def test_race_all_fail_returns_nonzero(root, capsys):
    write_spec(root, "failer", """
[adw]
name = "failer"
entry = "boom"
[node.boom]
type = "code"
command = ["bash", "-c", "exit 1"]
idempotent = true
max_retries = 0
on_pass = "__end__"
on_fail = "__fail__"
""")
    rc = adw.cmd_race(root, "failer", 2, {})
    report = json.loads(capsys.readouterr().out)
    assert rc == 1 and report["winner"] is None and report["merged_files"] == []


def test_claude_cmd_carries_permission_and_add_dirs(tmp_path):
    node = adw.Node(name="a", type="agent", raw={
        "permission_mode": "acceptEdits",
        "add_dirs": ["/x/rules", "/y/docs"],
        "allowed_tools": ["Edit"],
    })
    journal = adw.Journal(tmp_path)
    cmd = adw._claude_cmd(node, journal, "do it")
    assert ["--permission-mode", "acceptEdits"] == cmd[cmd.index("--permission-mode"):cmd.index("--permission-mode") + 2]
    assert cmd.count("--add-dir") == 2 and "/x/rules" in cmd and "/y/docs" in cmd


def test_cli_lint_and_list(root, capsys):
    write_spec(root, "ok", """
[adw]
name = "ok"
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "__end__"
""")
    assert adw.main(["--root", str(root), "lint", "ok"]) == 0
    lint_out = json.loads(capsys.readouterr().out)
    assert lint_out["valid"] is True
    assert adw.main(["--root", str(root), "list"]) == 0
    assert "ok" in json.loads(capsys.readouterr().out)["specs"]
    assert adw.main(["--root", str(root), "lint", "missing"]) == 1


def test_run_rejects_invalid_spec(root, capsys):
    write_spec(root, "inv", """
[adw]
name = "inv"
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "ghost"
""")
    rc = adw.main(["--root", str(root), "run", "inv"])
    assert rc == 1
    assert json.loads(capsys.readouterr().out)["error"] == "lint failed"


# ── multi-session: run dirs must never collide (2026-08-02) ──────────────────

def test_run_ids_never_collide_across_concurrent_sessions(monkeypatch):
    """Two Claude Code sessions starting one ADW in the same second, same project.

    The old `f"{name}-{int(time.time())}"` gave both the SAME run dir: the loser
    died on the flock, and a later acquisition would have replayed the other
    session's journal as its own resume state. Latent while ADWs were invoked by
    hand; live now that the deterministic OUTER is the default.
    """
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-AAAAAAAA")
    first = adw.new_run_id("strategy-loop")
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "sess-BBBBBBBB")
    second = adw.new_run_id("strategy-loop")
    assert first != second
    assert first.startswith("strategy-loop-") and second.startswith("strategy-loop-")
    # the pid tail keeps two runs of ONE session in the same second distinct too
    assert first.endswith(f".{os.getpid()}")
    monkeypatch.delenv("CLAUDE_CODE_SESSION_ID", raising=False)
    monkeypatch.delenv("TOURING_SESSION_ID", raising=False)
    assert "local" in adw.new_run_id("strategy-loop")  # no identity → still unique



# ── the two topologies: control vs data (T1.0-T1.2) ──────────────────────────


def test_the_data_graph_is_read_from_every_field_the_runner_interpolates(root):
    """`flow_dataflow` must see exactly what `render_template` resolves. If it
    reads fewer fields, the lint reasons about a graph the runner never runs."""
    write_spec(root, "io", """
[adw]
name = "io"
entry = "a"
[node.a]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "start"
on_pass = "b"
[node.b]
type = "code"
command = ["echo", "{{nodes.a.summary}}"]
on_pass = "c"
[node.c]
type = "agent"
driver = "mock"
prompt = "plain"
on_pass = "__end__"
[node.c.persona]
role = "r"
lens = "{{nodes.b.summary}}"
""")
    flow = adw.flow_dataflow(adw.load_spec(root, "io"))
    assert flow["b"].reads == {"a"}, "a `command` part carries references"
    assert flow["c"].reads == {"b"}, "a persona field carries references"


def test_readonly_is_tri_state_and_an_omission_is_not_a_promise(root):
    write_spec(root, "ro", """
[adw]
name = "ro"
entry = "a"
[node.a]
type = "agent"
driver = "mock"
allowed_tools = ["Read", "Grep"]
prompt = "x"
on_pass = "b"
[node.b]
type = "agent"
driver = "mock"
allowed_tools = ["Read", "Bash"]
prompt = "y {{nodes.a.summary}}"
on_pass = "c"
[node.c]
type = "agent"
driver = "mock"
prompt = "z {{nodes.b.summary}}"
on_pass = "__end__"
""")
    flow = adw.flow_dataflow(adw.load_spec(root, "ro"))
    assert flow["a"].readonly is True
    assert flow["b"].readonly is False, "Bash is a general-purpose writer"
    assert flow["c"].readonly is None, "no allowed_tools means unknown, never safe"


def test_fake_waiting_is_reported_when_both_nodes_are_provably_read_only(root):
    write_spec(root, "fw", """
[adw]
name = "fw"
entry = "scout"
[node.scout]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "look at A"
on_pass = "other"
[node.other]
type = "agent"
driver = "mock"
allowed_tools = ["Grep"]
prompt = "look at B, independently"
on_pass = "__end__"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "fw"))
    assert any("fake waiting" in w and "`other`" in w for w in warnings), warnings


def test_fake_waiting_stays_silent_when_a_node_may_write(root):
    """Coupling through a file is invisible to the interpolator. Telling someone
    to parallelise that pair would break their flow — so without proof, silence."""
    write_spec(root, "fw2", """
[adw]
name = "fw2"
entry = "produce"
[node.produce]
type = "agent"
driver = "mock"
allowed_tools = ["Read", "Write"]
prompt = "write the file"
on_pass = "consume"
[node.consume]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "read the file that was just written"
on_pass = "__end__"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "fw2"))
    assert not any("fake waiting" in w for w in warnings), warnings


def test_fake_waiting_never_accuses_a_gate(root):
    """A gate re-reads the preceding agent through `ctx.last_agent` (the Class-D
    check), so its dependency is real with no `{{nodes.}}` reference at all."""
    write_spec(root, "fw3", """
[adw]
name = "fw3"
entry = "scout"
[node.scout]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "look"
on_pass = "check"
[node.check]
type = "gate"
readonly = true
command = ["true"]
on_pass = "__end__"
on_fail = "__fail__"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "fw3"))
    assert not any("fake waiting" in w for w in warnings), warnings


def test_dead_node_is_reported_only_when_the_summary_is_the_whole_product(root):
    write_spec(root, "dn", """
[adw]
name = "dn"
entry = "scout"
[node.scout]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "produce a report nobody reads"
on_pass = "work"
[node.work]
type = "agent"
driver = "mock"
allowed_tools = ["Read", "Edit"]
prompt = "do the work"
on_pass = "__end__"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "dn"))
    assert any("nothing downstream reads" in w and "`scout`" in w for w in warnings), warnings
    assert not any("`work`" in w and "nothing downstream reads" in w for w in warnings), \
        "a node that may write delivers through the filesystem, not through its summary"


def test_a_parallel_branch_is_never_dead_weight(root):
    """Its product goes to the block's merge. The shipped critic-panel escaped
    this only because its critics' on_pass happened to be terminal."""
    write_spec(root, "pb", """
[adw]
name = "pb"
entry = "fan"
[node.fan]
type = "parallel"
branches = ["l1", "l2"]
merge = "concat"
on_branch_fail = "all"
max_branches = 4
on_pass = "after"
on_fail = "__fail__"
[node.l1]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "lens one"
on_pass = "__end__"
[node.l2]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "lens two"
on_pass = "__end__"
[node.after]
type = "agent"
driver = "mock"
allowed_tools = ["Read", "Write"]
prompt = "merge {{nodes.fan.summary}}"
on_pass = "__end__"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "pb"))
    assert not any("nothing downstream reads" in w for w in warnings), warnings


def test_control_successors_is_the_only_definition_of_an_edge(root):
    """Reachability and the new lints must agree about what an edge is. They did
    not before: the walk was written twice."""
    import inspect
    body = inspect.getsource(adw._lint_reachability)
    assert "control_successors" in body, \
        "reachability must use the shared helper, not its own copy of the walk"


# ── cost and the gauntlet's own caveat (T1.3, T2.1) ──────────────────────────


def test_cost_counts_the_declared_ceiling_not_a_guess(root):
    write_spec(root, "cost", """
[adw]
name = "cost"
entry = "sweep"
[node.sweep]
type = "parallel"
branches = "{{vars.lenses}}"
template = "lens"
merge = "concat"
on_branch_fail = "all"
max_branches = 6
on_pass = "__end__"
on_fail = "__fail__"
[node.lens]
type = "agent"
driver = "mock"
tier = "workhorse"
timeout_ms = 60000
allowed_tools = ["Read"]
prompt = "one lens"
""")
    cost = adw.flow_cost(adw.load_spec(root, "cost"))
    assert cost["max_agent_calls"] == 6, "a dynamic template costs its declared max_branches"
    assert cost["widest_fanout"] == {"node": "sweep", "branches": 6}
    assert cost["calls_by_tier"] == {"workhorse": 6}
    assert cost["serial_timeout_budget_s"] == 360.0


def test_a_panel_wired_to_the_entry_is_optimising_towards_nothing(root):
    """Jay E's caveat, made structural: the gauntlet is a polishing instrument."""
    write_spec(root, "bare", """
[adw]
name = "bare"
entry = "panel"
[node.panel]
type = "parallel"
branches = ["c1", "c2"]
merge = "tally"
on_branch_fail = "ignore"
max_branches = 2
on_pass = "__end__"
on_fail = "__fail__"
[node.c1]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "judge {{vars.thing}}"
on_pass = "__end__"
[node.c1.persona]
role = "critic"
emits = "VERDICT=PASS|REJECT"
[node.c2]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "judge {{vars.thing}}"
on_pass = "__end__"
[node.c2.persona]
role = "critic"
emits = "VERDICT=PASS|REJECT"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "bare"))
    assert sum("critique without brief" in w for w in warnings) == 2, warnings


def test_a_panel_downstream_of_a_producer_is_grounded(root):
    """A control construct must never count as the producer — a `parallel` block
    between the entry and its critics silenced this lint on the exact shape it
    exists to catch."""
    write_spec(root, "made", """
[adw]
name = "made"
entry = "make"
[node.make]
type = "agent"
driver = "mock"
allowed_tools = ["Read", "Write"]
prompt = "make the thing"
on_pass = "panel"
on_fail = "__fail__"
[node.panel]
type = "parallel"
branches = ["c1"]
merge = "tally"
on_branch_fail = "ignore"
max_branches = 1
on_pass = "__end__"
on_fail = "__fail__"
[node.c1]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "judge it"
on_pass = "__end__"
[node.c1.persona]
role = "critic"
emits = "VERDICT=PASS|REJECT"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "made"))
    assert not any("critique without brief" in w for w in warnings), warnings


def test_a_human_gate_grounds_a_critic_too(root):
    """Approval is a brief: someone chose the direction, even if this flow did
    not build the artifact."""
    write_spec(root, "appr", """
[adw]
name = "appr"
entry = "sign"
[node.sign]
type = "human"
prompt = "approve the brief?"
on_pass = "c1"
on_fail = "__fail__"
[node.c1]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
prompt = "judge {{vars.thing}}"
on_pass = "__end__"
[node.c1.persona]
role = "critic"
emits = "VERDICT=PASS|REJECT"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "appr"))
    assert not any("critique without brief" in w for w in warnings), warnings


# ── the two new kit pieces (T2.2, T4.1) ──────────────────────────────────────


def _library_root(tmp_path):
    """A project root carrying the whole shipped library, so `[[use]]` resolves."""
    import shutil
    lib = adw.library_dir()
    dest = adw.adw_dir(tmp_path)
    dest.mkdir(parents=True, exist_ok=True)
    for f in lib.glob("*.toml"):
        shutil.copy2(f, dest / f.name)
    if (lib / "fragments").is_dir():
        shutil.copytree(lib / "fragments", dest / "fragments", dirs_exist_ok=True)
    return tmp_path


def test_worker_critic_pair_composes_and_the_critic_never_reads_the_worker(tmp_path):
    """Pairing is the third fan-out shape: N workers each with their OWN critic.
    The critic must judge the artifact, not the worker's account of it."""
    if not (adw.library_dir() / "fragments" / "worker-critic-pair.toml").is_file():
        pytest.skip("worker-critic-pair not installed")
    root = _library_root(tmp_path)
    write_spec(root, "pair", """
[adw]
name = "pair"
entry = "brief"
[node.brief]
type = "human"
prompt = "approve the brief?"
on_pass = "p1"
on_fail = "__fail__"
[[use]]
module = "worker-critic-pair"
as = "p1"
with = { task = "build the parser", artifact = "src/parser.rs", bar = "the named test passes" }
on_pass = "__end__"
on_fail = "__fail__"
""")
    spec = adw.load_spec(root, "pair")
    assert adw.lint_spec(spec) == ([], []), adw.lint_spec(spec)
    assert {"p1.work", "p1.critic", "p1.judge"} <= set(spec.nodes)
    critic = spec.node("p1.critic")
    assert "nodes.p1.work" not in str(critic.raw["prompt"]), \
        "a critic that reads the worker's summary grades the story, not the artifact"
    assert critic.raw["session"] == "fresh"
    assert adw.node_readonly(critic) is True, "a critic must not be able to fix what it judges"
    assert "nodes.p1.judge" in str(spec.node("p1.work").raw["prompt"]), \
        "the retry must carry the verdict that rejected it"


def test_worker_critic_pair_composes_n_times_under_n_aliases(tmp_path):
    """The fleet is composition, not a bigger fragment: one pair is the unit."""
    if not (adw.library_dir() / "fragments" / "worker-critic-pair.toml").is_file():
        pytest.skip("worker-critic-pair not installed")
    root = _library_root(tmp_path)
    write_spec(root, "fleet", """
[adw]
name = "fleet"
entry = "brief"
[node.brief]
type = "human"
prompt = "approve?"
on_pass = "a"
on_fail = "__fail__"
[[use]]
module = "worker-critic-pair"
as = "a"
with = { task = "front", artifact = "a.rs", bar = "tests pass" }
on_pass = "b"
on_fail = "__fail__"
[[use]]
module = "worker-critic-pair"
as = "b"
with = { task = "back", artifact = "b.rs", bar = "tests pass" }
on_pass = "__end__"
on_fail = "__fail__"
""")
    spec = adw.load_spec(root, "fleet")
    assert adw.lint_spec(spec) == ([], []), adw.lint_spec(spec)
    assert {"a.work", "a.critic", "a.judge", "b.work", "b.critic", "b.judge"} <= set(spec.nodes)
    assert spec.node("a.judge").on_fail == "a.work", "each pair loops to its OWN worker"
    assert spec.node("b.judge").on_fail == "b.work"


def test_graph_pack_asks_for_relations_not_text(tmp_path):
    """`recall-pack` answers 'has anyone thought about this'; `graph-pack` answers
    'what breaks if I touch it'. Different question, different commands."""
    if not (adw.library_dir() / "fragments" / "graph-pack.toml").is_file():
        pytest.skip("graph-pack not installed")
    root = _library_root(tmp_path)
    write_spec(root, "gp", """
[adw]
name = "gp"
entry = "g"
[[use]]
module = "graph-pack"
as = "g"
with = { symbol = "{{vars.symbol}}", domain = "{{vars.domain}}" }
on_pass = "plan"
on_fail = "plan"
[node.plan]
type = "agent"
driver = "mock"
allowed_tools = ["Read", "Edit"]
prompt = "Plan it. Structure: {{nodes.g.relations.summary}}"
on_pass = "__end__"
on_fail = "__fail__"
""")
    spec = adw.load_spec(root, "gp")
    assert adw.lint_spec(spec) == ([], []), adw.lint_spec(spec)
    command = " ".join(str(x) for x in spec.node("g.relations").raw["command"])
    for expected in ("wiring impact", "index find", "memory moc"):
        assert expected in command, f"graph-pack must consult {expected}"


def test_every_fragment_command_passes_values_positionally(tmp_path):
    """Structural guard over the WHOLE kit, not one file: a value interpolated
    into a `bash -c` string is executed by that shell. The library shipped that
    bug for ten days once."""
    import tomllib
    lib = adw.library_dir() / "fragments"
    if not lib.is_dir():
        pytest.skip("fragment library not installed")
    offenders = []
    for path in sorted(lib.glob("*.toml")):
        data = tomllib.loads(path.read_text(encoding="utf-8"))
        for name, body in (data.get("node") or {}).items():
            command = body.get("command")
            if not isinstance(command, list) or len(command) < 3:
                continue
            if command[0] != "bash" or command[1] != "-c":
                continue
            if "{{" in str(command[2]):
                offenders.append(f"{path.name}:{name}")
    assert offenders == [], f"values interpolated into a shell string: {offenders}"


# ── promotion: the library carries evidence, not intent (T5.1) ───────────────


def test_every_shipped_flow_declares_its_evidence_or_the_lack_of_it():
    """A flow may not join the library on the strength of having been written.

    Either a real run is on record, or the ledger says in writing why there is
    none. The point is not to force runs — it is that "we never ran this" must be
    a sentence someone wrote, not a silence. When this guard was first run, six of
    the eight shipped flows had never completed a single real run.
    """
    lib = adw.library_dir()
    if not lib.is_dir():
        pytest.skip("library not installed")
    specs = {p.stem for p in lib.glob("*.toml")} - {"tiers"}
    ledger = adw.load_promotions()
    missing = sorted(specs - set(ledger))
    assert not missing, (
        f"shipped with no promotion record and no declared exemption: {missing}. "
        f"Record one with: touring adw promote <name> --run <run_id>  "
        f"(or --exempt '<why there is none>')")


def test_a_promotion_record_never_passes_a_mocked_walk_off_as_behaviour():
    """`adw test` proves the graph connects and nothing more. A record built from
    a synthesized walk must say so in the same field a reader looks at."""
    lib = adw.library_dir()
    if not lib.is_dir():
        pytest.skip("library not installed")
    for name, rec in adw.load_promotions().items():
        if "exempt" in rec:
            continue
        synthesized = rec.get("synthesized") or []
        evidence = rec.get("evidence", "")
        if synthesized:
            assert "wiring only" in evidence, (
                f"{name}: {len(synthesized)} node(s) were synthesized, so this is not "
                f"evidence of behaviour — evidence reads {evidence!r}")
        else:
            assert "behaviour" in evidence, f"{name}: evidence reads {evidence!r}"


def test_an_outcome_is_read_from_the_journal_not_reconstructed(tmp_path):
    """The journal is the source of truth for resume, so it is the source of truth
    for promotion too — anything else could disagree with what the runner did."""
    run_id = "demo-1700000000"
    run_dir = adw.runs_dir(tmp_path) / run_id
    run_dir.mkdir(parents=True)
    (run_dir / "journal.jsonl").write_text(
        '{"seq":0,"ts":1.0,"event":"run_started"}\n'
        '{"seq":1,"ts":2.0,"event":"run_finished","status":"completed",'
        '"class_d":false,"synthesized":["scout"]}\n',
        encoding="utf-8")
    out = adw.read_run_outcome(tmp_path, run_id)
    assert out["status"] == "completed"
    assert out["synthesized"] == ["scout"]
    assert out["finished_at"] == 2.0
    assert adw.read_run_outcome(tmp_path, "no-such-run") is None


def test_a_run_that_never_finished_is_not_promotable(tmp_path):
    """A journal with no `run_finished` describes work that stopped, not work that
    passed. Treating it as an outcome would promote on an interruption."""
    run_id = "halted-1700000000"
    run_dir = adw.runs_dir(tmp_path) / run_id
    run_dir.mkdir(parents=True)
    (run_dir / "journal.jsonl").write_text(
        '{"seq":0,"ts":1.0,"event":"run_started"}\n'
        '{"seq":1,"ts":2.0,"event":"node_started","node":"scout"}\n', encoding="utf-8")
    assert adw.read_run_outcome(tmp_path, run_id) is None


# ── the agent actually has to be able to act (2026-08-19) ───────────────────


def test_a_writing_agent_without_a_permission_mode_is_reported(root):
    """Observed, not theorised: `chore-1784540624` invoked a real agent four times
    over three minutes; every invocation returned exit 0 / `pass` with a summary
    that read "Permissão necessária para editar o arquivo. Você aprova …?".
    Nothing was written. Five of the shipped library's write-capable nodes were in
    that state — every flow that changes anything."""
    write_spec(root, "perm", """
[adw]
name = "perm"
entry = "do"
[node.do]
type = "agent"
driver = "claude"
prompt = "edit the file"
allowed_tools = ["Read", "Edit", "Write"]
on_pass = "__end__"
on_fail = "__fail__"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "perm"))
    assert any("permission_mode" in w and "`do`" in w for w in warnings), warnings


def test_a_declared_permission_mode_silences_it(root):
    write_spec(root, "perm2", """
[adw]
name = "perm2"
entry = "do"
[node.do]
type = "agent"
driver = "claude"
prompt = "edit the file"
allowed_tools = ["Read", "Edit", "Write"]
permission_mode = "acceptEdits"
on_pass = "__end__"
on_fail = "__fail__"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "perm2"))
    assert not any("permission_mode" in w for w in warnings), warnings


def test_a_read_only_agent_is_never_asked_for_a_permission_mode(root):
    """There is nothing to approve, so demanding the field would be noise — and
    noise is how a lint teaches people to stop reading it."""
    write_spec(root, "perm3", """
[adw]
name = "perm3"
entry = "look"
[node.look]
type = "agent"
driver = "claude"
prompt = "read it"
allowed_tools = ["Read", "Grep"]
on_pass = "__end__"
on_fail = "__fail__"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "perm3"))
    assert not any("permission_mode" in w for w in warnings), warnings


def test_every_shipped_writing_agent_can_actually_write():
    """Structural, over the whole library rather than one file: the defect was
    uniform, so the guard has to be too."""
    import shutil, tempfile
    lib = adw.library_dir()
    if not lib.is_dir():
        pytest.skip("library not installed")
    tmp = Path(tempfile.mkdtemp())
    try:
        dest = adw.adw_dir(tmp)
        dest.mkdir(parents=True, exist_ok=True)
        for f in lib.glob("*.toml"):
            shutil.copy2(f, dest / f.name)
        if (lib / "fragments").is_dir():
            shutil.copytree(lib / "fragments", dest / "fragments", dirs_exist_ok=True)
        offenders = []
        for name in sorted(p.stem for p in lib.glob("*.toml") if p.stem != "tiers"):
            _, warnings = adw.lint_spec(adw.load_spec(tmp, name))
            offenders += [f"{name}: {w}" for w in warnings if "permission_mode" in w]
        assert offenders == [], offenders
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def test_a_journal_without_the_synthesized_field_is_unknown_not_proof(tmp_path):
    """The field was only journaled from 2026-08-19. Reading its absence as an
    empty list promoted a fully MOCKED run of `feature` as evidence of behaviour
    on the ledger's first day — the same inversion `judge_attest` guards against,
    committed one function later."""
    run_id = "old-1700000000"
    run_dir = adw.runs_dir(tmp_path) / run_id
    run_dir.mkdir(parents=True)
    (run_dir / "journal.jsonl").write_text(
        '{"seq":0,"ts":1.0,"event":"run_started"}\n'
        '{"seq":1,"ts":2.0,"event":"run_finished","status":"completed","class_d":false}\n',
        encoding="utf-8")
    out = adw.read_run_outcome(tmp_path, run_id)
    assert out["synthesized"] is None, "absence of a reading is not a reading of absence"


def test_a_promotion_says_whether_an_agent_was_actually_invoked(tmp_path):
    """"The run completed" and "an agent ran" are different claims.

    A flow with no agent node completes trivially, and reporting that as evidence
    of behaviour answers a question nobody asked. Three of the eight shipped flows
    are agent-free by design, so the ledger has to say which is which."""
    run_id = "mixed-1700000000"
    run_dir = adw.runs_dir(tmp_path) / run_id
    run_dir.mkdir(parents=True)
    (run_dir / "journal.jsonl").write_text(
        '{"seq":0,"ts":1.0,"event":"run_started"}\n'
        '{"seq":1,"ts":2.0,"event":"node_completed","node":"a","session_id":"mock-a"}\n'
        '{"seq":2,"ts":3.0,"event":"node_completed","node":"b","session_id":"9f2c-real-uuid"}\n'
        '{"seq":3,"ts":4.0,"event":"run_finished","status":"completed",'
        '"class_d":false,"synthesized":[]}\n', encoding="utf-8")
    out = adw.read_run_outcome(tmp_path, run_id)
    assert out["agent_sessions"] == {"real": 1, "mock": 1}


def test_a_mocked_session_is_never_reported_as_behaviour(tmp_path):
    """`synthesized` covers what the runner stood in for; a `mock` driver leaves a
    `mock-` session id instead. Either one means the agent did not behave."""
    run_id = "mocked-1700000000"
    run_dir = adw.runs_dir(tmp_path) / run_id
    run_dir.mkdir(parents=True)
    (run_dir / "journal.jsonl").write_text(
        '{"seq":0,"ts":1.0,"event":"run_started"}\n'
        '{"seq":1,"ts":2.0,"event":"node_completed","node":"a","session_id":"mock-a"}\n'
        '{"seq":2,"ts":3.0,"event":"run_finished","status":"completed",'
        '"class_d":false,"synthesized":[]}\n', encoding="utf-8")
    assert adw.cmd_promote(tmp_path, "tmpflow", run_id, None) == 0
    rec = adw.load_promotions()["tmpflow"]
    assert "wiring only" in rec["evidence"], rec["evidence"]
    # limpa o ledger real — este é um fluxo de teste, não um fluxo embarcado
    ledger = adw.load_promotions()
    ledger.pop("tmpflow", None)
    adw.promotions_path().write_text(
        json.dumps(dict(sorted(ledger.items())), indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8")

if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))


# ── ADW template-injection guard (2026-08-08) ────────────────────────────────
#
# `run_code_node` renders each argv element and hands the list to
# subprocess.run WITHOUT shell=True. A value rendered INTO the script argument
# of a shell invocation is therefore parsed by that shell; a value passed as a
# trailing argv element is bound to a positional parameter and is not.
#
# This is a STRUCTURAL guard over every template at once: the narrow per-file
# fix is what let the pattern spread to seven specs in the first place.

#: The DEPLOYED library is listed first and is the one that matters: `library_dir()`
#: resolves here, so `from-template` copies from it into every new project. Until
#: 2026-08-18 this list held only the repo mirror and the instantiated copies — so
#: the shell-injection guard below scanned two places that had been fixed while the
#: source everyone instantiates from still shipped `'{{vars.x}}'` inside `bash -c`.
#: A guard that does not cover the artifact in use is not a guard.
ADW_LIBRARY_DIRS = [
    Path.home() / ".claude/skills/Touring/adw-library",
    Path.home() / "projects/touring/client/skills/Touring/adw-library",
    Path.home() / "projects/touring/.touring/adw",
]

#: (deployed, repo mirror) — kept byte-identical; see test_library_and_repo_mirror_agree.
LIBRARY_PAIR = (ADW_LIBRARY_DIRS[0], ADW_LIBRARY_DIRS[1])

#: Variables that are commands by contract, not data (the caller supplies a
#: command deliberately, e.g. `--var verify_cmd="cargo test"`).
COMMAND_BY_CONTRACT = {"verify_cmd"}


def _adw_specs():
    """Every ADW spec on disk, as (path, parsed) pairs — FRAGMENTS INCLUDED.

    Until 2026-08-23 this walked only the flow specs, so the moment `bugfix`
    COMPOSED its recall/diagnose nodes into fragments, those free-text commands
    left the injection guard's coverage entirely — the cross-audit's finding #1
    ("a guard that does not cover the artifact in use is not a guard") repeated
    inside the guard's own test file. Composition must never shrink coverage.
    """
    import tomllib

    out = []
    dirs = [d for base in ADW_LIBRARY_DIRS for d in (base, base / "fragments")]
    for d in dirs:
        if d.is_dir():
            for f in sorted(d.glob("*.toml")):
                try:
                    out.append((f, tomllib.loads(f.read_text(encoding="utf-8"))))
                except tomllib.TOMLDecodeError as exc:  # pragma: no cover
                    pytest.fail(f"{f} is not valid TOML: {exc}")
    return out


def test_no_data_variable_is_interpolated_inside_a_shell_script():
    """No `{{vars.X}}` / `{{inputs.X}}` may sit inside a shell node's script.

    `inputs.*` is the fragments' spelling of the same data channel — a guard
    that only knew `vars.*` went blind exactly where the free-text moved to.
    """
    import re

    offenders = []
    for path, spec in _adw_specs():
        for name, node in (spec.get("node") or {}).items():
            cmd = node.get("command")
            if not isinstance(cmd, list) or len(cmd) < 3:
                continue
            if cmd[0] not in ("bash", "sh") or cmd[1] not in ("-c", "-lc"):
                continue
            for var in re.findall(r"\{\{(?:vars|inputs)\.([\w.-]+)\}\}", str(cmd[2])):
                if var not in COMMAND_BY_CONTRACT:
                    offenders.append(f"{path.name}::{name} interpolates {var}")
    assert not offenders, (
        "template values must travel as positional parameters, not inside the "
        "shell script: " + "; ".join(offenders)
    )


def test_positional_values_reach_the_script_intact():
    """Hardening must not break the feature: the value still arrives verbatim."""
    import subprocess

    value = "gerar um mapa com acentuação e 'aspas'"
    cmd = [
        adw.render_template(p, {}, {"topic": value})
        for p in ["bash", "-c", 'printf "%s" "$1"', "--", "{{vars.topic}}"]
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    assert proc.stdout == value, f"value mangled: {proc.stdout!r}"


def test_flow_mermaid_draws_the_fan_out(tmp_path):
    """The fan-out IS the point of a parallel node — a drawing that omits it
    shows the critics floating free of the block that dispatches them (probe,
    cross-audit 2026-08-23). Static branches get |branch| edges; the parallel
    class must exist in the classDefs or mermaid renders a ghost class."""
    (tmp_path / ".touring" / "adw").mkdir(parents=True)
    (tmp_path / ".touring" / "adw" / "fan.toml").write_text(
        '[adw]\nname = "fan"\nentry = "p"\n'
        '[node.p]\ntype = "parallel"\n'
        'branches = ["c1", "c2"]\nmerge = "collect"\n'
        'on_pass = "__end__"\non_fail = "__fail__"\n'
        '[node.c1]\ntype = "code"\ncommand = ["true"]\n'
        '[node.c2]\ntype = "code"\ncommand = ["true"]\n',
        encoding="utf-8")
    spec = adw.load_spec(tmp_path, "fan")
    desenho = adw.flow_mermaid(spec)
    assert "classDef parallel" in desenho
    assert ":::parallel" in desenho
    assert "N_p -->|branch| N_c1" in desenho
    assert "N_p -->|branch| N_c2" in desenho


def test_specs_that_take_free_text_actually_use_positionals():
    """At least the known free-text specs must have been converted.

    Since composition (2026-08-23) a spec satisfies this in either of two ways:
    its OWN nodes carry positionals, or the free-text nodes moved into fragments
    it `[[use]]`s — and then THOSE must carry the positionals. `bugfix` is the
    second case: recall/diagnose live in recall-pack/diagnose-pack now.
    """
    by_name = {}
    converted = set()
    for path, spec in _adw_specs():
        by_name.setdefault(path.name, spec)
        for node in (spec.get("node") or {}).values():
            if isinstance(node.get("command"), list) and "--" in node["command"]:
                converted.add(path.name)
    for name in ("chore.toml", "feature.toml", "bugfix.toml"):
        if name in converted:
            continue
        usados = [f"{u.get('module')}.toml" for u in (by_name.get(name, {}).get("use") or [])]
        assert usados and all(u in converted for u in usados), (
            f"{name} still interpolates into the shell string "
            f"(own nodes lack positionals and so do its fragments: {usados})")


# ── A1: the cycle scan must not stop at the first cycle it finds ──────────────


def test_lint_finds_an_exitless_cycle_behind_a_legitimate_one(root):
    """Regression, 2026-08-18. `_lint_cycles` ended its per-node loop in a
    `return`, so the scan stopped at the FIRST node that reached any cycle. A
    spec whose first cycle had a legitimate exit therefore passed lint with a
    second, exitless cycle intact: `valid: true`, exit 0, on a graph that cannot
    terminate."""
    write_spec(root, "twocyc", """
[adw]
name = "twocyc"
entry = "start"
[node.start]
type = "code"
command = ["true"]
idempotent = true
on_pass = "a"
on_fail = "x"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "b"
on_fail = "b"
[node.b]
type = "code"
command = ["true"]
idempotent = true
on_pass = "a"
on_fail = "__fail__"
[node.x]
type = "code"
command = ["true"]
idempotent = true
on_pass = "y"
on_fail = "y"
[node.y]
type = "code"
command = ["true"]
idempotent = true
on_pass = "x"
on_fail = "x"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "twocyc"))
    exitless = [e for e in errors if "cycle without exit" in e]
    assert len(exitless) == 1, f"want exactly the x↔y cycle, got {errors}"
    assert "x" in exitless[0] and "y" in exitless[0]
    assert "a" not in exitless[0].split(":")[1], "a↔b has an exit — not a finding"


def test_one_cycle_is_reported_once_not_once_per_member(root):
    """Every node is a start point, so an N-node ring is rediscovered N times.
    Rotation-invariant dedupe keeps a 3-node cycle from becoming 3 findings."""
    write_spec(root, "ring", """
[adw]
name = "ring"
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "b"
on_fail = "b"
[node.b]
type = "code"
command = ["true"]
idempotent = true
on_pass = "c"
on_fail = "c"
[node.c]
type = "code"
command = ["true"]
idempotent = true
on_pass = "a"
on_fail = "a"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "ring"))
    assert len([e for e in errors if "cycle without exit" in e]) == 1, errors


# ── A2: a retry that cannot see the verdict is a blind retry ──────────────────


def test_gate_returning_to_an_agent_must_hand_over_its_verdict(root):
    write_spec(root, "blind", """
[adw]
name = "blind"
entry = "work"
[node.work]
type = "agent"
driver = "claude"
prompt = "Do the thing."
allowed_tools = ["Read"]
on_pass = "check"
on_fail = "__fail__"
[node.check]
type = "gate"
command = ["false"]
idempotent = true
on_pass = "__end__"
on_fail = "work"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "blind"))
    assert any("blind to the verdict" in w for w in warnings), warnings


def test_no_warning_once_the_prompt_reads_the_gate(root):
    write_spec(root, "sighted", """
[adw]
name = "sighted"
entry = "work"
[node.work]
type = "agent"
driver = "claude"
prompt = "Do the thing. Previous verdict: {{nodes.check.summary}}"
allowed_tools = ["Read"]
on_pass = "check"
on_fail = "__fail__"
[node.check]
type = "gate"
command = ["false"]
idempotent = true
on_pass = "__end__"
on_fail = "work"
""")
    _, warnings = adw.lint_spec(adw.load_spec(root, "sighted"))
    assert not any("blind to the verdict" in w for w in warnings), warnings


def test_every_library_retry_edge_hands_over_its_verdict():
    """The library is the reference implementation: no shipped spec may retry blind."""
    blind = []
    for path, spec_data in _adw_specs():
        nodes = spec_data.get("node") or {}
        for name, body in nodes.items():
            if body.get("type") != "gate":
                continue
            target = nodes.get(body.get("on_fail", ""), {})
            if target.get("type") == "agent" and f"nodes.{name}" not in str(target.get("prompt", "")):
                blind.append(f"{path.stem}: {name} → {body.get('on_fail')}")
    assert blind == [], f"blind retry edges: {blind}"


# ── B3: persona — posture declared in the flow, compiled to `--agents` ────────


CRITIC_SPEC = """
[adw]
name = "{name}"
entry = "judge"
[node.judge]
type = "agent"
driver = "claude"
prompt = "Judge the artifact."
allowed_tools = ["Read"]
session = "fresh"
on_pass = "__end__"
on_fail = "__fail__"
[node.judge.persona]
role = "gate-critic"
stance = "reject_by_default"
lens = "correctness"
scope = "the diff under review, nothing adjacent"
bar = "the named test passes on a clean checkout"
burden = "The author must prove it works; you never assume it does."
refuses = ["editing code", "running the fix yourself"]
forbids = ["approving on a summary alone"]
blind_to = ["author_notes"]
escalate_when = "the verification command could not run at all"
emits = "VERDICT=PASS|REJECT|ESCALATE + REASON=<one line>"
"""


def test_persona_reaches_the_cli_as_agents_and_agent(tmp_path):
    node = adw.Node(name="judge", type="agent", raw={
        "allowed_tools": ["Read"],
        "persona": {"role": "gate-critic", "stance": "reject_by_default",
                    "emits": "VERDICT=PASS|REJECT"},
    })
    cmd = adw._claude_cmd(node, adw.Journal(tmp_path), "judge it")
    assert "--agents" in cmd and "--agent" in cmd
    assert cmd[cmd.index("--agent") + 1] == "gate-critic"
    payload = json.loads(cmd[cmd.index("--agents") + 1])
    assert set(payload) == {"gate-critic"}
    assert "VERDICT=PASS|REJECT" in payload["gate-critic"]["prompt"]


def test_every_declared_technique_survives_into_the_system_prompt(root):
    """A posture the spec declares but the compiler drops is a silent lie: the
    flow reads as if the agent were constrained while the agent never saw it."""
    write_spec(root, "critic", CRITIC_SPEC.format(name="critic"))
    persona = adw.load_spec(root, "critic").node("judge").raw["persona"]
    compiled = adw.compile_persona(persona)
    for value in ("correctness", "the diff under review", "the named test passes",
                  "must prove it works", "editing code", "running the fix yourself",
                  "approving on a summary alone", "author_notes",
                  "could not run at all", "VERDICT=PASS|REJECT|ESCALATE"):
        assert value in compiled, f"technique dropped from the compiled persona: {value!r}"
    labels = [label for _, label, _ in adw.PERSONA_SECTIONS]
    assert [l for l in labels if l in compiled] == labels, "section order is not stable"


def test_persona_fields_resolve_vars_and_node_summaries(tmp_path):
    node = adw.Node(name="judge", type="agent", raw={"persona": {
        "role": "gate-critic", "stance": "reject_by_default",
        "bar": "{{vars.verify_cmd}} exits 0", "emits": "VERDICT=<v>"}})
    cmd = adw._claude_cmd(node, adw.Journal(tmp_path), "judge",
                          results={}, variables={"verify_cmd": "cargo test -p touring-ceg"})
    payload = json.loads(cmd[cmd.index("--agents") + 1])
    assert "cargo test -p touring-ceg exits 0" in payload["gate-critic"]["prompt"]


def test_critic_without_a_parseable_verdict_is_rejected(root):
    write_spec(root, "mute", CRITIC_SPEC.format(name="mute").replace(
        'emits = "VERDICT=PASS|REJECT|ESCALATE + REASON=<one line>"', ""))
    errors, _ = adw.lint_spec(adw.load_spec(root, "mute"))
    assert any("requires `emits`" in e for e in errors), errors


def test_critic_may_not_resume_the_session_it_judges(root):
    write_spec(root, "tainted", CRITIC_SPEC.format(name="tainted").replace(
        'session = "fresh"', 'session = "resume_on_fail"'))
    errors, _ = adw.lint_spec(adw.load_spec(root, "tainted"))
    assert any("distrust" in e for e in errors), errors


def test_declared_blindness_must_be_real(root):
    write_spec(root, "peeking", CRITIC_SPEC.format(name="peeking").replace(
        'prompt = "Judge the artifact."',
        'prompt = "Judge the artifact. Author says: {{nodes.author_notes.summary}}"'))
    errors, _ = adw.lint_spec(adw.load_spec(root, "peeking"))
    assert any("blindness is fiction" in e for e in errors), errors


def test_persona_on_a_non_agent_node_is_rejected(root):
    write_spec(root, "wrongtype", """
[adw]
name = "wrongtype"
entry = "g"
[node.g]
type = "gate"
command = ["true"]
idempotent = true
on_pass = "__end__"
[node.g.persona]
role = "critic"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "wrongtype"))
    assert any("only applies to an agent node" in e for e in errors), errors


def test_persona_typo_is_caught_rather_than_silently_ignored(root):
    write_spec(root, "typo", """
[adw]
name = "typo"
entry = "a"
[node.a]
type = "agent"
driver = "mock"
on_pass = "__end__"
[node.a.persona]
role = "critic"
stanze = "reject_by_default"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "typo"))
    assert any("unknown persona field" in e for e in errors), errors


def test_the_scanned_dirs_include_the_library_the_runner_actually_loads():
    """The structural guards must cover `library_dir()` itself, not only mirrors."""
    assert adw.library_dir() in ADW_LIBRARY_DIRS, (
        f"{adw.library_dir()} is where from-template copies from, and nothing scans it")


def test_library_and_repo_mirror_agree():
    """Deployed library ≡ repo mirror, byte for byte.

    Regression, 2026-08-18: the two drifted for ten days. The repo side received
    the shell-injection fix (positional args) and two context enrichments; the
    deployed side — the one `from-template` copies from — received neither, and
    kept handing every new project `touring memory recall '{{vars.symptom}}'`
    interpolated straight into a `bash -c` string. Nothing compared them, so
    nothing failed. This test is that comparison."""
    deployed, mirror = LIBRARY_PAIR
    if not (deployed.is_dir() and mirror.is_dir()):
        pytest.skip("one side of the library pair is not present on this machine")
    # rglob, not glob: the fragment kit lives in a subdirectory, and a guard that
    # stops at the top level would let exactly the same drift happen one level down.
    names = ({p.relative_to(deployed).as_posix() for p in deployed.rglob("*.toml")}
             | {p.relative_to(mirror).as_posix() for p in mirror.rglob("*.toml")})
    drifted = [
        name for name in sorted(names)
        if not ((deployed / name).is_file() and (mirror / name).is_file())
        or (deployed / name).read_bytes() != (mirror / name).read_bytes()
    ]
    assert drifted == [], (
        f"library drift between {deployed} and {mirror}: {drifted} — "
        f"the deployed side is what every new project instantiates")


# ── B1: fragments — composition resolved by inlining, engine untouched ────────


@pytest.fixture()
def frag(root: Path):
    """Write a fragment into the project's own fragment dir (shadows the library)."""
    def _write(module: str, body: str) -> None:
        d = adw.adw_dir(root) / "fragments"
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{module}.toml").write_text(body, encoding="utf-8")
    return _write


PACK = """
[fragment]
description = "two-step pack"
inputs = ["topic"]
entry = "first"
[node.first]
type = "code"
command = ["bash", "-c", "echo \\"$1\\"", "--", "{{inputs.topic}}"]
idempotent = true
on_pass = "second"
on_fail = "__exit_fail__"
[node.second]
type = "code"
command = ["bash", "-c", "echo {{nodes.first.summary}}"]
idempotent = true
on_pass = "__exit__"
on_fail = "__exit_fail__"
"""


def test_fragment_inlines_flat_with_namespaced_nodes(root, frag):
    frag("pack", PACK)
    write_spec(root, "comp", """
[adw]
name = "comp"
entry = "p"
[[use]]
module = "pack"
as = "p"
with = { topic = "{{vars.t}}" }
on_pass = "done"
on_fail = "__fail__"
[node.done]
type = "code"
command = ["true"]
idempotent = true
on_pass = "__end__"
""")
    spec = adw.load_spec(root, "comp")
    assert set(spec.nodes) == {"p.first", "p.second", "done"}
    assert spec.entry == "p.first", "a bare namespace must deref to the fragment's entry"
    assert spec.node("p.first").on_pass == "p.second", "internal edges stay inside the fragment"
    assert spec.node("p.second").on_pass == "done", "__exit__ lands on the host's on_pass"
    assert spec.node("p.first").on_fail == "__fail__", "__exit_fail__ lands on the host's on_fail"
    assert adw.lint_spec(spec) == ([], [])


def test_fragment_inputs_and_node_refs_are_rewritten(root, frag):
    frag("pack", PACK)
    write_spec(root, "comp", """
[adw]
name = "comp"
entry = "p"
[[use]]
module = "pack"
as = "p"
with = { topic = "{{vars.t}}" }
on_pass = "__end__"
on_fail = "__fail__"
""")
    spec = adw.load_spec(root, "comp")
    assert "{{vars.t}}" in spec.node("p.first").raw["command"], "{{inputs.x}} → the bound value"
    assert not any("{{inputs." in str(c) for c in spec.node("p.first").raw["command"])
    assert "{{nodes.p.first.summary}}" in spec.node("p.second").raw["command"][2], \
        "an intra-fragment node reference must be namespaced too"


def test_two_instances_of_one_fragment_stay_independent(root, frag):
    """The point of a fragment: use it twice with different bindings, no collision."""
    frag("pack", PACK)
    write_spec(root, "twice", """
[adw]
name = "twice"
entry = "a"
[[use]]
module = "pack"
as = "a"
with = { topic = "alpha" }
on_pass = "b"
on_fail = "__fail__"
[[use]]
module = "pack"
as = "b"
with = { topic = "beta" }
on_pass = "__end__"
on_fail = "__fail__"
""")
    spec = adw.load_spec(root, "twice")
    assert set(spec.nodes) == {"a.first", "a.second", "b.first", "b.second"}
    assert "alpha" in spec.node("a.first").raw["command"]
    assert "beta" in spec.node("b.first").raw["command"]
    assert spec.node("a.second").on_pass == "b.first"


def test_unbound_input_is_rejected(root, frag):
    frag("pack", PACK)
    write_spec(root, "bad", """
[adw]
name = "bad"
entry = "p"
[[use]]
module = "pack"
as = "p"
on_pass = "__end__"
""")
    with pytest.raises(adw.SpecError, match="unbound input"):
        adw.load_spec(root, "bad")


def test_input_the_fragment_never_declared_is_rejected(root, frag):
    frag("pack", PACK)
    write_spec(root, "bad", """
[adw]
name = "bad"
entry = "p"
[[use]]
module = "pack"
as = "p"
with = { topic = "t", nonsense = "x" }
on_pass = "__end__"
""")
    with pytest.raises(adw.SpecError, match="not declared"):
        adw.load_spec(root, "bad")


def test_fragment_may_not_name_a_host_node(root, frag):
    """A fragment that reaches into the host is not reusable — it is coupled."""
    frag("leaky", """
[fragment]
inputs = []
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "host_only_node"
""")
    write_spec(root, "bad", """
[adw]
name = "bad"
entry = "p"
[[use]]
module = "leaky"
as = "p"
on_pass = "__end__"
[node.host_only_node]
type = "code"
command = ["true"]
idempotent = true
on_pass = "__end__"
""")
    with pytest.raises(adw.SpecError, match="may not name a host node"):
        adw.load_spec(root, "bad")


def test_namespace_collision_with_a_host_node_is_rejected(root, frag):
    frag("pack", PACK)
    write_spec(root, "bad", """
[adw]
name = "bad"
entry = "p"
[[use]]
module = "pack"
as = "p"
with = { topic = "t" }
on_pass = "__end__"
[node.p]
type = "code"
command = ["true"]
idempotent = true
on_pass = "__end__"
""")
    with pytest.raises(adw.SpecError, match="collides"):
        adw.load_spec(root, "bad")


def test_missing_fragment_names_where_it_looked(root):
    write_spec(root, "bad", """
[adw]
name = "bad"
entry = "p"
[[use]]
module = "nope"
as = "p"
on_pass = "__end__"
""")
    with pytest.raises(adw.SpecError, match="searched:"):
        adw.load_spec(root, "bad")


def test_a_composed_flow_executes_exactly_like_its_monolith(root, frag):
    """The equivalence that matters: composition changes the SOURCE, never the run."""
    frag("pack", PACK)
    write_spec(root, "composed", """
[adw]
name = "composed"
entry = "p"
[[use]]
module = "pack"
as = "p"
with = { topic = "hello" }
on_pass = "__end__"
on_fail = "__fail__"
""")
    write_spec(root, "monolith", """
[adw]
name = "monolith"
entry = "p.first"
[node."p.first"]
type = "code"
command = ["bash", "-c", "echo \\"$1\\"", "--", "hello"]
idempotent = true
on_pass = "p.second"
on_fail = "__fail__"
[node."p.second"]
type = "code"
command = ["bash", "-c", "echo {{nodes.p.first.summary}}"]
idempotent = true
on_pass = "__end__"
on_fail = "__fail__"
""")
    composed = adw.flat_graph(adw.load_spec(root, "composed"))
    monolith = adw.flat_graph(adw.load_spec(root, "monolith"))
    composed.pop("name"), monolith.pop("name")
    assert composed == monolith, "a composed flow must resolve to the graph it replaces"

    outcome_c = adw.execute(adw.load_spec(root, "composed"), root)
    outcome_m = adw.execute(adw.load_spec(root, "monolith"), root)
    assert outcome_c.status == outcome_m.status == "completed"
    assert [s["node"] for s in outcome_c.steps] == [s["node"] for s in outcome_m.steps]


def test_every_shipped_fragment_is_loadable_and_declares_its_seams():
    """The kit is the portfolio's vocabulary — a broken piece breaks every flow using it."""
    frag_dir = adw.library_dir() / "fragments"
    if not frag_dir.is_dir():
        pytest.skip("fragment kit not installed")
    modules = sorted(frag_dir.glob("*.toml"))
    assert modules, "the fragment kit is empty"
    for path in modules:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
        meta = data.get("fragment") or {}
        nodes = data.get("node") or {}
        assert meta.get("description"), f"{path.stem}: a fragment without a description is undiscoverable"
        assert meta.get("entry") in nodes, f"{path.stem}: entry `{meta.get('entry')}` is not a node"
        reaches_seam = any(
            str(b.get(e, "")) in adw.FRAGMENT_SEAMS
            for b in nodes.values() for e in ("on_pass", "on_fail", "on_dry")
        )
        assert reaches_seam, f"{path.stem}: no node reaches __exit__/__exit_fail__ — it cannot return"
        declared = set(meta.get("inputs") or [])
        used = set(re.findall(r"\{\{inputs\.([\w.-]+?)\}\}", path.read_text(encoding="utf-8")))
        assert used <= declared, f"{path.stem}: uses undeclared input(s) {sorted(used - declared)}"


# ── B4: read-only fan-out with a real barrier ────────────────────────────────


FANOUT = """
[adw]
name = "{name}"
entry = "lenses"
[node.lenses]
type = "parallel"
branches = ["sec", "perf", "correct"]
merge = "{merge}"
on_branch_fail = "{policy}"
max_branches = 4
on_pass = "__end__"
on_fail = "__fail__"
[node.sec]
type = "code"
command = ["bash", "-c", "echo security-lens; echo NEW_FINDINGS=2"]
idempotent = true
[node.perf]
type = "code"
command = ["bash", "-c", "echo perf-lens; echo NEW_FINDINGS=3"]
idempotent = true
[node.correct]
type = "code"
command = ["bash", "-c", "{correct}"]
idempotent = true
"""


def test_fanout_runs_every_branch_and_joins(root):
    write_spec(root, "fan", FANOUT.format(name="fan", merge="collect", policy="all",
                                          correct="echo correctness-lens; echo NEW_FINDINGS=1"))
    outcome = adw.execute(adw.load_spec(root, "fan"), root)
    assert outcome.status == "completed"
    ran = [s["node"] for s in outcome.steps if s.get("parallel") == "lenses"]
    assert ran == ["sec", "perf", "correct"], "branches journal in DECLARED order, not finish order"


def test_every_branch_result_survives_the_join(root):
    """The defect a reducer exists to prevent: N branches, one slot, N-1 lost."""
    write_spec(root, "fan", FANOUT.format(name="fan", merge="collect", policy="all",
                                          correct="echo correctness-lens"))
    spec = adw.load_spec(root, "fan")
    run = adw.execute(spec, root)
    merged = json.loads((adw.runs_dir(root) / run.run_id / "lenses#0.json").read_text())["summary"]
    for lens in ("security-lens", "perf-lens", "correctness-lens"):
        assert lens in merged, f"{lens} was dropped by the join"


def test_tally_reemits_an_aggregate_findings_signal(root):
    """A loop wrapping a fan-out still needs its own termination signal (Law L2)."""
    write_spec(root, "fan", FANOUT.format(name="fan", merge="tally", policy="all",
                                          correct="echo c; echo NEW_FINDINGS=1"))
    run = adw.execute(adw.load_spec(root, "fan"), root)
    merged = json.loads((adw.runs_dir(root) / run.run_id / "lenses#0.json").read_text())["summary"]
    assert "NEW_FINDINGS=6" in merged, merged  # 2 + 3 + 1


def test_one_failed_branch_fails_the_block_by_default(root):
    write_spec(root, "fan", FANOUT.format(name="fan", merge="collect", policy="all",
                                          correct="exit 3"))
    outcome = adw.execute(adw.load_spec(root, "fan"), root)
    assert outcome.status == "failed", "on_branch_fail=all must be fail-closed"


def test_policy_any_passes_on_a_partial_success(root):
    write_spec(root, "fan", FANOUT.format(name="fan", merge="collect", policy="any",
                                          correct="exit 3"))
    assert adw.execute(adw.load_spec(root, "fan"), root).status == "completed"


def test_a_branch_that_raises_is_a_failed_branch_not_a_dead_run(root, monkeypatch):
    write_spec(root, "fan", FANOUT.format(name="fan", merge="collect", policy="any",
                                          correct="echo ok"))
    real = adw.run_code_node

    def explode(node, results, variables, cwd=None):
        if node.name == "sec":
            raise RuntimeError("boom")
        return real(node, results, variables, cwd=cwd)

    monkeypatch.setattr(adw, "run_code_node", explode)
    outcome = adw.execute(adw.load_spec(root, "fan"), root)
    assert outcome.status == "completed"
    sec = [s for s in outcome.steps if s["node"] == "sec"][0]
    assert sec["exit_code"] == 1 and sec["verdict"] == "fail"


def test_resume_mid_fanout_never_reruns_a_completed_branch(root):
    """The durability gate: `kill -9` between branches must not re-run (or re-bill)
    the ones that already finished."""
    write_spec(root, "fan", FANOUT.format(name="fan", merge="collect", policy="all",
                                          correct="echo correctness-lens"))
    spec = adw.load_spec(root, "fan")
    run_id = "fan-resume-1"
    run_path = adw.runs_dir(root) / run_id
    run_path.mkdir(parents=True)
    journal = adw.Journal(run_path)
    journal.append("run_started", adw="fan", run_id=run_id)
    journal.append("parallel_started", node="lenses", exec_key="lenses#0",
                   branches=["sec", "perf", "correct"], merge="collect", on_branch_fail="all")
    for done in ("sec", "perf"):
        journal.append("node_started", node=done, exec_key=f"{done}#0", type="code", parallel="lenses")
        adw.store_result(run_path, f"{done}#0", f"{done} already ran\n")
        journal.append("node_completed", node=done, exec_key=f"{done}#0", exit_code=0,
                       verdict="pass", next="lenses", session_id=None, class_d=False,
                       parallel="lenses")
    journal.append("node_started", node="correct", exec_key="correct#0", type="code",
                   parallel="lenses")
    # kill -9 here: `correct` never completed

    outcome = adw.execute(spec, root, resume_run=run_id)
    assert outcome.status == "completed"
    by_node = {s["node"]: s for s in outcome.steps if s.get("parallel") == "lenses"}
    assert by_node["sec"].get("replayed") is True
    assert by_node["perf"].get("replayed") is True
    assert by_node["correct"].get("replayed") is None, "the interrupted branch must re-run"
    assert by_node["correct"]["verdict"] == "pass"


def test_class_d_under_fanout_names_every_branch_that_claimed_success(root):
    """Law L3 degraded exactly where it mattered most: one `last_agent` slot held
    whichever branch finished last, so a downstream gate failure accused an
    arbitrary agent."""
    write_spec(root, "fanD", """
[adw]
name = "fanD"
entry = "panel"
[node.panel]
type = "parallel"
branches = ["a1", "a2"]
merge = "collect"
on_branch_fail = "ignore"
max_branches = 2
on_pass = "verify"
on_fail = "__fail__"
[node.a1]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
[node.a2]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
[node.verify]
type = "gate"
command = ["false"]
idempotent = true
on_pass = "__end__"
on_fail = "__fail__"
""")
    write_recording(root, "fanD", "a1", "All tests pass ✅ complete")
    # NB: the narrative heuristic is word-based, so "not done" WOULD match `done`.
    # It errs toward flagging — a false positive only makes Class-D more suspicious,
    # never less. The branch that genuinely claims nothing must therefore say nothing.
    write_recording(root, "fanD", "a2", "3 issues remain open")
    outcome = adw.execute(adw.load_spec(root, "fanD"), root, force_mock=True)
    assert outcome.class_d is True, "a gate FAIL after a narrating branch is a divergence"
    joined = [json.loads(l) for l in
              (adw.runs_dir(root) / outcome.run_id / "journal.jsonl").read_text().splitlines()]
    join = [e for e in joined if e["event"] == "parallel_joined"][0]
    assert join["narrated_success"] == ["a1"], "the journal must name WHICH branch claimed success"


# ── B4 lint: the three silent failure modes ──────────────────────────────────


PARALLEL_LINT_BASE = """
[adw]
name = "pl"
entry = "p"
[node.p]
type = "parallel"
branches = ["x", "y"]
{extra}
on_pass = "__end__"
on_fail = "__fail__"
[node.x]
type = "code"
command = ["true"]
idempotent = true
[node.y]
type = "code"
command = ["true"]
idempotent = true
"""


@pytest.mark.parametrize("extra,needle", [
    ('on_branch_fail = "all"\nmax_branches = 2', "`merge` must be declared"),
    ('merge = "collect"\nmax_branches = 2\non_branch_fail = "sometimes"', "on_branch_fail"),
    ('merge = "collect"\non_branch_fail = "all"', "max_branches"),
    ('merge = "collect"\non_branch_fail = "all"\nmax_branches = 1', "exceed max_branches"),
])
def test_fanout_lint_rejects_undeclared_joins(root, extra, needle):
    write_spec(root, "pl", PARALLEL_LINT_BASE.format(extra=extra))
    errors, _ = adw.lint_spec(adw.load_spec(root, "pl"))
    assert any(needle in e for e in errors), errors


def test_fanout_rejects_a_branch_that_can_write(root):
    write_spec(root, "pw", """
[adw]
name = "pw"
entry = "p"
[node.p]
type = "parallel"
branches = ["writer"]
merge = "collect"
on_branch_fail = "all"
max_branches = 2
on_pass = "__end__"
[node.writer]
type = "agent"
driver = "claude"
prompt = "go"
allowed_tools = ["Read", "Edit"]
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "pw"))
    assert any("fan-out is read-only" in e for e in errors), errors


def test_fanout_rejects_a_duplicated_branch(root):
    write_spec(root, "pd", """
[adw]
name = "pd"
entry = "p"
[node.p]
type = "parallel"
branches = ["x", "x"]
merge = "collect"
on_branch_fail = "all"
max_branches = 4
on_pass = "__end__"
[node.x]
type = "code"
command = ["true"]
idempotent = true
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "pd"))
    assert any("listed twice" in e for e in errors), errors


def test_branches_are_reachable_not_orphans(root):
    write_spec(root, "pr", PARALLEL_LINT_BASE.format(
        extra='merge = "collect"\non_branch_fail = "all"\nmax_branches = 2'))
    _, warnings = adw.lint_spec(adw.load_spec(root, "pr"))
    assert not any("orphan" in w for w in warnings), warnings


# ── B6: the verification contract — PASS | REJECT | ESCALATE ─────────────────


CONTRACT_SPEC = """
[adw]
name = "{name}"
entry = "work"
[node.work]
type = "agent"
driver = "mock"
on_pass = "verify"
on_fail = "__fail__"
[node.verify]
type = "gate"
command = ["bash", "-c", "{gate}"]
idempotent = true
verdict_contract = true
max_retries = 5
on_pass = "__end__"
on_fail = "work"
on_escalate = "triage"
[node.triage]
type = "code"
command = ["bash", "-c", "echo handed to triage"]
idempotent = true
on_pass = "__end__"
"""


@pytest.mark.parametrize("output,exit_code,expected", [
    ("VERDICT=PASS", 0, adw.PASS),
    ("VERDICT=REJECT + REASON=test failed", 1, adw.REJECT),
    ("VERDICT=ESCALATE + REASON=cargo not on PATH", 1, adw.ESCALATE),
    ("noise\nVERDICT=REJECT\nVERDICT=PASS", 0, adw.PASS),   # last verdict wins
    ("no verdict here", 0, adw.PASS),                        # falls back to exit code
    ("no verdict here", 1, adw.REJECT),                      # ...and to REJECT
    ("", 1, adw.REJECT),                                     # silence is never a pass
])
def test_verdict_parsing_is_fail_closed(output, exit_code, expected):
    assert adw.parse_verdict(output, exit_code) == expected


def test_escalation_routes_aside_without_spending_a_retry(root):
    """The case a boolean gate cannot express: the check could not run. Sending an
    agent back at it burns the budget against something no agent can fix."""
    write_spec(root, "esc", CONTRACT_SPEC.format(
        name="esc", gate="echo 'VERDICT=ESCALATE + REASON=cargo missing'; exit 1"))
    write_recording(root, "esc", "work", "attempted the change")
    outcome = adw.execute(adw.load_spec(root, "esc"), root, force_mock=True)
    assert outcome.status == "completed"
    assert [s["node"] for s in outcome.steps] == ["work", "verify", "triage"]
    assert len([s for s in outcome.steps if s["node"] == "work"]) == 1, \
        "an escalation must not re-invoke (or re-bill) the agent"
    events = adw.Journal(adw.runs_dir(root) / outcome.run_id).events
    assert any(e["event"] == "escalated" for e in events)
    assert not any(e["event"] == "retry_limit_exceeded" for e in events)


def test_reject_still_feeds_back_and_still_exhausts_its_budget(root):
    write_spec(root, "rej", CONTRACT_SPEC.format(
        name="rej", gate="echo 'VERDICT=REJECT + REASON=2 tests red'; exit 1"))
    write_recording(root, "rej", "work", "attempted the change")
    outcome = adw.execute(adw.load_spec(root, "rej"), root, force_mock=True)
    assert outcome.status == "failed"
    assert len([s for s in outcome.steps if s["node"] == "work"]) == 6  # initial + 5 retries
    assert any(e["event"] == "retry_limit_exceeded"
               for e in adw.Journal(adw.runs_dir(root) / outcome.run_id).events)


def test_a_gate_that_exits_zero_without_a_verdict_still_passes(root):
    """Backward compatibility: a gate that never heard of the contract keeps working."""
    write_spec(root, "plain", CONTRACT_SPEC.format(name="plain", gate="echo fine"))
    write_recording(root, "plain", "work", "did it")
    outcome = adw.execute(adw.load_spec(root, "plain"), root, force_mock=True)
    assert outcome.status == "completed"
    assert [s["node"] for s in outcome.steps] == ["work", "verify"]


def test_contract_gate_must_declare_where_escalation_goes(root):
    write_spec(root, "noesc", """
[adw]
name = "noesc"
entry = "verify"
[node.verify]
type = "gate"
command = ["true"]
idempotent = true
verdict_contract = true
on_pass = "__end__"
on_fail = "__fail__"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "noesc"))
    assert any("needs `on_escalate`" in e for e in errors), errors


def test_stagnation_is_opt_in_and_stops_a_retry_that_changes_nothing(root):
    write_spec(root, "stag", """
[adw]
name = "stag"
entry = "work"
[node.work]
type = "agent"
driver = "mock"
on_pass = "verify"
[node.verify]
type = "gate"
command = ["bash", "-c", "echo 'same rejection every time'; exit 1"]
idempotent = true
max_retries = 9
stagnation_rounds = 2
on_pass = "__end__"
on_fail = "work"
""")
    write_recording(root, "stag", "work", "tried")
    outcome = adw.execute(adw.load_spec(root, "stag"), root, force_mock=True)
    assert outcome.status == "failed"
    assert len([s for s in outcome.steps if s["node"] == "work"]) == 2, \
        "identical rejections must stop the loop well before max_retries=9"
    events = adw.Journal(adw.runs_dir(root) / outcome.run_id).events
    assert any(e["event"] == "stagnation_detected" for e in events)


def test_stagnation_off_by_default_never_overrides_a_declared_max_retries(root):
    """A silently-failing gate (`exit 1`, no output) is trivially 'stagnant'. If this
    were default-on, every such spec would stop at 2 while its author wrote 5."""
    write_spec(root, "silent", """
[adw]
name = "silent"
entry = "work"
[node.work]
type = "agent"
driver = "mock"
on_pass = "verify"
[node.verify]
type = "gate"
command = ["bash", "-c", "exit 1"]
idempotent = true
max_retries = 4
on_pass = "__end__"
on_fail = "work"
""")
    write_recording(root, "silent", "work", "tried")
    outcome = adw.execute(adw.load_spec(root, "silent"), root, force_mock=True)
    assert len([s for s in outcome.steps if s["node"] == "work"]) == 5  # initial + 4
    assert not any(e["event"] == "stagnation_detected"
                   for e in adw.Journal(adw.runs_dir(root) / outcome.run_id).events)


def test_kill_switch_stands_the_run_down_at_a_node_boundary(root):
    write_spec(root, "killable", """
[adw]
name = "killable"
entry = "a"
[node.a]
type = "code"
command = ["bash", "-c", "echo a"]
idempotent = true
on_pass = "b"
[node.b]
type = "code"
command = ["bash", "-c", "echo b"]
idempotent = true
on_pass = "__end__"
""")
    spec = adw.load_spec(root, "killable")
    run_id = "killable-switch"
    run_path = adw.runs_dir(root) / run_id
    run_path.mkdir(parents=True)
    (run_path / adw.KILL_SWITCH).write_text("stop please", encoding="utf-8")
    outcome = adw.execute(spec, root, resume_run=run_id)
    assert outcome.status == "aborted"
    assert outcome.steps == [], "the switch is honoured before the first node runs"
    assert any(e["event"] == "run_aborted" and e.get("reason") == "kill switch"
               for e in adw.Journal(run_path).events)


def test_cost_ceiling_stops_before_committing_to_another_node(root, monkeypatch):
    """A budget checked only after the fact is an epitaph, not a budget.

    The ceiling is enforced at NODE BOUNDARIES against MEASURED spend, so a single
    node may overshoot it — the runner refuses to start the next one, it does not
    predict what the next one would cost. Estimating that is guesswork; refusing to
    continue on measured evidence is not."""
    write_spec(root, "pricey", """
[adw]
name = "pricey"
entry = "a"
budget_usd = 0.5
[node.a]
type = "agent"
driver = "claude"
prompt = "spend"
allowed_tools = ["Read"]
on_pass = "b"
[node.b]
type = "agent"
driver = "claude"
prompt = "spend more"
allowed_tools = ["Read"]
on_pass = "__end__"
""")

    def pricey_agent(spec, node, journal, prompt, record, cwd=None, results=None, variables=None):
        return adw.ExecResult(exit_code=0, output=f"ran {node.name}", cost_usd=0.75)

    monkeypatch.setattr(adw, "_agent_claude", pricey_agent)
    outcome = adw.execute(adw.load_spec(root, "pricey"), root)
    assert outcome.status == "aborted"
    assert [s["node"] for s in outcome.steps] == ["a"], "node b must never be started"
    aborted = [e for e in adw.Journal(adw.runs_dir(root) / outcome.run_id).events
               if e["event"] == "run_aborted"]
    assert aborted and aborted[0]["reason"] == "budget exhausted"
    assert aborted[0]["spent_usd"] == pytest.approx(0.75)


# ── B5: the critic panel — distinct lenses, blind, quorum counted by code ─────


def test_panel_of_identical_critics_is_rejected(root):
    """N reviewers sharing one lens re-derive one opinion N times and bill N times."""
    write_spec(root, "echo", """
[adw]
name = "echo"
entry = "p"
[node.p]
type = "parallel"
branches = ["c1", "c2"]
merge = "concat"
on_branch_fail = "ignore"
max_branches = 2
on_pass = "__end__"
[node.c1]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
[node.c1.persona]
role = "critic-a"
stance = "reject_by_default"
lens = "correctness"
emits = "VERDICT=<v>"
[node.c2]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
[node.c2.persona]
role = "critic-b"
stance = "reject_by_default"
lens = "correctness"
emits = "VERDICT=<v>"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "echo"))
    assert any("multiply cost, not coverage" in e for e in errors), errors


def test_a_narrated_success_survives_a_panel_that_claims_nothing(root):
    """Law L3 regression. A critic panel narrates nothing about the work it judges,
    so treating the block like an agent erased the worker's own success claim and
    the rejection that followed stopped counting as a divergence."""
    write_spec(root, "claim", """
[adw]
name = "claim"
entry = "worker"
[node.worker]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
on_pass = "panel"
on_fail = "__fail__"
[node.panel]
type = "parallel"
branches = ["critic"]
merge = "concat"
on_branch_fail = "ignore"
max_branches = 2
on_pass = "quorum"
on_fail = "quorum"
[node.critic]
type = "agent"
driver = "mock"
allowed_tools = ["Read"]
[node.quorum]
type = "gate"
command = ["bash", "-c", "exit 1"]
idempotent = true
on_pass = "__end__"
on_fail = "__fail__"
""")
    write_recording(root, "claim", "worker", "Implemented it. All done ✅")
    write_recording(root, "claim", "critic", "VERDICT=REJECT + REASON=the test never runs")
    outcome = adw.execute(adw.load_spec(root, "claim"), root, force_mock=True)
    assert outcome.class_d is True, (
        "the worker claimed success and the verification rejected it — that is the divergence")


def test_shipped_critic_panel_composes_lints_and_reaches_a_quorum(root, tmp_path):
    """The kit's flagship piece, exercised as a consumer would use it: composed into
    a flow, linted, then run with mocked critics so the quorum arithmetic is real."""
    if not (adw.library_dir() / "fragments" / "critic-panel.toml").is_file():
        pytest.skip("critic-panel fragment not installed")
    write_spec(root, "gauntlet", """
[adw]
name = "gauntlet"
entry = "build"
[node.build]
type = "agent"
driver = "mock"
# A builder that can only Read is a fixture lying about itself, and the
# dead-node lint said so on its first run: with no write tool its whole product
# is its summary, which nothing here reads. The critics judge the ARTIFACT, so
# the real edge runs through the filesystem.
allowed_tools = ["Read", "Edit", "Write"]
prompt = "Build it. Panel verdict: {{nodes.judge.quorum.summary}}"
on_pass = "judge"
on_fail = "__fail__"
[[use]]
module = "critic-panel"
as = "judge"
with = { artifact = "the diff", bar = "the named test passes", quorum = "2" }
on_pass = "__end__"
on_fail = "__fail__"
""")
    spec = adw.load_spec(root, "gauntlet")
    assert adw.lint_spec(spec) == ([], []), adw.lint_spec(spec)
    assert {"judge.panel", "judge.correctness", "judge.security",
            "judge.reproducibility", "judge.quorum"} <= set(spec.nodes)
    assert spec.node("judge.panel").raw["branches"] == [
        "judge.correctness", "judge.security", "judge.reproducibility"], \
        "a fragment's fan-out must name its OWN branches, not the host's"

    write_recording(root, "gauntlet", "build", "Built it, all complete ✅")
    for node, verdict in (("judge.correctness", "REJECT"), ("judge.security", "REJECT"),
                          ("judge.reproducibility", "PASS")):
        write_recording(root, "gauntlet", node, f"reviewed\nVERDICT={verdict} + REASON=because")
    outcome = adw.execute(spec, root, force_mock=True)
    assert outcome.status == "failed", "2 rejections against a quorum of 2 must not pass"
    quorum = [s for s in outcome.steps if s["node"] == "judge.quorum"][0]
    assert quorum["contract_verdict"] == adw.REJECT
    assert outcome.class_d is True, "the builder claimed success; the panel disagreed"


def test_panel_escalates_when_no_critic_returns_a_parseable_verdict(root):
    """Three critics, three piles of prose, zero verdicts: that is not consensus to
    ship — it is a panel that could not decide, and it must say so."""
    if not (adw.library_dir() / "fragments" / "critic-panel.toml").is_file():
        pytest.skip("critic-panel fragment not installed")
    write_spec(root, "mute", """
[adw]
name = "mute"
entry = "judge"
[[use]]
module = "critic-panel"
as = "judge"
with = { artifact = "the diff", bar = "the named test passes", quorum = "2" }
on_pass = "__end__"
on_fail = "__fail__"
""")
    spec = adw.load_spec(root, "mute")
    for node in ("judge.correctness", "judge.security", "judge.reproducibility"):
        write_recording(root, "mute", node, "I had a look and it seems fine to me overall.")
    outcome = adw.execute(spec, root, force_mock=True)
    quorum = [s for s in outcome.steps if s["node"] == "judge.quorum"][0]
    assert quorum["contract_verdict"] == adw.ESCALATE, quorum


# ── B2: `adw new` — guided creation, born verified ───────────────────────────


@pytest.fixture()
def no_portfolio(monkeypatch):
    """The portfolio probe is a live daemon call; tests state their own prior art."""
    monkeypatch.setattr(adw, "prior_art", lambda intent: f"(stub prior art for {intent!r})")


def _new(root: Path, name: str, *argv: str) -> tuple[int, dict]:
    import io, contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        code = adw.main(["--root", str(root), "new", name, *argv])
    return code, json.loads(buf.getvalue())


BASE = ("--intent", "fix a rust bug with institutional memory and a verified gate",
        "--when-not-to-use", "not for production incidents — use hotfix")


def test_creation_refuses_without_a_prior_art_verdict(root, no_portfolio):
    code, out = _new(root, "f1", *BASE, "--job", "fix:agent")
    assert code == 1 and out["created"] is False
    assert "--verdict must be one of" in out["error"]
    assert out["prior_art"], "the refusal must SHOW the prior art it wants judged"


def test_creation_refuses_when_the_verdict_is_reuse(root, no_portfolio):
    code, out = _new(root, "f2", *BASE, "--verdict", "reuse", "--job", "fix:agent")
    assert code == 1 and out["created"] is False and out["verdict"] == "reuse"
    assert not (adw.adw_dir(root) / "f2.toml").exists()


def test_creation_requires_saying_when_the_flow_is_the_wrong_tool(root, no_portfolio):
    code, out = _new(root, "f3", "--intent", "something", "--verdict", "create_new",
                     "--job", "fix:agent")
    assert code == 1 and "--when-not-to-use is required" in out["error"]


def test_creation_requires_a_destination(root, no_portfolio):
    code, out = _new(root, "f4", "--verdict", "create_new", "--job", "fix:agent",
                     "--when-not-to-use", "x")
    assert code == 1 and "--intent is required" in out["error"]


def test_a_created_flow_lints_clean_with_no_warnings(root, no_portfolio):
    code, out = _new(root, "rustfix", *BASE, "--verdict", "create_new",
                     "--use", "recall-pack:recall", "--job", "fix:agent",
                     "--use", "gate-rust:check")
    assert code == 0 and out["created"] is True, out
    assert out["lint_warnings"] == [], out["lint_warnings"]
    spec = adw.load_spec(root, "rustfix")
    assert adw.lint_spec(spec) == ([], [])


def test_step_order_follows_the_command_line_not_the_option_kind(root, no_portfolio):
    """Regression: argparse groups by option, so `--use A --job B --use C` became
    A → C → B — the gate ran before the work it verifies."""
    _new(root, "ordered", *BASE, "--verdict", "create_new",
         "--use", "recall-pack:recall", "--job", "fix:agent", "--use", "gate-rust:check")
    spec = adw.load_spec(root, "ordered")
    assert spec.entry == "recall.memory"
    assert spec.node("recall.memory").on_pass == "fix"
    assert spec.node("fix").on_pass == "check.rust_gate"
    assert spec.node("check.rust_gate").on_pass == "__end__"


def test_a_created_flow_wires_its_own_feedback_and_reads_the_verdict(root, no_portfolio):
    """A creation path that emits specs the lint then complains about teaches the
    author only to ignore warnings."""
    _new(root, "fed", *BASE, "--verdict", "create_new",
         "--use", "recall-pack:recall", "--job", "fix:agent", "--use", "gate-rust:check")
    spec = adw.load_spec(root, "fed")
    assert spec.node("check.rust_gate").on_fail == "fix", "the gate must hand work back"
    assert "{{nodes.check.rust_gate.summary}}" in spec.node("fix").raw["prompt"]
    _, warnings = adw.lint_spec(spec)
    assert not any("blind to the verdict" in w for w in warnings)


def test_a_created_flow_declares_its_purpose(root, no_portfolio):
    """Findable by INTENT rather than by filename — including when NOT to use it."""
    _new(root, "purposeful", *BASE, "--verdict", "create_new", "--job", "fix:agent",
         "--tag", "#kind:flow", "--produces", "a verified rust change")
    data = tomllib.loads((adw.adw_dir(root) / "purposeful.toml").read_text(encoding="utf-8"))
    purpose = data["purpose"]
    assert purpose["intent"] and purpose["when_not_to_use"]
    assert purpose["prior_art_verdict"] == "create_new"
    assert purpose["tags"] == ["#kind:flow"]


def test_fragment_inputs_bind_to_run_vars_by_default(root, no_portfolio):
    """A created flow must be runnable as generated: every fragment input arrives
    from a --var, so there is nothing to hand-edit before the first run."""
    _new(root, "bound", *BASE, "--verdict", "create_new", "--use", "recall-pack:recall")
    spec = adw.load_spec(root, "bound")
    command = " ".join(str(c) for c in spec.node("recall.memory").raw["command"])
    assert "{{vars.topic}}" in command and "{{vars.area}}" in command
    assert "{{inputs." not in command


def test_explicit_binds_override_the_defaults(root, no_portfolio):
    _new(root, "custom", *BASE, "--verdict", "create_new", "--use", "recall-pack:recall",
         "--bind", "recall.topic={{vars.symptom}}")
    command = " ".join(str(c) for c in adw.load_spec(root, "custom").node("recall.memory").raw["command"])
    assert "{{vars.symptom}}" in command and "{{vars.topic}}" not in command


def test_a_created_flow_runs_end_to_end_once_its_agent_is_recorded(root, no_portfolio):
    """The full gate: created, linted, and executed without editing a line of TOML."""
    _new(root, "runnable", *BASE, "--verdict", "create_new",
         "--job", "fix:agent", "--job", "verify:gate")
    write_recording(root, "runnable", "fix", "applied the change")
    spec = adw.load_spec(root, "runnable")
    outcome = adw.execute(spec, root, force_mock=True,
                          variables={"verify_cmd": "true"})
    assert outcome.status == "completed", outcome.steps
    assert [s["node"] for s in outcome.steps] == ["fix", "verify"]


def test_creating_over_an_existing_flow_is_refused(root, no_portfolio):
    _new(root, "dup", *BASE, "--verdict", "create_new", "--job", "fix:agent")
    code, out = _new(root, "dup", *BASE, "--verdict", "create_new", "--job", "fix:agent")
    assert code == 1 and "already exists" in out["error"]


def test_a_flow_that_would_not_lint_is_never_left_on_disk(root, no_portfolio):
    """Half-created is worse than not created: the next author inherits a spec that
    looks official and does not work."""
    code, out = _new(root, "broken", *BASE, "--verdict", "create_new",
                     "--use", "recall-pack:recall", "--order", "nonexistent")
    assert code == 1 and out["created"] is False
    assert not (adw.adw_dir(root) / "broken.toml").exists()


# ── `adw test` walks a graph that was never run ──────────────────────────────
#
# The B2 contract is that a created flow "passes lint and test without manual
# editing". Lint passed; test could not: `adw test` replays agents from
# recordings, a new flow has none, so every agent node failed with `no recording`
# — and a human gate paused the walk on top of that. The gate was unreachable by
# construction for exactly the flows the command exists to produce.


HUMAN_GATE = """
[adw]
name = "{name}"
entry = "ask"
[node.ask]
type = "human"
message = "approve?"
on_pass = "__end__"
on_fail = "__fail__"
"""


def test_a_created_flow_passes_lint_and_the_mocked_edge_test(root, no_portfolio):
    """The B2 gate, as worded: born lint-clean AND testable, with no hand-editing.

    Composed from the shipped kit so the walk crosses a fan-out and a human gate —
    the two shapes that used to make the gate unreachable."""
    if not (adw.library_dir() / "fragments" / "critic-panel.toml").is_file():
        pytest.skip("fragment kit not installed")
    _new(root, "walkable", *BASE, "--verdict", "create_new",
         "--use", "recall-pack:recall", "--job", "plan:agent",
         "--use", "critic-panel:panel", "--use", "human-approve:sign")
    spec = adw.load_spec(root, "walkable")
    errors, _ = adw.lint_spec(spec)
    assert errors == [], f"a created flow must lint clean: {errors}"

    outcome = adw.execute(spec, root, force_mock=True)
    assert outcome.status == "completed", (
        f"`adw test` must walk a flow that never ran: {outcome.steps}")
    walked = {step["node"] for step in outcome.steps}
    assert "panel.panel" in walked and "sign.approve" in walked, (
        "the walk must cross the fan-out and the human gate, not stop at them")


def test_a_synthesized_walk_is_named_never_silent(root, no_portfolio):
    """A walk with nothing to replay is not evidence the agents behave.

    Reporting it as an ordinary pass would be the failure the sources name:
    treating possible output as proved output."""
    _new(root, "named", *BASE, "--verdict", "create_new", "--job", "worker:agent")
    outcome = adw.execute(adw.load_spec(root, "named"), root, force_mock=True)
    assert outcome.status == "completed"
    assert "worker" in outcome.synthesized, (
        "an agent walked without a recording must be named in the report")


def test_a_declared_mock_driver_still_demands_its_recording(root):
    """Synthesis belongs to `adw test` alone.

    A spec that DECLARES driver = "mock" is asking for one specific recording;
    inventing it would turn an explicit contract into a guess."""
    (adw.adw_dir(root) / "declared.toml").write_text("""
[adw]
name = "declared"
entry = "a"
[node.a]
type = "agent"
driver = "mock"
prompt = "x"
on_pass = "__end__"
on_fail = "__fail__"
""", encoding="utf-8")
    outcome = adw.execute(adw.load_spec(root, "declared"), root)
    assert outcome.status == "failed", "a declared mock with no recording must fail"
    assert outcome.synthesized == [], "a real run never synthesizes"


def test_a_synthesized_stub_never_narrates_success(root, no_portfolio):
    """Law L3 depends on narration being the agent's own claim.

    A stub that said "done" would manufacture the Class-D divergence the runner
    exists to detect — a false positive in the one signal that must stay earned."""
    _new(root, "quiet", *BASE, "--verdict", "create_new", "--job", "worker:agent")
    stub = adw._synthesized_mock(adw.load_spec(root, "quiet").node("worker"))
    assert stub.synthesized and stub.exit_code == 0
    assert not stub.narrated_success
    assert not adw.SUCCESS_NARRATIVE.search(stub.output), (
        f"the stub narrates success: {stub.output!r}")


def test_a_critic_stub_still_emits_the_verdict_its_gate_parses(root):
    """A stub that emitted nothing would stall every panel behind an ESCALATE and
    hide the rest of the graph — the part the edge test is there to check."""
    (adw.adw_dir(root) / "judged.toml").write_text("""
[adw]
name = "judged"
entry = "critic"
[node.critic]
type = "agent"
prompt = "judge"
session = "fresh"
on_pass = "__end__"
on_fail = "__fail__"
[node.critic.persona]
role = "critic"
stance = "reject_by_default"
lens = "correctness"
bar = "the bar"
burden = "prove it"
emits = "VERDICT=PASS|REJECT|ESCALATE"
""", encoding="utf-8")
    stub = adw._synthesized_mock(adw.load_spec(root, "judged").node("critic"))
    assert adw.parse_verdict(stub.output, stub.exit_code) == adw.PASS
    assert "SYNTHESIZED" in stub.output, "the verdict must carry that it was invented"


def test_a_human_gate_pauses_a_real_run_but_not_the_edge_test(root):
    """Both directions, because only the pair proves the fix did not disable the
    gate: a real run still waits for a person."""
    (adw.adw_dir(root) / "gated.toml").write_text(
        HUMAN_GATE.format(name="gated"), encoding="utf-8")
    spec = adw.load_spec(root, "gated")
    assert adw.execute(spec, root).status == "waiting_human"
    assert adw.execute(spec, root, force_mock=True).status == "completed"


# ── [purpose]: the portfolio's half of the contract ──────────────────────────
#
# C1 taught the miner to index a [purpose] block and `adw new` to demand one —
# and then shipped eight flows that had none. The consumer existed, the producer
# did not, so `when_not_to_use` (declared mandatory, and the only thing stopping
# the portfolio from pitching every candidate) was in effect for zero flows.


def test_every_shipped_flow_declares_what_it_is_for_and_when_not_to_use_it():
    """The portfolio is only honest if every entry can rule itself out."""
    lib = adw.library_dir()
    if not lib.is_dir():
        pytest.skip("library not installed")
    flows = [p for p in sorted(lib.glob("*.toml")) if p.stem != "tiers"]
    assert flows, "the library is empty"
    for path in flows:
        purpose = tomllib.loads(path.read_text(encoding="utf-8")).get("purpose") or {}
        assert purpose.get("intent"), f"{path.stem}: no [purpose].intent — findable by filename only"
        assert purpose.get("when_not_to_use"), (
            f"{path.stem}: no `when_not_to_use` — a portfolio entry that cannot rule "
            f"itself out can only be advertised")
        assert purpose.get("when_to_use"), f"{path.stem}: no `when_to_use`"


def test_a_private_flow_without_a_purpose_is_not_nagged(root):
    """The nudge is aimed, not broadcast.

    A one-off flow has nothing to be findable FOR, and warning on every one of
    them teaches the author to skim past lint output — the same reflex
    `test_a_created_flow_wires_its_own_feedback_and_reads_the_verdict` guards
    against. Presence is enforced where it buys something: the shipped library
    (by test) and every flow `adw new` creates (by refusal)."""
    (adw.adw_dir(root) / "anon.toml").write_text("""
[adw]
name = "anon"
entry = "a"
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "__end__"
on_fail = "__fail__"
""", encoding="utf-8")
    errors, warnings = adw.lint_spec(adw.load_spec(root, "anon"))
    assert errors == [], "a missing purpose must never block a run"
    assert not any("purpose" in w for w in warnings), warnings


def test_a_purpose_that_cannot_rule_itself_out_is_flagged(root):
    """The specific field, specifically checked: an `intent` with no boundary is
    the advertising failure the block exists to avoid."""
    (adw.adw_dir(root) / "pitch.toml").write_text("""
[adw]
name = "pitch"
entry = "a"
[purpose]
intent = "does everything you could possibly want"
when_to_use = ["always"]
[node.a]
type = "code"
command = ["true"]
idempotent = true
on_pass = "__end__"
on_fail = "__fail__"
""", encoding="utf-8")
    _, warnings = adw.lint_spec(adw.load_spec(root, "pitch"))
    assert any("when_not_to_use" in w for w in warnings), warnings


# ── dynamic fan-out: the branch COUNT decided at runtime ─────────────────────
#
# The static form names its branches, so the author fixes the count. The sources
# specify the other shape — LangGraph's `Send`, and what the gauntlet actually
# does: N critics from runtime input. Without it `max_branches` guards a number
# the lint could already count, and the canonical orchestrator-workers pattern
# (subtasks decided by the run, not the author) has no expression at all.
# Writing one used to shred the template string into one branch per CHARACTER.


DYNAMIC = """
[adw]
name = "{name}"
entry = "fan"
[node.fan]
type = "parallel"
branches = "{{{{vars.lenses}}}}"
template = "probe"
merge = "concat"
on_branch_fail = "{policy}"
max_branches = {cap}
on_pass = "__end__"
on_fail = "__fail__"
[node.probe]
type = "code"
command = ["bash", "-c", "test \\"$1\\" != fail", "--", "{{{{branch.value}}}}"]
idempotent = true
"""


def _dynamic(root: Path, name: str, policy: str = "all", cap: int = 8):
    write_spec(root, name, DYNAMIC.format(name=name, policy=policy, cap=cap))
    return adw.load_spec(root, name)


def test_a_dynamic_fanout_clones_its_template_once_per_runtime_value(root):
    spec = _dynamic(root, "dyn")
    assert adw.lint_spec(spec)[0] == [], "a dynamic fan-out must lint"
    outcome = adw.execute(spec, root, variables={"lenses": "alfa,beta,gama"})
    assert outcome.status == "completed"
    walked = [s["node"] for s in outcome.steps]
    assert walked == ["probe:alfa", "probe:beta", "probe:gama", "fan"], walked


def test_the_runtime_cap_refuses_the_block_instead_of_truncating(root):
    """`max_branches` is the only thing between a runtime-sized fan-out and an
    unbounded bill. Silently running the first N would bill for work the author
    never authorised AND report success on partial evidence."""
    spec = _dynamic(root, "capped", cap=2)
    outcome = adw.execute(spec, root, variables={"lenses": "a,b,c,d"})
    assert outcome.status == "failed"
    assert [s["node"] for s in outcome.steps] == ["fan"], "no branch may run past the cap"


@pytest.mark.parametrize(
    "policy,lenses,expected",
    [
        ("quorum:2", "ok,fail,ok2", "completed"),   # 2 of 3 passed — quorum met
        ("quorum:3", "ok,fail,ok2", "failed"),      # 2 of 3 passed — quorum missed
        ("all", "ok,fail", "failed"),
        ("any", "ok,fail", "completed"),
        ("best_effort", "fail,fail", "completed"),  # the sources' name for `ignore`
        ("ignore", "fail,fail", "completed"),
    ],
)
def test_branch_fail_policies_decide_the_block(root, policy, lenses, expected):
    """`quorum:N` is the case `all`/`any` cannot express: "enough branches ran
    clean". The downstream verdict gate answers a different question — what the
    critics CONCLUDED — so neither substitutes for the other."""
    spec = _dynamic(root, f"pol{policy.replace(':', '')}", policy=policy)
    outcome = adw.execute(spec, root, variables={"lenses": lenses})
    assert outcome.status == expected, [s.get("verdict") for s in outcome.steps]


def test_an_unreachable_quorum_is_refused_at_lint(root):
    """A quorum larger than the branch count is a gate that can only ever fail."""
    write_spec(root, "impossible", """
[adw]
name = "impossible"
entry = "fan"
[node.fan]
type = "parallel"
branches = ["a", "b"]
merge = "concat"
on_branch_fail = "quorum:3"
max_branches = 4
on_pass = "__end__"
on_fail = "__fail__"
[node.a]
type = "code"
command = ["true"]
idempotent = true
[node.b]
type = "code"
command = ["true"]
idempotent = true
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "impossible"))
    assert any("unsatisfiable" in e for e in errors), errors


def test_the_third_field_for_one_decision_is_refused(root):
    """The cartografia carries both `policy` and `on_branch_fail`, and its own
    example pairs policy = "quorum:2" with on_branch_fail = "all" — two sources of
    truth for one decision, already contradicting each other on the page. Accepting
    both would ship that contradiction; the lint names the field that owns it."""
    spec_text = DYNAMIC.format(name="two", policy="all", cap=4).replace(
        'merge = "concat"', 'merge = "concat"\npolicy = "quorum:2"')
    write_spec(root, "two", spec_text)
    errors, _ = adw.lint_spec(adw.load_spec(root, "two"))
    assert any("`policy` is not a field" in e for e in errors), errors


def test_n_clones_of_one_fixed_lens_are_refused(root):
    """The distinctness rule in its dynamic form: every branch is a clone of ONE
    node, so a lens that does not vary buys one opinion N times at N times the
    price — the "more agents, more noise" the sources warn against."""
    write_spec(root, "samey", """
[adw]
name = "samey"
entry = "fan"
[node.fan]
type = "parallel"
branches = "{{vars.lenses}}"
template = "critic"
merge = "tally"
on_branch_fail = "ignore"
max_branches = 5
on_pass = "__end__"
on_fail = "__fail__"
[node.critic]
type = "agent"
prompt = "judge"
session = "fresh"
[node.critic.persona]
role = "critic"
stance = "reject_by_default"
lens = "correctness"
bar = "the bar"
burden = "prove it"
emits = "VERDICT=PASS|REJECT|ESCALATE"
""")
    errors, _ = adw.lint_spec(adw.load_spec(root, "samey"))
    assert any("fixed lens" in e for e in errors), errors


def test_a_template_may_only_reference_the_bindings_it_gets(root):
    """An unbound {{branch.x}} would render empty at runtime — a prompt silently
    missing the thing that makes the branch different."""
    spec_text = DYNAMIC.format(name="typo", policy="all", cap=4).replace(
        "{{branch.value}}", "{{branch.lens}}")
    write_spec(root, "typo", spec_text)
    errors, _ = adw.lint_spec(adw.load_spec(root, "typo"))
    assert any("only ['value', 'index']" in e for e in errors), errors


def test_a_dynamic_fanout_survives_fragment_inlining(root):
    """Regression, 2026-08-19: the inliner iterated `branches` as a list, so a
    composed dynamic fan-out was namespaced one CHARACTER per branch. The template
    is the node reference that DOES need qualifying; the string must be left alone."""
    if not (adw.library_dir() / "fragments" / "fanout-lenses.toml").is_file():
        pytest.skip("fanout-lenses not installed")
    _new(root, "swept", *BASE, "--verdict", "create_new", "--use", "fanout-lenses:sweep")
    spec = adw.load_spec(root, "swept")
    fan = spec.node("sweep.sweep")
    assert isinstance(fan.raw["branches"], str), "the template string must stay a string"
    assert fan.raw["template"] == "sweep.lens", "the template node must be namespaced"
    outcome = adw.execute(spec, root, force_mock=True,
                          variables={"lenses": "a,b", "target": "x"})
    assert outcome.status == "completed", outcome.steps
    assert [s["node"] for s in outcome.steps][:2] == ["sweep.lens:a", "sweep.lens:b"]


def test_the_kit_ships_the_sectioning_piece(root):
    """`fanout-lenses` is named in both documents' ten-piece kit and was the one
    never delivered — which left `parallel`, the most expensive delivery, with a
    single consumer in the kit."""
    frag = adw.library_dir() / "fragments" / "fanout-lenses.toml"
    if not frag.is_file():
        pytest.skip("library not installed")
    data = tomllib.loads(frag.read_text(encoding="utf-8"))
    sweep = data["node"]["sweep"]
    assert isinstance(sweep["branches"], str), "sectioning here is the DYNAMIC form"
    assert sweep["template"] == "lens" and sweep["merge"] == "concat"


# ─────────────────────────────────────────────────────────────────────────────
# B4 — skill binding: the "O" of TACO (2026-08-20)
# ─────────────────────────────────────────────────────────────────────────────
#
# The persona said WHO acts; nothing said which CRAFT to apply, so every planning
# agent re-derived `taco-planning` and every auditing agent re-derived
# `TACO-cross-audit`. `Skill` appeared ZERO times in the runner and zero times in
# the ten library specs, while `claude -p --allowedTools Skill` was proven to load
# a skill. Both ways a binding can fail are SILENT, so both are lint verdicts.


def _agent(**raw) -> adw.Node:
    raw.setdefault("type", "agent")
    raw.setdefault("prompt", "do the thing")
    return adw.Node(name="n", type="agent", raw=raw)


def _skills_corpus(monkeypatch, tmp_path: Path, *names: str, disabled=()):
    for n in names:
        (tmp_path / n).mkdir(parents=True)
        (tmp_path / n / "SKILL.md").write_text(f"# {n}\n", encoding="utf-8")
    monkeypatch.setattr(adw, "SKILLS_ROOT", tmp_path)
    monkeypatch.setattr(adw, "disabled_skills", lambda: frozenset(disabled))


def test_skill_binding_normalises_string_and_list():
    assert adw.skill_binding(_agent(skill="taco-planning")) == ["taco-planning"]
    assert adw.skill_binding(_agent(skill=["a", "b"])) == ["a", "b"]
    assert adw.skill_binding(_agent()) == []


def test_skill_status_reports_missing(monkeypatch, tmp_path):
    _skills_corpus(monkeypatch, tmp_path, "taco-planning")
    assert adw.skill_status("taco-planning") == "ok"
    assert adw.skill_status("taco-planing") == "missing"      # a typo is a ghost


def test_skill_status_reports_disabled(monkeypatch, tmp_path):
    _skills_corpus(monkeypatch, tmp_path, "touring-query", disabled=["touring-query"])
    assert adw.skill_status("touring-query") == "disabled"


def test_an_uninspectable_corpus_never_accuses(monkeypatch, tmp_path):
    monkeypatch.setattr(adw, "SKILLS_ROOT", tmp_path / "nope")
    assert adw.skill_status("anything") == "ok"


def test_disabled_skills_reads_the_real_override_vocabulary(monkeypatch, tmp_path):
    settings = tmp_path / ".claude"
    settings.mkdir()
    (settings / "settings.json").write_text(
        json.dumps({"skillOverrides": {"a": "off", "b": "on", "c": "disabled"}}), encoding="utf-8")
    monkeypatch.setattr(adw.Path, "home", staticmethod(lambda: tmp_path))
    assert adw.disabled_skills() == frozenset({"a", "c"})


def test_unreadable_settings_disable_nothing(monkeypatch, tmp_path):
    monkeypatch.setattr(adw.Path, "home", staticmethod(lambda: tmp_path / "gone"))
    assert adw.disabled_skills() == frozenset()


# ── the lint: both silent failures fail HERE, not at run time ────────────────

def _lint_agent(node: adw.Node) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    warnings: list[str] = []
    adw._lint_node(node, {"n"}, errors, warnings)
    return errors, warnings


def test_a_missing_skill_is_a_lint_error(monkeypatch, tmp_path):
    _skills_corpus(monkeypatch, tmp_path, "taco-planning")
    errors, _ = _lint_agent(_agent(skill="taco-planing", allowed_tools=["Read"]))
    assert any("does not exist" in e for e in errors)


def test_a_disabled_skill_is_a_lint_error(monkeypatch, tmp_path):
    """It would call Skill, receive nothing, and read exactly like success."""
    _skills_corpus(monkeypatch, tmp_path, "touring-query", disabled=["touring-query"])
    errors, _ = _lint_agent(_agent(skill="touring-query", allowed_tools=["Read"]))
    assert any("skillOverrides" in e for e in errors)


def test_a_live_skill_lints_clean(monkeypatch, tmp_path):
    _skills_corpus(monkeypatch, tmp_path, "taco-planning")
    errors, _ = _lint_agent(_agent(skill="taco-planning", allowed_tools=["Read"]))
    assert errors == []


def test_readonly_promise_a_skill_cannot_keep_is_warned(monkeypatch, tmp_path):
    _skills_corpus(monkeypatch, tmp_path, "taco-planning")
    _, warnings = _lint_agent(
        _agent(skill="taco-planning", allowed_tools=["Read"], readonly=True))
    assert any("readonly=true" in w for w in warnings)


# ── the invocation: the tool the instruction needs must be there ─────────────

def test_skill_tool_is_injected_into_allowed_tools():
    assert adw.skill_tools(_agent(skill="x", allowed_tools=["Read"])) == ["Read", "Skill"]


def test_skill_tool_is_not_duplicated():
    assert adw.skill_tools(_agent(skill="x", allowed_tools=["Read", "Skill"])) == ["Read", "Skill"]


def test_an_unbound_node_keeps_its_tools_untouched():
    assert adw.skill_tools(_agent(allowed_tools=["Read", "Bash"])) == ["Read", "Bash"]


def test_the_command_carries_both_the_preamble_and_the_tool(tmp_path):
    node = _agent(skill="taco-planning", allowed_tools=["Read"], prompt="Plan the feature.")
    cmd = adw._claude_cmd(node, journal=None, prompt="Plan the feature.")
    assert "taco-planning" in cmd[2] and cmd[2].endswith("Plan the feature.")
    assert cmd[cmd.index("--allowedTools") + 1:] [:2] == ["Read", "Skill"]


def test_an_unbound_node_produces_the_same_command_as_before():
    """The unbound path must not move — every existing spec runs through it."""
    node = _agent(allowed_tools=["Read"], prompt="Do it.")
    cmd = adw._claude_cmd(node, journal=None, prompt="Do it.")
    assert cmd[2] == "Do it."
    assert "Skill" not in cmd


def test_the_preamble_is_deterministic():
    node = _agent(skill=["a", "b"])
    assert adw.skill_prompt_prefix(node) == adw.skill_prompt_prefix(node)
    assert adw.skill_prompt_prefix(node).count("SKILL:") == 2


def test_a_bound_node_is_never_provably_readonly(monkeypatch, tmp_path):
    """A skill may do anything the session can — `None` is the honest answer."""
    assert adw.node_readonly(_agent(skill="x", allowed_tools=["Read", "Grep"])) is None
    assert adw.node_readonly(_agent(allowed_tools=["Read", "Grep"])) is True


def test_the_flat_graph_shows_the_tools_that_will_actually_be_passed(root):
    """P4: the flat graph is what RUNS. Rendering the declared list while the
    invocation appends `Skill` would make it lie about its one promise."""
    write_spec(root, "g", """
[adw]
name = "g"
entry = "a"
[node.a]
type = "agent"
skill = "taco-planning"
prompt = "x"
allowed_tools = ["Read"]
""")
    graph = adw.flat_graph(adw.load_spec(root, "g"))
    assert graph["nodes"]["a"]["allowed_tools"] == ["Read", "Skill"]


def test_an_unbound_node_renders_exactly_as_before(root):
    write_spec(root, "g", """
[adw]
name = "g"
entry = "a"
[node.a]
type = "agent"
prompt = "x"
allowed_tools = ["Read"]
""")
    graph = adw.flat_graph(adw.load_spec(root, "g"))
    assert graph["nodes"]["a"]["allowed_tools"] == ["Read"]


# ── E4 campaign (2026-08-24) — flow-level loop with a CODE predicate ─────────

CAMPAIGN_SPEC = """
[adw]
name = "camp"
description = "trivial flow for campaign tests"
entry = "work"

[node.work]
type = "code"
command = ["bash", "-c", "echo did-something"]
on_pass = "__end__"
on_fail = "__fail__"
"""


def _campaign(root, until, extra=None, monkeypatch=None):
    """Run `adw campaign camp` capturing the JSON result (stdout)."""
    import io
    from contextlib import redirect_stdout
    if monkeypatch is not None:
        # The pheromone/memory deposits shell out to `touring`; tests must not
        # depend on a live daemon — neutralize the external calls only.
        real_run = adw.subprocess.run

        def fake_run(cmd, **kw):
            if cmd and cmd[0] == "touring":
                class R:  # noqa: N801 — minimal stand-in
                    returncode = 0
                    stdout = ""
                    stderr = ""
                return R()
            return real_run(cmd, **kw)

        monkeypatch.setattr(adw.subprocess, "run", fake_run)
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = adw.main(["--root", str(root), "campaign", "camp",
                       "--until", until, *(extra or [])])
    return rc, json.loads(buf.getvalue())


def test_campaign_converges_when_predicate_exits_zero(root, monkeypatch):
    write_spec(root, "camp", CAMPAIGN_SPEC)
    marker = root / "count.txt"
    # Predicate: converge on the 2nd round; emits a metric both rounds.
    until = (f"n=$(cat {marker} 2>/dev/null || echo 0); n=$((n+1)); echo $n > {marker}; "
             f"echo METRIC=0.$n; [ $n -ge 2 ]")
    rc, out = _campaign(root, until, monkeypatch=monkeypatch)
    assert rc == 0
    assert out["status"] == "converged"
    assert out["rounds"] == 2
    assert [c["metric"] for c in out["curve"]] == [0.1, 0.2]


def test_campaign_stops_at_max_rounds_when_never_converged(root, monkeypatch):
    write_spec(root, "camp", CAMPAIGN_SPEC)
    rc, out = _campaign(root, "echo METRIC=0.5; false",
                        extra=["--max-rounds", "3"], monkeypatch=monkeypatch)
    assert rc == 1
    assert out["status"] == "max_rounds"
    assert out["rounds"] == 3


def test_campaign_absent_metric_is_unknown_never_stagnation(root, monkeypatch):
    """FAIL-CLOSED: a predicate that emits NO metric must not trip stagnation —
    the same reading the loop node applies to an absent NEW_FINDINGS."""
    write_spec(root, "camp", CAMPAIGN_SPEC)
    rc, out = _campaign(root, "false",
                        extra=["--max-rounds", "4", "--stagnation-rounds", "2"],
                        monkeypatch=monkeypatch)
    assert out["status"] == "max_rounds", "absent metric must never count as stagnation"
    assert all(c["metric_signal"] == "absent" for c in out["curve"])


def test_campaign_stagnates_on_repeated_present_metric(root, monkeypatch):
    write_spec(root, "camp", CAMPAIGN_SPEC)
    rc, out = _campaign(root, "echo METRIC=0.42; false",
                        extra=["--max-rounds", "6", "--stagnation-rounds", "2"],
                        monkeypatch=monkeypatch)
    assert rc == 1
    assert out["status"] == "stagnation"
    assert out["rounds"] < 6


def test_campaign_flow_failure_stops_immediately(root, monkeypatch):
    write_spec(root, "camp", """
[adw]
name = "camp"
description = "failing flow"
entry = "work"

[node.work]
type = "code"
command = ["bash", "-c", "exit 7"]
on_pass = "__end__"
on_fail = "__fail__"
""")
    rc, out = _campaign(root, "true", monkeypatch=monkeypatch)
    assert rc == 1
    assert out["status"] == "flow_failed"
    assert out["rounds"] == 1


# ── sandbox: o envelope do `touring run` não pode engolir os marcadores ──────

def test_sandbox_output_is_unwrapped_so_loop_markers_still_match():
    """Sob `sandbox = true` o comando vira `touring run`, cuja saída é um JSON.

    Os três contratos do runner (`NEW_FINDINGS_RE`, `METRIC_RE`, `VERDICT_RE`)
    casam `^MARCADOR=` com MULTILINE. Dentro do envelope a linha vira
    `  "stdout": "NEW_FINDINGS=5\\n",` e nenhum casa — falha que não levanta
    erro nenhum: marcador ausente é *unknown*, então o loop exauriria
    `max_iters` em vez de convergir, e o gate leria REJECT. Medido em
    24/08/2026, quando 0 de 24 nós usavam sandbox e o caminho nunca fora
    exercido.
    """
    envelope = json.dumps({
        "stdout": "trabalhando\nNEW_FINDINGS=5\n",
        "stderr": "",
        "exit_code": 0,
    })
    # O envelope cru NÃO casa — é este o defeito que o desembrulho remove.
    assert not adw.NEW_FINDINGS_RE.search(envelope)

    desembrulhado = adw._unwrap_sandbox_output(envelope, "")
    achado = adw.NEW_FINDINGS_RE.search(desembrulhado)
    assert achado is not None, f"marcador perdido no desembrulho: {desembrulhado!r}"
    assert achado.group(1) == "5"


def test_sandbox_unwrap_carries_the_spill_locator():
    """Houve spill? O nó recebe ONDE está a saída completa, não um corte.

    É o ganho da W1 que o `head -c` das specs jogava fora: cortar cega perde o
    resto, enquanto o locator o mantém alcançável.
    """
    envelope = json.dumps({
        "stdout": "inicio da saida",
        "stderr": "",
        "exit_code": 0,
        "retrieval_hint": "Read /tmp/spill-abc.txt --offset 0 --limit 200",
    })
    saida = adw._unwrap_sandbox_output(envelope, "")
    assert "/tmp/spill-abc.txt" in saida
    assert "inicio da saida" in saida
    # Sem esta asserção o teste passa por ACIDENTE: devolver o envelope cru
    # também contém o path, então ele sobreviveria à remoção do desembrulho —
    # exatamente o falso-verde que a asserção genérica produz.
    assert '"stdout"' not in saida, f"o envelope JSON vazou para o nó: {saida!r}"


def test_sandbox_unwrap_is_fail_open_on_a_broken_envelope():
    """Envelope ilegível devolve o texto cru — perder a saída é pior que ruído."""
    saida = adw._unwrap_sandbox_output("isto nao e json", "aviso no stderr")
    assert "isto nao e json" in saida
    assert "aviso no stderr" in saida
