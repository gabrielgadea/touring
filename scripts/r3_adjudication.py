#!/usr/bin/env python3
"""S-5 — cross-vendor adjudication of the A2 audit sample: blind packs, Cohen's κ, gold, tie-break.

    r3_adjudication.py pack  --sample audit_sample.jsonl --out DIR [--seed N]
    r3_adjudication.py merge --adj adj_kimi.jsonl --adj adj_minimax.jsonl --out DIR [--tiebreak adj_tiebreak.jsonl]

Why two raters from OTHER model families: the agent that produced the actions is Claude, so the
judges are Kimi K3 and MiniMax — independent of it and of each other (CRITERIOS §0, lesson 5).

Definitions pinned here BEFORE any label exists (they refine, never move, a threshold):
  * the primary metric is BINARY (`followed` vs {`ignored`, `bypass`}) — E1.2 — so κ is computed on
    that same binary and `ignored` vs `bypass` is NOT a disagreement;
  * an item either rater calls `indeterminate` is excluded from κ and gold, and COUNTED;
  * gold = agreement of the two raters; each disagreement goes to a tie-break queue and is closed by
    a third judge (Claude Sonnet), recorded with provenance `tiebreak`;
  * κ below the pre-registered floor (0.60) declares the gold INSUFFICIENT — A2 then does not conclude.
"""
import argparse
import json
import random
from pathlib import Path

from r3_use_cases import REGISTRY, SEED

LABELS = ("followed", "ignored", "bypass", "indeterminate")
KAPPA_FLOOR = REGISTRY["a2_outcome"]["floors"]["kappa_gold"]
BLIND_FIELDS = ("item_id", "nudge_head", "cmds")  # nothing else ever reaches a rater

INSTRUCTIONS = """# Adjudicação cega — A2 (a oferta de rota foi seguida?)

Para cada item você recebe o texto do nudge e os PRÓXIMOS 3 comandos Bash da mesma sessão.
Responda UMA linha JSON por item em `adj_<seu-nome>.jsonl`: {"item_id": "...", "label": "..."}.

- `followed`      — nas 3 chamadas o agente executou a ação recomendada (a mesma rota, ou equivalente semântico).
- `ignored`       — executou outra coisa; a ação recomendada não aparece.
- `bypass`        — executou a ação por uma rota NÃO sancionada (ex.: python3 inline em vez de `touring run`).
- `indeterminate` — as 3 chamadas não bastam para decidir.

Não há resposta "certa" escondida: julgue só pelo que está no item. Não consulte nenhum outro arquivo.
"""


def cohen_kappa(a, b) -> float:
    n = len(a)
    assert n == len(b) and n > 0, "kappa needs two equal, non-empty label lists"
    po = sum(x == y for x, y in zip(a, b)) / n
    pe = sum((a.count(c) / n) * (b.count(c) / n) for c in set(a) | set(b))
    if pe == 1.0:
        return 1.0 if po == 1.0 else 0.0
    return (po - pe) / (1.0 - pe)


def pack(sample, out_dir, seed=SEED):
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    rows = list(sample)
    random.Random(seed).shuffle(rows)  # the order carries no information about B0 or time
    with (out / "pack_blind.jsonl").open("w") as f:
        for r in rows:
            f.write(json.dumps({k: r[k] for k in BLIND_FIELDS}, ensure_ascii=False) + "\n")
    with (out / "key.jsonl").open("w") as f:  # hidden fields: raters never receive this file
        for r in rows:
            f.write(json.dumps({k: v for k, v in r.items() if k not in BLIND_FIELDS[1:]}, ensure_ascii=False) + "\n")
    (out / "INSTRUCOES.md").write_text(INSTRUCTIONS)
    return out


def _binary(label):
    return label == "followed"


def merge(adj_a, adj_b):
    a = {r["item_id"]: r["label"] for r in adj_a}
    b = {r["item_id"]: r["label"] for r in adj_b}
    ids = sorted(set(a) & set(b))
    bad = [i for i in ids if a[i] not in LABELS or b[i] not in LABELS]
    if bad:
        raise ValueError(f"labels outside {LABELS}: {bad[:3]}")
    scored = [i for i in ids if "indeterminate" not in (a[i], b[i])]
    xs = [_binary(a[i]) for i in scored]
    ys = [_binary(b[i]) for i in scored]
    kappa = cohen_kappa(xs, ys) if scored else 0.0
    three = [i for i in scored if a[i] != "followed" and b[i] != "followed"]
    gold, tie = [], []
    for i, x, y in zip(scored, xs, ys):
        if x == y:
            gold.append({"item_id": i, "followed": x, "provenance": "agree", "labels": [a[i], b[i]]})
        else:
            tie.append({"item_id": i, "a": a[i], "b": b[i]})
    return {
        "kappa": kappa, "kappa_floor": KAPPA_FLOOR, "gold_insufficient": round(kappa, 9) < KAPPA_FLOOR,
        "n_items": len(ids), "n_scored": len(scored), "n_indeterminate": len(ids) - len(scored),
        "n_ignored_vs_bypass_only": len(three), "gold": gold, "tiebreak": tie,
    }


def finalize(merged, tiebreak_labels):
    labels = {r["item_id"]: r["label"] for r in tiebreak_labels}
    gold = list(merged["gold"])
    for t in merged["tiebreak"]:
        lab = labels.get(t["item_id"])
        if lab is None or lab == "indeterminate":
            continue  # unresolved stays out of the gold (counted by the caller)
        gold.append({"item_id": t["item_id"], "followed": _binary(lab), "provenance": "tiebreak",
                     "labels": [t["a"], t["b"], lab]})
    return sorted(gold, key=lambda g: g["item_id"])


def _read(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def main(argv=None):
    p = argparse.ArgumentParser(prog="r3_adjudication")
    sub = p.add_subparsers(dest="cmd", required=True)
    pk = sub.add_parser("pack")
    pk.add_argument("--sample", required=True)
    pk.add_argument("--out", required=True)
    pk.add_argument("--seed", type=int, default=SEED)
    mg = sub.add_parser("merge")
    mg.add_argument("--adj", action="append", required=True)
    mg.add_argument("--out", required=True)
    mg.add_argument("--tiebreak")
    a = p.parse_args(argv)
    if a.cmd == "pack":
        pack(_read(a.sample), a.out, a.seed)
        return 0
    if len(a.adj) != 2:
        p.error("merge needs exactly two --adj files (the two independent raters)")
    res = merge(_read(a.adj[0]), _read(a.adj[1]))
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "tiebreak_queue.jsonl").write_text("".join(json.dumps(t) + "\n" for t in res["tiebreak"]))
    gold = finalize(res, _read(a.tiebreak) if a.tiebreak else [])
    (out / "gold_a2.jsonl").write_text("".join(json.dumps(g) + "\n" for g in gold))
    summary = {k: v for k, v in res.items() if k not in ("gold", "tiebreak")}
    summary.update(n_gold=len(gold), n_tiebreak_open=len(res["tiebreak"]) - sum(g["provenance"] == "tiebreak" for g in gold))
    (out / "adjudication_summary.json").write_text(json.dumps(summary, indent=1))
    print(json.dumps(summary))
    return 2 if res["gold_insufficient"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
