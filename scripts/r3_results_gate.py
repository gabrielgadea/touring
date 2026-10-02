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
    arms = {name: _a2_arm(name, a, f) for name, a in r["arms"].items()}
    base["arms"] = arms
    passing = [(r["arms"][n]["delta_vs_b0"], n) for n, e in arms.items() if e["passes"]]
    if passing:
        return {**base, "verdict": "PASSA", "best_arm": max(passing)[1]}
    # PASSA > PARCIAL > INCONCLUSIVO_POR_POTENCIA > NAO_ADOTADO_PELOS_DADOS (CRITERIOS Errata 3, E3.2)
    if any(e["C-A2-1"] for e in arms.values()):
        return {**base, "verdict": "PARCIAL"}
    under = any(e["underpowered"] for e in arms.values())
    return {**base, "verdict": "INCONCLUSIVO_POR_POTENCIA" if under else "NAO_ADOTADO_PELOS_DADOS"}


def _a2_arm(name, a, f):
    """The four pre-registered criteria for ONE model arm (plus the Errata 3 underpowered flag)."""
    c1 = _ge(a["delta_vs_b0"], f["delta_agreement"]) and a["delta_ci_low"] > 0
    # Errata 3 (E3.2): the effect is at/above the floor but the CI crosses zero ⇒ the sample cannot decide
    under = _ge(a["delta_vs_b0"], f["delta_agreement"]) and not a["delta_ci_low"] > 0
    c2 = all(_ge(s["agreement_model"] - s["agreement_b0"], f["stratum_regression"])
             for s in a.get("strata", {}).values() if s["n_test"] >= MIN_STRATUM_N)
    c3 = _ge(a["precision"], f["precision"]) and _ge(a["coverage"], f["coverage"])
    cap = f["p95_ms_local"] if name in LOCAL_ARMS else f["p95_ms_api"]
    c4 = None if a.get("latency_p95_ms") is None else a["latency_p95_ms"] <= cap  # reported, never a quality failure
    return {"C-A2-1": c1, "C-A2-2": c2, "C-A2-3": c3, "C-A2-4": c4, "passes": c1 and c2 and c3, "underpowered": under}


def _and3(*vals):
    """Three-valued AND: a KNOWN failure decides (False); an unknown is never a pass (None); only all-known-True is True."""
    if any(v is False for v in vals):
        return False
    return None if any(v is None for v in vals) else True


def _known_ge(x, floor):
    return None if x is None else _ge(x, floor)   # an absent measurement is unknown, not zero and not a pass


def verdict_a1(r: dict) -> dict:
    """The REAL replay emits no `followed_recall` (it needs the A2 gold) and `harm: null` (it needs the cross-vendor
    raters), so those are UNKNOWN — and a known failure of another clause still decides."""
    f = uc.REGISTRY["a1_relevance"]["floors"]
    out = {"use_case": "a1_relevance", "arms": {}}
    for name, a in r["arms"].items():
        out["arms"][name] = {"valid": _ge(a["exempt_retained"], f["exempt_retained"])}  # one EXEMPT drop kills the arm
    d = r["arms"]["B-D"]
    c1 = _and3(out["arms"]["B-D"]["valid"], _known_ge(d.get("bytes_saved"), f["bytes_saved_rule"]),
               _known_ge(d.get("followed_recall"), f["followed_recall_rule"]))
    h = d.get("harm")
    c2 = None if h is None else _ge(min(h["lower_ci_a"], h["lower_ci_b"]), f["harm_lower_ci"])  # the WORSE rater decides
    out["arms"]["B-D"].update({"C-A1-1": c1, "C-A1-2": c2})
    out["harm_moot"] = c1 is False and c2 is None   # the harm check cannot change a verdict C-A1-1 already fixed
    if c1 is False or c2 is False:
        return {**out, "verdict": "NAO_ADOTADO_PELOS_DADOS"}
    if c1 is None:
        return {**out, "verdict": "INCONCLUSIVO_SEM_GOLD"}
    if c2 is None:
        return {**out, "verdict": "INCONCLUSIVO_SEM_HARM"}
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
