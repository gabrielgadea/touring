#!/usr/bin/env python3
"""Guards for `credit_recalls` — the affordance that closes Memento's Eq. 9 loop.

The regression these exist to prevent is the one measured on 2026-08-25: the
crediting mechanism was fully built (`cli_memory_credit`, the case ledger, the
CLI subcommand) and had **zero callers**, because closing the loop depended on
a caller remembering the queries it had recalled. `credited_count` was 0 for
the daemon's entire life and `outcome_reward` sat on 1.7% of the corpus.
"""
from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "loop_phase_close", Path(__file__).with_name("loop_phase_close.py")
)
pc = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pc)


class CreditRecallsTest(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self._real_run = pc.run

    def tearDown(self):
        pc.run = self._real_run

    def _stub(self, rc=0, out='{"credited": 20, "claimed_queries": ["a", "b"]}'):
        def fake_run(cmd, *a, **kw):
            self.calls.append(cmd)
            return rc, out, ""
        pc.run = fake_run

    def test_no_queries_falls_back_to_all_pending(self):
        """Without this the phase closes crediting nothing — the measured bug."""
        self._stub()
        n = pc.credit_recalls("task_1", "P0", "done", None)
        self.assertEqual(len(self.calls), 1)
        self.assertIn("--all-pending", self.calls[0])
        self.assertIn("1.0", self.calls[0], "a passing phase credits +1.0")
        self.assertEqual(n, 2, "count comes from claimed_queries, not from a guess")

    def test_failed_phase_credits_zero_not_one(self):
        """A failed phase must lower the value of the cases it rested on."""
        self._stub()
        pc.credit_recalls("task_1", "P0", "failed", None)
        self.assertIn("0.0", self.calls[0])

    def test_explicit_queries_still_credit_by_name_and_never_drain(self):
        """Naming the query stays exact; draining must not silently replace it."""
        self._stub(out='{"credited": 3}')
        n = pc.credit_recalls("task_1", "P0", "done", ["query one", "query two"])
        self.assertEqual(len(self.calls), 2)
        for cmd in self.calls:
            self.assertNotIn("--all-pending", cmd)
        self.assertEqual(n, 2)

    def test_old_binary_rejecting_the_flag_is_not_counted_as_credit(self):
        """Fail-open: crediting is best-effort and never gates a phase."""
        self._stub(rc=2, out="error: unexpected argument '--all-pending'")
        self.assertEqual(pc.credit_recalls("task_1", "P0", "done", None), 0)

    def test_unparseable_response_counts_nothing(self):
        """A response we cannot read is not evidence that anything was credited."""
        self._stub(out='{"credited": 5, truncated…')
        self.assertEqual(pc.credit_recalls("task_1", "P0", "done", None), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
