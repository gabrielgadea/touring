#!/usr/bin/env python3
"""
Top Orphan Clusters — uses networkx for community detection on orphan symbols.

Input:  {"min_cluster_size": 3}
Output: {"clusters": [{"size": 5, "symbols": ["Foo", "Bar", ...]}], "total_orphans": 89}
Return: 1 if clusters found, 0 if no clusters

NOTE: Uses networkx for community detection if available, otherwise falls back
      to simple greedy clustering based on crate co-occurrence.
"""

import json
import sys
import argparse
import subprocess
from collections import defaultdict


def query_wiring_orphans() -> list[dict]:
    """Query touring wiring orphans."""
    try:
        result = subprocess.run(
            ["touring", "wiring", "orphans", "-j"],
            capture_output=True,
            text=True,
            timeout=15,
        )
        if result.returncode == 0:
            data = json.loads(result.stdout)
            return data.get("orphans", [])
    except (subprocess.TimeoutExpired, FileNotFoundError, json.JSONDecodeError):
        pass
    return []


def greedy_cluster(orphans: list[dict], min_size: int) -> list[dict]:
    """
    Simple greedy clustering when networkx is unavailable.
    Groups symbols by crate co-occurrence.
    """
    crate_symbols: dict[str, list[str]] = defaultdict(list)
    for orphan in orphans:
        crate = orphan.get("module_file", "").split("/crates/")[-1].split("/")[0] if orphan.get("module_file") else "unknown"
        symbol = orphan.get("symbol_name", orphan.get("name", ""))
        if symbol:
            crate_symbols[crate].append(symbol)

    clusters = []
    used: set[str] = set()
    for crate, symbols in crate_symbols.items():
        if len(symbols) >= min_size:
            cluster_symbols = [s for s in symbols if s not in used]
            for s in cluster_symbols:
                used.add(s)
            if len(cluster_symbols) >= min_size:
                clusters.append({"size": len(cluster_symbols), "symbols": cluster_symbols})

    clusters.sort(key=lambda c: c["size"], reverse=True)
    return clusters


def detect_clusters(orphans: list[dict], min_size: int) -> list[dict]:
    """Detect clusters using networkx if available."""
    try:
        import networkx as nx
        from networkx.algorithms.community import greedy_modularity_communities

        G = nx.Graph()
        for orphan in orphans:
            symbol = orphan.get("symbol_name", orphan.get("name", ""))
            if symbol:
                G.add_node(symbol)

        for i, o1 in enumerate(orphans):
            s1 = o1.get("symbol_name", o1.get("name", ""))
            if not s1:
                continue
            for o2 in orphans[i + 1 :]:
                s2 = o2.get("symbol_name", o2.get("name", ""))
                if not s2 or s1 == s2:
                    continue
                crate1 = o1.get("module_file", "").split("/crates/")[-1].split("/")[0] if o1.get("module_file") else ""
                crate2 = o2.get("module_file", "").split("/crates/")[-1].split("/")[0] if o2.get("module_file") else ""
                if crate1 == crate2:
                    G.add_edge(s1, s2)

        if G.number_of_nodes() == 0:
            return []

        communities = list(greedy_modularity_communities(G))
        clusters = []
        for community in communities:
            syms = list(community)
            if len(syms) >= min_size:
                clusters.append({"size": len(syms), "symbols": syms})

        clusters.sort(key=lambda c: c["size"], reverse=True)
        return clusters
    except ImportError:
        return greedy_cluster(orphans, min_size)


def main() -> int:
    parser = argparse.ArgumentParser(description="Top orphan clusters")
    parser.add_argument("input_json", nargs="?", help="JSON input string")
    args = parser.parse_args()

    if args.input_json:
        input_obj = json.loads(args.input_json)
    else:
        input_obj = json.loads(sys.stdin.read())

    min_size = int(input_obj.get("min_cluster_size", 3))

    orphans = query_wiring_orphans()
    total_orphans = len(orphans)
    clusters = detect_clusters(orphans, min_size)

    result = {
        "clusters": clusters,
        "total_orphans": total_orphans,
    }
    print(json.dumps(result))
    return 1 if clusters else 0


if __name__ == "__main__":
    sys.exit(main())
