#!/usr/bin/env python3
"""Errata 10 — sensitivity gold from THREE blind raters (rater 1 MiniMax, rater 2 Claude, the blind Sonnet judge).

The registered gold (`gold_a2.jsonl`) is NOT replaced: this builds an ALTERNATIVE gold by majority of the raters, with the SAME
binary projection (followed x not followed) and the SAME exclusions as `r3_adjudication.merge`, scores the frozen predictions against
it with the pure scorer, and takes every verdict from the gate (`r3_results_gate.verdict_a2`), never from prose.
"""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_adjudication as adj  # noqa: E402
import r3_results_gate as gate  # noqa: E402
import r3_score_a2 as score_a2  # noqa: E402

NAMES = ("r1", "r2", "r3")
PAIRS = ((0, 1), (0, 2), (1, 2))


def _labels(rows) -> dict:
    out = {r["item_id"]: r["label"] for r in rows}
    bad = sorted(i for i, lab in out.items() if lab not in adj.LABELS)
    if bad:
        raise ValueError(f"labels outside {adj.LABELS}: {bad[:3]}")
    return out


def build(r1, r2, r3) -> dict:
    """Majority gold over the items all three raters labelled. An `indeterminate` rater abstains; fewer than two valid votes or a
    tie excludes the item; unanimity among the valid votes is provenance `agree`, a 2-1 split is `tiebreak` (so the scorer's own
    "agree only" reading is the unanimous reading)."""
    rows = (r1, r2, r3)
    labs = [_labels(r) for r in rows]
    ids = sorted(set(labs[0]) & set(labs[1]) & set(labs[2]))
    gold = []
    for i in ids:
        got = [lab[i] for lab in labs]
        votes = [adj._binary(x) for x in got if x != "indeterminate"]
        yes, no = votes.count(True), votes.count(False)
        if len(votes) < 2 or yes == no:
            continue
        gold.append({"item_id": i, "followed": yes > no, "provenance": "tiebreak" if yes and no else "agree", "labels": got})
    pairwise = {f"{NAMES[a]}_{NAMES[b]}": adj.merge(rows[a], rows[b])["kappa"] for a, b in PAIRS}
    low = min(pairwise.values())
    summary = {"kappa": low, "kappa_floor": adj.KAPPA_FLOOR, "pairwise_kappa": pairwise,
               "min_kappa_below_floor": round(low, 9) < adj.KAPPA_FLOOR,
               "gold_insufficient": False,   # a sensitivity reading is scored regardless; the floor is REPORTED above
               "n_items": len(ids), "n_scored": len(gold), "n_indeterminate": len(ids) - len(gold)}
    return {"gold": gold, "summary": summary}


def report(gold_path, predictions, registered) -> dict:
    """Scores the frozen predictions against the alternative gold; every verdict comes from the gate."""
    res = score_a2.score_files(predictions, gold_path)
    unanimous = (res.get("sensitivity_agree_only") or {}).get("verdict")
    summ = Path(gold_path).with_name("adjudication_summary.json")
    return {"summary": json.loads(summ.read_text(encoding="utf-8")) if summ.exists() else None,
            "verdict_registered": gate.verdict_a2(json.loads(Path(registered).read_text(encoding="utf-8")))["verdict"],
            "verdict_alt_majority": gate.verdict_a2(res)["verdict"], "verdict_alt_unanimous": unanimous, "results": res}


def _read(path):
    return [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    for n in NAMES:
        ap.add_argument(f"--{n}", required=True, help=f"adj_*.jsonl of rater {n[1]}")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--predictions", help="predictions-a2.jsonl (frozen); with --registered also scores")
    ap.add_argument("--registered", help="results-a2-gold.json of the registered gold, for the side-by-side verdict")
    a = ap.parse_args(argv)
    out = build(_read(a.r1), _read(a.r2), _read(a.r3))
    d = Path(a.out_dir)
    d.mkdir(parents=True, exist_ok=True)
    (d / "gold_alt3.jsonl").write_text("".join(json.dumps(g, ensure_ascii=False) + "\n" for g in out["gold"]), encoding="utf-8")
    (d / "adjudication_summary.json").write_text(json.dumps(out["summary"], ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(out["summary"], ensure_ascii=False))
    if a.predictions and a.registered:
        rep = report(d / "gold_alt3.jsonl", a.predictions, a.registered)
        (d / "results-a2-alt3.json").write_text(json.dumps(rep["results"], ensure_ascii=False, indent=1), encoding="utf-8")
        print(json.dumps({k: v for k, v in rep.items() if k != "results"}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
