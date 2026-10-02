#!/usr/bin/env python3
"""Errata 11 — Jev stability recheck: the distinct REDACTED payloads of the A2 replay asked LIVE (client cache bypassed), compared with the
cached answers the B-J arm was scored on.

Provenance is already closed by the client journal (25 real API calls on 02/10, `route_taken: api`); what this adds is STABILITY — does the
same payload get the same probability? Hard cap on calls (Gabriel, 02/10: up to ~50); a failed call is an ERROR, never `p = 0`; the verdict
of any rescored arm comes from the gate.
"""
import argparse
import hashlib
import json
import statistics
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

TOLERANCE = 0.05          # pre-registered (Errata 11): |p_live - p_cached| <= 0.05 for EVERY payload => "estavel"
MAX_CALLS = 50            # Gabriel's authorisation, 02/10/2026
MODEL = "jev-1.13.0"


def payload_hash(payload: dict) -> str:
    return hashlib.sha256(json.dumps(payload, sort_keys=True, ensure_ascii=False).encode()).hexdigest()[:8]


def distinct_payloads(payloads) -> dict:
    out = {}
    for p in payloads:
        out.setdefault(payload_hash(p), p)
    return out


def has_answer(resp, qid: str) -> bool:
    """A response is usable only if it carries the answer to OUR question (the client fails open to something empty)."""
    if not isinstance(resp, dict):
        return False
    return qid in resp or qid in (resp.get("answers") or {})


def compare(cached: dict, live: dict) -> dict:
    keys = sorted(set(cached) & set(live))
    diffs = [abs(cached[k] - live[k]) for k in keys]
    return {"n": len(keys), "n_equal": sum(d == 0 for d in diffs), "max_abs_diff": max(diffs) if diffs else 0.0,
            "mean_abs_diff": statistics.fmean(diffs) if diffs else 0.0, "by_hash": {k: [cached[k], live[k]] for k in keys}}


def needs_second_pass(cmp: dict) -> bool:
    return cmp["max_abs_diff"] > 0


def stability(cmp: dict) -> str:
    return "estavel" if cmp["max_abs_diff"] <= TOLERANCE else "instavel"


def run(payloads: dict, call, max_calls: int = MAX_CALLS, already_spent: int = 0) -> dict:
    """One live pass. `call(payload)` returns a probability or None (failure). The cap is checked BEFORE every call."""
    live, errors, n = {}, {}, 0
    for h, payload in payloads.items():
        if already_spent + n >= max_calls:
            raise RuntimeError(f"call cap {max_calls} reached after {already_spent + n} calls: the next call does not happen")
        n += 1
        try:
            p = call(payload)
        except Exception as e:  # noqa: BLE001 — a failed call is data, and it still spent the call
            errors[h] = f"{type(e).__name__}: {str(e)[:80]}"
            continue
        if p is None:
            errors[h] = "no valid answer"
        else:
            live[h] = p
    return {"live": live, "errors": errors, "n_calls": n}


def substitute(preds, hash_by_item: dict, live: dict) -> list:
    """The predictions with the B-J probability replaced by the LIVE one where the payload was re-asked; nothing else changes."""
    out = []
    for r in preds:
        h = hash_by_item.get(r["item_id"])
        if h in live:
            r = {**r, "B-J": {**r["B-J"], "p_followed": live[h]}}
        out.append(r)
    return out


def _inputs(rp, jev_client, schema) -> dict:
    """The items, their redacted payloads (hash by item), the distinct ones and the cached probability of each — or what is missing."""
    items = rp.carregar_amostra()
    payload_by_item = {it["item_id"]: rp.payload_of(rp.evento_de(it)) for it in items}
    distinct = distinct_payloads(payload_by_item.values())
    bad = {h: schema.validate_jev_payload(p) for h, p in distinct.items() if schema.validate_jev_payload(p)}
    if bad:
        raise SystemExit(f"payload with free text would leave the machine: {bad}")
    spec = {rp.QID: {"type": "noul", "instructions": "Did the agent follow the recommended route after the nudge?"}}
    cache = jev_client._read_cache(jev_client.DEFAULT_CACHE_FILE)
    cached, missing = {}, []
    for h, p in distinct.items():
        k = jev_client._cache_key(p, spec, MODEL)
        if k in cache:
            cached[h] = rp._prob_de_resposta(cache[k], rp.QID)
        else:
            missing.append(h)
    return {"items": items, "hash_by_item": {i: payload_hash(p) for i, p in payload_by_item.items()}, "distinct": distinct, "spec": spec,
            "cached": cached, "missing": missing}


