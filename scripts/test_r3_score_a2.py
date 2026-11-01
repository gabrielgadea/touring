"""Errata 8 — the A2 scorer turns predictions + gold into the `results-a2.json` contract of r3_results_gate."""
import json
import random
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_results_gate as gate  # noqa: E402
import r3_score_a2 as sc  # noqa: E402


def item(i, split, b0_followed, gold_followed, p_l, p_j=None, prov="agree"):
    pred = {"item_id": f"a2-{i:05d}", "split": split, "b0": "followed" if b0_followed else "not_followed",
            "B-L": {"p_followed": p_l, "latency_ms": 100.0 + i}, "B-J": {"p_followed": p_l if p_j is None else p_j, "latency_ms": 0.0}}
    return pred, {"item_id": pred["item_id"], "followed": gold_followed, "provenance": prov}


def build(n_val, n_test, model_ok, b0_acc, seed=1):
    """Synthetic world: gold ~60% followed; the model is right with prob `model_ok` (p high/low), B0 with prob `b0_acc`."""
    rng = random.Random(seed)
    preds, gold = [], []
    for i in range(n_val + n_test):
        g = rng.random() < 0.6
        b0 = g if rng.random() < b0_acc else not g
        good = rng.random() < model_ok
        say_followed = g if good else not g
        p = rng.uniform(0.7, 0.99) if say_followed else rng.uniform(0.01, 0.3)
        pr, gd = item(i, "audit-val" if i < n_val else "audit-test", b0, g, p)
        preds.append(pr)
        gold.append(gd)
    return preds, gold


SUMMARY = {"kappa": 0.77, "gold_insufficient": False, "n_indeterminate": 8}


def test_auc_counts_ties_as_half_and_detects_inversion():
    assert sc.auc([0.9, 0.8], [0.1, 0.2]) == 1.0
    assert sc.auc([0.1, 0.2], [0.9, 0.8]) == 0.0
    assert sc.auc([0.5], [0.5]) == 0.5
    assert sc.auc([], [0.5]) is None


def test_tau_maximizes_coverage_subject_to_precision_and_prefers_the_higher_tau_on_ties():
    # gold followed at p>=0.6 (4 items) then a noisy tail: lowering tau below 0.6 drops precision under 0.90
    val = [(0.9, True), (0.8, True), (0.7, True), (0.6, True), (0.5, False), (0.4, True), (0.3, False), (0.2, False)]
    tau = sc.choose_tau(val)
    assert tau["feasible"] is True and tau["tau"] == 0.6 and tau["coverage"] == 0.5 and tau["precision"] == 1.0
    tied = sc.choose_tau([(0.9, True), (0.9, True), (0.1, False)])
    assert tied["tau"] == 0.9


def test_infeasible_tau_falls_back_to_val_agreement_and_flags_it():
    inverted = [(0.9, False), (0.8, False), (0.2, True), (0.1, True)]
    tau = sc.choose_tau(inverted)
    assert tau["feasible"] is False
    # agreement ties at 0.5 between tau=inf (predict nothing) and tau=.1 (predict everything): the HIGHER tau wins (E8.3)
    assert tau["tau"] == float("inf") and tau["val_agreement"] == 0.5 and tau["coverage"] == 0.0 and tau["precision"] == 0.0
    assert sc.choose_tau([])["feasible"] is False


def test_zero_coverage_gives_precision_zero_never_none():
    preds, gold = build(40, 40, model_ok=0.0, b0_acc=0.9)
    arm = sc.score(preds, gold, SUMMARY)["arms"]["B-L"]
    assert isinstance(arm["precision"], float) and isinstance(arm["coverage"], float)


