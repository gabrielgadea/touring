#!/usr/bin/env python3
"""prose_integrity_gate: the signal, the escape, "introduced only", and the hook's two modes.

Contract: `docs/plans/2026-10-04-integridade-prosa/pre-registro.md` (touring repo, committed before any data).
The first four controls are the analise-ef scanner's, verbatim: a detector that fails them measures something
else (its first version counted typography and produced a rate 8x too high).
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
import prose_integrity_gate as g  # noqa: E402

GATE = HOOKS / "prose_integrity_gate.py"


# ── the signal ────────────────────────────────────────────────────────────────

def test_the_scanner_controls_hold():
    c = g.controls()
    assert all(c.values()), c
    assert {"C1_limpo_nao_acusa", "C2_positivo_detecta_cjk", "C3_negativo_ignora_tipografia",
            "C4_negativo_ignora_grego"} <= set(c)


@pytest.mark.parametrize("text,scripts", [
    ("uma связывание no meio", {"cirilico"}),
    ("vírgula de largura total，aqui", {"largura_total"}),
    ("ponto 。 chinês", {"cjk_pontuacao"}),
    ("カタカナ", {"kana"}),
    ("한국어", {"hangul"}),
])
def test_each_listed_script_is_signal(text, scripts):
    assert {h.script for h in g.find(text)} == scripts


@pytest.mark.parametrize("text", [
    "Δ τ κ são notação, não corrupção",
    "dAUC -0,0187 · t = 0,95 → ★ ─ « » °C ≈ ≤ ≥ —",
    "acentos do português: ação, ênfase, pôr, çã",
])
def test_greek_typography_and_portuguese_are_not_signal(text):
    assert g.find(text) == []


# ── the escape (SendMessage) ──────────────────────────────────────────────────

def test_inline_code_and_fenced_blocks_are_quotation():
    text = "o defeito aparece como `a terceira返回值` e também:\n```\n路径 决 臂\n```\nfim"
    hits, escaped = g.split_quoted(text)
    assert hits == [] and escaped == 7  # 返回值 (3) + 路径 决 臂 (4)


def test_an_unquoted_char_is_a_hit_even_beside_a_quoted_one():
    hits, escaped = g.split_quoted("citado `路径` e solto 决 aqui")
    assert [h.char for h in hits] == ["决"] and escaped == 2


# ── introduced only (Write / Edit) ────────────────────────────────────────────

def test_an_edit_that_keeps_existing_cjk_is_not_a_detection():
    old = 'let s = "日本語テスト";'
    assert g.introduced(old + " // ok", old) == []


def test_an_edit_that_adds_a_char_is_a_detection_of_that_char_only():
    old = 'let s = "日本語";'
    new = 'let s = "日本語"; // o公共 enum'
    assert [h.char for h in g.introduced(new, old)] == ["公", "共"]


def test_write_compares_with_the_file_on_disk(tmp_path):
    f = tmp_path / "t.rs"
    f.write_text('assert_eq!(f("你好"), 2);\n', encoding="utf-8")
    payload = {"tool_name": "Write", "tool_input": {"file_path": str(f),
               "content": 'assert_eq!(f("你好"), 2);\n// Para节省 espaço\n'}}
    v = g.assess(payload)
    assert [h.char for h in v.hits] == ["节", "省"]


def test_multiedit_reads_every_edit():
    payload = {"tool_name": "MultiEdit", "tool_input": {"file_path": "/nao/existe.md", "edits": [
        {"old_string": "a", "new_string": "a返"}, {"old_string": "b", "new_string": "b"}]}}
    assert [h.char for h in g.assess(payload).hits] == ["返"]


def test_sendmessage_reads_the_message_and_counts_escapes():
    v = g.assess({"tool_name": "SendMessage", "tool_input": {"to": "x", "message": "ok `路径` e 决"}})
    assert [h.char for h in v.hits] == ["决"] and v.escaped == 2


def test_a_structured_sendmessage_is_not_read():
    v = g.assess({"tool_name": "SendMessage", "tool_input": {"to": "x", "message": {"type": "shutdown_request"}}})
    assert v.hits == []


# ── the hook ──────────────────────────────────────────────────────────────────

def run_hook(payload, tmp_path, **env):
    e = {**os.environ, "TOURING_PROSE_GATE_LOG": str(tmp_path / "events.jsonl"), **env}
    e.pop("TOURING_PROSE_GATE_DISABLED", None) if "TOURING_PROSE_GATE_DISABLED" not in env else None
    for k in ("TOURING_PROSE_GATE_MODE", "TOURING_PROSE_GATE_FILE_MODE"):
        if k not in env:
            e.pop(k, None)
    data = payload if isinstance(payload, str) else json.dumps(payload)
    p = subprocess.run([sys.executable, str(GATE)], input=data, capture_output=True, check=False, text=True,
                       env=e, timeout=20)
    log = tmp_path / "events.jsonl"
    events = [json.loads(l) for l in log.read_text().splitlines()] if log.exists() else []
    return p.returncode, p.stdout, p.stderr, events


SEND_DIRTY = {"session_id": "s1", "tool_name": "SendMessage",
              "tool_input": {"to": "peer", "message": "o relatório tem 路径 no meio"}}


def test_shadow_never_blocks_logs_and_advises(tmp_path):
    rc, out, err, events = run_hook(SEND_DIRTY, tmp_path)
    assert rc == 0, err
    ctx = json.loads(out)["hookSpecificOutput"]["additionalContext"]
    assert "路" in ctx and "U+8DEF" in ctx and "crases" in ctx, ctx
    assert len(events) == 1 and events[0]["action"] == "advised" and events[0]["mode"] == "shadow"
    assert events[0]["n_hits"] == 2 and events[0]["tool"] == "SendMessage"


def test_block_mode_blocks_sendmessage_and_teaches_the_escape(tmp_path):
    rc, _out, err, events = run_hook(SEND_DIRTY, tmp_path, TOURING_PROSE_GATE_MODE="block")
    assert rc == 2
    assert "U+8DEF" in err and "crases" in err and "TOURING_PROSE_GATE_DISABLED" not in err, err
    assert events[0]["action"] == "blocked"


def test_the_sendmessage_mode_does_not_govern_files(tmp_path):
    payload = {"tool_name": "Write", "tool_input": {"file_path": str(tmp_path / "n.md"), "content": "Para节省"}}
    rc, _out, err, events = run_hook(payload, tmp_path, TOURING_PROSE_GATE_MODE="block",
                                    TOURING_PROSE_GATE_FILE_MODE="shadow")
    assert rc == 0 and events[0]["action"] == "advised" and events[0]["mode"] == "shadow", (rc, err)


# ── files block by default (Gabriel, 06/10/2026) ──────────────────────────────

CJK_A, CJK_B = chr(0x8DEF), chr(0x51B3)  # new tests build their signal, so no literal one is introduced


def _write(tmp_path, content, name="n.md"):
    return {"session_id": "s1", "tool_name": "Write",
            "tool_input": {"file_path": str(tmp_path / name), "content": content}}


def test_a_write_that_introduces_signal_is_blocked_by_default(tmp_path):
    rc, out, err, events = run_hook(_write(tmp_path, f"o relatório tem {CJK_A} no meio"), tmp_path)
    assert rc == 2, (rc, out, err)
    assert "U+8DEF" in err and "Write recusado" in err, err
    assert events[0]["action"] == "blocked" and events[0]["mode"] == "block"


def test_an_edit_that_introduces_signal_is_blocked_and_one_that_keeps_it_passes(tmp_path):
    base = {"file_path": str(tmp_path / "t.py"), "old_string": f'x = "{CJK_A}"'}
    rc, _, err, _ = run_hook({"tool_name": "Edit", "tool_input": {**base, "new_string": f'x = "{CJK_A}"  # ok'}},
                             tmp_path)
    assert rc == 0, err
    rc, _, err, events = run_hook({"tool_name": "Edit", "tool_input": {**base, "new_string": f'x = "{CJK_A}{CJK_B}"'}},
                                  tmp_path)
    assert rc == 2 and "U+51B3" in err and events[-1]["action"] == "blocked", err


def test_file_mode_shadow_advises_and_never_blocks(tmp_path):
    rc, out, err, events = run_hook(_write(tmp_path, f"a {CJK_A} b"), tmp_path, TOURING_PROSE_GATE_FILE_MODE="shadow")
    assert rc == 0 and events[0]["action"] == "advised", (rc, err)
    assert "prose-integrity: allow" in json.loads(out)["hookSpecificOutput"]["additionalContext"]


def test_backticks_in_a_file_are_quotation(tmp_path):
    rc, out, err, events = run_hook(_write(tmp_path, f"o defeito aparece como `a{CJK_A}` no texto"), tmp_path)
    assert rc == 0 and out.strip() == "", (rc, err)
    assert [e["action"] for e in events] == ["escaped"] and events[0]["escaped"] == 1


def test_the_pragma_escapes_its_line_only(tmp_path):
    content = f'DADO = "{CJK_A}"  # prose-integrity: allow\nOUTRO = "{CJK_B}"\n'
    rc, _out, err, events = run_hook(_write(tmp_path, content, "t.py"), tmp_path)
    assert rc == 2 and "U+51B3" in err and "U+8DEF" not in err, err
    assert events[0]["escaped"] == 1


def test_every_route_the_file_refusal_names_is_open(tmp_path):
    """D8: the refusal teaches three routes; each one must actually pass this executor."""
    dirty = f"texto com {CJK_A} solto"
    rc, _, err, _ = run_hook(_write(tmp_path, dirty), tmp_path)
    assert rc == 2
    assert "crases" in err and g.PRAGMA in err and "Bash" in err, err
    assert run_hook(_write(tmp_path, f"texto com `{CJK_A}` citado"), tmp_path)[0] == 0
    assert run_hook(_write(tmp_path, f"{dirty}  <!-- {g.PRAGMA} -->"), tmp_path)[0] == 0
    assert "Bash" not in g.TOOLS  # the script route is open by construction


def test_an_unclosed_fence_quotes_nothing(tmp_path):
    """Review touring-b4, 06/10: a lone ``` silenced the rest of the payload; an unclosed inline tick never did."""
    rc, _, err, _ = run_hook(_write(tmp_path, f"```py\n{CJK_B * 3}"), tmp_path)
    assert rc == 2 and "U+51B3" in err, (rc, err)
    hits, escaped = g.split_quoted(f"```\n{CJK_A}")
    assert len(hits) == 1 and escaped == 0


