#!/usr/bin/env python3
"""Complete deterministic 50-dim harness matrix for an arbitrary touring crate.

Captures the FULL `touring-quality score` output — every field at every depth,
LOSSLESS — at EVERY granularity:
  * each individual .rs file (--scope file)  [ENTIRE crate tree, incl. tests/]
  * each individual directory (--scope path)
  * the whole crate in aggregate (--scope crate)

Complete per-target schema captured (verified against the binary, schema_version=1):
  top-level (10): scope_kind, root, file_count, total_loc, composite, tier,
                  blockers[], warnings[], schema_version, dimensions{50}
  per-dim  (5):   value, status, evidence, suggestions[], latency_ms

Persists THREE artifacts to disk:
  1. <slug>_50dim_matrix.json — COMPLETE raw score per target×scope, nothing
                                 dropped or rounded. The lossless source of truth.
  2. <slug>_50dim_matrix.tsv  — WIDE value grid (target × 50 dims); NotApplicable
                                 cells marked `na`. Quick spreadsheet / plot view.
  3. <slug>_50dim_long.tsv    — LONG format: one row per (target, scope, dim)
                                 carrying EVERY per-dim field (value, status,
                                 latency_ms, evidence, suggestions). Deep view.

Zero-LLM, code-mode (1 script vs N calls).

Usage: crate_50dim_matrix.py [--json] <crate> [out-slug]
  <crate>   a crate NAME (touring-cortex) OR a rel/abs PATH (crates/touring-cortex) — both resolve
  --json    emit the raw matrix to stdout as JSON (suppresses the human digest)
  e.g. crate_50dim_matrix.py touring-cortex   |   crate_50dim_matrix.py crates/touring-cortex touring_cortex
"""
import json
import os
import subprocess
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report_contract import print_contract  # noqa: E402 — sibling module, path set above
from arsenal_cli import resolve_crate, split_flags  # noqa: E402 — sibling, path set above

USAGE = """crate_50dim_matrix.py — lossless 50-dim harness matrix for a crate

Usage: crate_50dim_matrix.py [--json] <crate> [out-slug]
  <crate>   a crate NAME (touring-cortex) OR a rel/abs PATH (crates/touring-cortex) — both resolve
  out-slug  artifact filename prefix (default: crate basename, '-'→'_')
  --json    emit the raw matrix to stdout as JSON (suppresses the human digest)
  -h,--help show usage"""

# Portable, no session-specific hardcoded absolute path; override with the env var.
BIN = os.environ.get("TOURING_QUALITY_BIN") or str(
    Path.home() / ".claude/rust/target/release/touring-quality"
)
# CLI args and CLI-derived constants materialize ONLY when run as a script — an
# importer (pytest, a composing tool) must never inherit this process's argv
# (pytest's -q/--collect-only leaked into split_flags → SystemExit) nor die on
# an unresolvable default crate / missing binary at import (fixed 2026-07-23).
if __name__ == "__main__":
    if not Path(BIN).is_file():
        raise SystemExit(f"touring-quality binary not found at {BIN}; run `update-touring` first")
    _POS, _FLAGS = split_flags(sys.argv[1:], {"-j": "json", "--json": "json"})
    if "help" in _FLAGS:
        print(USAGE)
        raise SystemExit(0)
    WANT_JSON = "json" in _FLAGS
    _RAW = _POS[0] if _POS else "crates/touring-cortex"  # default only resolves from the rust root
    ROOT = resolve_crate(_RAW)
    if ROOT is None:
        raise SystemExit(f"cannot resolve crate '{_RAW}' (no '{_RAW}/src' or 'crates/{_RAW}/src')\n\n{USAGE}")
    SLUG = _POS[1] if len(_POS) > 1 else Path(ROOT).name.replace("-", "_")
else:  # import-safe defaults; tests and composing tools override per call
    WANT_JSON = False
    ROOT = "crates/touring-cortex"
    SLUG = "touring_cortex"
# Artifacts land next to this script by default (portable — no hardcoded,
# session-specific absolute path); override with MATRIX_OUT.
# Artifacts land in the invocation dir (or DIAG_OUT/MATRIX_OUT) — a permanent,
# relocatable tool must not write next to itself in the skill directory.
OUT = os.environ.get("DIAG_OUT") or os.environ.get("MATRIX_OUT") or os.getcwd()
# Dims that measure per-repo artifacts (arch-docs/README/changelog/CI) and so
# cannot genuinely apply at file/path scope — for files inside a repo the harness
# walk-up resolves them, but for out-of-repo files they fail 0.3 (confirmed by
# self-dogfood: F3_10/F3_11/F3_13/F4_7 all fired on a standalone script). Coarse
# filter for the ranking print only — NOT a genuineness verdict (see below).
SCOPE_ARTIFACT = {"F3_10", "F3_11", "F3_13", "F4_7"}


