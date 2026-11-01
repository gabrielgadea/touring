#!/usr/bin/env python3
"""A2 scorer (CRITERIOS Errata 8): predictions + gold -> the `results-a2.json` contract of r3_results_gate.

Pure and deterministic: no model call, no API call, seeded bootstrap. It never decides a verdict — the gate does.

    r3_score_a2.py --predictions predictions-a2.jsonl --gold gold_a2.jsonl [--summary adjudication_summary.json] --out results-a2.json
"""
import argparse
import json
import random
import statistics as st
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_results_gate as gate  # noqa: E402

SEED = 20261002          # E8.5 / §2
N_BOOT = 10000
PRECISION_MIN = 0.90     # C-A2-3, used to choose tau (E8.3)
ARMS = ("B-L", "B-J")
KEEP = ("agreement", "delta_vs_b0", "delta_ci_low", "precision", "coverage", "tau", "tau_feasible", "n_test")


def load_jsonl(path) -> list:
    return [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]


def auc(pos, neg):
    """Mann-Whitney AUC, ties count half (E8.8). None when a class is empty."""
    if not pos or not neg:
        return None
    return sum((p > n) + 0.5 * (p == n) for p in pos for n in neg) / (len(pos) * len(neg))


def _cov_prec(rows, tau):
    covered = [g for p, g in rows if p >= tau]
    return (len(covered) / len(rows) if rows else 0.0), (sum(covered) / len(covered) if covered else 0.0)


def _agree(rows, tau):
    return sum((p >= tau) == g for p, g in rows) / len(rows) if rows else 0.0


def _tau_entry(val, tau, feasible):
    cov, prec = _cov_prec(val, tau)
    return {"tau": tau, "feasible": feasible, "coverage": cov, "precision": prec, "val_agreement": _agree(val, tau)}


def choose_tau(val) -> dict:
    """E8.3: max coverage s.t. precision >= 0.90 (tie -> higher tau); infeasible -> max val agreement (tie -> higher tau)."""
    if not val:
        return {"tau": None, "feasible": False, "coverage": 0.0, "precision": 0.0, "val_agreement": 0.0}
    grid = sorted({p for p, _ in val}, reverse=True)
    feasible = [(_cov_prec(val, t)[0], t) for t in grid if _cov_prec(val, t)[1] >= PRECISION_MIN and _cov_prec(val, t)[0] > 0]
    if feasible:
        return _tau_entry(val, max(feasible, key=lambda ct: (ct[0], ct[1]))[1], True)
    return _tau_entry(val, max([float("inf")] + grid, key=lambda t: (_agree(val, t), t)), False)


def _p95(vals):
    return round(st.quantiles(vals, n=20)[-1], 2) if len(vals) >= 20 else (round(max(vals), 2) if vals else None)


def _rows(preds, gold, arm, split):
    out = []
    for r in preds:
        a = r.get(arm) or {}
        if r.get("split") == split and r["item_id"] in gold and "p_followed" in a and "erro" not in a:
            out.append((a["p_followed"], gold[r["item_id"]], r["b0"] == "followed"))
    return out


def _ci_low(model_ok, b0_ok):
    """E8.5: paired bootstrap of (model correct - B0 correct) over the test items."""
    d = [int(m) - int(b) for m, b in zip(model_ok, b0_ok)]
    rng, n = random.Random(SEED), len(d)
    deltas = sorted(sum(rng.choices(d, k=n)) / n for _ in range(N_BOOT))
    return deltas[int(0.025 * N_BOOT)]


def _strata(b0_flags, m_ok, b_ok):
    """E8.6: by B0 decision, with the stratum's own n."""
    out = {}
    for name, flag in (("followed", True), ("not_followed", False)):
        idx = [i for i, b in enumerate(b0_flags) if b == flag]
        out[name] = {"n_test": len(idx),
                     "agreement_model": sum(m_ok[i] for i in idx) / len(idx) if idx else None,
                     "agreement_b0": sum(b_ok[i] for i in idx) / len(idx) if idx else None}
    return out


