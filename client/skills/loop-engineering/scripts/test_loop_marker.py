#!/usr/bin/env python3
"""Guards for `write_marker` promote-only semantics (24/09/2026, touring-36).

The defect these tests encode: the armer's refresh rebuilt the marker from
scratch and silently dropped a real task registered at step 11 — the Stop
guard then accused "INNER never registered" minutes after it had been
(measured live: the [OUTER · efeito] executor rewrote the marker ~190 s after
the registration, restoring `task: "OUTER"` over the real id).

The contract: a refresh may PROMOTE the OUTER placeholder to a real task;
it must NEVER demote one. Mutation-sensitive: rebuild `data` from scratch
(as before) and the first test fails — the demotion returns.
"""
from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "loop_marker",
    Path(__file__).resolve().parent / "hooks" / "loop_marker.py",
)
lm = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(lm)

PLACEHOLDER = "OUTER"
REAL = "task_1790275375586783377"


class PromoteOnlyTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self._tmp.name)
        self._old = lm.MARKER_DIR
        lm.MARKER_DIR = self.dir
        self.cwd = str(self.dir / "proj")
        Path(self.cwd).mkdir()

    def tearDown(self):
        lm.MARKER_DIR = self._old
        self._tmp.cleanup()

    @staticmethod
    def _read(p):
        return json.loads(Path(p).read_text(encoding="utf-8"))

    def test_armer_refresh_never_demotes_a_real_registration(self):
        """The 24/09 incident: registered real task → armer refresh → the
        read-back must keep the real task, its status and its topic."""
        lm.write_marker(REAL, scope=self.cwd, cwd=self.cwd, status="active",
                        session_id="s1", flow="work-outer", topic="fase-renew")
        # The armer's refresh — the [OUTER · efeito] gate call shape.
        p = lm.write_marker(PLACEHOLDER, scope=self.cwd, cwd=self.cwd,
                            status="outer", session_id="s1",
                            topic="decompose.rs", flow="work-outer")
        after = self._read(p)
        self.assertEqual(after["task"], REAL,
                         "a refresh must never demote a real task to the placeholder")
        self.assertEqual(after["status"], "active",
                         "nor demote the loop's status back to armed")
        self.assertEqual(after["topic"], "fase-renew",
                         "nor overwrite the real topic with the gate's file topic")

    def test_placeholder_marker_refreshes_normally(self):
        """CONTROL: with no real registration in place, the armer updates
        everything as before."""
        lm.write_marker(PLACEHOLDER, scope=self.cwd, cwd=self.cwd, status="outer",
                        session_id="s1", topic="a.rs")
        p = lm.write_marker(PLACEHOLDER, scope=self.cwd, cwd=self.cwd,
                            status="outer", session_id="s1", topic="b.rs")
        after = self._read(p)
        self.assertEqual(after["task"], PLACEHOLDER)
        self.assertEqual(after["topic"], "b.rs", "placeholder markers refresh normally")

    def test_registration_promotes_placeholder_to_real(self):
        """The PROMOTE direction: step 11 lands after an armed placeholder."""
        lm.write_marker(PLACEHOLDER, scope=self.cwd, cwd=self.cwd, status="outer",
                        session_id="s1", flow="work-outer")
        p = lm.write_marker(REAL, scope=self.cwd, cwd=self.cwd, status="active",
                            session_id="s1", flow="work-outer")
        after = self._read(p)
        self.assertEqual(after["task"], REAL)
        self.assertEqual(after["status"], "active")

    def test_a_real_task_from_another_write_is_never_silent_dropped(self):
        """Two real registrations in a row: the second (a re-registration or a
        correction) wins — promote-only is about placeholders, not about
        freezing the task."""
        lm.write_marker(REAL, scope=self.cwd, cwd=self.cwd, status="active",
                        session_id="s1")
        other = "task_1790275399999999999"
        p = lm.write_marker(other, scope=self.cwd, cwd=self.cwd, status="active",
                            session_id="s1")
        after = self._read(p)
        self.assertEqual(after["task"], other)


if __name__ == "__main__":
    unittest.main()