def test_the_pragma_needs_a_comment_leader(tmp_path):
    """Review touring-b4, 06/10: inside a JSON value the pragma 'worked' by corrupting the data."""
    rc, _, err, _ = run_hook(_write(tmp_path, f'{{"nota": "{CJK_B} {g.PRAGMA}"}}\n', "d.json"), tmp_path)
    assert rc == 2, (rc, err)
    for leader in ("#", "//", "/*", "<!--", "--", ";"):
        rc, _, err, _ = run_hook(_write(tmp_path, f"x = 1  {leader} {CJK_B} {g.PRAGMA}\n", "c.txt"), tmp_path)
        assert rc == 0, (leader, err)


def test_the_file_refusal_states_the_same_line_limit_of_backticks(tmp_path):
    rc, _, err, _ = run_hook(_write(tmp_path, f"a `{CJK_A}\n` b"), tmp_path)
    assert rc == 2 and "mesma linha" in err, err


def test_replace_all_cannot_launder_an_introduced_char(tmp_path):
    """Review touring-b4 (2), refuted by reproduction: any introduced char refuses the whole edit."""
    f = tmp_path / "r.txt"
    f.write_text(f"{CJK_B} alvo\n" * 300, encoding="utf-8")
    ti = {"file_path": str(f), "replace_all": True}
    assert run_hook({"tool_name": "Edit", "tool_input": {**ti, "old_string": "alvo", "new_string": CJK_B}},
                    tmp_path)[0] == 2
    assert run_hook({"tool_name": "Edit", "tool_input": {**ti, "old_string": f"{CJK_B} alvo",
                                                          "new_string": f"{CJK_B} novo"}}, tmp_path)[0] == 0