def _live(inp: dict, out: Path, cap: int, rp, jev_client) -> tuple:
    """Pass 1 (always), pass 2 (only when pass 1 differs from the cache and the cap allows). Returns (report, live probabilities)."""
    journal, lats, n = out / "jev_journal_recheck.jsonl", [], {"i": 0}

    def call(payload):
        n["i"] += 1
        t0 = time.perf_counter()
        r = jev_client.system_one(state=payload, questions=inp["spec"], model=MODEL, timeout=20.0,
                                  cache_file=out / f"empty_cache_{n['i']}.json", journal_file=journal)
        lats.append((time.perf_counter() - t0) * 1000)
        return rp._prob_de_resposta(r, rp.QID) if has_answer(r, rp.QID) else None

    distinct = inp["distinct"]
    p1 = run(distinct, call, cap)
    cmp1 = compare(inp["cached"], p1["live"])
    report = {"n_distinct": len(distinct), "cap": cap, "pass1": p1, "compare_pass1_vs_cache": cmp1, "stability": stability(cmp1)}
    if needs_second_pass(cmp1) and p1["n_calls"] + len(distinct) <= cap:
        p2 = run(distinct, call, cap, already_spent=p1["n_calls"])
        report["pass2"], report["compare_pass2_vs_pass1"] = p2, compare(p1["live"], p2["live"])
    routes = {}
    for line in journal.read_text().splitlines():
        r = json.loads(line)
        routes[r.get("route_taken")] = routes.get(r.get("route_taken"), 0) + 1
    report.update({"n_calls_total": n["i"], "journal_routes": routes,
                   "latency_ms": {"n": len(lats), "median": statistics.median(lats) if lats else None, "max": max(lats) if lats else None}})
    return report, dict(p1["live"])


def _rescore(inp: dict, live: dict, out: Path, rp) -> dict:
    """B-J rescored with the LIVE probabilities, against the registered gold; the verdict is the gate's."""
    import r3_results_gate as gate
    import r3_score_a2 as score_a2
    preds = [json.loads(line) for line in (rp.SAIDA / "predictions-a2.jsonl").read_text().splitlines() if line.strip()]
    path = out / "predictions-a2-jevlive.jsonl"
    path.write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in substitute(preds, inp["hash_by_item"], live)), encoding="utf-8")
    res = score_a2.score_files(path, Path(rp.CORPUS) / "adj_merge_final" / "gold_a2.jsonl")
    registered = json.loads((rp.SAIDA / "results-a2-gold.json").read_text(encoding="utf-8"))
    return {"verdict_registered": gate.verdict_a2(registered)["verdict"], "verdict_with_live_p": gate.verdict_a2(res)["verdict"],
            "agreement_registered": registered["arms"]["B-J"]["agreement"], "agreement_with_live_p": res["arms"]["B-J"]["agreement"]}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out-dir", default=str(Path.home() / ".claude/touring/r3-data/jev_recheck"))
    ap.add_argument("--max-calls", type=int, default=MAX_CALLS)
    ap.add_argument("--dry-run", action="store_true", help="no API call: counts, hashes and the cached probabilities only")
    a = ap.parse_args(argv)
    if a.max_calls > MAX_CALLS:
        raise SystemExit(f"--max-calls {a.max_calls} exceeds the authorised cap {MAX_CALLS}")
    import jev_client
    import r3_event_schema as schema
    import r3_replay_a2 as rp
    inp = _inputs(rp, jev_client, schema)
    print(json.dumps({"items": len(inp["items"]), "distinct_payloads": len(inp["distinct"]), "cached": len(inp["cached"]),
                      "missing_in_cache": len(inp["missing"])}))
    if inp["missing"] or a.dry_run:
        return 2 if inp["missing"] else 0
    out = Path(a.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    report, live = _live(inp, out, a.max_calls, rp, jev_client)
    report["rescored_B-J"] = _rescore(inp, live, out, rp)
    (out / "recheck.json").write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps({k: report[k] for k in ("stability", "n_calls_total", "latency_ms", "journal_routes", "rescored_B-J")}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
