#!/usr/bin/env python3
"""loop_phase_close.py — close a loop phase and persist its knowledge.

Runs the loop's step-14 phase-close as ONE deterministic operation:
  1. mark the subtask done      touring decompose update <task> <phase>
  2. persist the lesson         touring memory store … --tier semantic
  3. reward the outcome         touring learning reward orchestrate
  4. OKF phase report           bundle/phases/<phase>.md   (frontmatter + gates)
  5. Hyper-Extract abstract     bundle/knowledge/<phase>.json  (typed hypergraph,
                                deterministic entity_id / relation_id — REGRA #17)
  6. append the bundle log      bundle/log.md

Zero external deps. IDs are deterministic (derived from canonical name, not
order) so abstracts diff + merge cleanly across runs (the L4 compounding basis).

Usage:
    loop_phase_close.py --task <task> --phase <Pn> --summary "<text>"
        [--status done] [--bundle <dir>] [--reward 1.0]
        [--gates <json>] [--abstract <json>] [--json] [--quiet]
"""
from __future__ import annotations

import argparse
import datetime
import json
import shlex
import subprocess
import sys
from pathlib import Path


def run(cmd, timeout=120):
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr
    except Exception as exc:  # noqa: BLE001 — fail-open
        return 127, "", str(exc)


def load_json_file(path):
    if not path:
        return None
    try:
        return json.loads(Path(path).read_text())
    except Exception:  # noqa: BLE001
        return None


def plan_id_from_bundle(bundle: Path):
    idx = bundle / "index.md"
    if not idx.exists():
        return None
    for line in idx.read_text(errors="ignore").splitlines():
        if line.startswith("plan_id:"):
            return line.split(":", 1)[1].strip()
    return None


# ── Touring side effects ─────────────────────────────────────────────────────
def update_dag(task, phase, status):
    rc, out, err = run(["touring", "decompose", "update", task, phase, "--status", status])
    return rc == 0 or '"subtask_updated":true' in (out + err)


def store_memory(task, phase, status, summary):
    key = f"loop:{task}:{phase}:{status}"
    rc, out, _ = run(["touring", "memory", "store", key, summary or f"{phase} {status}",
                      "--tier", "semantic", "--type", "lesson"])
    return '"status":"stored"' in out or rc == 0


def reward(phase, value):
    rc, out, _ = run(["touring", "learning", "reward", "orchestrate", str(value),
                      f"loop_phase_close:{phase}"])
    return '"reward_injected"' in out or rc == 0


# ── Hyper-Extract typed abstract ─────────────────────────────────────────────
def _rel(source, rtype, target):
    return {"relation_id": f"{source}|{rtype}|{target}", "source": source,
            "type": rtype, "target": target}


def build_abstract(phase, summary, extra):
    """Deterministic typed hypergraph: nodes + typed edges, ids from canonical name."""
    phase_id = f"phase:{phase}"
    entities = [{"entity_id": phase_id, "type": "phase", "description": summary or phase}]
    relations = []
    seen_e = {phase_id}
    seen_r = set()

    def add_entity(name, etype, desc):
        eid = f"{etype}:{name}"
        if eid not in seen_e:
            seen_e.add(eid)
            entities.append({"entity_id": eid, "type": etype, "description": desc})
        return eid

    def add_rel(source, rtype, target):
        r = _rel(source, rtype, target)
        if r["relation_id"] not in seen_r:
            seen_r.add(r["relation_id"])
            relations.append(r)

    for e in (extra or {}).get("entities", []) or []:
        name = e.get("name") or e.get("entity_id")
        if not name:
            continue
        eid = add_entity(name, e.get("type", "entity"), e.get("description", ""))
        add_rel(phase_id, "produces", eid)
    for r in (extra or {}).get("relations", []) or []:
        if r.get("source") and r.get("target"):
            add_rel(r["source"], r.get("type", "relates"), r["target"])

    return {"phase": phase, "entities": entities, "relations": relations}


def run_extractor(cmd, summary):
    """Optional real-tool adapter (ref-b): shell out to an external extractor —
    e.g. a real **Hyper-Extract** — with the phase summary on stdin and parse its
    ``{entities:[], relations:[]}`` JSON. The native deterministic abstract stays
    the default; this only *enriches* it. Absent tool / any error → ``{}`` (native
    fallback), so the loop never hard-depends on the external tool.
    """
    if not cmd:
        return {}
    try:
        proc = subprocess.run(shlex.split(cmd), input=summary or "",
                              capture_output=True, text=True, timeout=120)
        data = json.loads(proc.stdout)
        return data if isinstance(data, dict) else {}
    except Exception:  # noqa: BLE001 — optional adapter, fail-open to native
        return {}


