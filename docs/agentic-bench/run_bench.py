#!/usr/bin/env python3
"""Agentic Tool Benchmark — τ-bench-style harness for Touring's MCP tools.

Turns the qualitative "touring covers 9/10 agentic best-practices" verdict into a
*measured* score by exercising the REAL `touring` binary across three axes:

  1. selection   — precision@1 + MRR of intent→tool ranking (`touring search-tools`),
                   the BFCL / τ-bench "did the agent pick the right tool?" signal.
  2. outcome     — `touring_audit` correctness on known-vulnerable fixtures (does the
                   tool actually surface the SQLi / XSS / clean verdict?).
  3. conformance — MCP best-practice presence from `tools/list` (annotations on every
                   curated tool, curated-surface size, non-empty descriptions).

Composite = weighted mean → 6-tier (Diamond .. Unranked), mirroring touring-quality.

F6 (telemetry §10) adds a control/treatment **A/B axis** for causal attribution: the
same bench run with the coupling ON (treatment) vs OFF (control —
`TOURING_SUGGESTER_DISABLED=1` + `TOURING_MCP_ALL_TOOLS=1`) isolates "did the COUPLING
help?" from "is the model/task good?". `--compare` runs both arms and emits the
attributable delta (and persists the `ab` block the `touring kpi` snapshot consumes).

Contract: JSON to stdout (`--json`, default) or a markdown report (`--md`). The pure
scoring functions (`precision_at_1`, `reciprocal_rank`, `composite_score`, `tier_for`,
`arm_env`, `attribution`) have no I/O and are unit-tested in `test_run_bench.py`.

Usage:
    python3 run_bench.py                       # JSON report (treatment arm)
    python3 run_bench.py --md                  # markdown report
    python3 run_bench.py --arm control         # run with the coupling disabled
    python3 run_bench.py --compare             # run BOTH arms → causal attribution
    python3 run_bench.py --bin /path/to/touring --fail-below 0.80
"""

from __future__ import annotations

# ── Phase 1: constants ──────────────────────────────────────────────────────
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

# Labeled selection scenarios: (intent, acceptable tool-name substrings). Each intent
# maps to the SET of genuinely-relevant tools (standard IR multi-relevance — an intent
# can have more than one correct tool); a hit is the top result containing ANY of them.
SCENARIOS_SELECTION: list[tuple[str, list[str]]] = [
    ("find who calls a function", ["wiring impact"]),
    ("audit a file for security vulnerabilities", ["touring_audit"]),
    ("check overall system health", ["doctor", "e2e", "status"]),
    ("aggregate or count data across many files", ["ctx_execute"]),
    ("full-text search the codebase for a term", ["tantivy"]),
    ("recall how I solved a problem before", ["memory recall"]),
    ("decompose a task into validated steps", ["decompose"]),
    ("file metadata and blast radius before editing", ["ast meta", "blast"]),
]

# Outcome scenarios: (label, file content, expected verdict, expected cwe or None).
SCENARIOS_OUTCOME: list[tuple[str, str, str, int | None]] = [
    ("sql_injection", 'let q = "UNION SELECT password FROM users";', "block", 89),
    ("xss", 'render("<script>alert(1)</script>")', "block", 79),
    ("clean", "fn add(a: i32, b: i32) -> i32 { a + b }\n", "info", None),
]

# Composite weights — selection + outcome are the agentic core; conformance is the
# substrate that lets an agent succeed.
WEIGHTS = {"selection": 0.40, "outcome": 0.40, "conformance": 0.20}

# 6-tier mapping, identical bands to touring-quality / touring-elite.
TIERS = [
    (0.95, "Diamond"),
    (0.90, "Platinum"),
    (0.80, "Gold"),
    (0.70, "Silver"),
    (0.60, "Bronze"),
    (0.0, "Unranked"),
]

CURATED_MIN, CURATED_MAX = 18, 26

# F6 — A/B arms (telemetry §10). The toggles already exist in the binary: `control`
# disables the coupling (cli_suggester silenced + MCP exposes all ~160 tools, defeating
# the C2 curation), `treatment` is the shipped default coupled surface. Running the SAME
# bench in both arms is the level-1 contrafactual that attributes a delta to the coupling.
ARM_ENV: dict[str, dict[str, str]] = {
    "treatment": {},  # coupling active — the shipped default surface
    "control": {"TOURING_SUGGESTER_DISABLED": "1", "TOURING_MCP_ALL_TOOLS": "1"},
}

# Canonical landing spot for the latest A/B attribution — the `touring kpi` snapshot
# reads this to populate its `ab` block (telemetry §11.1).
DEFAULT_AB_OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".ab-latest.json")


