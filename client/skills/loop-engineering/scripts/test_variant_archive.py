#!/usr/bin/env python3
"""Guards for the stepping-stone archive.

What is being protected is a claim with a number behind it: arXiv:2505.22954
measures 50.0% with the archive against 39.7% greedy on SWE-bench. The mechanism
that produces the gap is that a LOWER-scoring variant keeps a real chance of
being branched from — so the tests that matter are the ones asserting the weak
variant is still reachable, and that a strong one which has been mined already
steps aside.
"""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import variant_archive as va  # noqa: E402


@pytest.fixture()
def arc(tmp_path):
    return tmp_path


def test_a_variant_has_one_identity_whatever_the_run(arc):
    """Derived, never emergent (REGRA #17): the archive must recognise a variant
    it has seen rather than storing a second copy."""
    assert va.variant_id("diff-A") == va.variant_id("diff-A")
    assert va.variant_id("diff-A") != va.variant_id("diff-B")


def test_what_fails_the_gate_is_kept_not_discarded(arc):
    """The entire discipline in one assertion."""
    va.record("crate/x", "attempt-1", score=0.31, verdict="REJECT", root=arc)
    rows = va.weights("crate/x", root=arc)
    assert len(rows) == 1
    assert rows[0]["score"] == 0.31
    assert rows[0]["p"] > 0.0, "a rejected variant must remain reachable"


def test_a_weaker_variant_keeps_a_real_chance(arc):
    """Figure 3: "many paths to innovation traverse lower-performing nodes". If
    the weak variant's probability rounded to nothing, this would be a
    leaderboard with extra steps."""
    va.record("t", "weak", score=0.35, root=arc)
    va.record("t", "strong", score=0.85, root=arc)
    p = {r["variant_id"]: r["p"] for r in va.weights("t", root=arc)}
    weak, strong = p[va.variant_id("weak")], p[va.variant_id("strong")]
    assert strong > weak, "the sigmoid must still prefer the better variant"
    assert weak > 0.02, f"the weak variant was effectively pruned: {weak}"


def test_a_much_mined_leader_yields_to_an_unexplored_peer(arc):
    """The novelty bonus h = 1/(1+children) is what makes this an archive rather
    than a leaderboard."""
    va.record("t", "leader", score=0.80, root=arc)
    va.record("t", "fresh", score=0.75, root=arc)
    leader = va.variant_id("leader")
    for i in range(5):
        va.record("t", f"child-{i}", score=0.5, parent=leader, root=arc)
    p = {r["variant_id"]: r["p"] for r in va.weights("t", root=arc)}
    assert p[va.variant_id("fresh")] > p[leader], (
        "a leader branched from five times must yield to an unexplored peer")


def test_a_perfect_variant_is_not_sampled(arc):
    """Appendix C.2's eligible set is `α < 1`. Sampling a finished answer spends
    budget re-deriving it."""
    va.record("t", "done", score=1.0, root=arc)
    va.record("t", "open", score=0.4, root=arc)
    ids = [r["variant_id"] for r in va.weights("t", root=arc)]
    assert va.variant_id("done") not in ids
    assert va.variant_id("open") in ids


def test_the_same_seed_draws_the_same_parents(arc):
    """A selection nobody can replay is a selection nobody can audit."""
    for i in range(20):
        va.record("t", f"v-{i}", score=(i % 10) / 10.0, root=arc)
    a = va.sample_parents("t", k=4, seed=7, root=arc)
    b = va.sample_parents("t", k=4, seed=7, root=arc)
    c = va.sample_parents("t", k=4, seed=8, root=arc)
    assert a == b, "same seed must draw the same parents"
    assert len(a) == 4
    assert a != c or len(set(a)) == 1, "a different seed should generally differ"


def test_twenty_variants_are_archived_and_scored(arc):
    """The delivery gate, asserted rather than asserted-about."""
    for i in range(20):
        va.record("crate/y", f"variant-{i}", score=i / 20.0, verdict="REJECT" if i < 15 else "PASS",
                  root=arc)
    rows = va.weights("crate/y", root=arc)
    assert len(rows) == 20
    assert all(0.0 <= r["score"] <= 1.0 for r in rows)
    assert abs(sum(r["p"] for r in rows) - 1.0) < 1e-9, "probabilities must normalise"


def test_the_archive_has_a_ceiling_and_drops_the_weakest_first(arc):
    """An archive with no cap is a disk bug waiting to happen; the cap has to be
    decided before the first write, not after the complaint."""
    for i in range(30):
        va.record("t", f"v-{i}", score=i / 30.0, root=arc)
    result = va.prune("t", keep=10, root=arc)
    assert result == {"kept": 10, "dropped": 20}
    survivors = {r["variant_id"] for r in va.weights("t", root=arc)}
    assert va.variant_id("v-29") in survivors, "the best must survive"
    assert va.variant_id("v-00") not in survivors, "the weakest must be dropped"


def test_rescoring_a_variant_replaces_its_score_and_does_not_clone_it(arc):
    va.record("t", "v", score=0.2, root=arc)
    va.record("t", "v", score=0.9, root=arc)
    rows = va.weights("t", root=arc)
    assert len(rows) == 1, "one variant, one row"
    assert rows[0]["score"] == 0.9, "the latest score wins"