def merge_extra(*extras):
    """Union of several ``{entities, relations}`` sources (native `--abstract`
    file + `--extractor` output). Deterministic dedup happens in `build_abstract`."""
    out = {"entities": [], "relations": []}
    for e in extras:
        if isinstance(e, dict):
            out["entities"] += e.get("entities", []) or []
            out["relations"] += e.get("relations", []) or []
    return out


# ── OKF emission ─────────────────────────────────────────────────────────────
def write_phase_report(bundle: Path, plan_id, phase, status, summary, gates, ts):
    path = bundle / "phases" / f"{phase}.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    fm = (
        "---\n"
        "type: PhaseReport\n"
        f"title: {phase} — phase report\n"
        f"description: {(summary or phase)[:140]}\n"
        f"plan_id: {plan_id or 'unknown'}\n"
        f"tags: [loop, phase, {phase}]\n"
        f"timestamp: {ts}\n"
        'okf_version: "0.1"\n'
        "---\n\n"
    )
    # Link only to bundle docs that actually exist — a report must never emit a
    # broken bundle-relative link (E2E finding 2026-07-02).
    refs = [f"[{label}](/{fn})" for label, fn in
            (("bundle", "index.md"), ("plan", "plan.md"), ("log", "log.md"))
            if (bundle / fn).exists()]
    part_of = ("Part of the " + " · ".join(refs) + ".") if refs else ""
    lines = [f"# {phase} — phase report", "",
             f"**Status**: {status}", "",
             part_of,
             "", "## Summary", "", summary or "_(no summary)_", ""]
    if gates:
        lines += ["## Schema", "", "| Gate clause | Result | Evidence |",
                  "|-------------|--------|----------|"]
        for name, c in (gates.get("clauses", {}) or {}).items():
            lines.append(f"| {name} | {c.get('result')} | {c.get('evidence', '')} |")
        lines.append("")
    lines += ["## Knowledge", "", f"Typed abstract: [/knowledge/{phase}.json](/knowledge/{phase}.json)."]
    path.write_text(fm + "\n".join(lines) + "\n")
    return str(path)


def write_abstract(bundle: Path, phase, abstract):
    path = bundle / "knowledge" / f"{phase}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(abstract, indent=2) + "\n")
    return str(path)


def append_log(bundle: Path, phase, status, summary, ts):
    log = bundle / "log.md"
    entry = f"\n## {ts} — {phase} {status}\n\n{summary or ''}\n"
    if log.exists():
        log.write_text(log.read_text() + entry)
    else:
        log.write_text(entry)
    return str(log)


# ── Orchestration ────────────────────────────────────────────────────────────
def main(argv=None):
    ap = argparse.ArgumentParser(description="Close a loop phase and persist its knowledge.")
    ap.add_argument("--task", required=True)
    ap.add_argument("--phase", required=True)
    ap.add_argument("--summary", default="")
    ap.add_argument("--status", default="done")
    ap.add_argument("--bundle", default=None, help="OKF bundle dir (writes report + abstract + log)")
    ap.add_argument("--reward", type=float, default=1.0)
    ap.add_argument("--gates", default=None, help="JSON file: loop_converged report to embed")
    ap.add_argument("--abstract", default=None, help="JSON file: {entities:[],relations:[]} to enrich")
    ap.add_argument("--extractor", default=None,
                    help="optional external extractor cmd (real Hyper-Extract adapter): "
                         "reads the summary on stdin, emits {entities,relations} JSON")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args(argv)

    result = {
        "task": args.task, "phase": args.phase, "status": args.status,
        "dag_updated": update_dag(args.task, args.phase, args.status),
        "memory_stored": store_memory(args.task, args.phase, args.status, args.summary),
        "rewarded": reward(args.phase, args.reward),
    }

    if args.bundle:
        bundle = Path(args.bundle)
        plan_id = plan_id_from_bundle(bundle)
        ts = datetime.datetime.now().astimezone().isoformat()
        gates = load_json_file(args.gates)
        extra = merge_extra(load_json_file(args.abstract),
                            run_extractor(args.extractor, args.summary))
        abstract = build_abstract(args.phase, args.summary, extra)
        result["phase_report"] = write_phase_report(bundle, plan_id, args.phase, args.status,
                                                    args.summary, gates, ts)
        result["abstract"] = write_abstract(bundle, args.phase, abstract)
        result["log"] = append_log(bundle, args.phase, args.status, args.summary, ts)
        result["entities"] = len(abstract["entities"])
        result["relations"] = len(abstract["relations"])

    if args.json:
        print(json.dumps(result, indent=2))
    elif not args.quiet:
        print(f"phase-close · {args.phase} → {args.status}")
        print(f"  dag={result['dag_updated']} memory={result['memory_stored']} reward={result['rewarded']}")
        if args.bundle:
            print(f"  OKF report: {result['phase_report']}")
            print(f"  abstract:   {result['abstract']} ({result['entities']} entities, {result['relations']} relations)")

    ok = result["dag_updated"] and result["memory_stored"]
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
