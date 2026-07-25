#!/usr/bin/env python3
"""loop_converged.py — the Loop Engineering convergence gate.

Measures whether a loop run is "complete AND perfect" instead of asserting it.
Evaluates the convergence clauses from the ``loop-engineering`` skill and exits
0 (converged) or 1 (continue). Composes Touring CLI (decompose, touring-quality,
wiring, cargo) as a Layer-3 deterministic pipeline; fail-open on daemon errors.

Clauses (a clause is PASS / FAIL / N/A; converged ⟺ no clause is FAIL):
  1. dag_done      — every subtask of --task is done/finalized.            (always)
  2. quality_gold  — touring-quality score --fail-below 0.80 passes.       (Rust scope)
  3. no_p0_fail    — no P0 BLOCK dim (F2.1/2.4/2.5/2.6/F4.3/4.5) in Fail.  (Rust scope)
  4. orphans_base  — wiring orphans <= baseline.                           (Rust scope)
  5. cargo_green   — cargo check (+ test+clippy with --rust-full).         (Rust scope)
  6. cross_audit   — the bundle's audit-plan-completion.sh exits 0.        (if present)

Usage:
    loop_converged.py --task <task_id> [--scope <path>] [--bundle <dir>]
                      [--rust-full] [--json] [--quiet]

Exit codes: 0 converged · 1 not converged · 2 usage error.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

P0_DIMS = {"F2_1", "F2_4", "F2_5", "F2_6", "F4_3", "F4_5"}
GOLD_OR_BETTER = {"Gold", "Platinum", "Diamond"}


def run(cmd, timeout=900):
    """Run a command; return (rc, stdout, stderr). Never raises (fail-open)."""
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr
    except Exception as exc:  # noqa: BLE001 — fail-open by design
        return 127, "", str(exc)


def parse_json(text):
    """Best-effort: parse the first JSON object/array in ``text``."""
    text = (text or "").strip()
    if not text:
        return None
    try:
        return json.loads(text)
    except Exception:  # noqa: BLE001
        start = text.find("{")
        end = text.rfind("}")
        if 0 <= start < end:
            try:
                return json.loads(text[start : end + 1])
            except Exception:  # noqa: BLE001
                return None
    return None


def is_rust_scope(scope: Path) -> bool:
    """A scope is Rust when it (or an ancestor) carries a Cargo.toml."""
    scope = scope.resolve()
    for d in (scope, *scope.parents):
        if (d / "Cargo.toml").exists():
            return True
    return False


# ── Clauses ──────────────────────────────────────────────────────────────────
def clause_dag_done(task):
    # `touring decompose get <task>` emits JSON by default (no -j flag). Fail
    # CLOSED: a convergence gate must NEVER declare "done" without positive
    # evidence that every subtask is done — missing evidence is "continue".
    _, out, _err = run(["touring", "decompose", "get", task], timeout=60)
    data = parse_json(out)
    if data is None or not isinstance(data.get("subtasks"), list):
        return False, "decompose get failed — DAG unverifiable (fail-closed)", "ensure the daemon is up and the task exists"
    subs = data["subtasks"]
    if not subs:
        return False, f"task {task} has no subtasks — cannot confirm completion", None
    pending = [
        str(s.get("subtask_id", "")).split("::")[-1]
        for s in subs
        # o DAG mistura vocabulários: "completed" (closes legados) e "done"
        # (loop_phase_close) — ambos são terminais; tratar um como pendente
        # tornaria dag_done inalcançável (observado em task_1784839254210619613)
        if s.get("status") not in ("done", "completed", "finalized")
    ]
    ok = not pending
    ev = f"{len(subs) - len(pending)}/{len(subs)} subtasks done"
    nxt = None if ok else f"execute pending subtask(s): {','.join(pending[:6])}"
    return ok, ev, nxt


def clause_quality(scope):
    """Returns ``(gold_ok, p0_fail_list, tier, composite)``.

    ``gold_ok`` ⟺ **tier ≥ Gold** — the honest verdict. The quality-gate caps
    the tier to Silver when a WARN/BLOCK dim fails, so ``composite ≥ 0.80`` alone
    can still be Silver (a lenient false-pass). Scores the SCOPE directly: NO
    ``--workspace`` (which would resolve to the ambient workspace and score the
    wrong tree — audit finding 2026-07-02). Fail-CLOSED on tool error.
    """
    _, out, _err = run(
        ["touring-quality", "score", str(scope), "--format", "json"], timeout=1800)
    data = parse_json(out)
    if not data:
        return False, [], None, None  # applicable but unverifiable → fail-closed
    tier = data.get("tier")
    composite = data.get("composite")
    p0_fail = []
    dims = data.get("dimensions")
    if isinstance(dims, dict):
        for d in P0_DIMS:
            dim = dims.get(d)
            if isinstance(dim, dict) and dim.get("status") == "Fail":
                p0_fail.append(d)
    return (tier in GOLD_OR_BETTER), p0_fail, tier, composite


def clause_orphans(scope, bundle: Path):
    rc, out, _ = run(["touring", "wiring", "orphans", "-j"], timeout=120)
    data = parse_json(out)
    if rc != 0 or data is None:
        return False, "wiring orphans unavailable (fail-closed)"
    count = data.get("orphan_count", data.get("count"))
    if count is None and isinstance(data.get("orphans"), list):
        count = len(data["orphans"])
    if count is None:
        return False, "orphan count not found (fail-closed)"
    base_file = bundle / ".baseline" / "orphans.txt"
    if base_file.exists():
        try:
            baseline = int(base_file.read_text().strip())
        except Exception:  # noqa: BLE001
            baseline = count
        ok = count <= baseline
        return ok, f"orphans={count} baseline={baseline}"
    base_file.parent.mkdir(parents=True, exist_ok=True)
    base_file.write_text(str(count))
    return True, f"orphans={count} (baseline recorded, first run)"


def clause_cargo(rust_full):
    rc, _, err = run(["cargo", "check", "--workspace"], timeout=1800)
    if rc != 0:
        return False, f"cargo check FAILED (rc={rc}): {err.strip().splitlines()[-1] if err.strip() else ''}"
    if not rust_full:
        return True, "cargo check green (test+clippy deferred; pass --rust-full for the final gate)"
    for name, cmd in (("test", ["cargo", "test", "--workspace"]),
                      ("clippy", ["cargo", "clippy", "--workspace", "--", "-D", "warnings"])):
        r, _, e = run(cmd, timeout=2400)
        if r != 0:
            return False, f"cargo {name} FAILED (rc={r})"
    return True, "cargo check + test + clippy green"


def clause_cross_audit(bundle: Path):
    script = bundle / "audit-plan-completion.sh"
    if not script.exists():
        return None, "no audit-plan-completion.sh — skipped"
    rc, out, err = run(["bash", str(script)], timeout=1200)
    return rc == 0, f"cross-audit rc={rc}"


# ── Orchestration ────────────────────────────────────────────────────────────
def _gather_clauses(task, scope: Path, bundle: Path, rust_full, rust):
    """Yield ``(name, ok, evidence, action)`` for every convergence clause.

    ``ok`` is True (pass) / False (fail — blocks) / None (N/A — does not block).
    """
    yield ("dag_done", *clause_dag_done(task))

    if rust:
        gold, p0, tier, comp = clause_quality(scope)
        yield ("quality_gold", gold, f"tier={tier} composite={comp}",
               "raise touring-quality to >= Gold (0.80)")
        yield ("no_p0_fail", len(p0) == 0, f"P0 fails: {p0 or 'none'}",
               f"fix P0 BLOCK dims: {','.join(p0)}" if p0 else None)
        yield ("orphans_base", *clause_orphans(scope, bundle),
               "wire new orphan pub symbols (REGRA #0)")
        yield ("cargo_green", *clause_cargo(rust_full),
               "fix the failing cargo check/test/clippy")
    else:
        for k in ("quality_gold", "no_p0_fail", "orphans_base", "cargo_green"):
            yield (k, None, "scope is not a Rust crate", None)

    yield ("cross_audit", *clause_cross_audit(bundle), "resolve cross-audit findings")


def evaluate(task, scope: Path, bundle: Path, rust_full):
    rust = is_rust_scope(scope)
    clauses, unmet, next_action = {}, [], None
    for name, ok, evidence, action in _gather_clauses(task, scope, bundle, rust_full, rust):
        clauses[name] = {"result": _label(ok), "evidence": evidence}
        if ok is False:
            unmet.append(name)
            next_action = next_action or action
    return {
        "converged": not unmet,
        "task": task,
        "scope": str(scope),
        "rust_scope": rust,
        "clauses": clauses,
        "unmet": unmet,
        "next_action": None if not unmet else (next_action or "address unmet clauses"),
    }


def _label(ok):
    return "N/A" if ok is None else ("PASS" if ok else "FAIL")


def main(argv=None):
    ap = argparse.ArgumentParser(description="Loop Engineering convergence gate.")
    ap.add_argument("--task", required=True, help="Touring decompose task_id")
    ap.add_argument("--scope", default=".", help="path scored for quality/cargo (default: cwd)")
    ap.add_argument("--bundle", default=None, help="OKF bundle dir (baseline + cross-audit); default: --scope")
    ap.add_argument("--rust-full", action="store_true", help="also run cargo test + clippy (the final gate)")
    ap.add_argument("--json", action="store_true", help="emit JSON only")
    ap.add_argument("--quiet", action="store_true", help="no human output, just the exit code")
    args = ap.parse_args(argv)

    scope = Path(args.scope)
    bundle = Path(args.bundle) if args.bundle else scope
    report = evaluate(args.task, scope, bundle, args.rust_full)

    if args.json:
        print(json.dumps(report, indent=2))
    elif not args.quiet:
        state = "✅ CONVERGED" if report["converged"] else "🔄 CONTINUE"
        print(f"{state}  task={report['task']}  scope={report['scope']}")
        for name, c in report["clauses"].items():
            glyph = {"PASS": "✅", "FAIL": "❌", "N/A": "➖"}.get(c["result"], "?")
            print(f"  {glyph} {name:<14} {c['evidence']}")
        if not report["converged"]:
            print(f"  → next: {report['next_action']}")

    return 0 if report["converged"] else 1


if __name__ == "__main__":
    sys.exit(main())
