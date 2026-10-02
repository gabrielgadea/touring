"""Errata 9 — the S4-forte harness: the pure parts (path rewrite, outcome rule, comparison, verdict) are proven without daemons."""
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_event_schema as schema  # noqa: E402
import r3_s4_ab as ab  # noqa: E402

ROOT = "/tmp/r3s4/on"
PRE = {"hook_event_name": "PreToolUse", "tool_name": "Bash", "session_id": "abc",
       "cwd": "/home/gabrielgadea/projects/touring/scripts", "transcript_path": "/home/gabrielgadea/.claude/projects/x/y.jsonl",
       "tool_input": {"command": "grep -n x /home/gabrielgadea/projects/touring/crates/a.rs ~/.claude/x /home/gabrielgadea/.claude/rules"}}


def test_rewrite_moves_every_real_path_inside_the_project_and_prefixes_the_session():
    out = ab.rewrite(PRE, ROOT)
    assert "/home/gabrielgadea" not in json.dumps(out)                    # a pinned daemon refuses a request that names it
    assert out["cwd"] == ROOT and out["transcript_path"] == ROOT + "/t.jsonl" and out["session_id"] == "s4ab-abc"
    assert ROOT + "/crates/a.rs" in out["tool_input"]["command"] and ROOT + "/home/.claude/rules" in out["tool_input"]["command"]


def test_rewrite_is_the_same_rule_for_every_arm_modulo_the_root():
    on, off = ab.rewrite(PRE, "/tmp/r3s4/on"), ab.rewrite(PRE, "/tmp/r3s4/off")
    assert json.dumps(on).replace("/tmp/r3s4/on", "@") == json.dumps(off).replace("/tmp/r3s4/off", "@")


def test_the_outcome_rule_is_deterministic_and_fails_about_a_third_of_the_commands():
    cmds = [f"cmd-{i} --flag" for i in range(600)]
    codes = [ab.exit_code_for(c) for c in cmds]
    assert codes == [ab.exit_code_for(c) for c in cmds] and set(codes) == {0, 1}
    assert 0.25 < sum(codes) / len(codes) < 0.42


def test_post_payload_carries_the_outcome_in_the_shape_post_bash_reads():
    fail = next(c for c in (f"c{i}" for i in range(50)) if ab.exit_code_for(c) == 1)
    ok = next(c for c in (f"c{i}" for i in range(50)) if ab.exit_code_for(c) == 0)
    pf, po = ab.post_payload({**PRE, "tool_input": {"command": fail}}), ab.post_payload({**PRE, "tool_input": {"command": ok}})
    assert pf["tool_response"]["exit_code"] == 1 and pf["tool_response"]["stderr"] and pf["hook_event_name"] == "PostToolUse"
    assert po["tool_response"]["exit_code"] == 0 and po["tool_input"]["command"] == ok


def test_normalise_hides_only_the_project_root():
    assert ab.normalise(b"see /tmp/r3s4/on/x and /tmp/r3s4/off/y", ROOT) == b"see @ROOT@/x and /tmp/r3s4/off/y"


@pytest.mark.parametrize("n,expected", [(0, "anulado"), (1, "fraco"), (49, "fraco"), (50, "forte"), (400, "forte")])
def test_strength_thresholds_are_the_errata_9_ones(n, expected):
    assert ab.classify(n) == expected


def rows(on, off, ctl):
    return [{"on": a, "off": b, "ctl": c} for a, b, c in zip(on, off, ctl)]


CTX = b'{"hookSpecificOutput":{"additionalContext":"x"}}'


def test_identical_arms_with_content_pass_and_count_the_strength():
    r = ab.compare_hook(rows([CTX, b"{}"], [CTX, b"{}"], [CTX, b"{}"]))
    assert r["passes"] is True and r["n_nonempty"] == 1 and r["n_diff"] == 0 and r["strength"] == "fraco"


def test_a_difference_between_on_and_off_fails_and_names_the_first_input():
    r = ab.compare_hook(rows([b"{}", CTX, CTX], [b"{}", b"{}", CTX], [b"{}", b"{}", CTX]))
    assert r["passes"] is False and r["n_diff"] == 1 and r["first_diff_input"] == 1


def test_a_nondeterministic_control_voids_the_run_even_when_on_equals_off():
    r = ab.compare_hook(rows([CTX, CTX], [CTX, CTX], [CTX, b"{}"]))
    assert r["passes"] is None and r["n_control_diff"] == 1 and "VOID" in r["reason"]


def test_all_silent_outputs_are_annulled_not_a_pass_of_strength():
    r = ab.compare_hook(rows([b"{}"] * 3, [b"{}"] * 3, [b"{}"] * 3))
    assert r["passes"] is True and r["strength"] == "anulado" and r["n_nonempty"] == 0


def per(passes, ne=60):
    return {h: {"passes": passes, "strength": ab.classify(ne)} for h in ab.HOOKS}


def test_overall_needs_the_flag_to_have_had_an_effect_in_the_on_arm():
    assert ab.overall(per(True), on_events=3)["passes"] is True
    idle = ab.overall(per(True), on_events=0)
    assert idle["passes"] is None and idle["idle_emitter"] is True        # ON == OFF proves nothing about an idle emitter


def test_overall_a_diff_fails_even_with_an_idle_emitter_and_a_void_control_wins_over_pass():
    assert ab.overall(per(False), on_events=0)["passes"] is False
    mixed = per(True)
    mixed["pre-bash"]["passes"] = None
    assert ab.overall(mixed, on_events=5)["passes"] is None and ab.overall(mixed, on_events=5)["void"] is True


def test_the_real_journal_is_searched_by_the_experiment_session_hashes(tmp_path):
    journal = tmp_path / "r3_events.jsonl"
    sessions = {"s4ab-one", "s4ab-two"}
    journal.write_text(json.dumps({"session_hash": schema.hash_session("s4ab-two")}) + "\n" + json.dumps({"session_hash": "ffffffffffff"}) + "\n")
    assert ab.real_journal_contaminated(sessions, journal) == [schema.hash_session("s4ab-two")]
    assert ab.real_journal_contaminated({"s4ab-clean"}, journal) == []
    assert ab.real_journal_contaminated(sessions, tmp_path / "missing.jsonl") == []


def test_hook_env_drops_the_session_touring_vars_and_pins_the_project(monkeypatch):
    monkeypatch.setenv("TOURING_WORKSPACE_ROOT", "/home/gabrielgadea/projects/touring")
    monkeypatch.setenv("TOURING_PILLAR_INDUCTION_ARMED", "1")
    e = ab.hook_env(ROOT, {"TOURING_R3_EVENTS_FILE": "/tmp/x"})
    assert "TOURING_WORKSPACE_ROOT" not in e and e["TOURING_PILLAR_INDUCTION_ARMED"] == "1"
    assert e["CLAUDE_PROJECT_DIR"] == ROOT and e["TOURING_R3_EVENTS_FILE"] == "/tmp/x"
