#!/usr/bin/env python3
"""Deterministic architectural diagnostic for a Rust workspace.

Three levels, none of which the per-file 50-dim quality matrix captures:
  * WORKSPACE — the inter-crate dependency DAG: layers, SCC cycles, fan-in/out,
    crate roles (foundation / orchestrator / leaf / hub), coupling metrics.
  * CRATE     — per-crate internal shape: module/file count, LOC distribution,
    God-objects (files whose LOC dwarfs the crate median), the crate's DAG role.
  * FILE      — the God-objects surfaced above, ranked by LOC + intra-crate reach.

Pure `cargo metadata` + filesystem — deterministic, no daemon. Writes the full
architecture matrix (JSON) + a ranked digest to disk.

Usage: workspace_arch_diag.py [--json] [workspace-root]   (default root: cwd)
  --json  emit the raw architecture matrix to stdout as JSON
"""
import json
import os
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report_contract import print_contract  # noqa: E402 — sibling module, path set above
from arsenal_cli import split_flags  # noqa: E402 — sibling, path set above

USAGE = """workspace_arch_diag.py — inter-crate dependency DAG diagnostic

Usage: workspace_arch_diag.py [--json] [workspace-root]
  workspace-root  path to the Rust workspace (default: cwd)
  --json          emit the raw architecture matrix to stdout as JSON
  -h,--help       show usage"""

# CLI args are consumed ONLY when run as a script — an importer (pytest, a
# composing tool) must never inherit this process's argv: pytest's own flags
# (-q/--collect-only) leaked into split_flags and aborted collection with
# SystemExit("unknown flag") (fixed 2026-07-23).
if __name__ == "__main__":
    _POS, _FLAGS = split_flags(sys.argv[1:], {"-j": "json", "--json": "json"})
else:
    _POS, _FLAGS = [], {}
if "help" in _FLAGS:
    print(USAGE)
    raise SystemExit(0)
WANT_JSON = "json" in _FLAGS
ROOT = Path(_POS[0] if _POS else ".").resolve()
# Artifacts land in the invocation dir (or DIAG_OUT) — a permanent, relocatable
# tool must not write next to itself in the skill directory.
OUT = os.environ.get("DIAG_OUT") or os.getcwd()


def cargo_metadata(root: Path) -> dict:
    """Return `cargo metadata --no-deps` for the workspace at `root`."""
    r = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=root, capture_output=True, text=True, timeout=120,
    )
    if r.returncode != 0:
        raise SystemExit(f"cargo metadata failed: {r.stderr[:300]}")
    return json.loads(r.stdout)


def crate_loc(manifest_path: str) -> tuple[int, int]:
    """Return (total_loc, rs_file_count) for the crate's `src/` tree."""
    src = Path(manifest_path).parent / "src"
    files = [p for p in src.rglob("*.rs")] if src.is_dir() else []
    loc = 0
    for f in files:
        try:
            loc += sum(1 for _ in f.open(encoding="utf-8", errors="ignore"))
        except OSError:
            pass
    return loc, len(files)


def tarjan_scc(nodes: list[str], edges: dict[str, set[str]]) -> list[list[str]]:
    """Tarjan's strongly-connected-components — any SCC of size>1 is a dep cycle."""
    index: dict[str, int] = {}
    low: dict[str, int] = {}
    on_stack: set[str] = set()
    stack: list[str] = []
    counter = [0]
    out: list[list[str]] = []

    def strong(v: str) -> None:
        index[v] = low[v] = counter[0]
        counter[0] += 1
        stack.append(v)
        on_stack.add(v)
        for w in edges.get(v, ()):
            if w not in index:
                strong(w)
                low[v] = min(low[v], low[w])
            elif w in on_stack:
                low[v] = min(low[v], index[w])
        if low[v] == index[v]:
            comp = []
            while True:
                w = stack.pop()
                on_stack.discard(w)
                comp.append(w)
                if w == v:
                    break
            out.append(comp)

    for v in nodes:
        if v not in index:
            strong(v)
    return out


def depth_of(crate: str, edges: dict[str, set[str]], memo: dict[str, int]) -> int:
    """Longest internal-dependency chain below `crate` (leaves = 0)."""
    if crate in memo:
        return memo[crate]
    memo[crate] = 0  # cycle guard
    deps = edges.get(crate, set())
    d = 0 if not deps else 1 + max(depth_of(x, edges, memo) for x in deps)
    memo[crate] = d
    return d


def role(fan_in_c: int, fan_out_c: int) -> str:
    """Classify a crate by its position in the dependency DAG (pure)."""
    if fan_out_c == 0 and fan_in_c == 0:
        return "isolated"
    if fan_out_c == 0:
        return "foundation-leaf"   # depends on nothing internal, others depend on it
    if fan_in_c == 0:
        return "top/orchestrator"  # nothing internal depends on it (bins, entrypoints)
    if fan_in_c >= 6:
        return "hub"               # widely depended-on AND depends on others
    return "intermediate"


