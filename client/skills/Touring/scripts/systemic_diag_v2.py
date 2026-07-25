#!/usr/bin/env python3
"""Complete systemic + integrated diagnostic — ALL 50 dims × 42 crates × arch.

Supersedes systemic_diag.py (which only fused the 6 P0 dims → an incomplete view).
This captures EVERY verifier the harness runs (touring-quality/src/verifications,
50 dims), grouped by the AUTHORITATIVE enforcement map read from the source
(`DimId::enforcement()` in touring-quality/src/lib.rs):

  * 6 BLOCK  (P0, weight 2.0, fail disqualifies tier): F2.1 F2.4 F2.5 F2.6 F4.3 F4.5
  * 13 WARN  (P1, weight 1.5): F1.1-1.6 F3.1-3.7
  * 31 ADVISORY (weight 1.0): the rest

Two views nothing before produced:
  1. PER-DIMENSION WORKSPACE HEALTH — for each of the 50 dims, how many of the 42
     crates Fail / Warn / Pass. Reveals which dimensions the workspace is
     *systemically* weak on (not one crate — the whole tree).
  2. PER-CRATE all-50 fused risk — the composite weight (2.0/1.5/1.0) × architecture
     blast (fan_in). A BLOCK-dim fail in a fan_in=20 foundation crate ranks first.

Pure cargo + touring-quality + filesystem — deterministic. Writes JSON + digest.

Usage (run from the workspace root):
  systemic_diag_v2.py                              # whole workspace (all crates)
  systemic_diag_v2.py crates/touring-cortex        # one crate (crate scope)
  systemic_diag_v2.py crates                       # every crate under a folder
  systemic_diag_v2.py crates/touring-cortex/src/handlers  # a sub-crate folder (path scope)
  systemic_diag_v2.py crates/touring-cortex/src/pipeline.rs  # a single file
Blast (fan_in) is always workspace-relative, so a scoped run keeps the
architectural amplification the fused risk depends on.
"""
import json
import os
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report_contract import print_contract  # noqa: E402 — sibling module, path set above

ROOT = Path(".").resolve()
# Artifacts land in the invocation dir (or DIAG_OUT) — a permanent, relocatable
# tool must not write next to itself in the skill directory.
OUT = os.environ.get("DIAG_OUT") or os.getcwd()
QBIN = str(Path.home() / ".claude/rust/target/release/touring-quality")
if not Path(QBIN).is_file():
    raise SystemExit(f"touring-quality binary not found at {QBIN}; run `update-touring` first")

BLOCK = {"F2_1", "F2_4", "F2_5", "F2_6", "F4_3", "F4_5"}
WARN = {"F1_1", "F1_2", "F1_3", "F1_4", "F1_5", "F1_6",
        "F3_1", "F3_2", "F3_3", "F3_4", "F3_5", "F3_6", "F3_7"}
WEIGHT = {"Block": 2.0, "Warn": 1.5, "Advisory": 1.0}
NAME = {  # META_TABLE (touring-quality/src/lib.rs)
    "F1_1": "complexity", "F1_2": "maintainability", "F1_3": "duplication",
    "F1_4": "solid", "F1_5": "tech-debt", "F1_6": "error-handling",
    "F1_7": "boundaries", "F1_8": "dep-cycles", "F1_9": "api-design",
    "F1_10": "data-model", "F1_11": "patterns", "F1_12": "arch-consistency",
    "F2_1": "owasp", "F2_2": "input-validation", "F2_3": "authz", "F2_4": "secrets",
    "F2_5": "dep-cves", "F2_6": "config", "F2_7": "db-perf", "F2_8": "memory",
    "F2_9": "caching", "F2_10": "io", "F2_11": "concurrency", "F2_12": "frontend",
    "F2_13": "scalability", "F3_1": "coverage", "F3_2": "test-quality",
    "F3_3": "test-pyramid", "F3_4": "edge-cases", "F3_5": "test-maint",
    "F3_6": "sec-tests", "F3_7": "perf-tests", "F3_8": "inline-doc",
    "F3_9": "api-doc", "F3_10": "arch-doc", "F3_11": "readme",
    "F3_12": "doc-accuracy", "F3_13": "changelog", "F4_1": "idioms",
    "F4_2": "frameworks", "F4_3": "deprecated", "F4_4": "modernization",
    "F4_5": "pkg-mgmt", "F4_6": "build-config", "F4_7": "cicd", "F4_8": "deploy",
    "F4_9": "iac", "F4_10": "monitoring", "F4_11": "incident", "F4_12": "env",
}


def enforcement(dim: str) -> str:
    """Authoritative enforcement class (mirrors DimId::enforcement())."""
    return "Block" if dim in BLOCK else "Warn" if dim in WARN else "Advisory"