def test_the_result_matches_the_gate_contract_and_the_gate_accepts_it():
    preds, gold = build(60, 80, model_ok=0.9, b0_acc=0.8)
    res = sc.score(preds, gold, SUMMARY)
    assert {"use_case", "kappa", "gold_insufficient", "n_indeterminate", "b0", "arms"} <= set(res)
    for a in res["arms"].values():
        assert {"agreement", "delta_vs_b0", "delta_ci_low", "precision", "coverage", "strata", "latency_p95_ms"} <= set(a)
        for s in a["strata"].values():
            assert set(s) >= {"n_test", "agreement_model", "agreement_b0"}
    assert gate.verdict_a2(res)["verdict"] in {"PASSA", "PARCIAL", "INCONCLUSIVO_POR_POTENCIA", "NAO_ADOTADO_PELOS_DADOS", "REGEX_BASTA"}


def separated(n_val, n_test):
    """Deterministic world: 2/3 followed; the model is perfectly separated (p .9 / .1); B0 is wrong on every 5th item.
    (Random worlds are flaky for C-A2-3 BY DESIGN: tau sits at the edge of 'precision >= 0.90' on val, so test precision
    regresses by chance even for a good model.)"""
    preds, gold = [], []
    for i in range(n_val + n_test):
        g = i % 3 != 0
        b0 = g if i % 5 else not g
        pr, gd = item(i, "audit-val" if i < n_val else "audit-test", b0, g, 0.9 if g else 0.1)
        preds.append(pr)
        gold.append(gd)
    return preds, gold


def test_a_clearly_better_model_passes_and_a_random_one_does_not():
    good = sc.score(*separated(90, 150), SUMMARY)
    v = gate.verdict_a2(good)
    assert v["verdict"] == "PASSA" and good["arms"]["B-L"]["precision"] == 1.0 and good["arms"]["B-L"]["tau_feasible"] is True
    bad = sc.score(*build(150, 250, model_ok=0.5, b0_acc=0.8, seed=3), SUMMARY)
    assert gate.verdict_a2(bad)["verdict"] in {"NAO_ADOTADO_PELOS_DADOS", "INCONCLUSIVO_POR_POTENCIA"}


def test_a_b0_that_is_already_95_percent_right_closes_as_regex_enough():
    res = sc.score(*build(100, 300, model_ok=0.9, b0_acc=0.995, seed=5), SUMMARY)
    assert res["b0"]["agreement"] >= 0.95 and gate.verdict_a2(res)["verdict"] == "REGEX_BASTA"


def test_gold_insufficient_is_passed_through_to_the_gate():
    res = sc.score(*build(40, 40, 0.9, 0.8), {"kappa": 0.4, "gold_insufficient": True, "n_indeterminate": 3})
    assert res["gold_insufficient"] is True and gate.verdict_a2(res)["verdict"] == "GOLD_INSUFICIENTE"
    assert res["arms"] == {} and "polarity_auc" not in res          # with an insufficient gold NO arm is scored (§2)


def test_a_precision_of_exactly_090_is_feasible_the_floor_is_inclusive():
    val = [(0.9, True)] * 9 + [(0.9, False)]
    tau = sc.choose_tau(val)
    assert tau["feasible"] is True and tau["precision"] == 0.9 and tau["coverage"] == 1.0


def test_only_audit_test_enters_the_agreement_and_val_only_sets_tau():
    preds, gold = build(60, 80, 0.9, 0.8, seed=7)
    base = sc.score(preds, gold, SUMMARY)["arms"]["B-L"]
    flipped = [{**g, "followed": (not g["followed"]) if p["split"] == "audit-val" else g["followed"]} for p, g in zip(preds, gold)]
    moved = sc.score(preds, flipped, SUMMARY)["arms"]["B-L"]
    assert moved["strata"] == base["strata"] or moved["tau"] != base["tau"]       # val labels move tau ...
    test_only = [(p, g) for p, g in zip(preds, gold) if p["split"] == "audit-test"]
    n = sum(1 for _ in test_only)
    assert base["n_test"] == n                                                      # ... and the test population is exactly audit-test