def score_arm(arm, preds, gold) -> dict:
    val = [(p, g) for p, g, _ in _rows(preds, gold, arm, "audit-val")]
    test = _rows(preds, gold, arm, "audit-test")
    if not test:
        raise ValueError(f"{arm}: no scored item in audit-test")
    t = choose_tau(val)
    tau = float("inf") if t["tau"] is None else t["tau"]
    m_ok = [(p >= tau) == g for p, g, _ in test]
    b_ok = [b == g for _, g, b in test]
    a_m, a_b = sum(m_ok) / len(test), sum(b_ok) / len(test)
    cov, prec = _cov_prec([(p, g) for p, g, _ in test], tau)
    lat = [r[arm]["latency_ms"] for r in preds if "latency_ms" in (r.get(arm) or {})]
    return {"agreement": a_m, "delta_vs_b0": a_m - a_b, "delta_ci_low": _ci_low(m_ok, b_ok), "precision": prec, "coverage": cov,
            "strata": _strata([b for _, _, b in test], m_ok, b_ok), "latency_p95_ms": _p95(lat),
            "tau": "inf" if tau == float("inf") else tau, "tau_feasible": t["feasible"],
            "tau_val_coverage": t["coverage"], "tau_val_precision": t["precision"],
            "n_test": len(test), "n_val": len(val), "agreement_b0": a_b}


def _population(preds, gold, gold_rows) -> dict:
    in_gold = [r for r in preds if r["item_id"] in gold]
    prov = lambda p: sum(1 for g in gold_rows if g.get("provenance") == p and g["item_id"] in gold)  # noqa: E731
    return {"n_gold": len(gold), "n_test": sum(1 for r in in_gold if r.get("split") == "audit-test"),
            "n_val": sum(1 for r in in_gold if r.get("split") == "audit-val"),
            "n_gold_agree": prov("agree"), "n_gold_tiebreak": prov("tiebreak")}


def _b0_block(preds, gold) -> dict:
    hit = lambda r: (r["b0"] == "followed") == gold[r["item_id"]]  # noqa: E731
    test = [hit(r) for r in preds if r.get("split") == "audit-test" and r["item_id"] in gold]
    full = [hit(r) for r in preds if r["item_id"] in gold]
    return {"agreement": sum(test) / len(test), "agreement_full_gold": sum(full) / len(full)}


def _polarity(preds, gold, arms) -> dict:
    def scores(arm, want):
        return [r[arm]["p_followed"] for r in preds if gold.get(r["item_id"]) is want and "p_followed" in (r.get(arm) or {})]
    return {a: auc(scores(a, True), scores(a, False)) for a in arms}


def _sensitivity_block(preds, gold_rows, summary, arms) -> dict:
    """E8.7: secondary reading without the tie-break; never alters the primary fields."""
    sub = score(preds, [g for g in gold_rows if g.get("provenance") == "agree"], summary, arms, _sensitivity=False)
    return {"n_gold": sub["population"]["n_gold"], "verdict": gate.verdict_a2(sub)["verdict"], "b0": sub["b0"],
            "arms": {a: {k: e[k] for k in KEEP} for a, e in sub["arms"].items()}}


def score(preds, gold_rows, summary, arms=ARMS, _sensitivity=True) -> dict:
    gold = {g["item_id"]: g["followed"] for g in gold_rows if isinstance(g.get("followed"), bool)}
    res = {"use_case": "a2_outcome", "kappa": summary.get("kappa"), "gold_insufficient": bool(summary.get("gold_insufficient")),
           "n_indeterminate": summary.get("n_indeterminate", 0), "b0": {"agreement": None}, "arms": {},
           "population": _population(preds, gold, gold_rows)}
    if res["gold_insufficient"]:
        return res
    res["b0"] = _b0_block(preds, gold)
    res["arms"] = {a: score_arm(a, preds, gold) for a in arms}
    res["polarity_auc"] = _polarity(preds, gold, arms)
    if _sensitivity:
        res["sensitivity_agree_only"] = _sensitivity_block(preds, gold_rows, summary, arms)
    return res


def score_files(predictions, gold, summary=None) -> dict:
    summ = Path(summary) if summary else Path(gold).with_name("adjudication_summary.json")
    return score(load_jsonl(predictions), load_jsonl(gold), json.loads(summ.read_text(encoding="utf-8")))


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--predictions", required=True)
    ap.add_argument("--gold", required=True)
    ap.add_argument("--summary")
    ap.add_argument("--out", required=True)
    a = ap.parse_args(argv)
    res = score_files(a.predictions, a.gold, a.summary)
    Path(a.out).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"-> {a.out} (gold {res['population']['n_gold']} · test {res['population']['n_test']} · κ {res['kappa']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