def test_a_directory_path_is_not_unknown_content(tmp_path):
    rc, _, err, _ = run_hook({"tool_name": "Write", "tool_input": {"file_path": str(tmp_path), "content": CJK_A}},
                             tmp_path)
    assert rc == 2, (rc, err)


def test_a_sendmessage_escape_alone_is_logged(tmp_path):
    payload = {"tool_name": "SendMessage", "tool_input": {"to": "p", "message": f"cito `{CJK_A}`"}}
    rc, out, _err, events = run_hook(payload, tmp_path)
    assert (rc, out.strip()) == (0, "")
    assert [e["action"] for e in events] == ["escaped"]


def test_a_write_over_a_file_too_large_to_read_never_blocks(tmp_path, monkeypatch):
    big = tmp_path / "big.md"
    big.write_text("x" * 64, encoding="utf-8")
    monkeypatch.setattr(g, "MAX_OLD_BYTES", 16)
    monkeypatch.setenv("TOURING_PROSE_GATE_LOG", str(tmp_path / "events.jsonl"))
    monkeypatch.delenv("TOURING_PROSE_GATE_FILE_MODE", raising=False)
    monkeypatch.delenv("TOURING_PROSE_GATE_DISABLED", raising=False)
    payload = {"tool_name": "Write", "tool_input": {"file_path": str(big), "content": f"x {CJK_A}"}}
    rc, out, err = g.run_hook(json.dumps(payload))
    assert rc == 0 and "não pôde ser lido" in out, (rc, out, err)


