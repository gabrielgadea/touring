#!/usr/bin/env python3
"""Guards for the integrity ledger of the loop's own graders.

The threat model is drift, not sabotage. Nothing on a writable filesystem stops
an adversarial writer, and these tests do not pretend otherwise. What they pin
down is that a grader cannot change *silently* -- the failure this repository has
already shipped twice (`find-code` with two tests asserting the bug; a decompose
test that encoded `fog.clear == 2`) and that arXiv:2505.22954 Appendix H
documents as node 114: a perfect score reached by deleting the markers the
detector counted.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import judge_attest as ja  # noqa: E402


@pytest.fixture()
def judge(tmp_path):
    """A complete, self-contained copy of the graders, attested and clean."""
    (tmp_path / "hooks").mkdir()
    for rel in ja.JUDGE_FILES:
        shutil.copy2(HERE / rel, tmp_path / rel)
    shutil.copy2(HERE / "judge_attest.py", tmp_path / "judge_attest.py")
    ja.write_attestation(root=tmp_path, why="fixture")
    return tmp_path


def drift_kinds(root):
    return {d["kind"] for d in ja.verdict(root=root)["drift"]}


def test_an_intact_judge_reports_clean(judge):
    rep = ja.verdict(root=judge)
    assert rep["clean"] and not rep["blocking"]
    assert rep["attested"] is True


def test_a_removed_clause_blocks(judge):
    """Node 114: the rubric loses an entry and the score stops meaning what it meant."""
    target = judge / "loop_converged.py"
    src = target.read_text()
    victim = "cross_audit"
    assert victim in ja.inspect_judge(judge)["clauses"]
    target.write_text(src.replace(f'yield ("{victim}"', 'yield ("_deleted_"'))

    rep = ja.verdict(root=judge)
    assert rep["blocking"], "a vanished clause must block — that is the whole point"
    removed = [d for d in rep["drift"] if d["kind"] == "clause_removed"]
    assert [d["detail"].split(" —")[0] for d in removed] == [victim]


def test_a_changed_grader_speaks_but_does_not_block(judge):
    """Legitimate improvement must stay cheap; only silence is forbidden.

    Blocking every edit would make the remedy costlier than the bypass, which is
    how a gate teaches people to route around it (lesson of 2026-08-19: a gate
    whose failure has no automated remedy)."""
    target = judge / "loop_converged.py"
    target.write_text(target.read_text() + "\n# a harmless, honest edit\n")

    rep = ja.verdict(root=judge)
    assert not rep["blocking"], "an ordinary edit must not stall the loop"
    assert not rep["clean"], "...but it must never pass unmentioned"
    assert "file_changed" in {d["kind"] for d in rep["drift"]}


def test_an_unattested_judge_never_stalls_a_loop(tmp_path):
    """Fail-open, like every hook in this skill: a new guard that halts running
    loops would be a worse defect than the one it closes."""
    (tmp_path / "hooks").mkdir()
    for rel in ja.JUDGE_FILES:
        shutil.copy2(HERE / rel, tmp_path / rel)
    rep = ja.verdict(root=tmp_path)
    assert rep["attested"] is False
    assert not rep["blocking"]
    assert "unattested" in drift_kinds(tmp_path)


def test_an_unreadable_rubric_is_drift_not_an_empty_rubric(judge):
    """If a parse failure returned [], every clause would read as 'removed' — or,
    worse, a comparison of [] against [] would read as intact. Absence of a
    reading is not a reading of absence."""
    (judge / "loop_converged.py").write_text("def _gather_clauses(:\n  syntax error\n")
    rep = ja.verdict(root=judge)
    assert rep["blocking"]
    assert "rubric_unreadable" in {d["kind"] for d in rep["drift"]}
    assert ja.inspect_judge(judge)["clauses"] is None


def test_a_missing_grader_blocks(judge):
    (judge / "hooks" / "flow_manifests.json").unlink()
    rep = ja.verdict(root=judge)
    assert rep["blocking"]
    assert "file_missing" in {d["kind"] for d in rep["drift"]}


def test_a_clause_yielded_in_two_branches_counts_once(judge):
    """`cargo_green` is yielded twice (Rust scope and not). The rubric is the SET
    of names a verdict can carry, so that is one clause; counting it twice made
    the ledger's own report wrong on its first run."""
    clauses = ja.inspect_judge(judge)["clauses"]
    assert len(clauses) == len(set(clauses)), f"duplicates in {clauses}"
    assert "cargo_green" in clauses


def test_clauses_are_reported_in_source_order(judge):
    """`ast.walk` is breadth-first: a yield nested in an `if` surfaces after its
    top-level siblings. A ledger a human reads must follow the file."""
    clauses = ja.inspect_judge(judge)["clauses"]
    src = (judge / "loop_converged.py").read_text()
    positions = [src.index(f'yield ("{c}"') for c in clauses]
    assert positions == sorted(positions), f"out of order: {clauses}"


def test_every_law_bearing_grader_is_in_the_ledger():
    """Structural: both laws must be covered. L2 is the judge; L3 is the artifact
    gate AND the manifest that says which artifacts are owed -- a ledger holding
    the gate but not its rubric would leave the rubric freely editable."""
    assert "loop_converged.py" in ja.JUDGE_FILES
    assert "hooks/loop_outer_gate.py" in ja.JUDGE_FILES
    assert "hooks/flow_manifests.json" in ja.JUDGE_FILES
    for rel in ja.JUDGE_FILES:
        assert (HERE / rel).is_file(), f"{rel} is attested but does not exist"


def test_attesting_records_who_and_why_and_when(judge):
    doc = ja.write_attestation(root=judge, why="porque sim")
    stored = json.loads((judge / "judge.attest.json").read_text())
    assert stored["why"] == "porque sim"
    assert stored["attested_by"] and stored["attested_at"]
    assert stored["clauses"] == doc["clauses"]


def test_the_declared_rubric_matches_the_enforced_one():
    """The drift that motivated all of this: the module docstring listed six
    clauses while seven ran, and nothing reconciled the two. Now something does."""
    path = HERE / "loop_converged.py"
    enforced = ja.enforced_clauses(path, "_gather_clauses")
    doc = path.read_text().split('"""')[1]
    declared = [c for c in enforced if f" {c} " in doc or f" {c}\n" in doc or f"{c}  " in doc]
    missing = [c for c in enforced if c not in declared]
    assert not missing, f"clauses enforced but not declared in the docstring: {missing}"


def test_the_verdict_carries_the_identity_of_the_judge_that_issued_it():
    """End to end: `loop_converged.py --json` must name its own grader state, so a
    verdict can never be read without knowing what produced it."""
    proc = subprocess.run(
        [sys.executable, str(HERE / "loop_converged.py"),
         "--task", "FAKE_TASK_FOR_TEST", "--scope", str(HERE), "--json"],
        capture_output=True, text=True, timeout=600,
    )
    report = json.loads(proc.stdout)
    assert "judge" in report, "the verdict must carry the judge's identity"
    assert "judge_intact" in report["clauses"]
    assert report["clauses"]["judge_intact"]["result"] in {"PASS", "FAIL", "N/A"}


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v", "--tb=short", "-p", "no:cacheprovider"]))
