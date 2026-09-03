#!/usr/bin/env python3
"""Guards for `resolve_subtask_id` — the short phase → full DAG subtask id.

Regression measured on 2026-09-02: seven phases (B1..C4) closed with
`dag_updated: false` because their subtask ids were `B1-ceg-tmp-...` and the
resolver only accepted an exact match or `phase + " "`. Every DAG registered
with hyphenated slugs (the shape `touring decompose add` receives from the
loop) was silently left pending while report/memory/log all said "done".
"""
from __future__ import annotations

import importlib.util
import json
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "loop_phase_close", Path(__file__).with_name("loop_phase_close.py")
)
pc = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pc)

TASK = "task_1"
SUBTASKS = [
    "task_1::W-wiring-rebuild",
    "task_1::B1-ceg-tmp-privado-por-run",
    "task_1::B10-something-else",
    "task_1::P2 plan step",
    "task_1::C4",
]


class ResolveSubtaskIdTest(unittest.TestCase):
    def setUp(self):
        self._real_run = pc.run
        payload = json.dumps({"subtasks": [{"subtask_id": s} for s in SUBTASKS]})
        pc.run = lambda cmd, *a, **kw: (0, payload, "")

    def tearDown(self):
        pc.run = self._real_run

    def test_exact_suffix_resolves(self):
        self.assertEqual(pc.resolve_subtask_id(TASK, "C4"), "task_1::C4")

    def test_space_separated_slug_resolves(self):
        self.assertEqual(pc.resolve_subtask_id(TASK, "P2"), "task_1::P2 plan step")

    def test_hyphen_separated_slug_resolves(self):
        """The 2026-09-02 miss: `B1` must find `B1-ceg-...`, not `B10-...`."""
        self.assertEqual(
            pc.resolve_subtask_id(TASK, "B1"), "task_1::B1-ceg-tmp-privado-por-run"
        )
        self.assertEqual(pc.resolve_subtask_id(TASK, "W"), "task_1::W-wiring-rebuild")

    def test_prefix_of_a_longer_phase_does_not_match(self):
        """`B1` is not a prefix match for `B10-...` — the separator is required."""
        self.assertEqual(pc.resolve_subtask_id(TASK, "B10"), "task_1::B10-something-else")
        self.assertNotEqual(pc.resolve_subtask_id(TASK, "B1"), "task_1::B10-something-else")

    def test_unknown_phase_is_returned_verbatim(self):
        self.assertEqual(pc.resolve_subtask_id(TASK, "Z9"), "Z9")

    def test_daemon_failure_returns_phase(self):
        pc.run = lambda cmd, *a, **kw: (1, "", "down")
        self.assertEqual(pc.resolve_subtask_id(TASK, "B1"), "B1")


if __name__ == "__main__":
    unittest.main()