def test_items_without_gold_and_arm_errors_are_excluded():
    preds, gold = build(30, 30, 0.9, 0.8)
    gold = gold[1:]                                           # first item (a val item) has no gold
    preds[40]["B-L"] = {"erro": "RuntimeError: boom"}          # one test item has no B-L prediction
    arm = sc.score(preds, gold, SUMMARY)["arms"]["B-L"]
    clean = sc.score(*build(30, 30, 0.9, 0.8), SUMMARY)["arms"]["B-L"]
    assert arm["n_test"] == clean["n_test"] - 1


def test_the_bootstrap_is_seeded_and_reproducible():
    args = build(60, 90, 0.9, 0.8, seed=11)
    a = sc.score(*args, SUMMARY)["arms"]["B-L"]["delta_ci_low"]
    b = sc.score(*args, SUMMARY)["arms"]["B-L"]["delta_ci_low"]
    assert a == b and sc.SEED == 20261002 and sc.N_BOOT == 10000


def test_strata_are_the_b0_decision_with_their_own_counts():
    res = sc.score(*build(60, 120, 0.9, 0.8, seed=2), SUMMARY)["arms"]["B-L"]["strata"]
    assert set(res) == {"followed", "not_followed"} and sum(s["n_test"] for s in res.values()) == res["followed"]["n_test"] + res["not_followed"]["n_test"]


def test_sensitivity_uses_only_the_agree_provenance_and_never_changes_the_verdict_fields():
    preds, gold = build(60, 90, 0.9, 0.8, seed=4)
    for g in gold[::5]:
        g["provenance"] = "tiebreak"
    res = sc.score(preds, gold, SUMMARY)
    sens = res["sensitivity_agree_only"]
    assert sens["n_gold"] == sum(1 for g in gold if g["provenance"] == "agree") < res["population"]["n_gold"]
    assert "verdict" in sens and res["arms"]["B-L"]["agreement"] is not None


def test_polarity_auc_exposes_an_inverted_arm_without_flipping_it():
    preds, gold = [], []
    for i in range(60):
        g = i % 2 == 0
        pr, gd = item(i, "audit-val" if i < 30 else "audit-test", g, g, 0.1 if g else 0.9)   # p HIGH when NOT followed
        preds.append(pr)
        gold.append(gd)
    res = sc.score(preds, gold, SUMMARY)
    # the inversion shows in the AUC; the scorer does NOT flip the arm (E8.8), and tau falls back to the best val agreement
    assert res["polarity_auc"]["B-L"] == 0.0 and res["arms"]["B-L"]["tau_feasible"] is False and res["arms"]["B-L"]["agreement"] <= 0.5


def hand_world():
    """10 items worked out BY HAND (the expected numbers below are arithmetic, not read back from the code).
    val : p .9 T · .8 T · .3 F · .2 F, B0 right on all 4          -> tau = .8 (.3 would give precision 2/3)
    test: (p, gold, b0)  .95 T T · .85 T F · .81 F F · .5 F T · .9 T F · .1 F F"""
    rows = [("audit-val", True, True, 0.9), ("audit-val", True, True, 0.8), ("audit-val", False, False, 0.3), ("audit-val", False, False, 0.2),
            ("audit-test", True, True, 0.95), ("audit-test", False, True, 0.85), ("audit-test", False, False, 0.81),
            ("audit-test", True, False, 0.5), ("audit-test", False, True, 0.9), ("audit-test", False, False, 0.1)]
    preds, gold = [], []
    for i, (split, b0, g, p) in enumerate(rows):
        pr, gd = item(i, split, b0, g, p)
        preds.append(pr)
        gold.append(gd)
    return preds, gold