# ── Phase 2: CLI ────────────────────────────────────────────────────────────
def parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="τ-bench-style agentic tool benchmark for Touring")
    p.add_argument("--bin", default=None, help="path to the touring binary (default: auto-discover)")
    p.add_argument("--md", action="store_true", help="emit a markdown report instead of JSON")
    p.add_argument("--fail-below", type=float, default=None, help="exit 1 if composite < this floor")
    p.add_argument("--timeout", type=int, default=30, help="per-subprocess timeout (s)")
    p.add_argument("--arm", choices=sorted(ARM_ENV), default="treatment",
                   help="A/B arm: 'control' disables the coupling, 'treatment' is the default (F6 §10)")
    p.add_argument("--compare", action="store_true",
                   help="run BOTH arms and emit the causal attribution delta (F6 §10)")
    p.add_argument("--ab-out", default=None,
                   help=f"persist the A/B attribution block here (default: {DEFAULT_AB_OUT})")
    return p.parse_args(argv)


def discover_binary(explicit: str | None) -> str:
    """Resolve the touring binary: explicit → PATH → canonical release symlink."""
    if explicit:
        return explicit
    found = shutil.which("touring")
    if found:
        return found
    home = os.path.expanduser("~")
    fallback = os.path.join(home, ".claude/rust/target/release/touring")
    if os.path.exists(fallback):
        return fallback
    raise FileNotFoundError("touring binary not found (PATH, --bin, or target/release)")


# ── Phase 3a: pure scoring (unit-tested, no I/O) ────────────────────────────
def _accepted(expected: "str | list[str]") -> list[str]:
    """Normalize a relevance label (single substring or set) to a lowercase list."""
    items = [expected] if isinstance(expected, str) else list(expected)
    return [e.lower() for e in items]


def precision_at_1(result_names: list[str], expected: "str | list[str]") -> bool:
    """True iff the top-ranked result name contains ANY accepted substring."""
    if not result_names:
        return False
    top = result_names[0].lower()
    return any(e in top for e in _accepted(expected))


def reciprocal_rank(result_names: list[str], expected: "str | list[str]") -> float:
    """1/rank of the first result matching ANY accepted substring (0.0 if absent)."""
    accepted = _accepted(expected)
    for i, name in enumerate(result_names, start=1):
        nl = name.lower()
        if any(e in nl for e in accepted):
            return 1.0 / i
    return 0.0


def composite_score(selection: float, outcome: float, conformance: float) -> float:
    """Weighted mean of the three axis scores in [0,1]."""
    return (
        selection * WEIGHTS["selection"]
        + outcome * WEIGHTS["outcome"]
        + conformance * WEIGHTS["conformance"]
    )


def tier_for(score: float) -> str:
    for floor, name in TIERS:
        if score >= floor:
            return name
    return "Unranked"


# ── Phase 3a (cont.): F6 A/B causal attribution (telemetry §10) ─────────────
def arm_env(arm: str) -> dict[str, str]:
    """Env overrides that place the binary in the given A/B arm (pure).

    Returns a FRESH copy so callers can mutate it without corrupting `ARM_ENV`.
    """
    if arm not in ARM_ENV:
        raise ValueError(f"unknown arm {arm!r}; expected one of {sorted(ARM_ENV)}")
    return dict(ARM_ENV[arm])


def attribution(control_composite: float, treatment_composite: float,
                epsilon: float = 0.01) -> dict:
    """Causal attribution of a composite delta to the coupling (telemetry §10, level-1).

    `attributable` is True iff treatment beats control by MORE than `epsilon` (the noise
    floor) — only then is the gain credited to the coupling rather than to chance/model.
    """
    delta = treatment_composite - control_composite
    if delta > epsilon:
        verdict = "coupling_helps"
    elif delta < -epsilon:
        verdict = "coupling_hurts"
    else:
        verdict = "coupling_neutral"
    return {
        "control_composite": round(control_composite, 4),
        "treatment_composite": round(treatment_composite, 4),
        "delta": round(delta, 4),
        "epsilon": epsilon,
        "attributable": delta > epsilon,
        "verdict": verdict,
    }


# ── Phase 3b: binary drivers (real I/O) ─────────────────────────────────────
def run_search(binary: str, intent: str, timeout: int, env: dict | None = None) -> list[str]:
    """Run `touring search-tools <intent> -j` → ranked result names."""
    try:
        out = subprocess.run(
            [binary, "search-tools", intent, "-j"],
            capture_output=True, text=True, timeout=timeout, env=env,
        ).stdout
        data = json.loads(out)
        return [r["name"] for r in data.get("results", [])]
    except (subprocess.SubprocessError, json.JSONDecodeError, KeyError, OSError):
        return []


