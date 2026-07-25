#!/usr/bin/env python3
"""Crate-internal + file-level architectural diagnostic.

Complements workspace_arch_diag.py by looking INSIDE crates:
  * FILE   — LOC ranking, God-objects (LOC >> crate median), intra-crate fan-in
             (how many sibling files reference each top-level module).
  * CRATE  — module coupling shape + the 50-dim ARCHITECTURAL dims at crate scope
             (F1.7 boundaries, F1.8 deps, F1.11 patterns, F1.12 arch-consistency,
             F2.13 scalability) — the harness's architectural verdict, cross-read
             against the structural graph.

Pure filesystem + `touring-quality` — deterministic. Writes JSON + digest.

Usage: crate_arch_diag.py [--json] <crate> [<crate> ...]
  <crate>   a crate NAME (touring-hooks-core) OR a rel/abs PATH (crates/…) — both resolve
  --json    emit the raw matrix to stdout as JSON (suppresses the human digest)
  -h,--help show usage
Unresolvable crates are skipped with a stderr warning; all-unresolvable → exit 2.
"""
import json
import os
import re
import statistics
import subprocess
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report_contract import print_contract  # noqa: E402 — sibling module, path set above
from arsenal_cli import RUST_ROOT, resolve_crate, split_flags  # noqa: E402 — sibling, path set above

BIN = str(Path.home() / ".claude/rust/target/release/touring-quality")
# Artifacts land in the invocation dir (or DIAG_OUT) — a permanent, relocatable
# tool must not write next to itself in the skill directory.
OUT = os.environ.get("DIAG_OUT") or os.getcwd()
ARCH_DIMS = ["F1_7", "F1_8", "F1_11", "F1_12", "F2_13"]  # component/dep/patterns/consistency/scale
USE_RE = re.compile(r"\b(?:use\s+crate::|crate::)([a-z_][a-z0-9_]*)", re.IGNORECASE)

USAGE = """crate_arch_diag.py — intra-crate architectural diagnostic

Usage: crate_arch_diag.py [--json] <crate> [<crate> ...]
  <crate>   a crate NAME (e.g. touring-hooks-core) OR a rel/abs PATH to the
            crate dir (e.g. crates/touring-hooks-core) — both resolve.
  --json    emit the raw matrix to stdout as JSON (suppresses the human digest)
  -h,--help show this help

Artifacts: writes crate_arch_matrix.json to $DIAG_OUT (or the cwd)."""


def arch_dims(crate: str) -> dict:
    """The 50-dim architectural dimensions at crate scope (harness verdict)."""
    try:
        r = subprocess.run(
            [BIN, "score", crate, "--scope", "crate", "--format", "json"],
            capture_output=True, text=True, timeout=180,
        )
        d = json.loads(r.stdout)
        dm = d["dimensions"]
        return {
            "composite": d.get("composite"), "tier": d.get("tier"),
            "arch": {k: {"value": round(dm[k]["value"], 3), "status": dm[k]["status"],
                         "evidence": dm[k]["evidence"][:150]}
                     for k in ARCH_DIMS if k in dm},
        }
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError, KeyError) as e:
        return {"error": str(e)}


def analyze_crate(crate_rel: str) -> dict:
    """Structural shape of one crate: God-objects + intra-crate module fan-in."""
    src = Path(crate_rel) / "src"
    files = sorted(p for p in src.rglob("*.rs") if "target" not in p.parts)
    per_file = {}
    module_fanin: Counter = Counter()
    for f in files:
        text = f.read_text(encoding="utf-8", errors="ignore")
        loc = text.count("\n") + 1
        rel = str(f.relative_to(src))
        per_file[rel] = loc
        # intra-crate references: `use crate::<mod>` / `crate::<mod>::`
        for mod in set(USE_RE.findall(text)):
            module_fanin[mod] += 1
    locs = list(per_file.values()) or [0]
    median = statistics.median(locs)
    # God-objects: > 3× crate median AND > 800 LOC (absolute floor)
    gods = sorted(
        ((r, n) for r, n in per_file.items() if n > max(3 * median, 800)),
        key=lambda x: -x[1],
    )
    return {
        "files": len(files), "total_loc": sum(locs),
        "median_loc": median, "max_loc": max(locs),
        "god_objects": [{"file": r, "loc": n} for r, n in gods],
        "top_module_fanin": module_fanin.most_common(8),
    }


def analyze(crate_rels: list[str]) -> dict:
    """Run the structural + harness-dim analysis for each crate path."""
    return {crate_rel.rstrip("/").rsplit("/", 1)[-1]:
            {"struct": analyze_crate(crate_rel), "dims": arch_dims(crate_rel)}
            for crate_rel in crate_rels}


def main() -> None:
    """Parse args, resolve each crate name/path, write the matrix, print it."""
    tokens, flags = split_flags(sys.argv[1:], {"-j": "json", "--json": "json"})
    if "help" in flags:
        print(USAGE)
        raise SystemExit(0)
    if not tokens:
        raise SystemExit(f"no crate given\n\n{USAGE}")
    want_json = "json" in flags
    resolved: list[str] = []
    for t in tokens:
        path = resolve_crate(t)
        if path:
            resolved.append(path)
        else:
            print(f"⚠ skip: cannot resolve crate '{t}' "
                  f"(no '{t}/src', 'crates/{t}/src' under cwd or {RUST_ROOT})", file=sys.stderr)
    if not resolved:
        raise SystemExit(2)
    results = analyze(resolved)
    with open(f"{OUT}/crate_arch_matrix.json", "w") as fh:
        json.dump(results, fh, indent=1)
    if want_json:
        print(json.dumps(results, indent=1))  # machine-readable: raw matrix, no digest/contract
    else:
        _print_digest(results)


def _print_digest(results: dict) -> None:
    """Print the per-crate structural + architectural-dim digest."""
    for name, r in results.items():
        s, d = r["struct"], r["dims"]
        print(f"\n{'='*78}\n{name} — {s['files']} files, {s['total_loc']} LOC "
              f"(median {s['median_loc']:.0f}/file, max {s['max_loc']})")
        comp = d.get("composite")
        print(f"  crate composite={comp:.4f} {d.get('tier')}" if comp else f"  dims error: {d.get('error')}")
        print("  ARCHITECTURAL dims (harness verdict):")
        for k, v in d.get("arch", {}).items():
            print(f"    {k}={v['value']:.3f} {v['status']:14} {v['evidence'][:88]}")
        if s["god_objects"]:
            print(f"  GOD-OBJECTS (>{max(3*s['median_loc'],800):.0f} LOC — 3× median or >800):")
            for g in s["god_objects"]:
                print(f"    {g['loc']:5} LOC  {g['file']}")
        else:
            print("  God-objects: none (no file >3× median & >800 LOC)")
        print(f"  most-referenced intra-crate modules (fan-in): {s['top_module_fanin'][:6]}")
    print(f"\nCrate architecture matrix written: {OUT}/crate_arch_matrix.json")
    print_contract(
        f"{OUT}/crate_arch_matrix.json",
        "crate_arch_diag — intra-crate architecture",
        [
            "the God-objects per crate (files >3× median LOC & >800) — every one, with LOC",
            "the most-referenced intra-crate modules (fan-in coupling) per crate",
            "the architecture dims F1_7/F1_8/F1_11/F1_12/F2_13 scored for each crate",
        ],
    )


if __name__ == "__main__":
    main()
