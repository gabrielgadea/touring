#!/usr/bin/env python3
"""e2e_test.py — end-to-end integration proof for the Loop Engineering scripts.

RUN, not just written. Exercises the whole loop flow on a throwaway bundle and
asserts the invariants that make the engine safe:
  - diagnose returns a well-formed digest; structure is NEVER empty (ref-c);
  - convergence is FAIL-CLOSED (never "converged" without DAG evidence);
  - phase-close writes valid OKF report + Hyper-Extract abstract;
  - the doc-link gate validates the bundle;
  - both hooks are FAIL-OPEN (exit 0) when inert, and the Stop hook blocks
    (decision:block) when a loop is active and unconverged.

Exit 0 ⟺ every assertion passed. Usage: `e2e_test.py [--verbose]`.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
HOOKS = HERE / "hooks"
FAKE_TASK = "task_e2e_nonexistent_xyz"  # never a real DAG → forces fail-closed
RESULTS = []


def check(name, cond, detail=""):
    RESULTS.append((name, bool(cond), detail))


def run(args, **kw):
    return subprocess.run([sys.executable, *args], capture_output=True, text=True, timeout=120, **kw)


def make_bundle(td):
    bundle = Path(td)
    (bundle / "index.md").write_text(
        '---\ntype: LoopBundle\ntitle: e2e\ndescription: e2e test bundle.\n'
        f'plan_id: {FAKE_TASK}\ntags: [loop]\ntimestamp: 2026-01-01T00:00:00Z\n'
        'okf_version: "0.1"\n---\n\n# e2e\n\n[log](/log.md) · [phases](/phases/)\n'
    )
    (bundle / "log.md").write_text(
        f'---\ntype: Log\ntitle: e2e\ndescription: e2e.\nplan_id: {FAKE_TASK}\n'
        'tags: [loop]\ntimestamp: 2026-01-01T00:00:00Z\nokf_version: "0.1"\n---\n\n# log\n'
    )
    return bundle


def test_diagnose():
    p = run([str(HERE / "loop_diagnose.py"), "--scope", str(HERE), "--json"])
    check("diagnose_exit0", p.returncode == 0)
    dg = json.loads(p.stdout)
    check("diagnose_digest_shape", all(k in dg for k in ("health", "quality50", "wiring", "memory", "structure")))
    check("diagnose_structure_never_empty", dg["structure"]["available"] is True, "ref-c")


def test_converged_fail_closed():
    p = run([str(HERE / "loop_converged.py"), "--task", FAKE_TASK, "--scope", str(HERE), "--json"])
    cv = json.loads(p.stdout)
    check("converged_fail_closed", cv["converged"] is False and "dag_done" in cv["unmet"],
          "never converged without DAG evidence")
    check("converged_exit1", p.returncode == 1)


def test_phase_close_and_gate(bundle):
    (bundle / "phases").mkdir(exist_ok=True)
    (bundle / "knowledge").mkdir(exist_ok=True)
    p = run([str(HERE / "loop_phase_close.py"), "--task", FAKE_TASK, "--phase", "E1",
             "--status", "done", "--summary", "e2e phase", "--bundle", str(bundle)])
    check("phase_close_exit_defined", p.returncode in (0, 1))
    report = bundle / "phases" / "E1.md"
    absj = bundle / "knowledge" / "E1.json"
    check("phase_close_wrote_okf_report", report.exists())
    check("phase_close_wrote_abstract", absj.exists())
    if absj.exists():
        data = json.loads(absj.read_text())
        ids = [e["entity_id"] for e in data.get("entities", [])]
        check("abstract_deterministic_phase_id", "phase:E1" in ids)
    g = run([str(HERE / "loop_doc_link_gate.py"), "--bundle", str(bundle), "--json"])
    gr = json.loads(g.stdout)
    check("doc_gate_no_broken_links", not gr["broken_links"])
    check("doc_gate_no_missing_plan_id", not gr["missing_plan_id"])


def test_hooks_fail_open():
    with tempfile.NamedTemporaryFile(suffix=".json", delete=True) as tf:
        missing = tf.name + ".absent"  # a marker path that does not exist
    for hook in ("loop_stop_guard.py", "loop_snapshot.py"):
        p = run([str(HOOKS / hook), "--marker", missing], stdin=subprocess.DEVNULL)
        check(f"hook_{hook}_inert_exit0", p.returncode == 0 and not p.stdout.strip())


def test_stop_guard_blocks_active():
    with tempfile.TemporaryDirectory() as td:
        marker = Path(td) / "active.json"
        marker.write_text(json.dumps({"task": FAKE_TASK, "scope": str(HERE)}))
        p = run([str(HOOKS / "loop_stop_guard.py"), "--marker", str(marker)], stdin=subprocess.DEVNULL)
        check("stop_guard_exit0_always", p.returncode == 0, "fail-open invariant")
        blocked = False
        try:
            blocked = json.loads(p.stdout).get("decision") == "block"
        except Exception:  # noqa: BLE001
            blocked = False
        check("stop_guard_blocks_when_active_unconverged", blocked)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Loop Engineering E2E integration proof.")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args(argv)

    test_diagnose()
    test_converged_fail_closed()
    with tempfile.TemporaryDirectory() as td:
        test_phase_close_and_gate(make_bundle(td))
    test_hooks_fail_open()
    test_stop_guard_blocks_active()

    passed = sum(1 for _, ok, _ in RESULTS if ok)
    total = len(RESULTS)
    for name, ok, detail in RESULTS:
        if args.verbose or not ok:
            glyph = "✅" if ok else "❌"
            print(f"  {glyph} {name}" + (f"  ({detail})" if detail else ""))
    print(f"E2E: {passed}/{total} assertions passed")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
