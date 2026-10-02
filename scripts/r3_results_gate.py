#!/usr/bin/env python3
"""S-14 — mechanical verdicts for r3, computed from the FROZEN thresholds in r3_use_cases.REGISTRY.

    r3_results_gate.py a2 results-a2.json      r3_results_gate.py a1 results-a1.json

The verdict is never written by hand: change a floor and the verdict follows (the floors are READ
from the registry at call time, not copied). Comparisons use round(x, 9) so a value sitting exactly
on a pre-registered floor passes it (0.05 passes "≥ 0.05") without floating-point accidents.

Results contract (what S-6 / S-7 must emit)
  results-a2.json: {use_case, kappa, gold_insufficient, n_indeterminate, b0:{agreement},
                    arms:{"B-L"|"B-J": {agreement, delta_vs_b0, delta_ci_low, precision, coverage,
                                        strata:{<name>:{n_test, agreement_model, agreement_b0}},
                                        latency_p95_ms}}}
  results-a1.json: {use_case, arms:{"B-D": {bytes_saved, followed_recall, exempt_retained,
                                            harm:{lower_ci_a, lower_ci_b}},
                                    "B-L"|"B-J": {bytes_saved, followed_recall, exempt_retained}}}
"""
import json
import sys
from pathlib import Path

import r3_use_cases as uc

MIN_STRATUM_N = 20  # C-A2-2 judges only strata with n_test >= 20 (CRITERIOS Errata 1, E1.3)
LOCAL_ARMS = {"B-L"}


def _ge(x, floor):
    return round(x, 9) >= round(floor, 9)


def verdict_a2(r: dict) -> dict:
    f = uc.REGISTRY["a2_outcome"]["floors"]
    base = {"use_case": "a2_outcome", "kappa": r.get("kappa"), "n_indeterminate": r.get("n_indeterminate"), "arms": {}}
    if r.get("gold_insufficient"):
        return {**base, "verdict": "GOLD_INSUFICIENTE"}
    if _ge(r["b0"]["agreement"], f["b0_sufficient_agreement"]):
        return {**base, "verdict": "REGEX_BASTA"}
    passing, c1_any = [], False
    for name, a in r["arms"].items():
        c1 = _ge(a["delta_vs_b0"], f["delta_agreement"]) and a["delta_ci_low"] > 0
        c2 = all(_ge(s["agreement_model"] - s["agreement_b0"], f["stratum_regression"])
                 for s in a.get("strata", {}).values() if s["n_test"] >= MIN_STRATUM_N)
        c3 = _ge(a["precision"], f["precision"]) and _ge(a["coverage"], f["coverage"])
        cap = f["p95_ms_local"] if name in LOCAL_ARMS else f["p95_ms_api"]
        c4 = None if a.get("latency_p95_ms") is None else a["latency_p95_ms"] <= cap  # reported, never a quality failure
        ok = c1 and c2 and c3
        base["arms"][name] = {"C-A2-1": c1, "C-A2-2": c2, "C-A2-3": c3, "C-A2-4": c4, "passes": ok}
        c1_any |= c1
        if ok:
            passing.append((a["delta_vs_b0"], name))
    if passing:
        return {**base, "verdict": "PASSA", "best_arm": max(passing)[1]}
    return {**base, "verdict": "PARCIAL" if c1_any else "NAO_ADOTADO_PELOS_DADOS"}


def verdict_a1(r: dict) -> dict:
    f = uc.REGISTRY["a1_relevance"]["floors"]
    out = {"use_case": "a1_relevance", "arms": {}}
    for name, a in r["arms"].items():
        out["arms"][name] = {"valid": _ge(a["exempt_retained"], f["exempt_retained"])}  # one EXEMPT drop kills the arm
    d = r["arms"]["B-D"]
    harm = min(d["harm"]["lower_ci_a"], d["harm"]["lower_ci_b"])  # the WORSE of the two raters decides
    c1 = (out["arms"]["B-D"]["valid"] and _ge(d["bytes_saved"], f["bytes_saved_rule"])
          and _ge(d["followed_recall"], f["followed_recall_rule"]))
    c2 = _ge(harm, f["harm_lower_ci"])
    out["arms"]["B-D"].update({"C-A1-1": c1, "C-A1-2": c2})
    if not (c1 and c2):
        return {**out, "verdict": "NAO_ADOTADO_PELOS_DADOS"}
    winners = []
    for name, a in r["arms"].items():
        if name == "B-D":
            continue
        inc = a["bytes_saved"] - d["bytes_saved"]  # increment over the RULE, never over B0
        c3 = (out["arms"][name]["valid"] and _ge(a["followed_recall"], f["followed_recall_model"])
              and _ge(inc, f["model_increment"]))
        out["arms"][name].update({"C-A1-3": c3, "increment_over_b_d": round(inc, 9)})
        if c3:
            winners.append((a["bytes_saved"], name))
    if winners:
        return {**out, "verdict": "MODELO_PASSA", "best_model_arm": max(winners)[1]}
    return {**out, "verdict": "REGRA_BASTA"}


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if len(argv) != 2 or argv[0] not in ("a1", "a2"):
        print("usage: r3_results_gate.py {a1|a2} results.json", file=sys.stderr)
        return 64
    results = json.loads(Path(argv[1]).read_text())
    print(json.dumps((verdict_a1 if argv[0] == "a1" else verdict_a2)(results), indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
