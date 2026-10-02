"""S-14 — the verdict is COMPUTED from the frozen thresholds, never edited by hand."""
import copy
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_results_gate as gate  # noqa: E402
import r3_use_cases as uc  # noqa: E402


def a2_results(**over):
    base = {
        "use_case": "a2_outcome", "kappa": 0.72, "gold_insufficient": False, "n_indeterminate": 4,
        "b0": {"agreement": 0.80},
        "arms": {
            "B-L": {"agreement": 0.88, "delta_vs_b0": 0.08, "delta_ci_low": 0.02, "precision": 0.93, "coverage": 0.41,
                    "strata": {"b0_followed": {"n_test": 30, "agreement_model": 0.90, "agreement_b0": 0.88},
                               "b0_not_followed": {"n_test": 30, "agreement_model": 0.86, "agreement_b0": 0.72}},
                    "latency_p95_ms": 31},
        },
    }
    base.update(over)
    return base


def a1_results():
    return {
        "use_case": "a1_relevance",
        "arms": {
            "B-D": {"bytes_saved": 0.31, "followed_recall": 1.0, "exempt_retained": 1.0,
                    "harm": {"lower_ci_a": 0.93, "lower_ci_b": 0.91}},
            "B-L": {"bytes_saved": 0.45, "followed_recall": 0.96, "exempt_retained": 1.0},
        },
    }


def test_verdict_is_computed_from_frozen_thresholds_not_edited_by_hand():
    assert gate.verdict_a2(a2_results())["verdict"] == "PASSA"
    # one hair under the pre-registered +0.05 floor fails C-A2-1; exactly on it passes
    r = a2_results()
    r["arms"]["B-L"].update(delta_vs_b0=0.0499, delta_ci_low=0.01)
    assert gate.verdict_a2(r)["arms"]["B-L"]["C-A2-1"] is False
    r["arms"]["B-L"].update(delta_vs_b0=0.05, delta_ci_low=0.01)
    assert gate.verdict_a2(r)["arms"]["B-L"]["C-A2-1"] is True
    # the floors are READ from the registry, not copied: change the registry ⇒ the verdict follows
    saved = copy.deepcopy(uc.REGISTRY["a2_outcome"]["floors"])
    try:
        uc.REGISTRY["a2_outcome"]["floors"]["delta_agreement"] = 0.50
        assert gate.verdict_a2(a2_results())["arms"]["B-L"]["C-A2-1"] is False
    finally:
        uc.REGISTRY["a2_outcome"]["floors"].update(saved)


def test_a2_insufficient_gold_blocks_any_model_verdict():
    out = gate.verdict_a2(a2_results(gold_insufficient=True, kappa=0.41))
    assert out["verdict"] == "GOLD_INSUFICIENTE" and out["arms"] == {}


def test_a2_regex_is_enough_when_b0_is_already_at_the_ceiling():
    out = gate.verdict_a2(a2_results(b0={"agreement": 0.96}))
    assert out["verdict"] == "REGEX_BASTA"


def test_a2_a_regressing_stratum_makes_the_arm_partial_not_pass():
    r = a2_results()
    r["arms"]["B-L"]["strata"]["b0_followed"].update(agreement_model=0.84, agreement_b0=0.88)  # −0.04 < −0.03
    out = gate.verdict_a2(r)
    assert out["arms"]["B-L"]["C-A2-2"] is False and out["verdict"] == "PARCIAL"


def test_a2_small_strata_are_not_judged():
    r = a2_results()
    r["arms"]["B-L"]["strata"]["b0_followed"].update(n_test=19, agreement_model=0.10, agreement_b0=0.99)
    assert gate.verdict_a2(r)["arms"]["B-L"]["C-A2-2"] is True  # n_test < 20 ⇒ descriptive only


def test_a2_nothing_beats_b0_is_not_adopted_by_the_data():
    r = a2_results()
    r["arms"]["B-L"].update(delta_vs_b0=0.01, delta_ci_low=-0.04, precision=0.80, coverage=0.10)
    assert gate.verdict_a2(r)["verdict"] == "NAO_ADOTADO_PELOS_DADOS"


def test_a1_an_exempt_drop_invalidates_the_arm():
    r = a1_results()
    r["arms"]["B-L"]["exempt_retained"] = 0.999
    out = gate.verdict_a1(r)
    assert out["arms"]["B-L"]["valid"] is False and out["verdict"] == "REGRA_BASTA"


def test_a1_model_increment_is_measured_over_b_d_not_b0():
    out = gate.verdict_a1(a1_results())               # 0.45 − 0.31 = 0.14 ≥ 0.10 at recall 0.96 ≥ 0.95
    assert out["verdict"] == "MODELO_PASSA" and out["best_model_arm"] == "B-L"
    r = a1_results()
    r["arms"]["B-L"]["bytes_saved"] = 0.40            # 0.40 − 0.31 = 0.09 < 0.10 although 0.40 ≫ 0.10 over B0
    assert gate.verdict_a1(r)["verdict"] == "REGRA_BASTA"


def test_a1_the_rule_arm_needs_zero_followed_dropped_and_the_harm_lower_bound():
    r = a1_results()
    r["arms"]["B-D"]["followed_recall"] = 0.99
    assert gate.verdict_a1(r)["verdict"] == "NAO_ADOTADO_PELOS_DADOS"
    r = a1_results()
    r["arms"]["B-D"]["harm"]["lower_ci_b"] = 0.89     # worst of the two raters decides
    assert gate.verdict_a1(r)["verdict"] == "NAO_ADOTADO_PELOS_DADOS"
    r = a1_results()
    r["arms"]["B-D"]["bytes_saved"] = 0.249
    assert gate.verdict_a1(r)["verdict"] == "NAO_ADOTADO_PELOS_DADOS"