def score(target: str, scope: str) -> dict:
    """Return the COMPLETE touring-quality score JSON for one target×scope.

    Nothing is dropped or rounded — this is the lossless source of truth. On any
    failure (missing/unexecutable binary → OSError, TimeoutExpired →
    SubprocessError, empty/garbled stdout → JSONDecodeError) returns an error
    dict so one bad cell degrades gracefully instead of aborting the whole matrix.
    """
    try:
        r = subprocess.run(
            [BIN, "score", target, "--scope", scope, "--format", "json"],
            capture_output=True, text=True, timeout=180,
        )
        d = json.loads(r.stdout)
        if r.returncode != 0 and "dimensions" not in d:
            d.setdefault("error", f"exit {r.returncode}: {r.stderr[:200]}")
        return d
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError) as e:
        return {"target": target, "scope_kind": scope, "error": f"{type(e).__name__}: {e}"}


def digest(d: dict) -> dict:
    """Compact per-target view derived from the complete score (ranking/print only)."""
    if "error" in d or "dimensions" not in d:
        return {"target": d.get("root", d.get("target")), "scope": d.get("scope_kind"),
                "error": d.get("error", "no dimensions")}
    dims = d["dimensions"]
    return {
        "target": d["root"], "scope": d["scope_kind"],
        "files": d.get("file_count"), "loc": d.get("total_loc"),
        "composite": d.get("composite"), "tier": d.get("tier"),
        "blockers": d.get("blockers", []),
        "fails": sorted(k for k, v in dims.items() if v["status"] == "Fail"),
        "warns": sorted(k for k, v in dims.items() if v["status"] == "Warn"),
        "n_na": sum(1 for v in dims.values() if v["status"] == "NotApplicable"),
    }


def non_artifact_fails(dg: dict) -> list:
    """Fails minus the per-crate scope-artifact FPs (README/changelog/CI).

    Coarse filter, NOT a genuineness verdict: survivors like F3_1 (harness never
    runs coverage) and F1_7 (plain-data boundaries) are still quasi-FPs. True
    genuine-vs-FP classification needs the companion `clone_blocks.py` + manual
    structural judgment — this only strips dims that trivially can't apply here.
    """
    return [x for x in dg.get("fails", []) if x not in SCOPE_ARTIFACT]


def clean(s: str) -> str:
    """Flatten a field for single-cell TSV embedding."""
    return str(s).replace("\t", " ").replace("\n", " ").replace("\r", " ")


def enumerate_targets(root: str) -> tuple[list[str], list[str]]:
    """(files, dirs): every .rs file + every dir under `root` holding .rs (excl. target/).

    ROOT itself is excluded from `dirs` — it is covered by the crate aggregate.
    """
    root_path = Path(root)
    rs_files = [p for p in root_path.rglob("*.rs") if "target" not in p.parts]
    files = sorted(str(p) for p in rs_files)
    dir_set: set[str] = set()
    for p in rs_files:
        for anc in p.parents:
            if anc == root_path:
                break
            try:
                anc.relative_to(root_path)
            except ValueError:
                break
            dir_set.add(str(anc))
    return files, sorted(dir_set)


def build_matrix(root: str, slug: str) -> dict:
    """Score the crate + every file + every dir at their scopes → complete raw matrix."""
    files, dirs = enumerate_targets(root)
    return {
        "meta": {
            "root": root, "slug": slug, "bin": BIN, "schema": "complete-raw",
            "file_scopes": len(files), "path_scopes": len(dirs),
            "scope_artifact_dims": sorted(SCOPE_ARTIFACT),
        },
        "aggregate": score(root, "crate"),
        "files": [score(f, "file") for f in files],
        "paths": [score(d, "path") for d in dirs],
    }


def write_artifacts(res: dict, out: str, slug: str) -> int:
    """Write the 3 artifacts (lossless JSON + wide TSV + long TSV); return long-row count."""
    with open(f"{out}/{slug}_50dim_matrix.json", "w") as fh:
        json.dump(res, fh, indent=1)
    scored = [r for r in [res["aggregate"], *res["files"], *res["paths"]] if "dimensions" in r]
    dim_ids = sorted(res["aggregate"].get("dimensions", {}).keys())
    with open(f"{out}/{slug}_50dim_matrix.tsv", "w") as fh:
        fh.write("target\tscope\tcomposite\ttier\tloc\tblockers\t" + "\t".join(dim_ids) + "\n")
        for raw in scored:
            dims = raw["dimensions"]
            cells = ("na" if dims[d]["status"] == "NotApplicable" else f'{dims[d]["value"]:.4f}'
                     for d in dim_ids)
            fh.write(
                f'{raw["root"]}\t{raw["scope_kind"]}\t{raw.get("composite", "")}\t'
                f'{raw.get("tier", "")}\t{raw.get("total_loc", "")}\t'
                f'{"|".join(raw.get("blockers", []))}\t' + "\t".join(cells) + "\n"
            )
    long_rows = 0
    with open(f"{out}/{slug}_50dim_long.tsv", "w") as fh:
        fh.write("target\tscope\tloc\tdim\tvalue\tstatus\tlatency_ms\tevidence\tsuggestions\n")
        for raw in scored:
            for d in dim_ids:
                v = raw["dimensions"][d]
                sugg = " ⏎ ".join(clean(s) for s in v.get("suggestions", []))
                fh.write(
                    f'{raw["root"]}\t{raw["scope_kind"]}\t{raw.get("total_loc", "")}\t{d}\t'
                    f'{v["value"]}\t{v["status"]}\t{v.get("latency_ms", "")}\t'
                    f'{clean(v.get("evidence", ""))}\t{sugg}\n'
                )
                long_rows += 1
    return long_rows


