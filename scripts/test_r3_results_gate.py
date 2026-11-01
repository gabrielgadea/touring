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


def real_a1(**b_d):
    """The shape the REAL replay emits (r3_replay_a1.py): no `followed_recall` (it needs the A2 gold) and `harm: null`
    (it needs the cross-vendor raters). The first version of the gate only ever saw a fixture with every field filled and
    crashed with TypeError on the first real file."""
    arm = {"bytes_totais": 149773, "bytes_mantidos": 133382, "bytes_saved": 0.1094, "n_blocos": 262, "n_exempt": 128,
           "exempt_retained": 1.0, "harm": None}
    arm.update(b_d)
    return {"use_case": "a1_relevance", "arms": {"B-D": arm}}


def test_a1_real_shape_with_failed_bytes_is_not_adopted_and_harm_is_moot():
    out = gate.verdict_a1(real_a1())                                  # 0.1094 < 0.25: definitive, whatever recall/harm are
    assert out["verdict"] == "NAO_ADOTADO_PELOS_DADOS" and out["arms"]["B-D"]["C-A1-1"] is False
    assert out["arms"]["B-D"]["C-A1-2"] is None and out["harm_moot"] is True


def test_a1_unknown_is_never_a_pass_it_is_inconclusive_with_the_reason():
    ok_bytes = real_a1(bytes_saved=0.30)                              # bytes pass, recall AND harm still unknown
    assert gate.verdict_a1(ok_bytes)["verdict"] == "INCONCLUSIVO_SEM_GOLD"
    with_recall = real_a1(bytes_saved=0.30, followed_recall=1.0)      # recall known, harm unknown
    assert gate.verdict_a1(with_recall)["verdict"] == "INCONCLUSIVO_SEM_HARM"


def test_a1_a_known_failure_decides_even_when_other_fields_are_unknown():
    assert gate.verdict_a1(real_a1(bytes_saved=0.9, exempt_retained=0.99))["verdict"] == "NAO_ADOTADO_PELOS_DADOS"   # EXEMPT drop kills it
    assert gate.verdict_a1(real_a1(bytes_saved=0.9, followed_recall=0.97))["verdict"] == "NAO_ADOTADO_PELOS_DADOS"   # recall < 1.00
    bad_harm = real_a1(bytes_saved=0.30, followed_recall=1.0, harm={"lower_ci_a": 0.95, "lower_ci_b": 0.80})
    assert gate.verdict_a1(bad_harm)["verdict"] == "NAO_ADOTADO_PELOS_DADOS"                                          # worse rater decides


def test_a1_complete_results_still_reach_the_model_verdicts():
    assert gate.verdict_a1(a1_results())["verdict"] == "MODELO_PASSA"


def test_a2_c2_with_no_judgeable_stratum_is_unmeasured_never_a_pass():
    """Cross-audit 02/10 finding 6: `all()` over an empty iterable is True, so an arm whose strata were ALL below
    n_test 20 got C-A2-2 = True without a single stratum having been judged."""
    r = a2_results()
    for s in r["arms"]["B-L"]["strata"].values():
        s["n_test"] = 19
    small = gate.verdict_a2(r)["arms"]["B-L"]
    assert small["C-A2-2"] is None and small["passes"] is False
    r["arms"]["B-L"]["strata"] = {}
    none = gate.verdict_a2(r)["arms"]["B-L"]
    assert none["C-A2-2"] is None and none["passes"] is False


def test_a2_c2_judges_the_strata_that_are_big_enough_and_ignores_the_small_one():
    """The REAL shape (02/10): `followed` n=66 is judged, `not_followed` n=19 is not — one judged stratum decides."""
    r = a2_results()
    r["arms"]["B-L"]["strata"] = {"followed": {"n_test": 66, "agreement_model": 0.91, "agreement_b0": 0.88},
                                  "not_followed": {"n_test": 19, "agreement_model": 0.0, "agreement_b0": 1.0}}
    assert gate.verdict_a2(r)["arms"]["B-L"]["C-A2-2"] is True
    r["arms"]["B-L"]["strata"]["followed"]["agreement_model"] = 0.80          # −0.08 < −0.03 in the judged stratum
    assert gate.verdict_a2(r)["arms"]["B-L"]["C-A2-2"] is False
    # every judged stratum counts, not just the first: here only the SECOND (n >= 20) regresses
    r["arms"]["B-L"]["strata"]["followed"]["agreement_model"] = 0.91
    r["arms"]["B-L"]["strata"]["not_followed"] = {"n_test": 30, "agreement_model": 0.60, "agreement_b0": 0.90}
    assert gate.verdict_a2(r)["arms"]["B-L"]["C-A2-2"] is False


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


def test_a2_underpowered_effect_is_inconclusive_not_a_failure():
    """Errata 3 (E3.2): delta >= the floor but the CI crosses zero = the sample cannot decide."""
    r = a2_results()
    r["arms"]["B-L"].update(delta_vs_b0=0.07, delta_ci_low=-0.01)
    out = gate.verdict_a2(r)
    assert out["verdict"] == "INCONCLUSIVO_POR_POTENCIA" and out["arms"]["B-L"]["underpowered"] is True


def test_a2_underpowered_never_outranks_a_real_pass_or_a_partial():
    r = a2_results()
    r["arms"]["B-J"] = dict(r["arms"]["B-L"], delta_vs_b0=0.07, delta_ci_low=-0.01)  # underpowered sibling
    assert gate.verdict_a2(r)["verdict"] == "PASSA"                                    # B-L passes outright
    r = a2_results()
    r["arms"]["B-L"]["strata"]["b0_followed"].update(agreement_model=0.80, agreement_b0=0.88)  # B-L only PARTIAL
    r["arms"]["B-J"] = dict(r["arms"]["B-L"], delta_vs_b0=0.07, delta_ci_low=-0.01)
    assert gate.verdict_a2(r)["verdict"] == "PARCIAL"


def test_a2_a_delta_below_the_floor_is_not_inconclusive_it_is_not_adopted():
    r = a2_results()
    r["arms"]["B-L"].update(delta_vs_b0=0.03, delta_ci_low=-0.05, precision=0.80, coverage=0.10)
    out = gate.verdict_a2(r)
    assert out["verdict"] == "NAO_ADOTADO_PELOS_DADOS" and out["arms"]["B-L"]["underpowered"] is False


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
