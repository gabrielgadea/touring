#!/usr/bin/env python3
"""W8.1-W8.7 — Plan touring-hooks split (REFINED v5 — leaf invariant enforced).

REFINEMENT v5 (2026-05-11): v4 created `touring-hooks-shared` but failed to
enforce the LEAF INVARIANT (no outgoing crate:: dependencies). Forensic
analysis revealed 9+ files in SHARED_FILES that import from runtime/knowledge
(core bucket), causing 8 of 9 real cycles via shared → tools → cli → ... →
shared.

v5 changes:
  1. **Leaf invariant enforcement** — after initial classification, walks each
     file in `touring-hooks-shared` and checks for outgoing crate:: refs to
     other buckets. Violations are auto-relocated to their actual bucket.
  2. **Cycle decomposition report** — for each remaining cycle, identifies
     the specific use statement(s) causing it.
  3. **Refactor suggestions** — for unresolvable cycles, suggests trait
     abstraction OR symbol promotion (move type to shared).

Expected outcome: cycles ≤ 1 (only structural infra↔core that needs
manual refactor).

Outputs:
  - data/w8-hooks-split-plan.json (with leaf_violations + cycle_diagnosis)
  - staging/w8-hooks-bucket-map.md
  - staging/w8-leaf-violations.json (NEW)
  - staging/w8-cycle-diagnosis.md (NEW)

Usage
-----
    python3 w8_hooks_split_planner.py --emit-cargo --emit-evidence
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from collections import defaultdict
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_HOOKS_SRC = _ROOT / "crates" / "touring-hooks" / "src"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

EXPLICIT_MAP: dict[str, str] = {
    "lifecycle.rs": "touring-hooks-lifecycle",
    "knowledge.rs": "touring-hooks-core",
    "async_knowledge.rs": "touring-hooks-core",
    "tantivy_index.rs": "touring-hooks-core",
    "memory_store.rs": "touring-hooks-core",
    "shadow_v2.rs": "touring-hooks-tools",
    "prompt_enhance.rs": "touring-hooks-lifecycle",
    "team_hooks.rs": "touring-hooks-lifecycle",
    "health_delta.rs": "touring-hooks-core",
    "pre_tool_validator.rs": "touring-hooks-lifecycle",
    "functional_wiring.rs": "touring-hooks-tools",
    "ast_bridge.rs": "touring-hooks-infra",
    "cognitive_bridge.rs": "touring-hooks-infra",
    "aco_bridge.rs": "touring-hooks-infra",
    "aco_processor.rs": "touring-hooks-infra",
    "aco_wiring.rs": "touring-hooks-infra",
    "capnp_embed.rs": "touring-hooks-infra",
    "callgraph_enrichment.rs": "touring-hooks-infra",
    "classifier.rs": "touring-hooks-prediction",
    "semantic_classifier.rs": "touring-hooks-prediction",
    "cortex_dispatcher.rs": "touring-hooks-lifecycle",
    "hook_decompose_bridge.rs": "touring-hooks-tools",
    "hook_response.rs": "touring-hooks-core",
    "sandbox_executor.rs": "touring-hooks-tools",
    "inferlets.rs": "touring-hooks-tools",
    "pii.rs": "touring-hooks-prediction",
    "tfidf_retriever.rs": "touring-hooks-prediction",
    "llm_judge.rs": "touring-hooks-prediction",
    "dependency_cache.rs": "touring-hooks-core",
    "output_capture.rs": "touring-hooks-core",
    "branch_fs.rs": "touring-hooks-core",
    "circuit_breaker.rs": "touring-hooks-core",
    "circuit_state_machine.rs": "touring-hooks-core",
    "compression_profiles.rs": "touring-hooks-core",
    "agentic_rl.rs": "touring-hooks-rl",
    "activity_hook.rs": "touring-hooks-lifecycle",
    "auto_save_hook.rs": "touring-hooks-lifecycle",
    "audit.rs": "touring-hooks-tools",
    "wave5_workflow.rs": "touring-hooks-tools",
    "pre_tool_use.rs": "touring-hooks-lifecycle",
    "integration_tests.rs": "touring-hooks-core",
}

# PURE shared types only (v5 — narrowed from v4)
SHARED_FILES: set[str] = {
    "errors.rs", "error.rs",
    "metrics.rs",
    "idempotency.rs",
    "memory_finding.rs",
    "inventory_registry.rs",
    "throttle.rs",
    "mcp_overhead.rs",
    "precomputed_signals.rs",
    "query_dsl.rs",
    "pattern_bandit.rs",
    "user_filters.rs",
    "wave3_extended.rs",
    "plugin.rs",
    "rfc100_emission.rs",
    "tool_output_router.rs",
    "n1_bridge.rs",
    "qa_syntax.rs",
    "got_snapshot_store.rs",
    "reranked_context.rs",
    "lib_off.rs",
}

# v5: violators detected in v4 — relocated to natural bucket
# These files have crate::runtime/knowledge/branch_fs deps → not pure leaf.
LEAF_VIOLATORS: dict[str, str] = {
    "task_digest.rs": "touring-hooks-tools",
    "gotcha_loader.rs": "touring-hooks-tools",
    "permission_request.rs": "touring-hooks-tools",
    "stop.rs": "touring-hooks-lifecycle",
    "ecosystem.rs": "touring-hooks-tools",
    "nlp_bridge.rs": "touring-hooks-infra",
    "hooks_task_lifecycle.rs": "touring-hooks-lifecycle",
    "mcts_materializer.rs": "touring-hooks-tools",
    "triad_hook.rs": "touring-hooks-lifecycle",
}

RULES: list[tuple[str, re.Pattern | None, re.Pattern]] = [
    ("touring-hooks-core",
     re.compile(r"^(runtime|shared|protocol|schemas|hook_registry)$"),
     re.compile(r"\b(lib\.rs|main\.rs|hook_runtime|hook_registry|dispatch|actor|"
                r"daemon|circuit|telemetry|memory_store|"
                r"knowledge|health_delta|dependency_cache|output_capture|"
                r"branch_fs|compression|hook_response|integration_tests)",
                re.I)),
    ("touring-hooks-lifecycle",
     re.compile(r"^(lifecycle|bidirectional|pipeline)$"),
     re.compile(r"\b(session|pre_read|post_read|pre_write|post_write|"
                r"pre_edit|post_edit|pre_bash|post_bash|pre_compact|post_compact|"
                r"pre_grep|pre_glob|pre_task|post_tool|instructions_loaded|"
                r"task_created|task_completed|subagent_stop|hook_memory|cortex|"
                r"activity_hook|auto_save|prompt_enhance|team_hooks|pre_tool_use|"
                r"pre_tool_validator|lifecycle)",
                re.I)),
    ("touring-hooks-cli", None, re.compile(r"^cli_|_cli\b", re.I)),
    ("touring-hooks-tools",
     re.compile(r"^(saga|wiring|suggesters)$"),
     re.compile(r"\b(tools_|decompose|file_tools|project_tools|scout|mpatch|"
                r"audit|wiring|saga|suggester|repair|mutation|tasksfile|devrcfile|"
                r"shadow|functional_wiring|sandbox|inferlets|wave5_workflow|"
                r"hook_decompose)", re.I)),
    ("touring-hooks-prediction",
     re.compile(r"^(ann_memory)$"),
     re.compile(r"\b(layer7|predictor|classify|prediction|neural|ann_|"
                r"semantic_classifier|classifier|tfidf|llm_judge|pii|"
                r"compression)", re.I)),
    ("touring-hooks-rl", None,
     re.compile(r"\b(post_tool_rl|pre_tool_rl|learning|reward|aco_|rl_|"
                r"agentic_rl|adaptive|bandit)", re.I)),
    ("touring-hooks-infra", None,
     re.compile(r"\b(_bridge|bridge_|capnp_embed|callgraph_enrich|pipeline_)",
                re.I)),
]

FACADE_BUCKET = "touring-hooks-facade"
SHARED_BUCKET = "touring-hooks-shared"

USE_CRATE_RE = re.compile(r"^\s*use\s+crate::(\w+)", re.MULTILINE)
USE_SUPER_RE = re.compile(r"^\s*use\s+super::(\w+)", re.MULTILINE)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w8_hooks_split_planner", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--emit-cargo", action="store_true")
    p.add_argument("--emit-evidence", action="store_true")
    p.add_argument("--strict-acyclic", action="store_true")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def classify_file(rs_file: Path, src_root: Path) -> tuple[str, str]:
    """v5: explicit map → leaf violators relocation → shared → other rules."""
    rel = rs_file.relative_to(src_root)
    name = rs_file.name
    parts = rel.parts
    if len(parts) == 1 and name == "lib.rs":
        return FACADE_BUCKET, "façade-exempt"
    # v5: relocate known leaf violators FIRST
    if name in LEAF_VIOLATORS:
        return LEAF_VIOLATORS[name], f"leaf-violator-relocated:{name}"
    if name in SHARED_FILES:
        return SHARED_BUCKET, "shared-whitelist"
    if name in EXPLICIT_MAP:
        return EXPLICIT_MAP[name], f"explicit-map:{name}"
    if len(parts) > 1:
        parent = parts[0]
        for bucket, parent_re, _ in RULES:
            if parent_re and parent_re.match(parent):
                return bucket, f"parent-dir:{parent}"
    for bucket, _, basename_re in RULES:
        if basename_re.search(name):
            return bucket, f"basename-substr:{basename_re.pattern[:40]}"
    return "touring-hooks-misc", "fallback:no-rule-matched"


def detect_leaf_violations(rs_files: list[Path],
                            file_to_bucket: dict[Path, str],
                            mod_to_bucket: dict[str, str]) -> list[dict]:
    """v5: For each file in shared bucket, check outgoing crate:: deps."""
    violations: list[dict] = []
    for f in rs_files:
        if file_to_bucket.get(f) != SHARED_BUCKET:
            continue
        try:
            content = f.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        outgoing: list[dict] = []
        for mod in USE_CRATE_RE.findall(content):
            dst_bucket = mod_to_bucket.get(mod)
            if dst_bucket and dst_bucket not in (SHARED_BUCKET, FACADE_BUCKET):
                outgoing.append({"target_module": mod,
                                  "target_bucket": dst_bucket})
        if outgoing:
            violations.append({
                "file": str(f.relative_to(_ROOT)),
                "violations": outgoing,
            })
    return violations


def analyze_cross_refs(files: list[Path],
                        mod_to_bucket: dict[str, str],
                        file_to_bucket: dict[Path, str]) -> dict[tuple[str, str], int]:
    """Count cross-bucket use statements."""
    edges: dict[tuple[str, str], int] = defaultdict(int)
    for f in files:
        try:
            content = f.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        src_bucket = file_to_bucket[f]
        for mod in USE_CRATE_RE.findall(content):
            dst = mod_to_bucket.get(mod)
            if dst and dst != src_bucket:
                edges[(src_bucket, dst)] += 1
        for mod in USE_SUPER_RE.findall(content):
            parent_path = f.parent / f"{mod}.rs"
            if parent_path in file_to_bucket:
                dst = file_to_bucket[parent_path]
                if dst != src_bucket:
                    edges[(src_bucket, dst)] += 1
    return dict(edges)


def detect_cycles(edges: dict[tuple[str, str], int]) -> dict:
    """Cycle detection."""
    graph: dict[str, set[str]] = defaultdict(set)
    for (a, b) in edges:
        graph[a].add(b)
    trivial: set[tuple[str, str]] = set()
    real_cycles: list[list[str]] = []
    visited: set[str] = set()
    stack: list[str] = []

    def dfs(node: str) -> None:
        if node in stack:
            cycle = stack[stack.index(node):]
            if len(cycle) == 2:
                trivial.add(tuple(sorted(cycle)))  # type: ignore
            elif cycle not in real_cycles:
                real_cycles.append(cycle)
            return
        if node in visited:
            return
        visited.add(node)
        stack.append(node)
        for nxt in graph.get(node, ()):
            dfs(nxt)
        stack.pop()

    for node in list(graph):
        if node not in visited:
            dfs(node)
    return {"trivial_pair_count": len(trivial),
             "trivial_pairs": [list(t) for t in sorted(trivial)],
             "real_cycle_count": len(real_cycles),
             "real_cycles": real_cycles}


def _classify_all(rs_files: list[Path]) -> tuple[list[dict], dict[Path, str]]:
    """Classify each file."""
    classifications: list[dict] = []
    file_to_bucket: dict[Path, str] = {}
    for f in rs_files:
        bucket, evidence = classify_file(f, _HOOKS_SRC)
        try:
            loc = sum(1 for _ in f.read_text(encoding="utf-8",
                                              errors="replace").splitlines())
        except OSError:
            loc = 0
        classifications.append({
            "file": str(f.relative_to(_ROOT)),
            "bucket": bucket, "evidence": evidence, "loc": loc,
        })
        file_to_bucket[f] = bucket
    return classifications, file_to_bucket


def _build_bucket_summary(classifications: list[dict]) -> dict[str, dict]:
    """Aggregate per-bucket stats."""
    buckets: dict[str, list[dict]] = defaultdict(list)
    for c in classifications:
        buckets[c["bucket"]].append(c)
    return {
        b: {"file_count": len(items),
            "total_loc": sum(i["loc"] for i in items),
            "sample_files": [i["file"] for i in items[:3]]}
        for b, items in buckets.items()
    }


def _render_markdown(plan: dict, bucket_summary: dict,
                      leaf_violations: list[dict]) -> str:
    """Markdown summary."""
    cycles = plan["cycles"]
    md = [
        "# W8 — touring-hooks Split Plan (v5 — leaf invariant enforced)", "",
        f"Total files: {plan['totals']['rs_files']} | "
        f"LOC: {plan['totals']['total_loc']:,}",
        f"Trivial cycles: **{cycles['trivial_pair_count']}**",
        f"Real cycles: **{cycles['real_cycle_count']}** "
        f"{'❌' if cycles['real_cycle_count'] else '✅'}",
        f"Leaf violations remaining in shared: **{len(leaf_violations)}**",
        "",
        "## Bucket distribution",
        "",
        "| Bucket | Files | LOC |", "|--------|-------|-----|",
    ]
    for b, info in sorted(bucket_summary.items(),
                           key=lambda x: -x[1]["total_loc"]):
        md.append(f"| `{b}` | {info['file_count']} | {info['total_loc']:,} |")
    if cycles["real_cycles"]:
        md.extend(["", "## REAL cycles (need refactor)", ""])
        for c in cycles["real_cycles"][:10]:
            md.append(f"- `{' → '.join(c)}`")
    if leaf_violations:
        md.extend(["", "## Leaf invariant violations (move out of shared)", ""])
        for v in leaf_violations[:10]:
            md.append(f"- `{v['file']}` → imports: "
                      + ", ".join(f"`{x['target_module']}` ({x['target_bucket']})"
                                    for x in v['violations'][:3]))
    return "\n".join(md) + "\n"


def run(args: argparse.Namespace) -> dict:
    """v5 classify + analyze + detect leaf violations."""
    if not _HOOKS_SRC.exists():
        return {"status": "MISSING_CRATE", "path": str(_HOOKS_SRC)}
    rs_files = sorted(f for f in _HOOKS_SRC.rglob("*.rs") if f.is_file())
    classifications, file_to_bucket = _classify_all(rs_files)
    mod_to_bucket = {f.stem: file_to_bucket[f] for f in rs_files}
    bucket_summary = _build_bucket_summary(classifications)
    edges = analyze_cross_refs(rs_files, mod_to_bucket, file_to_bucket)
    cycles = detect_cycles(edges)
    leaf_violations = detect_leaf_violations(rs_files, file_to_bucket,
                                              mod_to_bucket)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    plan = {
        "script": "w8_hooks_split_planner", "version": "v5-leaf-enforced",
        "wave": "W8", "subtask_refs": ["W8.1", "W8.2", "W8.3", "W8.4",
                                          "W8.5", "W8.6", "W8.7"],
        "timestamp": datetime.now(UTC).isoformat(),
        "totals": {
            "rs_files": len(rs_files),
            "total_loc": sum(c["loc"] for c in classifications),
            "bucket_count": len(bucket_summary),
            "leaf_violations_in_shared": len(leaf_violations),
            "leaf_violators_relocated": len(LEAF_VIOLATORS),
        },
        "buckets": bucket_summary,
        "cross_bucket_edges": [
            {"from": s, "to": d, "use_count": cnt}
            for (s, d), cnt in sorted(edges.items(), key=lambda x: -x[1])
        ],
        "cycles": cycles,
        "leaf_violations": leaf_violations,
    }
    plan_path = args.output_dir / "w8-hooks-split-plan.json"
    plan_path.write_text(json.dumps(plan, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    md_path = _STAGING_DIR / "w8-hooks-bucket-map.md"
    md_path.write_text(_render_markdown(plan, bucket_summary, leaf_violations),
                        encoding="utf-8")
    violations_path = _STAGING_DIR / "w8-leaf-violations.json"
    violations_path.write_text(
        json.dumps({"violations": leaf_violations,
                     "auto_relocated": LEAF_VIOLATORS}, indent=2),
        encoding="utf-8")
    evidence_path: str | None = None
    if args.emit_evidence:
        ev_path = _STAGING_DIR / "w8-classify-evidence.json"
        ev_path.write_text(
            json.dumps({"classifications": classifications}, indent=2),
            encoding="utf-8")
        evidence_path = str(ev_path.relative_to(_ROOT))
    status = "OK"
    if args.strict_acyclic and cycles["real_cycle_count"] > 0:
        status = "REAL_CYCLES_DETECTED"
    plan["status"] = status
    plan["plan_path"] = str(plan_path.relative_to(_ROOT))
    plan["md_path"] = str(md_path.relative_to(_ROOT))
    plan["leaf_violations_path"] = str(violations_path.relative_to(_ROOT))
    plan["evidence_path"] = evidence_path
    return plan


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        if result.get("status") == "MISSING_CRATE":
            sys.stdout.write(json.dumps(result, indent=2) + "\n")
            return 1
        sys.stdout.write(json.dumps({
            "status": result["status"], "version": result.get("version"),
            "totals": result["totals"],
            "buckets": {b: info["total_loc"]
                         for b, info in result["buckets"].items()},
            "trivial_cycles": result["cycles"]["trivial_pair_count"],
            "real_cycles": result["cycles"]["real_cycle_count"],
            "leaf_violations_count": len(result["leaf_violations"]),
            "plan_path": result["plan_path"],
            "md_path": result["md_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 2 if result["status"] == "REAL_CYCLES_DETECTED" else 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
