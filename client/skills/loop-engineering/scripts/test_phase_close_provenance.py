"""P4 of the memory-graph contract (2026-08-30): the phase close creates the
provenance edges itself, and knowledge/<phase>.json is a projection of that
same material — never again 1 node / 0 relations while the real graph has
both (knowledge/P1.json measured 2026-08-29: 1 entity, 0 relations, because
nobody passes --abstract).

Each test names the mutation that kills it.
"""
from unittest import mock

import loop_phase_close as lpc


def test_abstract_projects_the_wired_material_even_with_no_extra():
    # Mutation killed: dropping the task-seeded block from build_abstract.
    abstract = lpc.build_abstract("P1", "resumo", None, task="task_x", status="done")
    ids = {e["entity_id"] for e in abstract["entities"]}
    assert "memory:loop:task_x:P1:done" in ids
    assert "loop:loop:task_x" in ids
    assert "dag:decomp:task_x" in ids
    rels = {r["relation_id"] for r in abstract["relations"]}
    assert "phase:P1|produces|memory:loop:task_x:P1:done" in rels
    assert "memory:loop:task_x:P1:done|generated-by|loop:loop:task_x" in rels
    assert "memory:loop:task_x:P1:done|generated-by|dag:decomp:task_x" in rels
    assert len(abstract["entities"]) >= 4
    assert len(abstract["relations"]) >= 3


def test_abstract_without_task_keeps_the_legacy_shape():
    # Mutation killed: making the seed unconditional (would break callers
    # that build ad-hoc abstracts with no DAG behind them).
    abstract = lpc.build_abstract("P1", "resumo", None)
    assert len(abstract["entities"]) == 1
    assert abstract["relations"] == []


def test_link_provenance_wires_both_anchors_with_generated_by():
    # Mutation killed: dropping an anchor, or the --rel generated-by flag.
    calls = []

    def fake_run(cmd, timeout=120):
        calls.append(cmd)
        return 0, '{"id":"x"}', ""

    with mock.patch.object(lpc, "run", side_effect=fake_run):
        linked = lpc.link_provenance("task_x", "P1", "done")
    assert linked == 2
    assert len(calls) == 2
    for cmd in calls:
        assert cmd[:3] == ["touring", "memory", "link"]
        assert cmd[3] == "loop:task_x:P1:done"
        assert "--rel" in cmd and "generated-by" in cmd
    assert {c[4] for c in calls} == {"loop:task_x", "decomp:task_x"}


def test_link_provenance_is_best_effort_on_failure():
    # Mutation killed: letting an edge failure raise and gate the close.
    with mock.patch.object(lpc, "run", return_value=(1, "", "boom")):
        assert lpc.link_provenance("task_x", "P1", "done") == 0
