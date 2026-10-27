#!/usr/bin/env python3
"""Fixtures for scripts/measure_tool_trio.py -- each one is a round-5 artefact that
must NOT come back (REVISAO-2026-10-27.md, R1)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import measure_tool_trio as m


def _use(i, name, inp):
    return {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": f"t{i}", "name": name, "input": inp}]}}


def _res(i, text, err=False):
    return {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": f"t{i}", "content": text, "is_error": err}]}}


def _run(records):
    stats = {}
    m.measure_session(records, stats)
    return stats


def test_cd_prefix_is_not_the_verb():
    assert m.classify_bash("cd /home/x/touring && cargo test -p a") == "cargo test"
    assert m.classify_bash("cd /x; export A=1; timeout 30 python3 s.py") == "python3"
    assert m.classify_bash("TOURING_GATE_OK=1 touring run --lang bash") == "touring run"


def test_echo_header_is_not_the_verb_but_a_lone_echo_is():
    assert m.classify_bash('echo "### step 1"\ngit status --short') == "git status"
    assert m.classify_bash('echo "$PATH"') == "echo"


def test_non_text_result_blocks_are_not_empty():
    blocks = [{"type": "tool_reference", "tool_name": "mcp__x__some_long_tool_name_here"}]
    s = _run([_use(1, "ToolSearch", {"query": "select:x"}), _res(1, blocks)])
    assert s["ToolSearch"].outcomes["ok"] == 1


def test_heredoc_cat_is_a_write_and_its_empty_output_is_success():
    assert m.classify_bash("cat > f.txt <<'EOF'\nx\nEOF") == "cat-write"
    assert m.classify_bash("cat src/a.rs") == "cat-read"
    s = _run([_use(1, "Bash", {"command": "cat > f <<EOF\nx\nEOF"}), _res(1, "")])
    assert s["Bash:cat-write"].outcomes["ok"] == 1


def test_two_different_edits_of_one_file_are_not_a_retry():
    s = _run([
        _use(1, "Edit", {"file_path": "a.rs", "old_string": "x", "new_string": "y"}), _res(1, "ok " * 30),
        _use(2, "Edit", {"file_path": "a.rs", "old_string": "p", "new_string": "q"}), _res(2, "ok " * 30),
    ])
    assert s["Edit"].retry == 0


def test_byte_identical_call_is_a_retry():
    inp = {"file_path": "a.rs", "old_string": "x", "new_string": "y"}
    s = _run([_use(1, "Edit", inp), _res(1, "String not found", True), _use(2, "Edit", dict(inp)), _res(2, "String not found", True)])
    assert s["Edit"].retry == 1 and s["Edit"].outcomes["erro"] == 2


def test_grep_no_match_is_empty_not_error():
    s = _run([_use(1, "Bash", {"command": "grep -n foo a.rs"}), _res(1, "Exit code 1", True)])
    assert s["Bash:grep"].outcomes["vazio"] == 1 and s["Bash:grep"].outcomes["erro"] == 0


def test_hook_denial_is_not_a_tool_failure():
    s = _run([_use(1, "Bash", {"command": "ls"}), _res(1, "PreToolUse:Bash hook error: [G11] ...", True)])
    assert s["Bash:ls"].outcomes["negado"] == 1 and s["Bash:ls"].outcomes["erro"] == 0


def test_programs_with_different_code_are_not_a_retry():
    s = _run([
        _use(1, "mcp__touring__touring_ctx_execute", {"language": "python", "code": "print(1)"}), _res(1, "1" * 60),
        _use(2, "mcp__touring__touring_ctx_execute", {"language": "python", "code": "print(2)"}), _res(2, "2" * 60),
    ])
    assert s["mcp__touring__touring_ctx_execute"].retry == 0


def _extras(records):
    ex = m.Extras()
    m.measure_extras(records, ex)
    return ex


_DENY = "PreToolUse:Bash hook error: [G10 exec-burst] 5a chamada ..."


def test_deny_then_different_ok_call_is_recovery():
    ex = _extras([_use(1, "Bash", {"command": "ls a"}), _res(1, _DENY, True),
                  _use(2, "Bash", {"command": "touring run --code x"}), _res(2, "ok " * 30)])
    assert ex.deny_next[("G10", "recuperou")] == 1


def test_deny_then_same_call_is_repetition():
    ex = _extras([_use(1, "Bash", {"command": "ls a"}), _res(1, _DENY, True),
                  _use(2, "Bash", {"command": "ls a"}), _res(2, _DENY, True)])
    assert ex.deny_next[("G10", "repetiu")] == 1 and ex.deny_next[("G10", "abandonou")] == 1


def test_deny_then_other_denied_call_is_denied_again():
    ex = _extras([_use(1, "Bash", {"command": "ls a"}), _res(1, _DENY, True),
                  _use(2, "Bash", {"command": "ls b"}), _res(2, _DENY, True)])
    assert ex.deny_next[("G10", "negado-de-novo")] == 1


def test_read_followed_by_edit_of_same_file_is_superseded():
    ex = _extras([_use(1, "Read", {"file_path": "a.rs"}), _res(1, "x" * 100),
                  _use(2, "Read", {"file_path": "b.rs"}), _res(2, "y" * 50),
                  _use(3, "Edit", {"file_path": "a.rs", "old_string": "x", "new_string": "z"}), _res(3, "ok" * 30)])
    assert ex.read_bytes == 150 and ex.read_superseded_bytes == 100


def test_size_summary_reports_share_above_10k():
    s = m.size_summary([100, 200, 20_000, 300])
    assert s["n"] == 4 and s["gt10k%"] == 25.0 and s["gt10k_bytes%"] > 90


if __name__ == "__main__":
    fails = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"PASS {name}")
            except AssertionError as e:
                fails += 1
                print(f"FAIL {name}: {e!r}")
    sys.exit(1 if fails else 0)
