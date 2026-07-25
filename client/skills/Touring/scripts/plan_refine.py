#!/usr/bin/env python3
"""touring plan-refine — F2 of the ADW plan (refine-until-plateau loop).

Scores a plan document against an exploration ledger (produced by
explore_until_dry.py) and records each scoring as one ITERATION of the
refinement loop. The plan is APPROVED-ready only when the measured contract
holds — never when the author feels done:

  plateau        |score_i − score_{i−1}| < ε for the last 2 iterations
  coverage       ≥ --threshold (default 0.90) of ledger findings addressed,
                 weighted by depth (D2 corroborated findings weigh 3×)
  plan-delta     structural delta (headings added/removed + effort-tag
                 reclassifications, e.g. [XL]→[M]) is ZERO in the last iteration
  questions      every open ledger question is addressed in the plan text
  claims         no plan claim cites a symbol/file absent from ledger+disk (VGP-lite)

Each invocation = one measured iteration (the re-planning between invocations is
the author's/LLM's work; this script is the code side of the loop — Lei L2: the
verdict belongs to the runner, the author only feeds revisions).

Exit codes: 0 plateau contract holds · 1 continue refining (gaps listed) ·
2 usage/path error.

Usage:
  plan_refine.py plan.md --ledger .touring-explore/topic.ledger.json
  plan_refine.py plan.md --ledger L.json --threshold 0.85 --json
  plan_refine.py plan.md --ledger L.json --status        # report, no new iteration
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import time
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_kv, emit_result, emit_section, emit_table,
)

DEPTH_WEIGHT = {"D0": 1.0, "D1": 2.0, "D2": 3.0}
EFFORT_TAG = re.compile(r"\[(S|M|L|XL)\]")
HEADING = re.compile(r"^(#{1,4})\s+(.*)$", re.MULTILINE)
CODE_REF = re.compile(r"`([A-Za-z_][A-Za-z0-9_:.\-/]{3,60})`")
DEFAULT_THRESHOLD = 0.90
DEFAULT_EPSILON = 0.02


def tokens_of(key: str) -> list[str]:
    """Matchable stems for a ledger finding key: path basename + symbol parts."""
    base = key.split(":", 1)[0]
    parts = re.split(r"[/\\]", base)
    toks = {parts[-1]} if parts else set()
    for seg in re.split(r"[:.]{1,2}", key):
        seg = seg.strip()
        if len(seg) >= 4 and not seg.isdigit():
            toks.add(seg)
    return [t for t in toks if t]


def finding_addressed(finding: dict[str, Any], plan_text: str) -> bool:
    return any(tok in plan_text for tok in tokens_of(str(finding.get("key", ""))))


def coverage_score(ledger: dict[str, Any], plan_text: str) -> dict[str, Any]:
    """Depth-weighted share of ledger findings the plan text addresses."""
    total = addressed = 0.0
    gaps: list[dict[str, Any]] = []
    for f in ledger.get("findings", {}).values():
        w = DEPTH_WEIGHT.get(str(f.get("depth", "D0")), 1.0)
        total += w
        if finding_addressed(f, plan_text):
            addressed += w
        else:
            gaps.append({"key": f.get("key"), "lens": f.get("lens"),
                         "depth": f.get("depth"), "weight": w})
    gaps.sort(key=lambda g: -g["weight"])
    return {"score": (addressed / total) if total else 1.0,
            "total_weight": total, "gaps": gaps}


def questions_unaddressed(ledger: dict[str, Any], plan_text: str) -> list[dict[str, Any]]:
    out = []
    for q in ledger.get("questions", []):
        if q.get("status") != "open":
            continue
        toks = [w for w in re.findall(r"[A-Za-z\-]{5,}", q.get("text", ""))][:3]
        if not toks or not all(t.lower() in plan_text.lower() for t in toks):
            out.append({"id": q.get("id"), "text": q.get("text")})
    return out


def unverified_claims(plan_text: str, ledger: dict[str, Any],
                      plan_dir: Path) -> list[str]:
    """VGP-lite: back-tick citations must exist in the ledger keys or on disk."""
    keys = " ".join(str(f.get("key", "")) + " " + str(f.get("detail", ""))
                    for f in ledger.get("findings", {}).values())
    bad: list[str] = []
    for ref in sorted(set(CODE_REF.findall(plan_text))):
        if re.fullmatch(r"[a-z\-]+", ref) and len(ref) < 8:
            continue                      # prose-ish backtick word, not a claim
        if ref in keys:
            continue
        if (plan_dir / ref).exists() or Path(ref).expanduser().exists():
            continue
        if re.search(rf"\b{re.escape(ref.split('.')[0])}\b", keys):
            continue
        bad.append(ref)
    return bad


def structural_signature(plan_text: str) -> dict[str, Any]:
    """Headings + effort tags — the material for the plan-delta measure.

    Headings are normalized WITHOUT their effort tag, so a heading whose only
    change is [XL]→[M] counts as a reclassification (the round-4 event), not as
    one heading removed plus one added."""
    headings = [EFFORT_TAG.sub("", h[1]).strip() for h in HEADING.findall(plan_text)]
    efforts: dict[str, str] = {}
    for line in plan_text.splitlines():
        m = HEADING.match(line)
        if m:
            tag = EFFORT_TAG.search(line)
            if tag:
                efforts[m.group(2).split("—")[0].strip()[:60]] = tag.group(1)
    return {"headings": headings, "efforts": efforts,
            "hash": hashlib.sha1(plan_text.encode()).hexdigest()[:12]}


def plan_delta(prev: dict[str, Any] | None, cur: dict[str, Any]) -> dict[str, Any]:
    if prev is None:
        return {"headings_added": len(cur["headings"]), "headings_removed": 0,
                "effort_reclassified": 0, "is_zero": False, "first": True}
    pa, ca = set(prev["headings"]), set(cur["headings"])
    reclass = sum(1 for k, v in cur["efforts"].items()
                  if k in prev["efforts"] and prev["efforts"][k] != v)
    added, removed = len(ca - pa), len(pa - ca)
    return {"headings_added": added, "headings_removed": removed,
            "effort_reclassified": reclass,
            "is_zero": added == 0 and removed == 0 and reclass == 0,
            "first": False}


# === iteration ledger =======================================================

def load_refine_ledger(path: Path) -> dict[str, Any]:
    if path.is_file():
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
            if isinstance(data, dict):
                return data
        except (json.JSONDecodeError, OSError):
            pass
    return {"version": 1, "iterations": []}


def contract(iters: list[dict[str, Any]], threshold: float,
             epsilon: float) -> dict[str, Any]:
    """The refine-until-plateau verdict — computed, honest, conditioned."""
    last = iters[-1] if iters else None
    unmet: list[str] = []
    if last is None:
        return {"ready": False, "unmet": ["no iterations recorded yet"],
                "statement": "no measurement"}
    if last["coverage"] < threshold:
        unmet.append(f"coverage {last['coverage']:.2f} < {threshold:.2f} "
                     f"({len(last['top_gaps'])}+ gaps)")
    if last["open_questions"]:
        unmet.append(f"{last['open_questions']} open question(s) unaddressed")
    if last["unverified_claims"]:
        unmet.append(f"{last['unverified_claims']} unverified claim(s)")
    if len(iters) < 2:
        unmet.append("need ≥2 iterations to measure plateau")
    else:
        delta_score = abs(iters[-1]["coverage"] - iters[-2]["coverage"])
        if delta_score >= epsilon:
            unmet.append(f"Δcoverage {delta_score:.3f} ≥ ε {epsilon} — not a plateau")
        if not last["plan_delta"]["is_zero"]:
            unmet.append("structural plan-delta ≠ 0 in last iteration")
    ready = not unmet
    return {
        "ready": ready,
        "unmet": unmet,
        "statement": ("plateau under current ledger/questions at "
                      f"ε={epsilon} — NOT a claim the plan is complete"
                      if ready else "plan not plateaued — keep refining"),
        "next_action": None if ready else unmet[0],
        "at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    }


# === main ===================================================================

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("plan", help="Path to the plan .md under refinement")
    p.add_argument("--ledger", required=True,
                   help="Exploration ledger JSON (explore_until_dry.py)")
    p.add_argument("--refine-ledger", default=None,
                   help="Iteration ledger path (default: <plan>.refine.json)")
    p.add_argument("--threshold", type=float, default=DEFAULT_THRESHOLD)
    p.add_argument("--epsilon", type=float, default=DEFAULT_EPSILON)
    p.add_argument("--status", action="store_true",
                   help="Report current contract; record NO new iteration")
    p.add_argument("--top-gaps", type=int, default=8)
    add_common_args(p)
    return p


def measure(plan_path: Path, ledger: dict[str, Any],
            prev_sig: dict[str, Any] | None, top_gaps: int) -> dict[str, Any]:
    text = plan_path.read_text(encoding="utf-8", errors="replace")
    cov = coverage_score(ledger, text)
    sig = structural_signature(text)
    open_qs = questions_unaddressed(ledger, text)
    claims = unverified_claims(text, ledger, plan_path.parent)
    return {
        "coverage": round(cov["score"], 4),
        "gap_count": len(cov["gaps"]),
        "top_gaps": cov["gaps"][:top_gaps],
        "open_questions": len(open_qs),
        "open_question_items": open_qs[:5],
        "unverified_claims": len(claims),
        "unverified_items": claims[:8],
        "plan_delta": None,               # filled by caller with prev signature
        "signature": sig,
        "at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    }


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    plan_path = Path(args.plan).expanduser().resolve()
    ledger_path = Path(args.ledger).expanduser().resolve()
    if not plan_path.is_file():
        emit_error(f"plan not found: {plan_path}")
        return 2
    if not ledger_path.is_file():
        emit_error(f"exploration ledger not found: {ledger_path}")
        return 2
    ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
    rl_path = Path(args.refine_ledger).expanduser().resolve() if args.refine_ledger \
        else plan_path.with_suffix(".refine.json")
    rledger = load_refine_ledger(rl_path)
    iters = rledger["iterations"]

    if not args.status:
        prev_sig = iters[-1]["signature"] if iters else None
        entry = measure(plan_path, ledger, prev_sig, args.top_gaps)
        entry["iter"] = len(iters) + 1
        entry["plan_delta"] = plan_delta(prev_sig, entry["signature"])
        iters.append(entry)
        rl_path.write_text(json.dumps(rledger, indent=1, ensure_ascii=False),
                           encoding="utf-8")

    verdict = contract(iters, args.threshold, args.epsilon)
    payload = {
        "plan": str(plan_path), "exploration_ledger": str(ledger_path),
        "refine_ledger": str(rl_path),
        "iterations": [{k: it[k] for k in
                        ("iter", "coverage", "gap_count", "open_questions",
                         "unverified_claims")} for it in iters],
        "last": iters[-1] if iters else None,
        "verdict": verdict,
    }
    if not emit_result(payload, args):
        human_report(plan_path, iters, verdict)
    return 0 if verdict["ready"] else 1


def human_report(plan_path: Path, iters: list[dict[str, Any]],
                 verdict: dict[str, Any]) -> None:
    emit_section(f"plan-refine — {plan_path.name}")
    emit_kv("iterations", len(iters))
    if iters:
        curve = " → ".join(f"{it['coverage']:.2f}" for it in iters)
        emit_kv("coverage curve", curve)
        emit_kv("gaps", iters[-1]["gap_count"])
        emit_kv("unverified claims", iters[-1]["unverified_claims"])
        if iters[-1]["top_gaps"]:
            emit_section("top gaps (depth-weighted)", char="-")
            emit_table([[g["depth"], g["lens"], str(g["key"])[:70]]
                        for g in iters[-1]["top_gaps"]],
                       headers=["depth", "lens", "key"])
    emit_section("verdict", char="-")
    emit_kv("ready", verdict["ready"])
    emit_kv("statement", verdict["statement"])
    for u in verdict["unmet"]:
        print(f"  ✗ {u}")


if __name__ == "__main__":
    sys.exit(main())
