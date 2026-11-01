"""Laya local probe on Touring decisions (RTX 4060, zero-shot).

Measures latency/VRAM and zero-shot accuracy of the open Laya checkpoints on three
decisions taken from Touring's own data, each against the heuristic Touring uses today:

  T1  memory kind      — gold: key prefix chosen at store time; baseline: derived `kind` tag
  T2  ticket routing   — gold: DAG task_type mapped to an ADW; baseline: factory.py RULES
  T3  recall relevance — gold: which recalled memories are actually about the query

Read-only on the Touring databases. Writes results.json next to this file.
"""
from __future__ import annotations

import importlib.util
import json
import random
import re
import sqlite3
import statistics
import sys
import time
from pathlib import Path

import torch
import laya

HOME = Path.home()
REPO = HOME / "projects/touring"
OUT = Path(__file__).with_name("results.json")
SEED = 20260922
PER_CLASS = 30

MEM_CLASSES = {
    "lesson": "A general lesson learned: a principle or insight worth reusing later",
    "gotcha": "A specific pitfall or trap: something that breaks or misleads, and how to avoid it",
    "snippet": "A reusable piece of code or command, with what it is for",
    "audit": "The result of an audit or review: findings, verdicts or scores",
    "strategy": "A plan or strategy: goals, phases and decisions for future work",
}
PREFIX_TO_CLASS = {"lesson": "lesson", "licao": "lesson", "gotcha": "gotcha",
                   "snippet": "snippet", "audit": "audit", "strategy": "strategy"}

ADW_CLASSES = {
    "hotfix": "A production incident or outage that needs an urgent surgical fix",
    "bugfix": "A defect: something broken, failing or returning a wrong result",
    "audit": "An assessment: audit, review, security or quality check",
    "chore": "A small mechanical task: rename, typo, docs, formatting, cleanup",
    "feature": "Build or implement new capability, integration or endpoint",
    "explore-plan": "Research, investigate or plan before building anything",
}
TASKTYPE_TO_ADW = {"feature": "feature", "audit": "audit", "bugfix": "bugfix",
                   "plan": "explore-plan", "research": "explore-plan"}


def ro(path: Path) -> sqlite3.Connection:
    """Open a Touring SQLite database strictly read-only."""
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)


def load_factory_rules() -> list[tuple[str, str, str]]:
    """Import RULES from the live factory router without running its CLI."""
    path = HOME / ".claude/skills/Touring/scripts/factory.py"
    spec = importlib.util.spec_from_file_location("factory_probe", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)  # module guards main() behind __name__
    return mod.RULES


def dataset_memory_kind(rng: random.Random) -> list[dict]:
    """Balanced sample of memories labelled by their key prefix, with the derived tag."""
    conn = ro(REPO / ".claude/touring/memory.db")
    rows = conn.execute("select key, value from memory_entries where superseded_by is null").fetchall()
    tags = dict(conn.execute(
        "select entry_key, value from memory_tags where facet='kind' and source in ('auto','backfill')"))
    by_class: dict[str, list[dict]] = {c: [] for c in MEM_CLASSES}
    for key, value in rows:
        prefix = re.split(r"[:_/]", key)[0].lower()
        cls = PREFIX_TO_CLASS.get(prefix)
        if cls and value and len(value) > 40:
            by_class[cls].append({"key": key, "text": value[:1500], "gold": cls, "derived": tags.get(key)})
    sample = []
    for cls, items in by_class.items():
        rng.shuffle(items)
        sample.extend(items[:PER_CLASS])
    return sample


def dataset_routing(rules) -> list[dict]:
    """DAG task descriptions whose task_type maps to an ADW, with the regex baseline."""
    conn = ro(REPO / ".claude/touring/knowledge.db")
    out = []
    for task_type, desc in conn.execute("select task_type, description from task_decompositions"):
        gold = TASKTYPE_TO_ADW.get(task_type)
        if not gold or not desc or len(desc) < 20:
            continue
        lowered = desc.lower()
        regex = next((adw for adw, pat, _ in rules if re.search(pat, lowered)), None)
        out.append({"text": desc[:1500], "gold": gold, "regex": regex})
    return out


