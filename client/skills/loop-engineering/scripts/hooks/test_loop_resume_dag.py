#!/usr/bin/env python3
"""Guards for the resume's live-owner annotation (coordenação, 2026-09-23).

`decompose get` now carries `claim_live`/`claimed_by`, and the resume must name
them: a session opening a project with a live-claimed DAG must read "this is
held, alive, by X" — not mistake it for abandoned work or its own.
"""
from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "loop_resume", Path(__file__).with_name("loop_resume.py")
)
lr = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(lr)

GET_PAYLOAD = json.dumps({
    "task": {"task_id": "task_x", "status": "active"},
    "subtasks": [
        {"subtask_id": "task_x::S-00", "status": "in_progress",
         "claimed_by": "sessao-viva-1234", "claim_live": True},
        {"subtask_id": "task_x::S-01", "status": "pending",
         "claimed_by": None, "claim_live": False},
    ],
})


class LiveOwnerAnnotationTest(unittest.TestCase):
    def setUp(self):
        self._real = lr._run
        lr._run = lambda *a, **kw: GET_PAYLOAD

    def tearDown(self):
        lr._run = self._real

    def test_dag_state_names_live_owners(self):
        st = lr.dag_state("task_x")
        self.assertEqual(st["live_owners"], {"S-00": "sessao-viva-1234"})

    def test_build_context_says_the_dag_has_a_live_owner(self):
        marker = {"task": "task_x", "status": "active", "bundle": "b",
                  "scope": "/x", "cwd": "/x"}
        ctx = lr.build_context(marker)
        self.assertIn("dono(s) vivo(s)", ctx)
        self.assertIn("S-00", ctx)
        self.assertIn("sessao-viva", ctx)


if __name__ == "__main__":
    sys.exit(unittest.main())
