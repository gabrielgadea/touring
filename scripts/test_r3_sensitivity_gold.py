"""Errata 10 — the sensitivity gold from three blind raters: every rule is proven by a test that fails without it."""
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_sensitivity_gold as sg  # noqa: E402


def R(*labels):
    return [{"item_id": f"i{n}", "label": lab} for n, lab in enumerate(labels)]


F, I, B, X = "followed", "ignored", "bypass", "indeterminate"


def one(r1, r2, r3):
    out = sg.build(R(r1), R(r2), R(r3))
    return out["gold"][0] if out["gold"] else None


def test_three_raters_agreeing_is_agree_with_their_value():
    g = one(F, F, F)
    assert g == {"item_id": "i0", "followed": True, "provenance": "agree", "labels": [F, F, F]}
    assert one(B, I, B)["followed"] is False and one(B, I, B)["provenance"] == "agree"   # ignored and bypass are both "not followed"


def test_two_against_one_is_decided_by_the_majority_and_marked_tiebreak():
    assert (one(F, F, B)["followed"], one(F, F, B)["provenance"]) == (True, "tiebreak")
    assert (one(B, I, F)["followed"], one(B, I, F)["provenance"]) == (False, "tiebreak")
    assert one(F, B, F)["followed"] is True and one(I, F, F)["followed"] is True


def test_an_indeterminate_rater_abstains_the_other_two_still_decide_when_they_agree():
    assert one(X, F, F) == {"item_id": "i0", "followed": True, "provenance": "agree", "labels": [X, F, F]}
    assert one(F, X, F)["followed"] is True and one(B, B, X)["followed"] is False


def test_two_valid_raters_that_disagree_have_no_majority_and_are_excluded():
    out = sg.build(R(X), R(F), R(B))
    assert out["gold"] == [] and out["summary"]["n_scored"] == 0 and out["summary"]["n_indeterminate"] == 1


def test_fewer_than_two_valid_raters_is_excluded():
    assert one(X, X, F) is None and one(X, X, X) is None


def test_only_the_items_every_rater_labelled_count():
    # the THIRD rater is the one that lacks i2: an intersection of only the first two raters would index it and crash
    r1, r2, r3 = R(F, F, F), R(F, F, F), R(F, F)
    out = sg.build(r1, r2, r3)
    assert out["summary"]["n_items"] == 2 and [g["item_id"] for g in out["gold"]] == ["i0", "i1"]


def test_a_label_outside_the_domain_raises_even_on_an_item_not_every_rater_labelled():
    with pytest.raises(ValueError):
        sg.build(R("maybe"), R(F), R(F))
    with pytest.raises(ValueError):           # i9 is in r1 only, so the pairwise `merge` never sees it: only our own check can
        sg.build(R(F) + [{"item_id": "i9", "label": "maybe"}], R(F), R(F))


def world_k02():
    """10 items; r1 == r2 (5 followed / 5 not); r3 flips 4 of them. Hand-computed: κ(r1,r3) = κ(r2,r3) = (0.6 − 0.5) / (1 − 0.5) = 0.2."""
    base = [F] * 5 + [B] * 5
    flipped = base[:]
    flipped[0], flipped[1] = B, B                    # 2 followed → bypass
    flipped[5], flipped[6] = F, F                    # 2 bypass → followed
    return R(*base), R(*base), R(*flipped)


def test_pairwise_kappas_and_the_summary_kappa_is_the_smallest():
    out = sg.build(*world_k02())
    pk = out["summary"]["pairwise_kappa"]
    assert pk["r1_r2"] == pytest.approx(1.0) and pk["r1_r3"] == pytest.approx(0.2) and pk["r2_r3"] == pytest.approx(0.2)
    assert out["summary"]["kappa"] == pytest.approx(0.2)


def test_a_low_kappa_is_reported_but_never_hides_the_scoring():
    s = sg.build(*world_k02())["summary"]
    assert s["min_kappa_below_floor"] is True and s["gold_insufficient"] is False     # sensitivity is scored regardless
    good = sg.build(R(F, F, B, B), R(F, F, B, B), R(F, F, B, B))["summary"]
    assert good["min_kappa_below_floor"] is False and good["kappa"] == pytest.approx(1.0)


def test_the_summary_counts_items_scored_and_excluded():
    out = sg.build(R(F, F, X, F), R(F, B, X, F), R(F, F, F, B))
    s = out["summary"]
    assert (s["n_items"], s["n_scored"], s["n_indeterminate"]) == (4, 3, 1)
    assert [g["provenance"] for g in out["gold"]] == ["agree", "tiebreak", "tiebreak"]


def test_the_cli_writes_the_gold_and_the_summary_next_to_it(tmp_path):
    files = {}
    for name, rows in zip(("r1", "r2", "r3"), (R(F, B), R(F, B), R(F, F))):
        p = tmp_path / f"{name}.jsonl"
        p.write_text("\n".join(json.dumps(r) for r in rows) + "\n")
        files[name] = str(p)
    out_dir = tmp_path / "out"
    assert sg.main(["--r1", files["r1"], "--r2", files["r2"], "--r3", files["r3"], "--out-dir", str(out_dir)]) == 0
    gold = [json.loads(line) for line in (out_dir / "gold_alt3.jsonl").read_text().splitlines()]
    assert [g["item_id"] for g in gold] == ["i0", "i1"] and gold[1]["provenance"] == "tiebreak"
    summ = json.loads((out_dir / "adjudication_summary.json").read_text())
    assert summ["n_scored"] == 2 and "pairwise_kappa" in summ


def test_the_report_carries_the_kappa_summary_next_to_the_verdicts_so_a_low_floor_is_never_silent(tmp_path, monkeypatch):
    fake = {"gold_insufficient": False, "kappa": 0.2, "n_indeterminate": 0, "b0": {"agreement": 1.0}, "arms": {}, "sensitivity_agree_only": {"verdict": "PARCIAL"}}
    monkeypatch.setattr(sg.score_a2, "score_files", lambda predictions, gold: fake)
    (tmp_path / "adjudication_summary.json").write_text(json.dumps({"min_kappa_below_floor": True, "pairwise_kappa": {"r1_r3": 0.2}}))
    registered = tmp_path / "registered.json"
    registered.write_text(json.dumps({"gold_insufficient": True}))
    rep = sg.report(tmp_path / "gold_alt3.jsonl", tmp_path / "preds.jsonl", registered)
    assert rep["summary"] == {"min_kappa_below_floor": True, "pairwise_kappa": {"r1_r3": 0.2}}
    (tmp_path / "adjudication_summary.json").unlink()
    assert sg.report(tmp_path / "gold_alt3.jsonl", tmp_path / "preds.jsonl", registered)["summary"] is None


def test_the_report_takes_every_verdict_from_the_gate_never_from_prose(tmp_path, monkeypatch):
    fake = {"gold_insufficient": False, "kappa": 0.8, "n_indeterminate": 0, "b0": {"agreement": 1.0}, "arms": {},
            "sensitivity_agree_only": {"verdict": "PARCIAL"}}
    monkeypatch.setattr(sg.score_a2, "score_files", lambda predictions, gold: fake)
    registered = tmp_path / "registered.json"
    registered.write_text(json.dumps({"gold_insufficient": True}))
    rep = sg.report(tmp_path / "gold_alt3.jsonl", tmp_path / "preds.jsonl", registered)
    assert rep["verdict_registered"] == "GOLD_INSUFICIENTE"
    assert rep["verdict_alt_majority"] == "REGEX_BASTA" and rep["verdict_alt_unanimous"] == "PARCIAL"   # B0 already enough: the gate says so
