#!/usr/bin/env python3
"""Errata 12 (r4 minimo) — the A2 replay with the B-L arm fed the TRUNCATED TEXT.

CRITERIOS §1 (data policy) says the LOCAL Laya sees text truncated at 200 chars per command; the r3 arm saw only the redacted payload (first
token of each command). This arm sends `Recommended route: <≤200 chars>` and up to 3 `Next commands`, each ≤200 chars, whitespace collapsed first.
Same question as r3 (so only the input differs), scored against the SAME registered gold with the pure scorer; the verdict of the single PRIMARY
arm comes from the gate. Two checkpoints: the English one (repo root — what r3 ran, the primary) and `subfolder="multilingual"` (what the
pre-registration named; secondary). Local CPU only: no text leaves the machine and none is written to any output (predictions are numbers).
"""
import argparse
import json
import statistics
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_replay_a2 as rp  # noqa: E402
import r3_results_gate as gate  # noqa: E402
import r3_score_a2 as score_a2  # noqa: E402

QID = rp.QID
QUESTION = "Did the agent follow the recommended route after the nudge?"
SPEC = {QID: {"type": "noul", "instructions": QUESTION}}
ARMS = ("B-LT-EN", "B-LT-ML")
PRIMARY = "B-LT-EN"
SUBFOLDER = {"B-LT-EN": None, "B-LT-ML": "multilingual"}
MAX_CHARS, MAX_CMDS = 200, 3


def truncate(text, n: int = MAX_CHARS) -> str:
    return " ".join(str(text).split())[:n]


def build_state(item: dict) -> str:
    """The route and up to three following commands, each cut at 200 chars. Nothing else from the item enters: no decision, no session."""
    cmds = [truncate(c) for c in (item.get("cmds") or [])[:MAX_CMDS]]
    lines = ["Recommended route: " + truncate(item.get("rota_proposta", "")), "Next commands:"]
    lines += [f"{i}. {c}" for i, c in enumerate(cmds, 1)] if cmds else ["(none)"]
    return "\n".join(lines)


def _has_answer(resp) -> bool:
    """A response is usable only if it answers OUR question (an empty one would read as p = 0, the plausible-and-absurd number)."""
    for attr in ("answers", "values"):
        sub = getattr(resp, attr, None)
        if isinstance(sub, dict) and QID in sub:
            return True
    return isinstance(resp, dict) and (QID in resp or QID in (resp.get("answers") or {}))


def predict(items, agent) -> dict:
    out = {}
    for it in items:
        t0 = time.perf_counter()
        try:
            r = agent.decide(build_state(it), questions=SPEC, return_details=True)
        except Exception as e:  # noqa: BLE001 — a failed inference is data, not a crash, and never a probability
            out[it["item_id"]] = {"erro": f"{type(e).__name__}: {str(e)[:80]}"}
            continue
        if not _has_answer(r):
            out[it["item_id"]] = {"erro": "no answer to the question"}
            continue
        out[it["item_id"]] = {"p_followed": rp._prob_de_resposta(r, QID), "latency_ms": (time.perf_counter() - t0) * 1000}
    return out


def with_arms(rows, arm_preds: dict) -> list:
    out = []
    for r in rows:
        r = dict(r)
        for arm, preds in arm_preds.items():
            p = preds.get(r["item_id"])
            r[arm] = {"item": r.get("session_hash"), **p} if p else {"item": r.get("session_hash"), "erro": "sem predicao"}
        out.append(r)
    return out


def evaluate(rows, gold_rows, summary) -> dict:
    """Scores both arms; each verdict is the gate's, computed on ONE arm so a secondary arm can never carry the primary hypothesis."""
    res = score_a2.score(rows, gold_rows, summary, arms=ARMS)

    def verdict(arm):
        sub = res if res["gold_insufficient"] else {**res, "arms": {arm: res["arms"][arm]}}
        return gate.verdict_a2(sub)["verdict"]

    return {"results": res, "primary": PRIMARY, "verdict_primary": verdict(PRIMARY), "verdict_secondary": verdict("B-LT-ML")}


def check_alignment(rows, items) -> None:
    """The scored rows and the frozen items must be the SAME set: an item present in one and absent in the other would be dropped in silence."""
    in_rows, in_items = {r["item_id"] for r in rows}, {i["item_id"] for i in items}
    if in_rows != in_items:
        raise SystemExit(f"item_id sets differ: {len(in_rows - in_items)} only in predictions, {len(in_items - in_rows)} only in the sample")


def _load_agent(subfolder):
    import torch  # noqa: PLC0415 — late: importing this module must not pay torch
    torch.set_num_threads(2)
    import laya  # noqa: PLC0415
    return laya.Agent(model_id_or_path="convaiinnovations/laya", device="cpu", **({"subfolder": subfolder} if subfolder else {}))


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out-dir", default=str(Path.home() / ".claude/touring/r4-replay"))
    ap.add_argument("--limit", type=int, default=0, help="first N items only (0 = all); a smoke test, never the registered run")
    a = ap.parse_args(argv)
    items = rp.carregar_amostra()
    items = items[: a.limit] if a.limit else items
    rows = [json.loads(line) for line in (rp.SAIDA / "predictions-a2.jsonl").read_text(encoding="utf-8").splitlines() if line.strip()]
    rows = rows[: len(items)] if a.limit else rows
    check_alignment(rows, items)
    arm_preds = {}
    for arm in ARMS:
        t0 = time.perf_counter()
        arm_preds[arm] = predict(items, _load_agent(SUBFOLDER[arm]))
        errs = sum("erro" in p for p in arm_preds[arm].values())
        print(f"{arm}: {len(arm_preds[arm])} predictions, {errs} errors, {time.perf_counter() - t0:.0f}s", flush=True)
    out = Path(a.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    rows = with_arms(rows, arm_preds)
    (out / "predictions-a2-text.jsonl").write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rows), encoding="utf-8")
    gold_path = Path(rp.CORPUS) / "adj_merge_final" / "gold_a2.jsonl"
    gold = [json.loads(line) for line in gold_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    summary = json.loads((gold_path.parent / "adjudication_summary.json").read_text(encoding="utf-8"))
    ev = evaluate(rows, gold, summary)
    (out / "results-a2-text.json").write_text(json.dumps(ev, ensure_ascii=False, indent=1), encoding="utf-8")
    res = ev["results"]
    lat = {arm: statistics.median([p["latency_ms"] for p in arm_preds[arm].values() if "latency_ms" in p] or [0]) for arm in ARMS}
    print(json.dumps({"verdict_primary": ev["verdict_primary"], "verdict_secondary": ev["verdict_secondary"], "b0": res["b0"],
                      "arms": {k: {m: v[m] for m in ("agreement", "delta_vs_b0", "delta_ci_low", "precision", "coverage", "tau")} for k, v in res["arms"].items()},
                      "polarity_auc": res.get("polarity_auc"), "latency_median_ms": lat, "n_gold": res["population"]["n_gold"]}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