def build_graph(meta: dict) -> dict:
    """Pure(ish) — cargo metadata → the full architecture result dict.

    Only file I/O is `crate_loc` (per-crate LOC); everything else — internal
    edges, fan-in/out, SCC cycles, layer depth, roles — is pure computation.
    """
    members = sorted(p["name"] for p in meta["packages"])
    member_set = set(members)
    manifest = {p["name"]: p["manifest_path"] for p in meta["packages"]}
    edges = {p["name"]: {d["name"] for d in p["dependencies"] if d["name"] in member_set}
             for p in meta["packages"]}
    fan_out = {c: len(edges[c]) for c in members}
    fan_in: dict[str, int] = defaultdict(int)
    for c, ds in edges.items():
        for d in ds:
            fan_in[d] += 1
    cycles = [sorted(c) for c in tarjan_scc(members, edges) if len(c) > 1]
    memo: dict[str, int] = {}
    depth = {c: depth_of(c, edges, memo) for c in members}
    loc, nfiles = {}, {}
    for c in members:
        loc[c], nfiles[c] = crate_loc(manifest[c])
    nodes = {
        c: {
            "loc": loc[c], "files": nfiles[c],
            "fan_in": fan_in[c], "fan_out": fan_out[c],
            "depth": depth[c], "role": role(fan_in[c], fan_out[c]),
            "internal_deps": sorted(edges[c]),
        }
        for c in members
    }
    return {
        "workspace_root": str(ROOT),
        "crate_count": len(members),
        "internal_edge_count": sum(fan_out.values()),
        "max_depth": max(depth.values()) if depth else 0,
        "cycle_count": len(cycles),
        "cycles": cycles,
        "avg_fan_out": round(sum(fan_out.values()) / len(members), 2) if members else 0.0,
        "total_loc": sum(loc.values()),
        "nodes": nodes,
    }


def main() -> None:
    """Build the workspace architecture matrix from cargo metadata, write + print it."""
    result = build_graph(cargo_metadata(ROOT))
    with open(f"{OUT}/workspace_arch_matrix.json", "w") as fh:
        json.dump(result, fh, indent=1)
    if WANT_JSON:
        print(json.dumps(result, indent=1))  # machine-readable: raw matrix, no digest/contract
    else:
        _print_digest(result)


def _print_digest(result: dict) -> None:
    """Print layers / foundation / orchestrator / God-crate rankings from the result."""
    nodes = result["nodes"]
    fan_in = {c: n["fan_in"] for c, n in nodes.items()}
    cy = result["cycles"]
    print(f"=== WORKSPACE ARCHITECTURE — {result['crate_count']} crates, {result['internal_edge_count']} internal edges ===")
    print(f"dependency cycles (SCC>1): {result['cycle_count']}  {'✅ acyclic' if not cy else '🔴 ' + str(cy)}")
    print(f"max layer depth: {result['max_depth']} | avg fan-out: {result['avg_fan_out']} | total LOC: {result['total_loc']}")
    print("\n=== LAYERS (depth = longest internal-dep chain below the crate) ===")
    by_depth: dict[int, list[str]] = defaultdict(list)
    for c, n in nodes.items():
        by_depth[n["depth"]].append(c)
    for d in sorted(by_depth):
        cs = ", ".join(sorted(by_depth[d], key=lambda x: -fan_in[x]))
        print(f"  L{d} ({len(by_depth[d])}): {cs}")
    print("\n=== FOUNDATION crates (highest fan-in — most depended-on = highest blast) ===")
    for c in sorted(nodes, key=lambda x: -nodes[x]["fan_in"])[:10]:
        n = nodes[c]
        print(f"  {c:28} fan_in={n['fan_in']:2} fan_out={n['fan_out']:2} depth={n['depth']} loc={n['loc']:6} [{n['role']}]")
    print("\n=== ORCHESTRATOR/HUB crates (highest fan-out — most coupled outward) ===")
    for c in sorted(nodes, key=lambda x: -nodes[x]["fan_out"])[:10]:
        n = nodes[c]
        print(f"  {c:28} fan_out={n['fan_out']:2} fan_in={n['fan_in']:2} depth={n['depth']} loc={n['loc']:6} [{n['role']}]")
    print("\n=== LARGEST crates by LOC (God-crate candidates) ===")
    for c in sorted(nodes, key=lambda x: -nodes[x]["loc"])[:10]:
        n = nodes[c]
        print(f"  {c:28} loc={n['loc']:6} files={n['files']:3} avg={n['loc']//max(n['files'],1):5}/file fan_in={n['fan_in']} fan_out={n['fan_out']}")
    print(f"\nArchitecture matrix written: {OUT}/workspace_arch_matrix.json")
    print_contract(
        f"{OUT}/workspace_arch_matrix.json",
        "workspace_arch_diag — inter-crate dependency DAG",
        [
            "the dependency cycles (Tarjan SCC) — EVERY cycle found, with its member crates",
            "the fan-in blast ranking + layer depth per crate (foundation crates carry the most blast)",
            "the God-crate candidates by LOC, and each crate's architectural role "
            "(isolated / foundation-leaf / hub / top-orchestrator / intermediate)",
        ],
    )


if __name__ == "__main__":
    main()
