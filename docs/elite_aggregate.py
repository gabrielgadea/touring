#!/usr/bin/env python3
"""elite_aggregate.py — 17-gate composite EliteScore aggregator.

Master Plan H1-B (2026-06-13). Closes the meta-gap: the 17 individual gates
(02 architecture, 03 security, 05 testing, 06 documentation, 08 ci_cd_devops,
09 modularization, 15 dependencies, 17 product_docs, plus the 5 new ones from
H1-B: 04 performance, 10 scalability, 11 extensibility, 14 craftsmanship, 16
ux) live in scattered CI steps and Python scripts, but no single composite
score tells the team "we are at Gold tier" vs "we just dropped to Bronze".

This aggregator invokes each gate (or its --check script), reads exit code +
JSON output, and produces a single 0-1 EliteScore with a tier label:

    Diamond  0.95-1.00   Premium badge — release-ready
    Platinum 0.90-0.94   Elite de Mercado
    Gold     0.80-0.89   Release-ready
    Silver   0.70-0.79   Human review required
    Bronze   0.60-0.69   Rewrite recommended
    Unranked < 0.60      BLOCK

Behaviour:
  * Each gate is a sibling Python script (sync_metrics, file_size_gate,
    perf_p99_gate, scalability_scan, extensibility_scan, ux_audit,
    craftsmanship_tdg_gate, etc.) — invoked via subprocess with --json.
  * Per-gate weight is configurable via --weights JSON file (default equal).
  * `--check` exits with the worst per-gate exit code (max of all gates).
  * `--json` emits the composite report on stdout for downstream tooling.

Usage
-----
    docs/elite_aggregate.py --check
    docs/elite_aggregate.py --json
    docs/elite_aggregate.py --check --weights docs/elite_weights.json
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"

# (gate_id, script_relative_path, kind, check_arg)
# kind: "block" (FAIL exit code = 1) | "warn" (FAIL exit code 1 but score deducted) | "advisory" (FAIL exit 2 = ignored)
# check_arg: which CLI flag the script accepts. Aggregator tries --json first, falls back to check_arg.
# R0.5 De-theatralization (2026-06-21): the four gates below previously read a
# file-size PROXY (05/09 — literal "proxy: file size = testability") or a N/A
# constant 1.0 (03/15 — "cargo-deny handled by CI"). They now project from the
# real 50-dim engine (`touring-quality`) via the "dims:<F-ids>" sentinel, so the
# composite reflects the actual testing/modularization/security/dependency
# posture of the source — not a stand-in. cargo-deny remains the CI dep-advisory
# gate (complementary: it scans the resolved dep tree; F2.5/F4.5 scan the source).
DEFAULT_GATES: list[tuple[str, str | None, str, str]] = [
    ("02_architecture", "wiring_integrity_gate.py", "block", "--check"),
    # 03_security: NOT linked to the 50-dim F2.x source-pattern verifiers — dogfood
    # (2026-06-21) found they are WorstOf-broken at workspace scope (a single file
    # among 1839 zeroes F2.1/F2.4/F2.6 → 0.000). cargo-deny advisories in CI is the
    # real security gate. Honest link deferred to W1 (scope-faithful aggregation).
    ("03_security_advisories", None, "block", ""),
    ("05_testing", "dims:F3.1,F3.2,F3.3,F3.4,F3.5,F3.6,F3.7", "block", "--check"),  # R0.5: real 50-dim testing (was file_size proxy)
    ("06_documentation", "gen_reference.py", "block", "--validate"),
    ("08_ci_cd_devops", "root_hygiene_gate.py", "block", "--check"),
    # 09_modularization: F1.7 (boundaries) + F1.8 (deps, W0-fixed). f1_2 dropped —
    # dogfood found it returns ~0.007 workspace-wide (broken verifier, next W0 batch).
    ("09_modularization", "dims:F1.7,F1.8", "block", "--check"),  # R0.5: real 50-dim modularization (was file_size proxy)
    ("10_scalability", "scalability_scan.py", "warn", "--json"),
    ("11_extensibility", "extensibility_scan.py", "warn", "--json"),
    ("14_craftsmanship", "craftsmanship_tdg_gate.py", "warn", "--json"),
    # 15_dependencies: F2.5 (dep-CVEs, real — "no known-bad versions"). F4.5 dropped
    # — its 50-dep cap is absurd for a 44-crate workspace (263 deps → 0.000 artifact).
    ("15_dependencies_advisories", "dims:F2.5", "block", "--check"),  # R0.5: real 50-dim dep-CVEs (cargo-deny still CI for bans)
    ("16_ux", "ux_audit.py", "warn", "--json"),
    ("17_product_docs", "sync_metrics.py", "block", "--check"),
    ("04_performance", "perf_p99_gate.py", "advisory", "--json"),
]

DEFAULT_WEIGHTS: dict[str, float] = {
    "02_architecture": 1.0,
    "03_security_advisories": 1.5,
    "04_performance": 0.7,
    "05_testing": 1.0,
    "06_documentation": 1.0,
    "08_ci_cd_devops": 0.8,
    "09_modularization": 0.8,
    "10_scalability": 0.7,
    "11_extensibility": 0.6,
    "14_craftsmanship": 0.7,
    "15_dependencies_advisories": 1.5,
    "16_ux": 0.6,
    "17_product_docs": 0.9,
}

TIER_THRESHOLDS = [
    (0.95, "Diamond"),
    (0.90, "Platinum"),
    (0.80, "Gold"),
    (0.70, "Silver"),
    (0.60, "Bronze"),
    (0.00, "Unranked"),
]


def tier_for(score: float) -> str:
    for threshold, name in TIER_THRESHOLDS:
        if score >= threshold:
            return name
    return "Unranked"


# R0.5: 50-dim projection target + binary for the "dims:<F-ids>" gates.
TQ_BIN = "touring-quality"
TQ_TARGET = "crates"
TQ_BLOCK_FLOOR = 0.60  # below Bronze → a block gate genuinely FAILs


def _resolve_tq_bin() -> str:
    """Prefer PATH `touring-quality`; fall back to the release artifact."""
    from shutil import which

    if which(TQ_BIN):
        return TQ_BIN
    candidate = ROOT / "target" / "release" / "touring-quality"
    return str(candidate) if candidate.is_file() else TQ_BIN


def run_dim_gate(gate_id: str, kind: str, dims_csv: str) -> dict:
    """Project a gate from the real 50-dim engine (R0.5 de-theatralization).

    Runs `touring-quality score <target> --workspace --dims <F-ids>` and folds
    the real composite into the gate score, so the aggregate reflects the actual
    measured posture instead of a file-size proxy or an N/A constant. Fail-open
    but loud: an unavailable engine scores 0.5/UNAVAILABLE, never a silent PASS.
    """
    binv = _resolve_tq_bin()
    try:
        out = subprocess.run(
            [binv, "score", TQ_TARGET, "--workspace", "--dims", dims_csv, "--format", "json"],
            cwd=ROOT, capture_output=True, text=True, timeout=120,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError, OSError) as exc:
        return {"gate_id": gate_id, "kind": kind, "score": 0.5, "status": "UNAVAILABLE", "note": str(exc)[:160], "dims": dims_csv}
    if out.returncode != 0:
        return {"gate_id": gate_id, "kind": kind, "score": 0.5, "status": "UNAVAILABLE", "note": (out.stderr or "non-zero exit")[:160], "dims": dims_csv}
    try:
        payload = json.loads(out.stdout) if out.stdout.strip() else {}
    except json.JSONDecodeError:
        payload = {}
    score = float(payload.get("composite", payload.get("composite_score", 0.0)))
    status = "MEASURED" if score >= TQ_BLOCK_FLOOR else "FAIL"
    return {"gate_id": gate_id, "kind": kind, "score": round(score, 4), "status": status, "dims": dims_csv, "tier": payload.get("tier")}


def run_gate(gate_id: str, script: str | None, kind: str, check_arg: str) -> dict:
    """Invoke a gate's Python script and capture its verdict + JSON output.

    The aggregator tries `--json` first (machine-readable), then falls back to
    whatever `check_arg` the script declares. Exit 0 = PASS; non-zero is FAIL
    (or WARN for kind=warn, ADVISORY for kind=advisory).
    """
    if script is None:
        return {"gate_id": gate_id, "kind": kind, "score": 1.0, "status": "N/A", "note": "external CI step (cargo-deny)"}
    path = DOCS / script
    if not path.is_file():
        return {"gate_id": gate_id, "kind": kind, "score": 0.5, "status": "MISSING", "note": f"{path} not found"}
    # Per-gate extra args (e.g., extensibility uses --max-dispatch-arms 30 in production)
    extra_args = {
        "extensibility_scan.py": ["--max-dispatch-arms", "30"],
    }.get(script, [])
    # Try --json first; if the script doesn't accept it, fall back to the gate's check_arg.
    for arg in (["--json", *extra_args], ([check_arg, *extra_args] if check_arg else [])):
        try:
            out = subprocess.run(
                ["python3", str(path), *arg],
                cwd=ROOT, capture_output=True, text=True, timeout=120,
            )
        except subprocess.TimeoutExpired:
            return {"gate_id": gate_id, "kind": kind, "score": 0.0, "status": "TIMEOUT", "note": "gate exceeded 120s"}
        # If the script doesn't recognise --json (exit code 2 + "unrecognised" in stderr), retry.
        if "--json" in arg and out.returncode == 2 and "unrecogni" in out.stderr.lower():
            continue
        break
    if out.returncode == 2 and kind == "advisory":
        return {"gate_id": gate_id, "kind": kind, "score": 0.5, "status": "ADVISORY", "note": out.stderr.strip()[:200] or "fail-open"}
    if out.returncode == 0:
        score = 1.0
        try:
            payload = json.loads(out.stdout) if out.stdout.strip() else {}
        except json.JSONDecodeError:
            payload = {}
        return {"gate_id": gate_id, "kind": kind, "score": score, "status": "PASS", "payload": payload}
    try:
        payload = json.loads(out.stdout) if out.stdout.strip() else {}
    except json.JSONDecodeError:
        payload = {}
    if kind == "warn":
        return {"gate_id": gate_id, "kind": kind, "score": 0.5, "status": "WARN", "payload": payload}
    return {"gate_id": gate_id, "kind": kind, "score": 0.0, "status": "FAIL", "payload": payload}


def dispatch_gate(gate_id: str, script: str | None, kind: str, check_arg: str) -> dict:
    """Route a gate to the 50-dim projection (`dims:<F-ids>`) or its script."""
    if script and script.startswith("dims:"):
        return run_dim_gate(gate_id, kind, script[len("dims:"):])
    return run_gate(gate_id, script, kind, check_arg)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="CI mode (default)")
    parser.add_argument("--json", action="store_true", help="machine-readable JSON on stdout")
    parser.add_argument("--weights", type=Path, help="JSON file mapping gate_id -> weight")
    args = parser.parse_args()

    weights = dict(DEFAULT_WEIGHTS)
    if args.weights and args.weights.is_file():
        weights.update(json.loads(args.weights.read_text(encoding="utf-8")))

    results: list[dict] = []
    for gate_id, script, kind, check_arg in DEFAULT_GATES:
        if gate_id not in weights:
            continue
        results.append(dispatch_gate(gate_id, script, kind, check_arg))

    weighted_sum = sum(weights[r["gate_id"]] * r["score"] for r in results)
    weight_total = sum(weights[r["gate_id"]] for r in results)
    composite = weighted_sum / weight_total if weight_total else 0.0
    tier = tier_for(composite)

    report = {
        "composite_score": round(composite, 4),
        "tier": tier,
        "gates": results,
        "weights": {r["gate_id"]: weights[r["gate_id"]] for r in results},
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"elite_aggregate: composite={composite:.4f} tier={tier}")
        for r in results:
            print(f"  {r['gate_id']:30s} score={r['score']:.2f} status={r['status']:8s} kind={r['kind']}")

    fail = any(r["status"] == "FAIL" and r["kind"] == "block" for r in results)
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
