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
    if "TOURING_PROSE_GATE_MODE" not in env:
        e.pop("TOURING_PROSE_GATE_MODE", None)
    data = payload if isinstance(payload, str) else json.dumps(payload)
    p = subprocess.run([sys.executable, str(GATE)], input=data, capture_output=True, text=True,
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
    rc, out, err, events = run_hook(SEND_DIRTY, tmp_path, TOURING_PROSE_GATE_MODE="block")
    assert rc == 2
    assert "U+8DEF" in err and "crases" in err and "TOURING_PROSE_GATE_DISABLED" not in err, err
    assert events[0]["action"] == "blocked"


def test_block_mode_never_blocks_a_write(tmp_path):
    payload = {"tool_name": "Write", "tool_input": {"file_path": str(tmp_path / "n.md"), "content": "Para节省"}}
    rc, out, err, events = run_hook(payload, tmp_path, TOURING_PROSE_GATE_MODE="block")
    assert rc == 0 and events[0]["action"] == "advised", (rc, err)


def test_clean_text_is_silent(tmp_path):
    rc, out, err, events = run_hook({"tool_name": "SendMessage", "tool_input": {"to": "p", "message": "ok"}},
                                    tmp_path)
    assert (rc, out.strip(), events) == (0, "", [])


def test_kill_switch(tmp_path):
    rc, out, err, events = run_hook(SEND_DIRTY, tmp_path, TOURING_PROSE_GATE_DISABLED="1",
                                    TOURING_PROSE_GATE_MODE="block")
    assert (rc, out.strip(), events) == (0, "", [])


@pytest.mark.parametrize("raw", ["", "not json", "[]", '{"tool_name": 3}'])
def test_unreadable_payload_fails_open(tmp_path, raw):
    rc, out, err, events = run_hook(raw, tmp_path, TOURING_PROSE_GATE_MODE="block")
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
