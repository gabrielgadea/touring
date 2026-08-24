#!/usr/bin/env python3
"""e2e_test.py — end-to-end integration proof for the Loop Engineering scripts.

RUN, not just written. Exercises the whole loop flow on a throwaway bundle and
asserts the invariants that make the engine safe:
  - diagnose returns a well-formed digest; structure is NEVER empty (ref-c);
  - convergence is FAIL-CLOSED (never "converged" without DAG evidence);
  - phase-close writes valid OKF report + Hyper-Extract abstract;
  - the doc-link gate validates the bundle;
  - both hooks are FAIL-OPEN (exit 0) when inert; the Stop hook RELEASES an
    orphaned/vanished DAG and BLOCKS (decision:block) when a flow's artifact
    manifest is still unmet.

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
    # The bundle's chronological leg must EXIST from birth: loop_snapshot only
    # appends `if log.exists()`, and until 2026-08-02 nothing ever created the
    # file, so every PreCompact resume note was silently dropped.
    with tempfile.TemporaryDirectory() as td:
        bundle = Path(td) / "b"
        run([str(HERE / "loop_diagnose.py"), "--scope", str(HERE),
             "--bundle", str(bundle), "--json"])
        log = bundle / "log.md"
        check("diagnose_creates_bundle_log", log.is_file(), "PreCompact notes need it")
        if log.is_file():
            before = log.read_text()
            run([str(HERE / "loop_diagnose.py"), "--scope", str(HERE),
                 "--bundle", str(bundle), "--json"])
            check("diagnose_never_overwrites_log", log.read_text() == before)


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
    """Both halves of the Stop contract: release an orphan, block an unmet one.

    Corrected 2026-08-02. The old single assertion pointed the guard at
    ``FAKE_TASK`` — a task that by construction does not exist — and demanded a
    *block*. That is the ORPHANED-DAG case, which `loop_stop_guard` was
    deliberately hardened on 2026-07-02 to fail OPEN (release + archive) so a
    dead DAG can never hold a session hostage; SKILL.md states it and
    `test_flow_guard.py` asserts it. The e2e was therefore encoding
    pre-hardening semantics and failing against correct code.
    """
    with tempfile.TemporaryDirectory() as td:
        # (a) gone DAG → must RELEASE.
        orphan = Path(td) / "orphan.json"
        orphan.write_text(json.dumps({"task": FAKE_TASK, "scope": str(HERE),
                                      "status": "active"}))
        p = run([str(HOOKS / "loop_stop_guard.py"), "--marker", str(orphan)],
                stdin=subprocess.DEVNULL)
        check("stop_guard_exit0_always", p.returncode == 0, "fail-open invariant")
        check("stop_guard_orphan_dag_fails_open", p.stdout.strip() == "",
              "a vanished DAG must release the turn, never block it")

        # (b) OUTER marker with an unmet artifact manifest → must BLOCK.
        # The verdict comes from files on disk (ADW Law L3), so this needs no
        # daemon and no live DAG — it is deterministic anywhere.
        scope, bundle = Path(td) / "scope", Path(td) / "bundle"
        scope.mkdir()
        bundle.mkdir()
        outer = Path(td) / "outer.json"
        outer.write_text(json.dumps({
            "task": "OUTER", "status": "outer", "flow": "strategy-outer",
            "scope": str(scope), "bundle": str(bundle), "cwd": str(scope),
            "continuations": 0,
        }))
        q = run([str(HOOKS / "loop_stop_guard.py"), "--marker", str(outer)],
                stdin=subprocess.DEVNULL)
        try:
            blocked = json.loads(q.stdout).get("decision") == "block"
        except Exception:  # noqa: BLE001
            blocked = False
        check("stop_guard_blocks_when_manifest_unmet", blocked,
              "missing OUTER artifacts must block the turn")


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
