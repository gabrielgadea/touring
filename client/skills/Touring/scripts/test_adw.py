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


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