def mcp_session(binary: str, calls: list[dict], timeout: int, env: dict | None = None) -> list[dict]:
    """Spawn `touring serve`, perform the MCP handshake, send `calls`, parse responses."""
    reqs = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "agentic-bench", "version": "0"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
    ] + calls
    payload = "\n".join(json.dumps(r) for r in reqs) + "\n"
    try:
        proc = subprocess.run(
            [binary, "serve"], input=payload, capture_output=True, text=True,
            timeout=timeout, env=env,
        )
    except subprocess.SubprocessError:
        return []
    out: list[dict] = []
    for line in proc.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return out


def _result_for(responses: list[dict], req_id: int) -> dict | None:
    for m in responses:
        if m.get("id") == req_id and "result" in m:
            return m["result"]
    return None


def _tool_text(result: dict | None) -> str:
    if not result:
        return ""
    for c in result.get("content", []):
        if c.get("type") == "text":
            return c.get("text", "")
    return ""


def score_selection(binary: str, timeout: int, env: dict | None = None) -> dict:
    """precision@1 + MRR over the labeled selection scenarios."""
    hits, rr, details = 0, 0.0, []
    for intent, expected in SCENARIOS_SELECTION:
        names = run_search(binary, intent, timeout, env)
        p1 = precision_at_1(names, expected)
        r = reciprocal_rank(names, expected)
        hits += int(p1)
        rr += r
        details.append({"intent": intent, "expected": expected,
                        "top": names[0] if names else None, "p@1": p1, "rr": round(r, 3)})
    n = len(SCENARIOS_SELECTION)
    return {"precision_at_1": hits / n, "mrr": rr / n, "n": n, "details": details}


def score_outcome(binary: str, timeout: int, env: dict | None = None) -> dict:
    """touring_audit correctness on known fixtures (one MCP session, temp fixtures)."""
    tmp_paths, calls = [], []
    for i, (_label, content, _v, _cwe) in enumerate(SCENARIOS_OUTCOME):
        fd, path = tempfile.mkstemp(suffix=".rs", prefix="bench_audit_")
        os.write(fd, content.encode())
        os.close(fd)
        tmp_paths.append(path)
        calls.append({"jsonrpc": "2.0", "id": 100 + i, "method": "tools/call",
                      "params": {"name": "touring_audit",
                                 "arguments": {"path": path, "layers": ["vuln"]}}})
    try:
        responses = mcp_session(binary, calls, timeout, env)
    finally:
        for p in tmp_paths:
            try:
                os.unlink(p)
            except OSError:
                pass

    passed, details = 0, []
    for i, (label, _c, exp_verdict, exp_cwe) in enumerate(SCENARIOS_OUTCOME):
        text = _tool_text(_result_for(responses, 100 + i))
        verdict, cwes = None, []
        try:
            report = json.loads(text)
            verdict = report.get("verdict")
            cwes = [f.get("cwe_id") for f in report.get("findings", [])]
        except json.JSONDecodeError:
            pass
        ok = verdict == exp_verdict and (exp_cwe is None or exp_cwe in cwes)
        passed += int(ok)
        details.append({"scenario": label, "expected_verdict": exp_verdict,
                        "got_verdict": verdict, "expected_cwe": exp_cwe,
                        "got_cwes": cwes, "pass": ok})
    n = len(SCENARIOS_OUTCOME)
    return {"pass_rate": passed / n, "n": n, "details": details}