def test_a_corrupt_line_never_loses_the_archive(arc):
    va.record("t", "good", score=0.5, root=arc)
    path = va.archive_path("t", arc)
    with path.open("a", encoding="utf-8") as fh:
        fh.write("{not json at all\n")
    va.record("t", "also-good", score=0.6, root=arc)
    assert len(va.weights("t", root=arc)) == 2


def test_an_empty_archive_answers_with_nothing_not_an_error(arc):
    assert va.load("never-seen", root=arc) == []
    assert va.weights("never-seen", root=arc) == []
    assert va.sample_parents("never-seen", k=3, root=arc) == []



# ── the wiring: a phase close is where a variant gets its score ──────────────


def test_a_partial_gate_becomes_a_gradient_not_a_zero():
    """The reason the archive is worth having: a phase that met five clauses of
    six is a materially better stepping stone than one that met none. Collapsing
    both to 0.0 throws away exactly what makes a stepping stone one."""
    import loop_phase_close as pc
    gates = {"clauses": {
        "a": {"result": "PASS"}, "b": {"result": "PASS"}, "c": {"result": "PASS"},
        "d": {"result": "PASS"}, "e": {"result": "FAIL"}, "f": {"result": "N/A"}}}
    assert pc.variant_score(gates, "blocked") == pytest.approx(0.8)
    assert pc.variant_score({"clauses": {"a": {"result": "FAIL"}}}, "blocked") == 0.0
    assert pc.variant_score({"clauses": {"a": {"result": "PASS"}}}, "blocked") == 1.0


def test_without_a_gate_report_the_status_is_the_only_signal_left():
    import loop_phase_close as pc
    assert pc.variant_score(None, "done") == 1.0
    assert pc.variant_score({}, "blocked") == 0.0
    assert pc.variant_score({"clauses": {"a": {"result": "N/A"}}}, "done") == 1.0, \
        "all-N/A carries no information, so fall back to the status"


def test_a_freshly_written_log_satisfies_the_loop_s_own_doc_gate(tmp_path):
    """`loop_phase_close.py` writes the log; `loop_doc_link_gate.py` grades it.
    Until 2026-08-19 the first produced a file the second rejected, and every new
    bundle was repaired by hand — which is exactly why the defect survived: each
    repair erased the evidence of it."""
    import loop_phase_close as pc
    pc.append_log(tmp_path, "P1", "done", "summary", "2026-08-19T07:00:00-03:00")
    text = (tmp_path / "log.md").read_text(encoding="utf-8")
    assert text.startswith("---"), "a new log must open with OKF frontmatter"
    for field in ("okf_version:", "type: Log", "plan_id:", "timestamp:"):
        assert field in text, f"missing {field}"
    assert "/plan.md" in text, "every bundle doc links back to the plan"

    # Appending must never re-emit the header.
    pc.append_log(tmp_path, "P2", "done", "second", "2026-08-19T08:00:00-03:00")
    assert (tmp_path / "log.md").read_text(encoding="utf-8").count("okf_version:") == 1


def test_a_dead_end_child_does_not_decay_its_parents_novelty(arc):
    """Equation 2 is functioning_children_count: the novelty bonus decays by
    DISCOVERY, never by attempt. A parent whose child hit a dead end must look
    exactly as unexplored as one never branched from."""
    va.record("t", "open-parent", score=0.6, root=arc)
    pid = va.variant_id("open-parent")
    va.record("t", "dead-end", score=0.1, parent=pid, terminal=True, root=arc)
    rows = {r["variant_id"]: r for r in va.weights("t", root=arc)}
    assert rows[pid]["children"] == 0, "a terminal child must not count"
    assert rows[pid]["h"] == 1.0


def test_a_terminal_variant_is_never_sampled(arc):
    """A dead end stays in the archive as evidence, but re-drawing it as a
    parent burns budget rediscovering that it is one."""
    va.record("t", "alive", score=0.4, root=arc)
    va.record("t", "dead", score=0.4, terminal=True, root=arc)
    dead_id = va.variant_id("dead")
    drawn = set(va.sample_parents("t", k=50, seed=3, root=arc))
    assert dead_id not in drawn
    assert drawn == {va.variant_id("alive")}


def test_lineage_counts_the_dips_the_paper_promises(arc):
    """Fig. 2: the best agent's lineage includes two dips. Ours records parent
    and score — lineage() is the question nobody had asked of them."""
    va.record("t", "root", score=0.5, root=arc)
    va.record("t", "dip", score=0.3, parent=va.variant_id("root"), root=arc)
    va.record("t", "best", score=0.9, parent=va.variant_id("dip"), root=arc)
    lin = va.lineage("t", root=arc)
    assert lin["best"] == va.variant_id("best")
    assert [c["variant_id"] for c in lin["chain"]] == [
        va.variant_id("root"), va.variant_id("dip"), va.variant_id("best")]
    assert lin["dips"] == 1


def test_a_monotonic_lineage_names_the_greedy_risk(arc):
    """Zero dips is not a compliment — the verdict must say so, because a
    greedy-in-practice archive looks identical to a working one otherwise."""
    va.record("t", "a", score=0.3, root=arc)
    va.record("t", "b", score=0.6, parent=va.variant_id("a"), root=arc)
    va.record("t", "c", score=0.9, parent=va.variant_id("b"), root=arc)
    lin = va.lineage("t", root=arc)
    assert lin["dips"] == 0
    assert "greedy" in lin["verdict"]


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v", "--tb=short", "-p", "no:cacheprovider"]))