def main() -> None:
    """Build the complete 50-dim matrix for ROOT/SLUG, write the 3 artifacts, print it."""
    res = build_matrix(ROOT, SLUG)
    long_rows = write_artifacts(res, OUT, SLUG)
    if WANT_JSON:
        print(json.dumps(res, indent=1))  # machine-readable: raw matrix, no digest/contract
    else:
        _print_digest(res, long_rows)


def _print_digest(res: dict, long_rows: int) -> None:
    """Ranked digest (worst files/paths, dim frequencies) derived from the complete raw."""
    all_rows = [res["aggregate"], *res["files"], *res["paths"]]
    scored = [r for r in all_rows if "dimensions" in r]
    dim_ids = sorted(res["aggregate"].get("dimensions", {}).keys())
    dg_files = [digest(r) for r in res["files"] if "dimensions" in r]
    dg_paths = [digest(r) for r in res["paths"] if "dimensions" in r]
    a = digest(res["aggregate"])
    n_err = sum(1 for r in all_rows if "dimensions" not in r)
    print(f"=== SCORED: 1 crate + {len(dg_files)} files + {len(dg_paths)} paths — {len(dim_ids)} dims EACH ===")
    print(f"COMPLETENESS: {len(scored)} targets × {len(dim_ids)} dims = {len(scored) * len(dim_ids)} cells | "
          f"long-format rows written = {long_rows} | errors = {n_err}")
    print(f"AGGREGATE crate: composite={a['composite']} {a['tier']} loc={a['loc']} files={a['files']}")
    print(f"  fails={a['fails']} warns={a['warns']} n_na={a['n_na']}")
    print(f"  BLOCKERS (== Fail dims capping the tier, verified blockers==fail-set): {a['blockers']}  (empty = none)")
    print("\n=== WORST 20 FILES (composite asc) — non-artifact fails shown ===")
    for r in sorted(dg_files, key=lambda x: x["composite"])[:20]:
        print(f"{r['composite']:.3f} {r['tier'][:4]:5}{r['target'].replace(ROOT + '/', ''):46} "
              f"loc={str(r['loc']):5} nafails={non_artifact_fails(r)} blockers={r['blockers']}")
    code_fail = sorted([r for r in dg_files if non_artifact_fails(r)], key=lambda x: x["composite"])
    print(f"\n=== per-file hot-spots: {len(code_fail)} files with a non-artifact dim in Fail ===")
    for r in code_fail:
        print(f"  {r['target'].replace(ROOT + '/', ''):48} loc={str(r['loc']):5} {non_artifact_fails(r)}")
    print("\n=== WORST PATHS/dirs (composite asc) ===")
    for r in sorted(dg_paths, key=lambda x: x["composite"]):
        print(f"{r['composite']:.3f} {r['tier'][:4]:5}{r['target'].replace(ROOT, '.'):40} "
              f"({r['files']}f) nafails={non_artifact_fails(r)}")
    dimfail = Counter(d for r in dg_files for d in non_artifact_fails(r))
    print("\n=== dims failing across MOST files (non-artifact) ===")
    for d, n in dimfail.most_common(12):
        print(f"  {d}: {n} files")
    dimwarn = Counter(d for r in dg_files for d in r["warns"])
    print("\n=== dims WARN across MOST files ===")
    for d, n in dimwarn.most_common(12):
        print(f"  {d}: {n} files")
    print(f"\nFILE tiers: {dict(Counter(r['tier'] for r in dg_files))}")
    print(f"Artifacts written to {OUT}/: {SLUG}_50dim_matrix.{{json,tsv}} + {SLUG}_50dim_long.tsv")
    print_contract(
        f"{OUT}/{SLUG}_50dim_matrix.json (+ .tsv wide + _long.tsv per-cell)",
        "crate_50dim_matrix — lossless 50 dims × file/dir/crate",
        [
            "the per-target composite + tier roll-up for EVERY file, directory and the crate as a whole",
            "the dims failing (and warning) across MOST files — the systemic weak dimensions, full list",
            "the lossless long-form cells (evidence + suggestions) for each finding — nothing summarized away",
            "the three artifact paths (json raw + wide tsv + long tsv) so every cell stays auditable",
        ],
    )


if __name__ == "__main__":
    main()