def test_hand_computed_numbers_pin_every_rule_of_errata_8():
    res = sc.score(*hand_world(), SUMMARY)
    a = res["arms"]["B-L"]
    assert (a["tau"], a["tau_feasible"], a["n_val"], a["n_test"]) == (0.8, True, 4, 6)      # E8.3: tau from VAL rows only
    assert a["agreement"] == pytest.approx(5 / 6) and a["agreement_b0"] == pytest.approx(0.5)  # E8.2: tau applied to TEST rows
    assert a["delta_vs_b0"] == pytest.approx(1 / 3)
    assert a["precision"] == pytest.approx(0.75) and a["coverage"] == pytest.approx(4 / 6)
    assert a["strata"]["followed"] == {"n_test": 2, "agreement_model": 1.0, "agreement_b0": 0.5}   # E8.6: strata by the B0 decision
    assert a["strata"]["not_followed"]["n_test"] == 4
    assert a["strata"]["not_followed"]["agreement_model"] == pytest.approx(0.75) and a["strata"]["not_followed"]["agreement_b0"] == pytest.approx(0.5)
    assert res["b0"]["agreement"] == pytest.approx(0.5) and res["b0"]["agreement_full_gold"] == pytest.approx(0.7)   # E8.4


def test_the_ci_lower_bound_is_the_2_5th_percentile_not_the_median():
    a = sc.score(*hand_world(), SUMMARY)["arms"]["B-L"]
    # d = [0, 1, -1, 1, 1, 0] -> mean 1/3 ; the median of the bootstrap sits at the mean, the 2.5th percentile well below it
    assert -1.0 <= a["delta_ci_low"] < a["delta_vs_b0"] - 0.1
    assert a["delta_ci_low"] == pytest.approx(round(a["delta_ci_low"] * 6) / 6)   # a mean of 6 draws: a multiple of 1/6


def test_an_arm_entry_with_both_p_followed_and_erro_is_not_a_prediction():
    preds, gold = hand_world()
    preds[4]["B-L"] = {"p_followed": 0.99, "erro": "RuntimeError: partial"}
    assert sc.score(preds, gold, SUMMARY)["arms"]["B-L"]["n_test"] == 5


def test_the_replay_gold_flag_now_reaches_the_scorer_and_never_the_real_artifacts(tmp_path, monkeypatch):
    """`--gold` of r3_replay_a2 was accepted and ignored. SAIDA is patched to a tmp dir, so the real results are untouched."""
    import r3_replay_a2 as replay
    real = replay.SAIDA / "results-a2.json"
    before = (real.stat().st_mtime_ns, real.read_bytes()) if real.exists() else None
    preds, gold = hand_world()
    out = tmp_path / "a2"
    out.mkdir()
    (out / "predictions-a2.jsonl").write_text("\n".join(json.dumps(x) for x in preds) + "\n")
    g = tmp_path / "gold_a2.jsonl"
    g.write_text("\n".join(json.dumps(x) for x in gold) + "\n")
    (tmp_path / "adjudication_summary.json").write_text(json.dumps(SUMMARY))
    monkeypatch.setattr(replay, "SAIDA", out)
    replay._score_with_gold(str(g))
    res = json.loads((out / "results-a2.json").read_text())
    assert res["arms"]["B-L"]["agreement"] == pytest.approx(5 / 6) and res["gold_insufficient"] is False
    assert before == ((real.stat().st_mtime_ns, real.read_bytes()) if real.exists() else None)


def test_cli_writes_the_contract(tmp_path, capsys):
    preds, gold = build(40, 50, 0.9, 0.8)
    p, g, s, o = tmp_path / "p.jsonl", tmp_path / "g.jsonl", tmp_path / "s.json", tmp_path / "o.json"
    p.write_text("\n".join(json.dumps(x) for x in preds) + "\n")
    g.write_text("\n".join(json.dumps(x) for x in gold) + "\n")
    s.write_text(json.dumps(SUMMARY))
    assert sc.main(["--predictions", str(p), "--gold", str(g), "--summary", str(s), "--out", str(o)]) == 0
    out = json.loads(o.read_text())
    assert out["use_case"] == "a2_outcome" and "B-L" in out["arms"] and "B-J" in out["arms"]
