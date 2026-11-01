#!/usr/bin/env python3
"""Guards for `apply_derived_links` — the caller `memory suggest-links` never had.

The regression these exist to prevent is the one measured on 2026-09-23 (M2):
the suggestion engine shipped fully built and had **zero callers** —
`edge_density` 0.023 and `graph_contract_share` 0.0, the same class as
`credit_recalls`' zero-caller bug. Two invariants beyond "it runs": the link
call is built from STRUCTURED fields (never by executing the `apply` shell
string — the template-injection lesson), and the task's own lessons chain by
construction (a brand-new key has no co-service, so the engine alone could
never connect it).
"""
from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "loop_phase_close", Path(__file__).with_name("loop_phase_close.py")
)
pc = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pc)

SUGGEST = (
    '{"count": 2, "suggestions": ['
    '{"a": "k1", "b": "k2", "rel": "relates-to", "co_served": 9, "derived": true,'
    ' "apply": "touring memory link k1 k2 --rel relates-to"},'
    '{"a": "k3", "b": "k4", "rel": "relates-to", "co_served": 7, "derived": true,'
    ' "apply": "touring memory link k3 k4 --rel relates-to"},'
    '{"a": "k5", "b": "k6", "rel": "relates-to", "co_served": 5, "derived": true,'
    ' "apply": "touring memory link k5 k6 --rel relates-to"}'
    "]}"
)


class ApplyDerivedLinksTest(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self._real_run = pc.run

    def tearDown(self):
        pc.run = self._real_run

    def _stub(self, rc=0, out=SUGGEST):
        def fake_run(cmd, *a, **kw):
            self.calls.append(cmd)
            if cmd[:3] == ["touring", "memory", "suggest-links"]:
                return rc, out, ""
            return 0, '{"status": "linked"}', ""
        pc.run = fake_run

    def test_links_built_from_structured_fields_never_the_apply_string(self):
        self._stub()
        applied = pc.apply_derived_links("task_1", "P2", "done", bundle=None, limit=2)
        link_calls = [c for c in self.calls if c[1:3] == ["memory", "link"]]
        self.assertEqual(link_calls[0][:4], ["touring", "memory", "link", "k1"])
        for c in link_calls:
            self.assertNotIn(" ;", " ".join(c), "no shell composition leaks in")
            self.assertNotIn("--rel relates-to\"", " ".join(c))
        self.assertEqual(applied[:2], ["k1|relates-to|k2", "k3|relates-to|k4"])

    def test_engine_suggestions_are_bounded(self):
        self._stub()
        pc.apply_derived_links("task_1", "P2", "done", bundle=None, limit=1)
        link_calls = [c for c in self.calls if c[1:3] == ["memory", "link"]]
        self.assertEqual(len(link_calls), 1)

    def test_chain_edge_connects_the_new_lesson_to_the_previous_phase(self):
        self._stub()
        with tempfile.TemporaryDirectory() as bundle:
            Path(bundle, "phases").mkdir()
            Path(bundle, "phases", "P1.md").write_text("x")
            applied = pc.apply_derived_links("task_1", "P2", "done", bundle=bundle, limit=0)
        self.assertIn("loop:task_1:P2:done|extends|loop:task_1:P1:done", applied)

    def test_everything_is_fail_open(self):
        self._stub(rc=127, out="not-json")
        applied = pc.apply_derived_links("task_1", "P2", "done", bundle=None, limit=3)
        self.assertEqual(applied, [], "garbage in → nothing applied, no exception")


if __name__ == "__main__":
    sys.exit(unittest.main())
