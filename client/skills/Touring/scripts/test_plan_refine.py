#!/usr/bin/env python3
"""Functional tests for plan_refine.py (F2 of the ADW plan).

Encodes the round-4/5 forensic lessons as asserts: depth-weighted coverage,
plan-delta detects effort reclassification (the XL→M event of round 4),
open-question gating, unverified-claim detection (VGP-lite), and the
plateau contract with honest verdicts. Run: pytest -q this file.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import plan_refine as pr  # noqa: E402


def ledger_with(findings: list[dict], questions: list[dict] | None = None) -> dict:
    return {"findings": {f"f_{i}": f for i, f in enumerate(findings)},
            "questions": questions or []}


PLAN_V1 = """# Plano X
## F0 — Substrato [XL]
Usa o `run_gateway` do CEG em crates/touring-ceg/src/gateway/pre_exec.rs.
## F1 — Explorador [M]
Cobre calibrate-confidence e conflict-check.
"""

PLAN_V2 = """# Plano X
## F0 — Substrato [M]
Usa o `run_gateway` do CEG em crates/touring-ceg/src/gateway/pre_exec.rs.
## F1 — Explorador [M]
Cobre calibrate-confidence e conflict-check.
## F2 — Refino [M]
Novo estágio.
"""


# === coverage ==============================================================

def test_coverage_depth_weighted():
    led = ledger_with([
        {"key": "crates/touring-ceg/src/gateway/pre_exec.rs:10", "depth": "D2",
         "lens": "structural"},
        {"key": "crates/never/mentioned.rs:5", "depth": "D0", "lens": "lexical"},
    ])
    cov = pr.coverage_score(led, PLAN_V1)
    # D2 addressed (3.0) vs D0 gap (1.0) → 3/4
    assert cov["score"] == pytest.approx(0.75)
    assert cov["gaps"][0]["key"] == "crates/never/mentioned.rs:5"


def test_coverage_empty_ledger_is_full():
    assert pr.coverage_score(ledger_with([]), PLAN_V1)["score"] == 1.0


# === plan-delta (the round-4 XL→M event must be measurable) ================

def test_plan_delta_detects_effort_reclassification_and_new_heading():
    s1 = pr.structural_signature(PLAN_V1)
    s2 = pr.structural_signature(PLAN_V2)
    delta = pr.plan_delta(s1, s2)
    assert delta["effort_reclassified"] == 1      # F0 [XL] → [M]
    assert delta["headings_added"] == 1           # F2 novo
    assert delta["is_zero"] is False


def test_plan_delta_zero_on_identical():
    s = pr.structural_signature(PLAN_V1)
    assert pr.plan_delta(s, pr.structural_signature(PLAN_V1))["is_zero"] is True


# === questions + claims ====================================================

def test_open_question_unaddressed_detected():
    led = ledger_with([], [{"id": "q_1", "status": "open",
                            "text": "precedente workflow taco-forge existe?"}])
    assert len(pr.questions_unaddressed(led, PLAN_V1)) == 1
    plan_with = PLAN_V1 + "\nO precedente taco-forge foi analisado (workflow).\n"
    assert pr.questions_unaddressed(led, plan_with) == []


def test_unverified_claim_detected(tmp_path):
    led = ledger_with([{"key": "crates/a/real.rs:1", "depth": "D1",
                        "lens": "lexical", "detail": "RealSymbol"}])
    text = "Plano cita `RealSymbol` e também `PhantomEngineXYZ` inexistente."
    bad = pr.unverified_claims(text, led, tmp_path)
    assert bad == ["PhantomEngineXYZ"]


# === plateau contract ======================================================

def iter_entry(i, cov, delta_zero=True, open_q=0, claims=0):
    return {"iter": i, "coverage": cov, "gap_count": 0, "top_gaps": [],
            "open_questions": open_q, "unverified_claims": claims,
            "plan_delta": {"is_zero": delta_zero, "first": i == 1},
            "signature": {"headings": [], "efforts": {}, "hash": str(i)}}


def test_contract_requires_two_iterations():
    verdict = pr.contract([iter_entry(1, 0.95)], 0.9, 0.02)
    assert verdict["ready"] is False
    assert any("≥2 iterations" in u for u in verdict["unmet"])


def test_contract_plateau_holds():
    iters = [iter_entry(1, 0.94), iter_entry(2, 0.95)]
    verdict = pr.contract(iters, 0.9, 0.02)
    assert verdict["ready"] is True
    assert "NOT a claim the plan is complete" in verdict["statement"]


def test_contract_rejects_below_threshold_and_moving_score():
    iters = [iter_entry(1, 0.60), iter_entry(2, 0.80)]
    verdict = pr.contract(iters, 0.9, 0.02)
    assert verdict["ready"] is False
    assert any("coverage" in u for u in verdict["unmet"])
    assert any("plateau" in u for u in verdict["unmet"])


def test_contract_rejects_structural_delta():
    iters = [iter_entry(1, 0.95), iter_entry(2, 0.95, delta_zero=False)]
    verdict = pr.contract(iters, 0.9, 0.02)
    assert verdict["ready"] is False
    assert any("plan-delta" in u for u in verdict["unmet"])


# === main() end-to-end =====================================================

def test_main_iterates_and_converges(tmp_path):
    plan = tmp_path / "plan.md"
    ledger = tmp_path / "ledger.json"
    led = ledger_with([{"key": "crates/touring-ceg/src/gateway/pre_exec.rs:10",
                        "depth": "D2", "lens": "structural",
                        "detail": "run_gateway"}])
    ledger.write_text(json.dumps(led), encoding="utf-8")
    plan.write_text(PLAN_V1, encoding="utf-8")
    base = [str(plan), "--ledger", str(ledger), "--json", "--quiet"]
    assert pr.main(base) == 1                     # iter 1: no plateau yet
    assert pr.main(base) == 0                     # iter 2: same plan → plateau
    data = json.loads(plan.with_suffix(".refine.json").read_text())
    assert len(data["iterations"]) == 2


def test_main_detects_revision_reopens(tmp_path):
    plan = tmp_path / "plan.md"
    ledger = tmp_path / "ledger.json"
    ledger.write_text(json.dumps(ledger_with([])), encoding="utf-8")
    plan.write_text(PLAN_V1, encoding="utf-8")
    base = [str(plan), "--ledger", str(ledger), "--json", "--quiet"]
    pr.main(base)
    pr.main(base)
    plan.write_text(PLAN_V2, encoding="utf-8")    # author revises → delta ≠ 0
    assert pr.main(base) == 1


def test_main_missing_paths_exit_2(tmp_path):
    assert pr.main(["/no/plan.md", "--ledger", "/no/l.json", "--json"]) == 2


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