def dataset_relevance() -> tuple[str, list[dict]]:
    """The recall of this very research: 10 unrelated memories plus the relevant ones."""
    ledger = json.loads((REPO / ".touring-explore/jev-system-one--typesafe-ai---uso-no-touring---r.ledger.json").read_text())
    findings = ledger["findings"].values() if isinstance(ledger["findings"], dict) else ledger["findings"]
    irrelevant = [f["key"] for f in findings if f.get("lens") == "institutional"]
    conn = ro(REPO / ".claude/touring/memory.db")
    relevant = [k for (k,) in conn.execute(
        "select key from memory_entries where key like 'research:jev%' or key like 'loop:task_1790076481161575147%'")]
    items = []
    for key, gold in [(k, 0) for k in irrelevant] + [(k, 1) for k in relevant]:
        row = conn.execute("select value from memory_entries where key=?", (key,)).fetchone()
        if row and row[0]:
            items.append({"key": key, "text": row[0][:1500], "gold": gold})
    return ledger["topic"], items


def choice_q(instructions: str, criteria: dict[str, str]) -> dict:
    return {"type": "choice", "instructions": instructions, "criteria": criteria}


def run_choice(agent_or_router, items: list[dict], field: str, question: dict, key: str) -> list[dict]:
    """One predict per item; returns the answer dict, the route and the latency."""
    results = []
    for it in items:
        t0 = time.perf_counter()
        res = agent_or_router.predict({field: it["text"]}, {key: question})
        dt = (time.perf_counter() - t0) * 1000
        ans = res["answers"][key]
        results.append({"pred": ans.get("choice"), "conf": ans.get("confidence"),
                        "probs": ans.get("probabilities"), "ms": dt,
                        "route": (res.get("routing") or {}).get("model")})
    return results


def summarize(items: list[dict], preds: list[dict]) -> dict:
    n = len(items)
    correct = [p["pred"] == it["gold"] for it, p in zip(items, preds)]
    bins = {"<0.5": [], "0.5-0.8": [], ">=0.8": []}
    for ok, p in zip(correct, preds):
        c = p["conf"] if p["conf"] is not None else 0.0
        bins["<0.5" if c < 0.5 else "0.5-0.8" if c < 0.8 else ">=0.8"].append(ok)
    per_class = {}
    for cls in sorted({it["gold"] for it in items}):
        idx = [i for i, it in enumerate(items) if it["gold"] == cls]
        per_class[cls] = round(sum(correct[i] for i in idx) / len(idx), 3)
    routes = {}
    for p in preds:
        routes[p["route"]] = routes.get(p["route"], 0) + 1
    return {
        "n": n,
        "accuracy": round(sum(correct) / n, 3),
        "per_class": per_class,
        "calibration": {k: {"n": len(v), "acc": round(sum(v) / len(v), 3) if v else None} for k, v in bins.items()},
        "mean_conf": round(statistics.mean(p["conf"] or 0 for p in preds), 3),
        "p50_ms": round(statistics.median(p["ms"] for p in preds), 1),
        "routes": routes,
    }


def latency_bench(agent, label: str) -> dict:
    """1, 10 and 50 short questions per call; P50/P95 over repeated calls; peak VRAM."""
    state = {"ticket": "I was charged twice for order A-104. Please refund the duplicate today."}
    base = {"type": "noul", "instructions": "Does the ticket explicitly request a refund?"}
    out = {}
    torch.cuda.reset_peak_memory_stats()
    for nq, reps in ((1, 40), (10, 15), (50, 6)):
        qs = {f"q{i}": dict(base) for i in range(nq)}
        for _ in range(3):
            agent.predict(state, qs)
        times = []
        for _ in range(reps):
            torch.cuda.synchronize()
            t0 = time.perf_counter()
            agent.predict(state, qs)
            torch.cuda.synchronize()
            times.append((time.perf_counter() - t0) * 1000)
        times.sort()
        out[f"{nq}q"] = {"p50_ms": round(statistics.median(times), 1),
                         "p95_ms": round(times[min(len(times) - 1, int(0.95 * len(times)))], 1),
                         "per_q_ms": round(statistics.median(times) / nq, 2)}
    out["peak_vram_mib"] = round(torch.cuda.max_memory_allocated() / 2**20, 1)
    print(f"[latency] {label}: {out}", flush=True)
    return out