def run(cmd: list[str], timeout: int = 200) -> subprocess.CompletedProcess:
    """Run a subprocess capturing text output (never raises on non-zero exit)."""
    return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=timeout)


def workspace_fan_in() -> tuple[list[str], dict[str, str], dict[str, int]]:
    """Return (members, manifest_dir, fan_in) from cargo metadata.

    The dependency graph is the diagnostic's backbone — there is no useful
    output without it — so a missing `cargo` (OSError), a non-zero exit
    (SubprocessError), or unparseable output (JSONDecodeError) is fatal with a
    clear message rather than an opaque traceback.
    """
    try:
        meta = json.loads(run(["cargo", "metadata", "--format-version", "1", "--no-deps"]).stdout)
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError) as e:
        raise SystemExit(f"cargo metadata failed ({type(e).__name__}: {e}) — cannot build the crate graph")
    members = sorted(p["name"] for p in meta["packages"])
    mset = set(members)
    mdir = {p["name"]: str(Path(p["manifest_path"]).parent.relative_to(ROOT))
            for p in meta["packages"]}
    fan_in: dict[str, int] = defaultdict(int)
    for p in meta["packages"]:
        for d in p["dependencies"]:
            if d["name"] in mset:
                fan_in[d["name"]] += 1
    return members, mdir, fan_in


def score_all_dims(target_rel: str, scope: str = "crate") -> dict:
    """Full 50-dim score for a target at `scope` (crate/path/file): dims + composite + tier."""
    try:
        r = run([QBIN, "score", target_rel, "--scope", scope, "--format", "json"])
        d = json.loads(r.stdout)
        dm = d["dimensions"]
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError, KeyError) as e:
        # One un-scoreable crate degrades to an error row, never aborting the sweep.
        return {"error": f"{type(e).__name__}: {e}"}
    return {
        "composite": d.get("composite"), "tier": d.get("tier"),
        "dims": {k: {"value": round(v["value"], 3), "status": v["status"]}
                 for k, v in dm.items()},
    }


def dim_health(ok: dict) -> dict[str, dict[str, int]]:
    """View 1 — per-dimension workspace health: #crates Fail/Warn/Pass each dim."""
    health: dict[str, dict[str, int]] = {}
    for dim in NAME:
        cnt: Counter = Counter()
        for s in ok.values():
            st = s["dims"].get(dim, {}).get("status")
            if st:
                cnt[st] += 1
        health[dim] = dict(cnt)
    return health


def crate_risk(ok: dict, blast_of: dict[str, int]) -> dict:
    """View 2 — per-unit fused risk: weighted 50-dim defect load × architecture blast.

    `blast_of` maps each scored unit's label to its blast radius (a crate's own
    fan_in, or, for a sub-crate path, its owning crate's fan_in).
    """
    risk = {}
    for c, s in ok.items():
        fails = [k for k, v in s["dims"].items() if v["status"] == "Fail"]
        warns = [k for k, v in s["dims"].items() if v["status"] == "Warn"]
        load = sum(WEIGHT[enforcement(k)] for k in fails) + 0.5 * sum(WEIGHT[enforcement(k)] for k in warns)
        blast = blast_of.get(c, 0)
        risk[c] = {
            "risk": round(load * (1 + blast / 10), 3),
            "composite": round(s["composite"], 4), "tier": s["tier"],
            "blast_fan_in": blast, "block_fails": [k for k in fails if k in BLOCK],
            "n_fail": len(fails), "n_warn": len(warns), "weighted_load": round(load, 2),
        }
    return risk


def resolve_targets(
    target: str | None, members: list[str], mdir: dict[str, str], fan_in: dict[str, int]
) -> list[tuple[str, str, str, int]]:
    """Map an optional target path to diagnostic units [(label, rel_path, scope, blast)].

    - None → every workspace member at crate scope (whole-workspace mode).
    - a path that IS or CONTAINS workspace crates → those crates (crate scope).
    - a sub-path of one crate (a folder or file, not itself a crate) → that path
      at 'path'/'file' scope, its blast inherited from the owning crate.

    Blast is always workspace-relative (fan_in from the full graph) so a scoped
    run keeps the architectural amplification the systemic view depends on.
    """
    if target is None:
        return [(c, mdir[c], "crate", fan_in.get(c, 0)) for c in members]
    tgt = Path(target).resolve()
    if not tgt.exists():
        raise SystemExit(f"target path does not exist: {target}")
    try:
        tgt.relative_to(ROOT)
    except ValueError:
        raise SystemExit(f"target {target} is outside the workspace {ROOT}")
    # crates whose dir is the target or sits under it (exact crate, or folder-of-crates)
    under = [c for c in members
             if (ROOT / mdir[c]) == tgt or tgt in (ROOT / mdir[c]).parents]
    if under:
        return [(c, mdir[c], "crate", fan_in.get(c, 0)) for c in sorted(under)]
    # a sub-path of one crate → score it directly; blast = owning crate's fan_in
    owner = max((c for c in members if (ROOT / mdir[c]) in tgt.parents),
                key=lambda c: len(mdir[c]), default=None)
    rel = str(tgt.relative_to(ROOT))
    return [(rel, rel, "file" if tgt.is_file() else "path", fan_in.get(owner, 0) if owner else 0)]


