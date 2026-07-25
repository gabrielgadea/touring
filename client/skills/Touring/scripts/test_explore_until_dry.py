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


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