def score_conformance(binary: str, timeout: int, env: dict | None = None) -> dict:
    """MCP best-practice presence from a real tools/list."""
    responses = mcp_session(
        binary, [{"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}], timeout, env)
    result = _result_for(responses, 2) or {}
    tools = result.get("tools", [])
    total = len(tools)
    if total == 0:
        return {"score": 0.0, "curated_count": 0, "annotated_pct": 0.0,
                "described_pct": 0.0, "error": "empty tools/list"}
    annotated = sum(1 for t in tools if t.get("annotations"))
    described = sum(1 for t in tools if (t.get("description") or "").strip())
    curated_ok = CURATED_MIN <= total <= CURATED_MAX
    ann_pct = annotated / total
    desc_pct = described / total
    # conformance score = mean of three normalized best-practice signals
    score = (ann_pct + desc_pct + (1.0 if curated_ok else 0.0)) / 3.0
    return {"score": score, "curated_count": total, "curated_in_range": curated_ok,
            "annotated_pct": round(ann_pct, 3), "described_pct": round(desc_pct, 3)}


# ── Phase 4: assemble + emit ────────────────────────────────────────────────
def run_benchmark(binary: str, timeout: int, arm: str = "treatment") -> dict:
    overrides = arm_env(arm)
    # subprocess env=None inherits the parent environment; only override when the arm
    # actually changes something (treatment = no overrides → inherit unchanged).
    env = {**os.environ, **overrides} if overrides else None
    selection = score_selection(binary, timeout, env)
    outcome = score_outcome(binary, timeout, env)
    conformance = score_conformance(binary, timeout, env)
    sel_axis = (selection["precision_at_1"] + selection["mrr"]) / 2.0
    composite = composite_score(sel_axis, outcome["pass_rate"], conformance["score"])
    return {
        "binary": binary,
        "arm": arm,
        "axes": {
            "selection": {"score": round(sel_axis, 4), **selection},
            "outcome": {"score": round(outcome["pass_rate"], 4), **outcome},
            "conformance": conformance,
        },
        "composite": round(composite, 4),
        "tier": tier_for(composite),
        "weights": WEIGHTS,
    }


def persist_ab(block: dict, path: str) -> None:
    """Persist the A/B attribution block (the `touring kpi` snapshot reads it)."""
    with open(path, "w") as f:
        json.dump(block, f, indent=2)


def run_compare(binary: str, timeout: int) -> dict:
    """Run both arms and attribute the composite delta to the coupling (F6 §10)."""
    control = run_benchmark(binary, timeout, "control")
    treatment = run_benchmark(binary, timeout, "treatment")
    attr = attribution(control["composite"], treatment["composite"])
    return {"control": control, "treatment": treatment, "attribution": attr}


def to_markdown(report: dict) -> str:
    a = report["axes"]
    lines = [
        "# Agentic Tool Benchmark — Touring",
        "",
        f"**Composite: {report['composite']:.4f} — {report['tier']}**  "
        f"(arm: `{report.get('arm', 'treatment')}`, binary: `{report['binary']}`)",
        "",
        "| Axis | Score | Detail |",
        "|---|---|---|",
        f"| Selection (intent→tool) | {a['selection']['score']:.3f} | "
        f"p@1={a['selection']['precision_at_1']:.2f}, MRR={a['selection']['mrr']:.2f}, n={a['selection']['n']} |",
        f"| Outcome (touring_audit) | {a['outcome']['score']:.3f} | "
        f"pass {int(a['outcome']['pass_rate'] * a['outcome']['n'])}/{a['outcome']['n']} |",
        f"| Conformance (MCP best-practice) | {a['conformance']['score']:.3f} | "
        f"curated={a['conformance'].get('curated_count')}, "
        f"annotated={a['conformance'].get('annotated_pct')}, described={a['conformance'].get('described_pct')} |",
        "",
        "## Selection detail",
        "",
        "| intent | expected | top result | p@1 |",
        "|---|---|---|---|",
    ]
    for d in a["selection"]["details"]:
        lines.append(f"| {d['intent']} | `{d['expected']}` | `{d['top']}` | {'✅' if d['p@1'] else '❌'} |")
    return "\n".join(lines)


def to_markdown_compare(report: dict) -> str:
    attr = report["attribution"]
    return "\n".join([
        "# Agentic Tool Benchmark — A/B causal attribution (F6 §10)",
        "",
        f"**Verdict: {attr['verdict']}** — attributable to coupling: "
        f"{'✅ yes' if attr['attributable'] else '❌ no'} (ε={attr['epsilon']})",
        "",
        "| Arm | Composite | Tier |",
        "|---|---|---|",
        f"| control (coupling OFF) | {report['control']['composite']:.4f} | {report['control']['tier']} |",
        f"| treatment (coupling ON) | {report['treatment']['composite']:.4f} | {report['treatment']['tier']} |",
        f"| **Δ (treatment − control)** | **{attr['delta']:+.4f}** | — |",
    ])


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        binary = discover_binary(args.bin)
    except FileNotFoundError as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        return 2

    if args.compare:
        report = run_compare(binary, args.timeout)
        attr = report["attribution"]
        ab_block = {"schema": "coupling-ab-v1", "arm": "treatment", **attr}
        out_path = args.ab_out or DEFAULT_AB_OUT
        try:
            persist_ab(ab_block, out_path)
            report["ab_out"] = out_path
        except OSError as e:
            report["ab_out_error"] = str(e)
        print(to_markdown_compare(report) if args.md else json.dumps(report, indent=2))
        treatment_composite = report["treatment"]["composite"]
        if args.fail_below is not None and treatment_composite < args.fail_below:
            print(f"FAIL: treatment composite {treatment_composite:.4f} < floor {args.fail_below}",
                  file=sys.stderr)
            return 1
        return 0

    report = run_benchmark(binary, args.timeout, args.arm)
    print(to_markdown(report) if args.md else json.dumps(report, indent=2))
    if args.fail_below is not None and report["composite"] < args.fail_below:
        print(f"FAIL: composite {report['composite']:.4f} < floor {args.fail_below}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