def main() -> None:
    """Score the target (or whole workspace), fuse the 50-dim × arch matrix, write + print."""
    target = next((a for a in sys.argv[1:] if not a.startswith("-")), None)
    members, mdir, fan_in = workspace_fan_in()
    units = resolve_targets(target, members, mdir, fan_in)
    scores = {label: score_all_dims(path, scope) for label, path, scope, _ in units}
    blast_of = {label: blast for label, _, _, blast in units}
    ok = {k: s for k, s in scores.items() if "dims" in s}
    result = {
        "target": target or str(ROOT), "unit_count": len(units), "scored": len(ok),
        "enforcement": {"block": sorted(BLOCK), "warn": sorted(WARN)},
        "dim_health": dim_health(ok),
        "crate_risk": crate_risk(ok, blast_of),
        # Lossless: full per-unit 50-dim raw (value+status) so EVERY matrix cell
        # is auditable, not only the aggregates above.
        "crate_dims": {k: s["dims"] for k, s in ok.items()},
        "errors": {k: s["error"] for k, s in scores.items() if "error" in s},
    }
    with open(f"{OUT}/systemic_diag_v2_matrix.json", "w") as fh:
        json.dump(result, fh, indent=1)
    _print_digest(result)


def _print_digest(result: dict) -> None:
    """Print the workspace per-dim health table + the fused per-crate risk ranking."""
    dim_health = result["dim_health"]
    print("=" * 84)
    print(f"COMPLETE SYSTEMIC DIAGNOSTIC — 50 dims × {result['scored']}/{result['unit_count']} unit(s) × architecture")
    print(f"target: {result['target']}")
    print("=" * 84)
    if result["errors"]:
        print(f"⚠ {len(result['errors'])} crate(s) failed to score: {list(result['errors'])}")
    print("\n📊 PER-DIMENSION WORKSPACE HEALTH — which of the 50 dims is the tree weak on?")
    print("   (sorted: BLOCK first, then by #Fail across crates)")
    print(f"   {'dim':6} {'name':18} {'enf':9} {'Fail':>4} {'Warn':>4} {'Pass':>4} {'NA':>3}")
    order = sorted(NAME, key=lambda d: (
        {"Block": 0, "Warn": 1, "Advisory": 2}[enforcement(d)],
        -dim_health[d].get("Fail", 0), -dim_health[d].get("Warn", 0)))
    for d in order:
        h = dim_health[d]
        f, w, p, na = h.get("Fail", 0), h.get("Warn", 0), h.get("Pass", 0), h.get("NotApplicable", 0)
        if f or w:  # only dims with any Fail/Warn somewhere
            flag = "⛔" if enforcement(d) == "Block" else ("⚠" if enforcement(d) == "Warn" else "·")
            print(f"   {d:6} {NAME[d]:18} {flag} {enforcement(d):7} {f:4} {w:4} {p:4} {na:3}")
    print("\n🎯 PER-CRATE fused risk (weighted 50-dim load × architecture blast) — top 15:")
    print(f"   {'crate':26} {'risk':>6} {'tier':9} {'blast':>5} {'BLOCK-fail':16} F/W")
    for c, v in sorted(result["crate_risk"].items(), key=lambda x: -x[1]["risk"])[:15]:
        bf = ",".join(v["block_fails"]) or "—"
        print(f"   {c:26} {v['risk']:6.2f} {v['tier'][:8]:9} {v['blast_fan_in']:5} {bf:16} {v['n_fail']}/{v['n_warn']}")
    print(f"\nComplete systemic matrix written: {OUT}/systemic_diag_v2_matrix.json")
    print_contract(
        f"{OUT}/systemic_diag_v2_matrix.json",
        "systemic_diag_v2 — 50 dims × workspace × architecture",
        [
            "the FULL per-dimension health table — EVERY dim with any Fail OR Warn (not the top few), "
            "with enforcement glyph (⛔/⚠/·) and Fail/Warn/Pass counts",
            "the fused per-crate risk ranking — risk, tier, blast fan-in, BLOCK-fail, F/W for each unit",
            "'what the aggregate hid' — dims failing broadly (e.g. F4_5/F1_3/F3_11/F1_8) + the counterfactual "
            "(which crates a single BLOCK-dim fix unlocks)",
            "primary lever (the tier-gating BLOCK dim, with the exact CVE/advisory) + secondary levers "
            "(non-BLOCK but REGRA #0 potentializable)",
        ],
    )


if __name__ == "__main__":
    main()
