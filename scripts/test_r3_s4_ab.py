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


def test_rewrite_makes_every_real_path_relative_and_prefixes_the_session():
    out = ab.rewrite(PRE, ROOT)
    assert "/home/gabrielgadea" not in json.dumps(out)                    # no hook may read the live repository or home
    assert out["cwd"] == ROOT and out["transcript_path"] == ROOT + "/t.jsonl" and out["session_id"] == "s4ab-abc"
    cmd = out["tool_input"]["command"]
    assert cmd == "grep -n x crates/a.rs ~/.claude/x home/.claude/rules" and ROOT not in cmd


def test_rewrite_handles_the_bare_forms_without_a_trailing_slash():
    bare = {**PRE, "tool_input": {"command": "cd /home/gabrielgadea/projects/touring && ls /home/gabrielgadea"}}
    out = ab.rewrite(bare, ROOT)
    assert out["tool_input"]["command"] == "cd . && ls home"
    assert ab.rewrite(bare, "/tmp/r3s4/off")["tool_input"] == out["tool_input"]


def test_the_command_text_is_byte_identical_in_every_arm_because_the_pensieve_hashes_it():
    """E9.8-n: a root path inside the command made the arms embed different tokens, so two identical daemons answered
    differently (measured: control 7/20 and 4/20 apart, 0/40 once the command text is the same)."""
    on, off = ab.rewrite(PRE, "/tmp/r3s4/on"), ab.rewrite(PRE, "/tmp/r3s4/off")
    assert on["tool_input"] == off["tool_input"]
    strip = lambda p, r: json.dumps(p).replace(r, "@")  # noqa: E731
    assert strip(on, "/tmp/r3s4/on") == strip(off, "/tmp/r3s4/off")      # only cwd/transcript carry the arm's root


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


def test_only_the_volatile_fields_seen_in_the_control_are_masked_never_a_decision():
    raw = b"x: reliability 64% (7d), velocity 0.0 edits/day | repetido (h\xc3\xa1 9s, 1117 B) | Similar command failed recently (similarity=47%)"
    out = ab.normalise(raw, ROOT)
    assert b"reliability N%" in out and b"velocity V edits/day" in out and b"h\xc3\xa1 Ns" in out
    assert b"similarity=47%" in out and b"1117 B" in out          # what the Pensieve DECIDES and the budget are never masked


def test_the_call_order_rotates_so_no_arm_is_always_first_or_last():
    orders = [ab.order_for(i) for i in range(6)]
    assert all(sorted(o) == sorted(ab.NAMES) for o in orders)
    assert {o[0] for o in orders[:3]} == set(ab.NAMES) and {o[-1] for o in orders[:3]} == set(ab.NAMES)
    assert orders[0] == orders[3]


def test_the_noise_read_compares_the_flag_with_the_daemon_to_daemon_noise_floor():
    quiet = ab.compare_hook(rows([CTX] * 10, [CTX] * 10, [CTX] * 10))
    assert quiet["noise_read"] == "indistinguishable" and quiet["noise_excess"] == 0
    # ON differs from OFF on 6 inputs while the control (OFF x ctl) is clean: the flag EXCEEDS the noise
    loud = ab.compare_hook(rows([b"{}"] * 6 + [CTX] * 4, [CTX] * 10, [CTX] * 10))
    assert loud["noise_read"] == "exceeds_noise" and loud["noise_excess"] == 6
    # the same 6 diffs with a noisy control (OFF x ctl = 6) are indistinguishable from the noise
    noisy = ab.compare_hook(rows([b"{}"] * 6 + [CTX] * 4, [CTX] * 10, [b"{}"] * 6 + [CTX] * 4))
    assert noisy["n_control_diff"] == 6 and noisy["noise_read"] == "indistinguishable"


def test_overall_reports_the_noise_read_and_an_idle_emitter_has_no_signal():
    ok = {h: {"passes": None, "strength": "fraco", "noise_read": "indistinguishable"} for h in ab.HOOKS}
    assert ab.overall(ok, 3)["noise_read"] == "indistinguishable" and ab.overall(ok, 3)["passes"] is None
    assert ab.overall(ok, 0)["noise_read"] == "no_signal_idle_emitter"
    ok["post-bash"]["noise_read"] = "exceeds_noise"
    assert ab.overall(ok, 3)["noise_read"] == "exceeds_noise"


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


def test_divergent_inputs_come_with_their_bytes_so_the_cause_can_be_read():
    r = ab.compare_hook(rows([CTX, b"{}", b"a" * 900], [CTX, b"{}", b"b"], [CTX, b"{}", b"b"]))
    assert [s["input"] for s in r["samples"]] == [2] and r["samples"][0]["on"].startswith("aaa") and len(r["samples"][0]["on"]) == 400
    assert ab.compare_hook(rows([CTX], [CTX], [CTX]))["samples"] == []


def test_a_nondeterministic_control_voids_the_run_even_when_on_equals_off():
    r = ab.compare_hook(rows([CTX, CTX], [CTX, CTX], [CTX, b"{}"]))
    assert r["passes"] is None and r["n_control_diff"] == 1 and "VOID" in r["reason"]


def test_all_silent_outputs_are_annulled_not_a_pass_of_strength():
    r = ab.compare_hook(rows([b"{}"] * 3, [b"{}"] * 3, [b"{}"] * 3))
    assert r["passes"] is True and r["strength"] == "anulado" and r["n_nonempty"] == 0


def per(passes, ne=60):
    return {h: {"passes": passes, "strength": ab.classify(ne), "noise_read": "indistinguishable"} for h in ab.HOOKS}


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