def main() -> int:
    rng = random.Random(SEED)
    rules = load_factory_rules()
    mem = dataset_memory_kind(rng)
    routing = dataset_routing(rules)
    topic, relevance = dataset_relevance()
    print(f"datasets: memory_kind={len(mem)} routing={len(routing)} relevance={len(relevance)}", flush=True)

    report: dict = {"gpu": torch.cuda.get_device_name(0), "torch": torch.__version__,
                    "laya": getattr(laya, "__version__", "0.3.5"), "datasets": {}}

    # Baselines (what Touring does today)
    mem_base = sum(1 for it in mem if it["derived"] == it["gold"]) / len(mem)
    route_cov = sum(1 for it in routing if it["regex"]) / len(routing)
    route_acc = sum(1 for it in routing if it["regex"] == it["gold"]) / len(routing)
    route_acc_cov = (sum(1 for it in routing if it["regex"] and it["regex"] == it["gold"])
                     / max(1, sum(1 for it in routing if it["regex"])))
    report["baselines"] = {
        "memory_kind_derived_tag_acc": round(mem_base, 3),
        "routing_regex_coverage": round(route_cov, 3),
        "routing_regex_acc_all": round(route_acc, 3),
        "routing_regex_acc_when_matched": round(route_acc_cov, 3),
        "routing_gold_dist": {g: sum(1 for it in routing if it["gold"] == g) for g in sorted({it["gold"] for it in routing})},
    }
    print("[baselines]", report["baselines"], flush=True)

    t0 = time.perf_counter()
    en = laya.load("convaiinnovations/laya", device="cuda")
    t_en = time.perf_counter() - t0
    t0 = time.perf_counter()
    ml = laya.load("convaiinnovations/laya", device="cuda", subfolder="multilingual")
    t_ml = time.perf_counter() - t0
    report["load_seconds"] = {"english": round(t_en, 1), "multilingual": round(t_ml, 1)}
    router = laya.Router(device="cuda", max_loaded=2)
    router.attach("english", en)
    router.attach("multilingual", ml)

    # First raw answer, to record the exact output schema
    raw = en.predict({"ticket": "I was charged twice. Please refund."},
                     {"dept": choice_q("Which team should handle this?", {"billing": "charges, refunds", "technical": "bugs"})})
    report["raw_example"] = raw

    report["latency"] = {"english": latency_bench(en, "english"), "multilingual": latency_bench(ml, "multilingual")}

    mem_q = choice_q("What kind of note is this memory?", MEM_CLASSES)
    route_q = choice_q("Which workflow should handle this task?", ADW_CLASSES)
    for name, runner in (("english", en), ("multilingual", ml), ("router", router)):
        m = run_choice(runner, mem, "memory", mem_q, "kind")
        r = run_choice(runner, routing, "task", route_q, "adw")
        report["datasets"][f"memory_kind/{name}"] = summarize(mem, m)
        report["datasets"][f"routing/{name}"] = summarize(routing, r)
        print(f"[{name}] memory_kind={report['datasets'][f'memory_kind/{name}']['accuracy']} "
              f"routing={report['datasets'][f'routing/{name}']['accuracy']}", flush=True)

    # T3: relevance as a Noul per candidate; rank by P(yes)
    rel = []
    for it in relevance:
        res = router.predict({"query": topic, "candidate": it["text"]},
                             {"useful": {"type": "noul",
                                         "instructions": "Does the candidate contain information that helps with the query?"}})
        rel.append({"key": it["key"], "gold": it["gold"], "p": res["answers"]["useful"]["noul"],
                    "route": (res.get("routing") or {}).get("model")})
    rel.sort(key=lambda x: -x["p"])
    report["datasets"]["relevance/router"] = {
        "topic": topic, "ranking": rel,
        "relevant_in_top_k": sum(x["gold"] for x in rel[: sum(i["gold"] for i in relevance)]),
        "n_relevant": sum(i["gold"] for i in relevance),
    }
    print("[relevance]", [(x["key"][:40], x["gold"], round(x["p"], 3)) for x in rel], flush=True)

    OUT.write_text(json.dumps(report, ensure_ascii=False, indent=1, default=str))
    print(f"wrote {OUT}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
