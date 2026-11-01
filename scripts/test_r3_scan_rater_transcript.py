"""E7.2 — the rater-1 transcript scan decides validity by CODE: each guard is proven by a test that fails without it."""
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_scan_rater_transcript as sc  # noqa: E402

R = "/home/gabrielgadea/.claude/touring/r3-data"
PACK = f"{R}/adj_pack/rater_view/pack_blind.jsonl"
INSTR = f"{R}/adj_pack/rater_view/INSTRUCOES.md"
OUT = f"{R}/adj_out/adj_minimax.jsonl"


def turn(model="MiniMax-M2.7", calls=()):
    content = [{"type": "tool_use", "name": n, "input": i} for n, i in calls]
    return {"type": "assistant", "message": {"model": model, "content": content}}


def write(tmp_path, *entries):
    f = tmp_path / "sub.jsonl"
    f.write_text("\n".join(json.dumps(e) for e in entries) + "\n")
    return f


CLEAN = [("Read", {"file_path": INSTR}), ("Read", {"file_path": PACK}), ("Write", {"file_path": OUT, "content": "x"})]


def test_a_clean_minimax_rater_is_valid(tmp_path):
    out = sc.scan(write(tmp_path, turn(calls=CLEAN)))
    assert out["valid"] is True and out["violations"] == [] and out["n_tool_calls"] == 3


# The oracle is written out here (Errata 7, E7.2), NOT read from `sc.FORBIDDEN`: a test that iterates the constant it
# checks cannot notice a token being dropped from it.
E72_TOKENS = ("key.jsonl", "withheld.json", "adj_claude.jsonl", "a2_offers", "audit_sample", "predictions-a2",
              "results-a2", "r3-replay", "r3_b0", "r3_replay_a2", "r3_arms", "r3_results_gate", "RESULTS.md",
              "CRITERIOS-ACEITACAO", "frozen-verified")


def test_the_module_lists_exactly_the_errata_tokens():
    assert tuple(sc.FORBIDDEN) == E72_TOKENS


@pytest.mark.parametrize("tok", E72_TOKENS)
def test_every_forbidden_token_invalidates_the_rater(tmp_path, tok):
    out = sc.scan(write(tmp_path, turn(calls=CLEAN + [("Bash", {"command": f"cat x/{tok}"})])))
    assert out["valid"] is False and any(v.get("token") == tok for v in out["violations"])


def test_a_home_path_outside_the_three_files_invalidates(tmp_path):
    out = sc.scan(write(tmp_path, turn(calls=CLEAN + [("Read", {"file_path": f"{R}/s3_baseline.json"})])))
    assert out["valid"] is False and any(v["kind"] == "out_of_scope_path" for v in out["violations"])


def test_a_tilde_path_outside_scope_invalidates(tmp_path):
    out = sc.scan(write(tmp_path, turn(calls=[("Bash", {"command": "ls ~/projects/touring/scripts"})])))
    assert out["valid"] is False


def test_a_shell_line_continuation_backslash_is_not_part_of_the_path(tmp_path):
    """Found 02/10 on the real rater-1 transcript: `python3 x.py <pack> \\` + newline made the regex capture
    `<pack>\\` (an 'out of scope' path that was exactly the allowed file). The backslash is shell syntax, not path."""
    cmd = f"cd /tmp && python3 x.py {PACK} \\\n  --flag 1 {PACK}\\\n"
    assert sc.scan(write(tmp_path, turn(calls=CLEAN + [("Bash", {"command": cmd})])))["valid"] is True


def test_the_backslash_fix_does_not_hide_a_real_out_of_scope_path(tmp_path):
    cmd = f"cat {R}/s3_baseline.json \\\n  | head"
    out = sc.scan(write(tmp_path, turn(calls=CLEAN + [("Bash", {"command": cmd})])))
    assert out["valid"] is False and any(v["path"].endswith("s3_baseline.json") for v in out["violations"])


TB_OUT = f"{R}/adj_out/adj_tiebreak.jsonl"


def test_the_tiebreak_judge_runs_on_claude_and_may_write_only_its_own_file(tmp_path):
    """The tie-break judge (Errata 7, E7.5) is a CLEAN-context Claude subagent: the model rule flips and ONE extra
    output file is allowed — nothing else relaxes."""
    calls = [("Read", {"file_path": INSTR}), ("Read", {"file_path": PACK}), ("Write", {"file_path": TB_OUT, "content": "x"})]
    ok = write(tmp_path, turn(model="claude-sonnet-5-5", calls=calls))
    assert sc.scan(ok, model="claude", extra_allowed=("adj_out/adj_tiebreak.jsonl",))["valid"] is True
    # default (MiniMax) rule: a Claude turn and the unlisted output file both invalidate
    bad = sc.scan(ok)
    assert bad["valid"] is False and {v["kind"] for v in bad["violations"]} == {"not_expected_model", "out_of_scope_path"}