def test_clean_text_is_silent(tmp_path):
    rc, out, _err, events = run_hook({"tool_name": "SendMessage", "tool_input": {"to": "p", "message": "ok"}},
                                    tmp_path)
    assert (rc, out.strip(), events) == (0, "", [])


def test_kill_switch(tmp_path):
    rc, out, _err, events = run_hook(SEND_DIRTY, tmp_path, TOURING_PROSE_GATE_DISABLED="1",
                                    TOURING_PROSE_GATE_MODE="block")
    assert (rc, out.strip(), events) == (0, "", [])


@pytest.mark.parametrize("raw", ["", "not json", "[]", '{"tool_name": 3}'])
def test_unreadable_payload_fails_open(tmp_path, raw):
    rc, out, _err, events = run_hook(raw, tmp_path, TOURING_PROSE_GATE_MODE="block")
    assert (rc, out.strip(), events) == (0, "", [])


# ── replay and the pre-registered criterion ──────────────────────────────────

def test_replay_reads_tool_uses_from_transcripts(tmp_path):
    proj = tmp_path / "projects" / "p"
    proj.mkdir(parents=True)
    rec = {"type": "assistant", "timestamp": "t", "sessionId": "s", "message": {"model": "m", "content": [
        {"type": "tool_use", "id": "u1", "name": "SendMessage", "input": {"to": "x", "message": "a 返回值 b"}},
        {"type": "tool_use", "id": "u2", "name": "Edit", "input": {"file_path": "f", "old_string": "x",
                                                                    "new_string": "x"}},
        {"type": "text", "text": "texto do chat com 路径 não é ferramenta"}]}}
    (proj / "t.jsonl").write_text(json.dumps(rec) + "\nnot json\n", encoding="utf-8")
    out = g.replay(tmp_path / "projects")
    assert out["tool_uses"] == 2 and out["unreadable_lines"] == 1
    assert [d["tool"] for d in out["detections"]] == ["SendMessage"]
    assert out["detections"][0]["model"] == "m" and out["detections"][0]["id"] == "u1"


def test_wilson_upper_bound():
    assert g.wilson_upper(0, 30) == pytest.approx(0.1135, abs=1e-3)
    assert g.wilson_upper(3, 30) == pytest.approx(0.2562, abs=1e-3)


@pytest.mark.parametrize("legit,unclear,corrupt,promote", [
    (1, 0, 29, True),     # 1/30: upper 0.167
    (2, 0, 28, False),    # 2/30: upper 0.213
    (1, 1, 28, False),    # UNCLEAR counts as LEGIT: 2/30, upper 0.213 (1/30 would promote)
    (0, 0, 29, False),    # n = 29 < 30, however clean
    (5, 0, 25, False),    # 5/30: upper 0.34
])
def test_criterion_counts_unclear_as_legit_and_needs_thirty(legit, unclear, corrupt, promote):
    labels = ["LEGIT"] * legit + ["UNCLEAR"] * unclear + ["CORRUPT"] * corrupt
    r = g.criterion(labels)
    assert r["promote"] is promote, r
    assert r["legit_or_unclear"] == legit + unclear and r["n"] == len(labels)


def test_greek_stays_out_whatever_the_fast_path():
    assert g.LOWEST_SIGNAL >= 0x0400, "a range at or below Greek makes Greek notation signal"
    assert g.find("Δ τ κ λ μ σ ω") == []


@pytest.mark.parametrize("args", [["--criterion"], ["--criterion", "/nonexistent/labels.jsonl"]])
def test_criterion_without_a_readable_labels_file_is_a_usage_error_not_a_traceback(args, capsys):
    assert g.main(["prose_integrity_gate.py", *args]) == 2
    err = capsys.readouterr().err
    assert "--criterion <labels.jsonl>" in err
