#!/usr/bin/env python3
"""Functional tests for explore_until_dry.py (F1+F1.5 of the ADW plan).

Each test encodes a forensic lesson from the 2026-07-19 five-round session:
truncation accounting (the `head -100` failure), full-ledger dedupe, manual-lens
gating (the external lens that never ran), question-queue gating (endogenous
targets), D2 corroboration, and the honest verdict. Run: pytest -q this file.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import explore_until_dry as ex  # noqa: E402


class FakeResult:
    def __init__(self, parsed=None, degraded=False, exit_code=0):
        self.parsed = parsed
        self.daemon_degraded = degraded
        self.exit_code = exit_code
        self.stdout = json.dumps(parsed) if parsed is not None else ""
        self.stderr = ""


def make_runner(responses: dict[str, object]):
    """Runner stub: first arg-token join matched by prefix key."""
    def run(args, *, timeout=0.0, **_kw):
        joined = " ".join(args)
        for key, value in responses.items():
            if joined.startswith(key):
                return FakeResult(value)
        return FakeResult({})
    return run


DEFS = {"count": 2, "definitions": [
    {"file_path": "crates/a/src/lib.rs", "line_number": 10, "symbol_name": "Alpha"},
    {"file_path": "crates/b/src/x.rs", "line_number": 20, "symbol_name": "Beta"},
]}
RECALL = {"entries": [
    {"key": "lesson:one", "score": 0.9, "value": "strong lesson"},
    {"key": "noise:low", "score": 0.05, "value": "noise below floor"},
]}


def base_responses() -> dict[str, object]:
    return {
        "index find": DEFS,
        "tantivy search": {"hits": []},
        "ast find": {"count": 0, "definitions": []},
        "wiring impact": {"consumers": []},
        "memory recall": RECALL,
        "ast meta": {"blast_radius": 1, "quality_score": 0.9},
    }


@pytest.fixture()
def scope(tmp_path: Path) -> Path:
    (tmp_path / "src.rs").write_text("fn alpha() {}\n", encoding="utf-8")
    return tmp_path


def fresh_ledger(scope: Path) -> dict:
    return ex.load_ledger(scope / "none.json", "Alpha", scope)


# === determinism + dedupe ==================================================

def test_deterministic_ids():
    a = ex.det_id("f", "lexical", "definition", "x.rs:1")
    b = ex.det_id("f", "lexical", "definition", "x.rs:1")
    c = ex.det_id("f", "lexical", "definition", "x.rs:2")
    assert a == b and a != c and a.startswith("f_")


def test_round_dedupes_against_full_ledger(scope):
    ledger = fresh_ledger(scope)
    run = make_runner(base_responses())
    r1 = ex.run_round(ledger, scope, run, 5.0)
    r2 = ex.run_round(ledger, scope, run, 5.0)
    assert r1["new_findings"] > 0
    assert r2["new_findings"] == 0          # identical sweep → fully deduped


# === self-echo suppression (2026-08-29) ====================================

def test_stored_epoch_parses_both_column_shapes():
    """The live DB stores TEXT UTC; consolidated ones store INTEGER epoch."""
    assert ex.stored_epoch("2026-08-29 14:57:27") == pytest.approx(1788015447.0)
    assert ex.stored_epoch(1788016909) == 1788016909.0
    assert ex.stored_epoch("not a date") is None
    assert ex.stored_epoch(None) is None


def test_echo_memory_stored_after_campaign_never_wets_the_dry_tail(scope):
    """A memory written AFTER the campaign began (its own output echoing back)
    is recorded VISIBLE (echo: true) but never counts as a new finding — the
    [1,0,0,1,…] livelock measured live on 2026-08-29."""
    ledger = fresh_ledger(scope)
    responses = base_responses()
    run = make_runner(responses)
    r1 = ex.run_round(ledger, scope, run, 5.0)
    assert r1["new_findings"] > 0
    # the campaign now "stores a lesson about itself": recall grows one entry
    # younger than the ledger
    responses["memory recall"] = {"entries": [
        *RECALL["entries"],
        {"key": "lesson:self", "score": 0.9, "value": "the campaign's own lesson",
         "stored_at": int(ledger["created_at"]) + 3600},
    ]}
    r2 = ex.run_round(ledger, scope, run, 5.0)
    assert r2["new_findings"] == 0, "echo must not wet the dry tail"
    echoes = [f for f in ledger["findings"].values() if f.get("echo")]
    assert len(echoes) == 1 and echoes[0]["key"] == "lesson:self"
    verdict = ex.convergence(ledger, dry_rounds=2)
    assert verdict["clauses"]["echoes_suppressed"] == 1


def test_pre_campaign_memory_and_missing_timestamp_count_normally(scope):
    """Suppression needs POSITIVE proof: an older memory counts (it is the
    world, not an echo), and one with no stored_at counts too (an unknown age
    never suppresses — Lei L2)."""
    ledger = fresh_ledger(scope)
    responses = base_responses()
    run = make_runner(responses)
    ex.run_round(ledger, scope, run, 5.0)
    responses["memory recall"] = {"entries": [
        *RECALL["entries"],
        {"key": "lesson:old", "score": 0.9, "value": "pre-campaign lesson",
         "stored_at": int(ledger["created_at"]) - 3600},
        {"key": "lesson:undated", "score": 0.9, "value": "no timestamp"},
    ]}
    r2 = ex.run_round(ledger, scope, run, 5.0)
    assert r2["new_findings"] == 2
    assert not any(f.get("echo") for f in ledger["findings"].values())


def test_legacy_ledger_derives_birth_from_round_one(scope):
    """A pre-field ledger derives its birth from round 1's timestamp; with no
    rounds either, nothing is ever suppressed (no proof, no echo)."""
    import time as _t
    ledger = fresh_ledger(scope)
    del ledger["created_at"]
    responses = base_responses()
    responses["memory recall"] = {"entries": [
        {"key": "lesson:young", "score": 0.9, "value": "young lesson",
         "stored_at": int(_t.time()) + 3600},
    ]}
    run = make_runner(responses)
    r1 = ex.run_round(ledger, scope, run, 5.0)
    # no created_at and no prior rounds → no proof → the young memory counts
    assert any(f["key"] == "lesson:young" and not f.get("echo")
               for f in ledger["findings"].values())
    assert r1["new_findings"] > 0
    # once round 1 exists, birth derives from its timestamp — a later young
    # memory IS suppressible
    responses["memory recall"]["entries"].append(
        {"key": "lesson:younger", "score": 0.9, "value": "younger lesson",
         "stored_at": int(_t.time()) + 7200})
    r2 = ex.run_round(ledger, scope, run, 5.0)
    assert r2["new_findings"] == 0
    assert any(f["key"] == "lesson:younger" and f.get("echo")
               for f in ledger["findings"].values())


# === convergence contract ==================================================

def test_dry_rounds_plus_waived_external_converges(scope):
    ledger = fresh_ledger(scope)
    run = make_runner(base_responses())
    for _ in range(3):
        ex.run_round(ledger, scope, run, 5.0)
    assert ex.mark_lens(ledger, "external:waived", "no external practices apply")
    verdict = ex.convergence(ledger, dry_rounds=2)
    assert verdict["converged"] is True
    assert "NOT a claim of completeness" in verdict["statement"]


def test_external_lens_blocks_convergence(scope):
    ledger = fresh_ledger(scope)
    run = make_runner(base_responses())
    for _ in range(3):
        ex.run_round(ledger, scope, run, 5.0)
    verdict = ex.convergence(ledger, dry_rounds=2)
    assert verdict["converged"] is False
    assert any("external" in u for u in verdict["unmet"])


def test_open_question_blocks_convergence(scope):
    ledger = fresh_ledger(scope)
    run = make_runner(base_responses())
    for _ in range(3):
        ex.run_round(ledger, scope, run, 5.0)
    ex.mark_lens(ledger, "external:waived")
    qid = ex.add_question(ledger, "did we build this before?")
    assert ex.convergence(ledger, 2)["converged"] is False
    assert ex.answer_question(ledger, qid, "yes — taco-forge")
    assert ex.convergence(ledger, 2)["converged"] is True


def test_not_enough_rounds_never_converges(scope):
    ledger = fresh_ledger(scope)
    ex.mark_lens(ledger, "external:waived")
    verdict = ex.convergence(ledger, dry_rounds=2)
    assert verdict["converged"] is False    # zero rounds → no dry tail


# === truncation accounting (the head -100 lesson) ==========================

def test_truncation_is_accounted(scope):
    big = {"count": 40, "definitions": [
        {"file_path": f"crates/f{i}.rs", "line_number": i, "symbol_name": f"S{i}"}
        for i in range(40)]}
    responses = base_responses() | {"index find": big}
    ledger = fresh_ledger(scope)
    ex.run_round(ledger, scope, make_runner(responses), 5.0)
    assert ledger["coverage"]["lexical"]["truncated_total"] >= 40 - ex.TOP_HITS_PER_LENS


# === D2 corroboration ======================================================

def test_two_lenses_same_file_promotes_d2(scope):
    responses = base_responses() | {
        "ast find": {"count": 1, "definitions": [
            {"file_path": "crates/a/src/lib.rs", "symbol_name": "Alpha",
             "signature": "pub fn alpha()"}]},
    }
    ledger = fresh_ledger(scope)
    ex.run_round(ledger, scope, make_runner(responses), 5.0)
    depths = {f["key"].split(":", 1)[0].split("::", 1)[0]: f["depth"]
              for f in ledger["findings"].values()
              if "crates/a/src/lib.rs" in str(f["key"])}
    assert depths.get("crates/a/src/lib.rs") == "D2"


# === critic ingestion (fresh-eyes) =========================================

def test_critic_report_adds_and_resets_dryness(scope, tmp_path):
    ledger = fresh_ledger(scope)
    run = make_runner(base_responses())
    for _ in range(3):
        ex.run_round(ledger, scope, run, 5.0)
    ex.mark_lens(ledger, "external:waived")
    assert ex.convergence(ledger, 2)["converged"] is True
    report = tmp_path / "critic.json"
    report.write_text(json.dumps({
        "findings": [{"lens": "critic", "kind": "precedent",
                      "key": "taco-forge", "evidence": "memory 2026-07-02"}],
        "questions": ["why was it disconnected?"],
    }), encoding="utf-8")
    stats = ex.ingest_critic(ledger, report)
    assert stats == {"findings": 1, "questions": 1}
    verdict = ex.convergence(ledger, 2)
    assert verdict["converged"] is False    # critic findings re-open the loop


# === main() end-to-end with exit codes =====================================

def test_main_exit_codes_and_persistence(scope, monkeypatch):
    run = make_runner(base_responses())
    ledger_file = scope / "ledger.json"
    argv = ["Alpha", "--scope", str(scope), "--ledger", str(ledger_file),
            "--rounds", "3", "--json", "--quiet"]
    assert ex.main(argv, run=run) == 1      # dry but external pending → continue
    argv2 = ["Alpha", "--scope", str(scope), "--ledger", str(ledger_file),
             "--status", "--mark-lens", "external:waived", "--json", "--quiet"]
    assert ex.main(argv2, run=run) == 0     # contract now holds → converged
    data = json.loads(ledger_file.read_text(encoding="utf-8"))
    assert data["topic"] == "Alpha" and len(data["rounds"]) == 3


def test_main_bad_scope_exits_2():
    assert ex.main(["X", "--scope", "/nonexistent/dir/xyz", "--json"]) == 2


def test_until_dry_stops_at_max_rounds(scope):
    run = make_runner(base_responses())
    ledger_file = scope / "l2.json"
    rc = ex.main(["Alpha", "--scope", str(scope), "--ledger", str(ledger_file),
                  "--until-dry", "--max-rounds", "4", "--json", "--quiet"], run=run)
    data = json.loads(ledger_file.read_text(encoding="utf-8"))
    assert len(data["rounds"]) <= 4
    assert rc == 1                          # external still pending


# === ADW loop-node dryness protocol (Law L2) ===============================

def test_dry_signal_is_emitted_on_stderr(scope, capsys):
    """The producer states the round's yield so the RUNNER can end the loop.

    Regression for 2026-08-02: `touring explore` emitted no `NEW_FINDINGS=`
    marker at all, and adw.py read that silence as zero — so strategy-loop and
    explore-plan declared "dry" after exactly `dry_rounds` iterations while
    every round was still productive (30 then 4 new findings, both counted dry).
    stderr keeps `--json` stdout strictly parseable for programmatic callers.
    """
    run = make_runner(base_responses())
    ledger_file = scope / "sig.json"
    ex.main(["Alpha", "--scope", str(scope), "--ledger", str(ledger_file),
             "--rounds", "2", "--json"], run=run)
    out = capsys.readouterr()
    data = json.loads(ledger_file.read_text(encoding="utf-8"))
    expected = sum(r["new_findings"] for r in data["rounds"])
    assert f"NEW_FINDINGS={expected}" in out.err
    assert "NEW_FINDINGS" not in out.out     # stdout stays strict JSON
    json.loads(out.out)


def test_dry_signal_absent_when_no_round_ran(scope, capsys):
    """`--status` runs nothing, so it must stay SILENT rather than claim zero.

    Emitting `NEW_FINDINGS=0` here would let a read-only status call masquerade
    as a dry round and terminate an outer loop on no evidence at all.
    """
    run = make_runner(base_responses())
    ledger_file = scope / "sig2.json"
    ex.main(["Alpha", "--scope", str(scope), "--ledger", str(ledger_file),
             "--rounds", "1", "--json"], run=run)
    capsys.readouterr()
    ex.main(["Alpha", "--scope", str(scope), "--ledger", str(ledger_file),
             "--status", "--json"], run=run)
    assert "NEW_FINDINGS" not in capsys.readouterr().err


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