def test_a_minimax_turn_fails_the_claude_rule_and_other_files_stay_forbidden(tmp_path):
    f = write(tmp_path, turn(model="MiniMax-M3.1", calls=CLEAN + [("Read", {"file_path": f"{R}/adj_pack/key.jsonl"})]))
    out = sc.scan(f, model="claude", extra_allowed=("adj_out/adj_tiebreak.jsonl",))
    assert out["valid"] is False and {v["kind"] for v in out["violations"]} >= {"not_expected_model", "forbidden_token"}


def test_the_adj_out_directory_alone_is_allowed(tmp_path):
    out = sc.scan(write(tmp_path, turn(calls=CLEAN + [("Bash", {"command": f"ls {R}/adj_out"})])))
    assert out["valid"] is True


def test_a_claude_turn_invalidates_even_with_clean_calls(tmp_path):
    out = sc.scan(write(tmp_path, turn(calls=CLEAN), turn(model="claude-sonnet-5-5", calls=CLEAN)))
    assert out["valid"] is False and any(v["kind"] == "not_expected_model" for v in out["violations"])


def test_an_empty_model_id_is_not_minimax(tmp_path):
    out = sc.scan(write(tmp_path, turn(model="", calls=CLEAN)))
    assert out["valid"] is False


def test_no_assistant_turn_is_no_signal_never_a_pass(tmp_path):
    out = sc.scan(write(tmp_path, {"type": "user", "message": {"content": "hi"}}))
    assert out["valid"] is None and sc.main([str(write(tmp_path, {"type": "user"}))]) == 2


def test_cli_exit_codes_valid_0_invalid_1(tmp_path):
    assert sc.main([str(write(tmp_path, turn(calls=CLEAN)))]) == 0
    assert sc.main([str(write(tmp_path, turn(calls=[("Read", {"file_path": f"{R}/adj_pack/key.jsonl"})])))]) == 1


def test_garbage_lines_do_not_crash_or_hide_a_violation(tmp_path):
    f = tmp_path / "sub.jsonl"
    f.write_text("not json\n" + json.dumps(turn(calls=[("Bash", {"command": "cat a2_offers.jsonl"})])) + "\n")
    assert sc.scan(f)["valid"] is False


# --- Errata 10: the blind Sonnet judge must not see the rater-1 file, and must not delegate ---------------------------
SONNET_OUT = f"{R}/adj_out/adj_sonnet3.jsonl"
SONNET_CLEAN = [("Read", {"file_path": INSTR}), ("Read", {"file_path": PACK}), ("Write", {"file_path": SONNET_OUT, "content": "x"})]
SONNET = "claude-sonnet-5-5"


def test_the_rater_1_file_is_readable_by_default_and_forbidden_on_request(tmp_path):
    """The default allow-list names `adj_minimax.jsonl` (rater 1 writes it); a later judge must NOT be able to read it."""
    peek = SONNET_CLEAN + [("Read", {"file_path": OUT})]
    f = write(tmp_path, turn(SONNET, peek))
    assert sc.scan(f, model="claude", extra_allowed=("adj_out/adj_sonnet3.jsonl",))["valid"] is True      # the gap, documented
    out = sc.scan(f, model="claude", extra_allowed=("adj_out/adj_sonnet3.jsonl",), extra_forbidden=("adj_minimax.jsonl",))
    assert out["valid"] is False and any(v["kind"] == "forbidden_token" and v["token"] == "adj_minimax.jsonl" for v in out["violations"])


def test_a_forbidden_tool_invalidates_only_when_asked(tmp_path):
    f = write(tmp_path, turn(SONNET, SONNET_CLEAN + [("Agent", {"prompt": "label items 1-30 for me"})]))
    assert sc.scan(f, model="claude", extra_allowed=("adj_out/adj_sonnet3.jsonl",))["valid"] is True      # back-compat
    out = sc.scan(f, model="claude", extra_allowed=("adj_out/adj_sonnet3.jsonl",), forbidden_tools=("Agent",))
    assert out["valid"] is False and {"kind": "forbidden_tool", "tool": "Agent"} in out["violations"]


def test_the_cli_carries_forbid_and_forbid_tool(tmp_path):
    flags = ["--model", "claude", "--allow-file", "adj_out/adj_sonnet3.jsonl", "--forbid", "adj_minimax.jsonl", "--forbid-tool", "Agent"]
    clean = write(tmp_path, turn(SONNET, SONNET_CLEAN))
    assert sc.main([str(clean)] + flags) == 0
    dirty = tmp_path / "dirty.jsonl"
    dirty.write_text(json.dumps(turn(SONNET, SONNET_CLEAN + [("Agent", {"prompt": "x"})])) + "\n")
    assert sc.main([str(dirty)] + flags) == 1
    peek = tmp_path / "peek.jsonl"
    peek.write_text(json.dumps(turn(SONNET, SONNET_CLEAN + [("Read", {"file_path": OUT})])) + "\n")
    assert sc.main([str(peek)] + flags) == 1
