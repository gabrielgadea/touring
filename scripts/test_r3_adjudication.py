"""S-5 adjudication tooling: blind packs, Cohen's κ, gold, tie-break queue."""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_adjudication as adj  # noqa: E402

SAMPLE = [
    {"item_id": f"i{n:03d}", "nudge_head": "MUST touring run --lang bash --code '<loop>'",
     "cmds": [f"grep -r foo src/{n}", "cat README.md", "touring run --lang bash --code 'ls'"],
     "b0": {"followed": n % 2 == 0, "value": 1.0 if n % 2 == 0 else None}, "mode": "code"}
    for n in range(20)
]


def test_pack_never_leaks_the_b0_label(tmp_path):
    adj.pack(SAMPLE, tmp_path, seed=20261002)
    blind = (tmp_path / "pack_blind.jsonl").read_text()
    for forbidden in ('"b0"', "baseline", '"value"', "followed\":"):
        assert forbidden not in blind, forbidden
    rows = [json.loads(line) for line in blind.splitlines()]
    assert {r["item_id"] for r in rows} == {s["item_id"] for s in SAMPLE}
    # the hidden fields live ONLY in the key file, which raters never receive
    key = [json.loads(line) for line in (tmp_path / "key.jsonl").read_text().splitlines()]
    assert all("b0" in k for k in key)


def test_pack_order_is_a_seeded_shuffle(tmp_path):
    a, b, c = tmp_path / "a", tmp_path / "b", tmp_path / "c"
    adj.pack(SAMPLE, a, seed=20261002)
    adj.pack(SAMPLE, b, seed=20261002)
    adj.pack(SAMPLE, c, seed=7)
    order = lambda d: [json.loads(x)["item_id"] for x in (d / "pack_blind.jsonl").read_text().splitlines()]  # noqa: E731
    assert order(a) == order(b) and order(a) != order(c)
    assert order(a) != [s["item_id"] for s in SAMPLE]  # not the original order (blinds position effects)


def test_kappa_matches_a_hand_computed_case():
    # n=10 binary; p_o = 8/10, p_e = .5*.5 + .5*.5 = .5  =>  kappa = (0.8-0.5)/(1-0.5) = 0.6
    a = [1, 1, 1, 1, 1, 0, 0, 0, 0, 0]
    b = [1, 1, 1, 0, 1, 0, 0, 0, 1, 0]
    assert round(adj.cohen_kappa(a, b), 9) == 0.6
    assert adj.cohen_kappa([1, 0, 1, 0], [1, 0, 1, 0]) == 1.0
    assert adj.cohen_kappa([1, 1, 0, 0], [0, 0, 1, 1]) == -1.0


def _labels(pairs):
    """pairs: list of (rater_a, rater_b) labels -> (adj_a, adj_b) record lists"""
    ra = [{"item_id": f"i{n:03d}", "label": x} for n, (x, _) in enumerate(pairs)]
    rb = [{"item_id": f"i{n:03d}", "label": y} for n, (_, y) in enumerate(pairs)]
    return ra, rb


def test_low_kappa_flags_gold_insufficient():
    pairs = [("followed", "ignored"), ("ignored", "followed")] * 5 + [("followed", "followed"), ("ignored", "ignored")]
    ra, rb = _labels(pairs)
    res = adj.merge(ra, rb)
    assert res["kappa"] < 0.60 and res["gold_insufficient"] is True


def test_boundary_kappa_exactly_at_the_floor_is_sufficient():
    a = ["followed"] * 5 + ["ignored"] * 5
    b = ["followed", "followed", "followed", "ignored", "followed", "ignored", "ignored", "ignored", "followed", "ignored"]
    ra, rb = _labels(list(zip(a, b)))
    res = adj.merge(ra, rb)
    assert round(res["kappa"], 9) == 0.6 and res["gold_insufficient"] is False


def test_indeterminate_is_excluded_from_kappa_gold_and_counted():
    pairs = [("followed", "followed"), ("ignored", "ignored"), ("indeterminate", "followed"), ("followed", "indeterminate")]
    res = adj.merge(*_labels(pairs))
    assert res["n_indeterminate"] == 2 and res["n_scored"] == 2
    assert {g["item_id"] for g in res["gold"]} == {"i000", "i001"}


def test_bypass_and_ignored_both_collapse_to_not_followed():
    pairs = [("bypass", "ignored"), ("followed", "followed"), ("ignored", "bypass"), ("ignored", "ignored")]
    res = adj.merge(*_labels(pairs))
    assert res["tiebreak"] == []                      # bypass vs ignored is NOT a disagreement on the primary metric
    assert {g["item_id"]: g["followed"] for g in res["gold"]}["i000"] is False


def test_disagreements_go_to_the_tiebreak_queue_and_finalize_closes_them():
    pairs = [("followed", "followed")] * 4 + [("followed", "ignored")] + [("ignored", "ignored")] * 4
    res = adj.merge(*_labels(pairs))
    assert [t["item_id"] for t in res["tiebreak"]] == ["i004"]
    assert all(g["provenance"] == "agree" for g in res["gold"])
    final = adj.finalize(res, [{"item_id": "i004", "label": "followed"}])
    by = {g["item_id"]: g for g in final}
    assert by["i004"]["provenance"] == "tiebreak" and by["i004"]["followed"] is True and len(final) == 9
