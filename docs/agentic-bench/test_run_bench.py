"""Unit tests for the pure scoring functions of the agentic tool benchmark.

These cover the I/O-free core (`precision_at_1`, `reciprocal_rank`,
`composite_score`, `tier_for`, and the F6 A/B attribution helpers `arm_env` /
`attribution`) — the binary-driving axes are exercised by the live run
(`python3 run_bench.py`), not here.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import pytest  # noqa: E402
import run_bench as rb  # noqa: E402


class TestPrecisionAt1:
    def test_top_match(self):
        assert rb.precision_at_1(["touring wiring impact", "x"], "wiring impact")

    def test_case_insensitive(self):
        assert rb.precision_at_1(["TOURING_AUDIT"], "audit")

    def test_not_top(self):
        assert not rb.precision_at_1(["x", "touring_audit"], "audit")

    def test_empty(self):
        assert not rb.precision_at_1([], "audit")


class TestReciprocalRank:
    def test_first(self):
        assert rb.reciprocal_rank(["audit", "x"], "audit") == 1.0

    def test_second(self):
        assert rb.reciprocal_rank(["x", "audit"], "audit") == 0.5

    def test_third(self):
        assert abs(rb.reciprocal_rank(["a", "b", "audit"], "audit") - (1 / 3)) < 1e-9

    def test_absent(self):
        assert rb.reciprocal_rank(["a", "b"], "audit") == 0.0


class TestComposite:
    def test_all_perfect(self):
        assert abs(rb.composite_score(1.0, 1.0, 1.0) - 1.0) < 1e-9

    def test_weighting(self):
        # selection .40, outcome .40, conformance .20
        assert abs(rb.composite_score(1.0, 0.0, 0.0) - 0.40) < 1e-9
        assert abs(rb.composite_score(0.0, 0.0, 1.0) - 0.20) < 1e-9

    def test_zero(self):
        assert rb.composite_score(0.0, 0.0, 0.0) == 0.0


class TestTier:
    def test_diamond(self):
        assert rb.tier_for(0.97) == "Diamond"
        assert rb.tier_for(0.95) == "Diamond"

    def test_bands(self):
        assert rb.tier_for(0.92) == "Platinum"
        assert rb.tier_for(0.85) == "Gold"
        assert rb.tier_for(0.72) == "Silver"
        assert rb.tier_for(0.61) == "Bronze"
        assert rb.tier_for(0.40) == "Unranked"


class TestScenarioIntegrity:
    def test_selection_scenarios_well_formed(self):
        assert len(rb.SCENARIOS_SELECTION) >= 5
        for intent, expected in rb.SCENARIOS_SELECTION:
            assert intent and expected

    def test_outcome_scenarios_cover_block_and_info(self):
        verdicts = {v for _l, _c, v, _cwe in rb.SCENARIOS_OUTCOME}
        assert "block" in verdicts and "info" in verdicts

    def test_weights_sum_to_one(self):
        assert abs(sum(rb.WEIGHTS.values()) - 1.0) < 1e-9


# ── F6 — A/B causal attribution (telemetry §10) ─────────────────────────────
class TestArmEnv:
    def test_treatment_is_empty(self):
        # treatment = the shipped default surface: no env overrides.
        assert rb.arm_env("treatment") == {}

    def test_control_disables_coupling(self):
        env = rb.arm_env("control")
        assert env["TOURING_SUGGESTER_DISABLED"] == "1"
        assert env["TOURING_MCP_ALL_TOOLS"] == "1"

    def test_unknown_arm_raises(self):
        with pytest.raises(ValueError):
            rb.arm_env("bogus")

    def test_returns_fresh_copy(self):
        # mutating the returned dict must not corrupt the ARM_ENV table.
        rb.arm_env("control")["INJECTED"] = "1"
        assert "INJECTED" not in rb.ARM_ENV["control"]


class TestAttribution:
    def test_coupling_helps_when_treatment_beats_control(self):
        a = rb.attribution(0.80, 0.90)
        assert a["delta"] == pytest.approx(0.10)
        assert a["attributable"] is True
        assert a["verdict"] == "coupling_helps"
        assert a["control_composite"] == 0.80
        assert a["treatment_composite"] == 0.90

    def test_neutral_within_epsilon(self):
        a = rb.attribution(0.900, 0.905, epsilon=0.01)
        assert a["attributable"] is False
        assert a["verdict"] == "coupling_neutral"

    def test_coupling_hurts_when_control_wins(self):
        a = rb.attribution(0.90, 0.78)
        assert a["delta"] == pytest.approx(-0.12)
        assert a["attributable"] is False
        assert a["verdict"] == "coupling_hurts"

    def test_epsilon_threshold_is_respected(self):
        # delta 0.03 does NOT clear a 0.05 noise floor → not attributable.
        a = rb.attribution(0.80, 0.83, epsilon=0.05)
        assert a["attributable"] is False
        assert a["verdict"] == "coupling_neutral"
