#!/usr/bin/env python3
"""variant_archive.py — keep what failed, because good paths run through bad nodes.

The loop's L4 layer is named "hill-climb", and hill-climbing is precisely the
baseline that arXiv:2505.22954 (Darwin Godel Machine, ICLR 2026) measures itself
against and beats: **50.0% vs 39.7%** on SWE-bench, with the archive removed being
the single largest ablation of the three they ran (23.0% without it at all).

Their Figure 3 says why in one sentence: "many paths to innovation traverse
lower-performing nodes", and the lineage of the best agent they found contains
two performance dips. A greedy loop prunes exactly those, because at the moment
it sees them they look like regressions.

WHAT THIS IS NOT
----------------
Not the paper's system. The DGM spends ~2 weeks and significant API cost per run
because it has a cheap, repeatable, numeric benchmark to score variants with. Our
signal is `loop_converged.py`, which is expensive and largely binary. Reproducing
the full loop here would be dishonest arithmetic.

What transfers is the cheap half, and it is the half that carries the measured
gain: **stop discarding what fails the gate**. Record the variant with its score,
and let a later run start from it. Everything else — who to try next, when to
stop — stays where it already is.

SCORE
-----
`score` is a float in [0, 1] from whatever graded the attempt: the
`touring-quality` composite, or a convergence verdict reduced to 1.0/0.0. It is
stored as given; this module never invents one.

Usage:
    variant_archive.py record <target> <variant> --score 0.82 [--verdict …] [--parent …]
    variant_archive.py sample <target> [-k 2] [--seed 7]
    variant_archive.py stats  <target>
    variant_archive.py prune  <target> [--keep 200]
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import math
import os
import random
import sys
from pathlib import Path

#: The paper's constants (Appendix C.2), kept as named values so a future tuner
#: can see what was borrowed and what was chosen here.
SIGMOID_SHARPNESS = 10.0   # λ
SIGMOID_MIDPOINT = 0.5     # α₀
DEFAULT_KEEP = 200

ARCHIVE_ROOT = Path.home() / ".claude" / "loop-engineering" / "variants"


def archive_path(target: str, root: Path | None = None) -> Path:
    """One append-only file per target, named by a digest of the target.

    A digest rather than the target string: targets are paths and intents, and
    neither is safe as a filename.
    """
    base = root or ARCHIVE_ROOT
    digest = hashlib.sha256(target.encode("utf-8")).hexdigest()[:16]
    return base / f"{digest}.jsonl"


def variant_id(variant: str) -> str:
    """Deterministic identity of a variant — same content, same id, always.

    Aligns with REGRA #17: identity is derived, never emergent, so the archive
    recognises a variant it has already seen instead of storing it twice.
    """
    return hashlib.sha256(variant.encode("utf-8")).hexdigest()[:16]


def load(target: str, root: Path | None = None) -> list[dict]:
    """Every recorded variant for a target, oldest first. Never raises."""
    path = archive_path(target, root)
    out: list[dict] = []
    try:
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                try:
                    out.append(json.loads(line))
                except ValueError:
                    continue  # one corrupt line must not lose the archive
    except OSError:
        return []
    return out


def record(target: str, variant: str, score: float, verdict: str = "",
           parent: str | None = None, root: Path | None = None,
           terminal: bool = False) -> dict:
    """Archive one attempt WITH its score, whether or not it passed.

    The discipline is the whole point: a rejected variant is a stepping stone, and
    the only way it can be one is if it is still here later.

    ``terminal`` marks a DEAD END — an attempt that cannot produce children,
    because what it tried is impossible rather than merely wrong (the source does
    not exist, the capability is not ours, the approach is barred). The paper's
    archive DISCARDS these outright: *"Only agents that compile successfully and
    retain the ability to edit a given codebase are added to the DGM archive, as
    only they can continue self-modification. All others are discarded"* (§3).

    We keep them, because a rejected thesis with its reason is the material of a
    motivated decision, not a technical by-product. But we never SAMPLE them as
    parents: re-drawing a dead end burns budget rediscovering that it is one.
    """
    entry = {
        "variant_id": variant_id(variant),
        "target": target,
        "score": max(0.0, min(1.0, float(score))),
        "verdict": verdict,
        "parent": parent,
        "terminal": bool(terminal),
        "recorded_at": datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
    }
    path = archive_path(target, root)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(entry, ensure_ascii=False) + "\n")
        fh.flush()
        os.fsync(fh.fileno())
    return entry


def _latest_by_variant(entries: list[dict]) -> dict[str, dict]:
    """One row per variant: the most recent score wins, earlier ones are history."""
    out: dict[str, dict] = {}
    for e in entries:
        out[e.get("variant_id", "")] = e
    out.pop("", None)
    return out


def lineage(target: str, root: Path | None = None) -> dict:
    """The ancestry of the best variant, DIPS counted — Fig. 2 asked of our archive.

    The paper's empirical anchor: the best agent's lineage *"includes two dips in
    performance"* — many paths to a good variant run through worse ones, which is
    the whole reason an archive beats a leaderboard (ablation: greedy 39.7% vs
    archive 50.0% on SWE-bench). Our files always recorded ``parent`` and
    ``score``; until 23/08/2026 nobody ever asked them this question — so a
    monotonic, greedy-in-practice archive would have looked identical to a
    working one.

    A dip is a step in the root→best chain whose score is LOWER than its
    parent's. Zero dips over a long chain is not a compliment: it means either
    the terrain is easy, or sampling is effectively greedy and the archive is
    dead weight.
    """
    latest = _latest_by_variant(load(target, root))
    if not latest:
        return {"target": target, "chain": [], "dips": 0,
                "verdict": "empty archive — nothing to trace"}
    best = max(latest, key=lambda v: (float(latest[v].get("score", 0.0)), v))
    chain, vid, seen = [], best, set()
    while vid and vid in latest and vid not in seen:
        seen.add(vid)
        e = latest[vid]
        chain.append({"variant_id": vid, "score": float(e.get("score", 0.0)),
                      "terminal": bool(e.get("terminal"))})
        vid = e.get("parent")
    chain.reverse()   # root → best
    dips = sum(1 for i in range(1, len(chain))
               if chain[i]["score"] < chain[i - 1]["score"])
    if len(chain) < 2:
        verdict = "chain of one — no ancestry to measure yet"
    elif dips:
        verdict = (f"{dips} dip(s) — the path to the best passed through worse, "
                   "which is what the archive exists to allow")
    else:
        verdict = ("monotonic — either the terrain is easy, or sampling is "
                   "effectively greedy (paper's ablation: greedy 39.7% vs "
                   "archive 50.0%)")
    return {"target": target, "best": best, "chain": chain,
            "dips": dips, "verdict": verdict}


def child_counts(entries: list[dict]) -> dict[str, int]:
    """Equation 2 of the paper is ``functioning_children_count`` — FUNCTIONING.

    Counting every child inverts the intent: a parent whose five attempts all hit
    dead ends is penalised as though it had been explored productively, and
    ``h = 1/(1+n)`` pushes still-open territory behind exhausted territory.

    The novelty bonus must decay by DISCOVERY, never by attempt.
    """
    counts: dict[str, int] = {}
    for e in entries:
        parent = e.get("parent")
        if parent and not e.get("terminal"):
            counts[parent] = counts.get(parent, 0) + 1
    return counts


def weights(target: str, root: Path | None = None) -> list[dict]:
    """Selection weight per eligible variant: ``w = s · h`` (Appendix C.2).

    ``s`` is the score through a sigmoid — it separates the merely-passing from
    the good without letting a single top scorer swallow the whole distribution.
    ``h = 1/(1+children)`` is the novelty bonus, and it is what makes this an
    archive rather than a leaderboard: a strong variant that has already been
    branched from many times yields to one nobody has explored.

    A perfect variant is NOT eligible — there is nothing left to improve there,
    and sampling it burns the budget re-deriving a finished answer. Neither is a
    TERMINAL one: it stays in the archive as evidence, but it cannot be a parent.
    Everything else keeps a non-zero probability, so no path is ever closed off.
    """
    entries = load(target, root)
    latest = _latest_by_variant(entries)
    counts = child_counts(entries)
    rows = []
    for vid, e in latest.items():
        alpha = float(e.get("score", 0.0))
        if alpha >= 1.0 or e.get("terminal"):
            continue
        s = 1.0 / (1.0 + math.exp(-SIGMOID_SHARPNESS * (alpha - SIGMOID_MIDPOINT)))
        h = 1.0 / (1.0 + counts.get(vid, 0))
        rows.append({"variant_id": vid, "score": alpha, "children": counts.get(vid, 0),
                     "s": s, "h": h, "w": s * h, "verdict": e.get("verdict", "")})
    total = sum(r["w"] for r in rows)
    for r in rows:
        r["p"] = (r["w"] / total) if total > 0 else 0.0
    rows.sort(key=lambda r: (-r["p"], r["variant_id"]))
    return rows


def sample_parents(target: str, k: int = 2, seed: int | None = None,
                   root: Path | None = None) -> list[str]:
    """Draw ``k`` starting points, with replacement, proportional to ``w``.

    Seeded on purpose: a selection nobody can reproduce is a selection nobody can
    audit, and an archive whose behaviour cannot be replayed is worse than none.
    """
    rows = weights(target, root)
    if not rows:
        return []
    rng = random.Random(seed)
    ids = [r["variant_id"] for r in rows]
    ps = [r["p"] for r in rows]
    return rng.choices(ids, weights=ps, k=k)


def prune(target: str, keep: int = DEFAULT_KEEP, root: Path | None = None) -> dict:
    """Cap the archive, dropping the weakest and oldest first.

    An archive with no ceiling is a disk-usage bug waiting to happen, and the
    ceiling has to be decided before the first write rather than after the
    complaint. Ties break on age so a long-lived stepping stone outlives a
    freshly-recorded twin of the same score.
    """
    entries = load(target, root)
    latest = _latest_by_variant(entries)
    if len(latest) <= keep:
        return {"kept": len(latest), "dropped": 0}
    ordered = sorted(latest.values(),
                     key=lambda e: (float(e.get("score", 0.0)), e.get("recorded_at", "")),
                     reverse=True)
    survivors = ordered[:keep]
    path = archive_path(target, root)
    path.write_text(
        "".join(json.dumps(e, ensure_ascii=False) + "\n" for e in reversed(survivors)),
        encoding="utf-8")
    return {"kept": len(survivors), "dropped": len(latest) - len(survivors)}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Stepping-stone archive of scored variants.")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_rec = sub.add_parser("record", help="archive one attempt with its score")
    p_rec.add_argument("target")
    p_rec.add_argument("variant")
    p_rec.add_argument("--score", type=float, required=True)
    p_rec.add_argument("--verdict", default="")
    p_rec.add_argument("--parent", default=None)
    p_rec.add_argument("--terminal", action="store_true",
                       help="dead end: kept as evidence, never sampled as a parent, "
                            "and does not count against its parent's novelty bonus")

    p_sam = sub.add_parser("sample", help="draw starting points proportional to s·h")
    p_sam.add_argument("target")
    p_sam.add_argument("-k", type=int, default=2)
    p_sam.add_argument("--seed", type=int, default=None)

    p_st = sub.add_parser("stats", help="the distribution the sampler sees")
    p_st.add_argument("target")

    p_lin = sub.add_parser("lineage",
                           help="ancestry of the best variant, dips counted")
    p_lin.add_argument("target")

    p_pr = sub.add_parser("prune", help="cap the archive")
    p_pr.add_argument("target")
    p_pr.add_argument("--keep", type=int, default=DEFAULT_KEEP)

    args = ap.parse_args(argv)
    if args.cmd == "record":
        print(json.dumps(record(args.target, args.variant, args.score,
                                args.verdict, args.parent,
                                terminal=args.terminal), ensure_ascii=False))
    elif args.cmd == "sample":
        print(json.dumps({"parents": sample_parents(args.target, args.k, args.seed)},
                         ensure_ascii=False))
    elif args.cmd == "lineage":
        print(json.dumps(lineage(args.target), ensure_ascii=False, indent=1))
    elif args.cmd == "stats":
        rows = weights(args.target)
        print(json.dumps({"target": args.target, "eligible": len(rows),
                          "total_recorded": len(_latest_by_variant(load(args.target))),
                          "rows": rows}, ensure_ascii=False, indent=1))
    elif args.cmd == "prune":
        print(json.dumps(prune(args.target, args.keep), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
