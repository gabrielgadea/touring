"""The quality_gold remedy must name the field that actually fails the clause.

Origin (2026-08-29, measured by peer session on a live 8880-file scope):
``touring-quality`` returned ``composite=0.932`` with 0/50 dims below 0.80 and
STILL ``tier=Silver``, because ``blockers=["F1_3"]`` caps the tier. The old
fixed message — "raise touring-quality to >= Gold (0.80)" — told the operator
to raise a number that already passed. Same defect class as a deny that does
not teach the route back (A5/D8): the verdict lives in one field, the text
points at another.

Each test names the mutation that kills it.
"""
import itertools
import json
import unittest
from pathlib import Path
from unittest import mock

import loop_converged as lc


def _quality_json(tier, composite, blockers):
    return json.dumps({
        "tier": tier,
        "composite": composite,
        "blockers": blockers,
        "dimensions": {},
    })


def _first_three_clauses(scope):
    """Drive the lazy generator only up to quality_gold (3rd yield) so the
    later clauses (orphans/cargo/cross-audit — real IO) never execute."""
    with mock.patch.object(lc, "clause_judge_intact",
                           return_value=(True, "stub", None)), \
         mock.patch.object(lc, "clause_dag_done",
                           return_value=(True, "stub", None)):
        gen = lc._gather_clauses("task", Path(scope), Path(scope),
                                 rust_full=False, rust=False)
        return list(itertools.islice(gen, 3))


class QualityBlockersMessage(unittest.TestCase):
    def test_blockers_are_named_and_composite_is_not_the_remedy(self):
        # Mutation killed: reverting qual_fix to the fixed string.
        out = _quality_json("Silver", 0.932, ["F1_3"])
        with mock.patch.object(lc, "run", return_value=(0, out, "")):
            name, ok, evidence, action = _first_three_clauses("/tmp")[2]
        self.assertEqual(name, "quality_gold")
        self.assertFalse(ok)
        self.assertIn("F1_3", action)
        self.assertNotIn("raise touring-quality", action)
        self.assertIn("blockers=F1_3", evidence)

    def test_without_blockers_the_composite_message_survives(self):
        # Mutation killed: making the blockers branch unconditional.
        out = _quality_json("Silver", 0.72, [])
        with mock.patch.object(lc, "run", return_value=(0, out, "")):
            name, ok, evidence, action = _first_three_clauses("/tmp")[2]
        self.assertEqual(name, "quality_gold")
        self.assertFalse(ok)
        self.assertEqual(action, "raise touring-quality to >= Gold (0.80)")
        self.assertNotIn("blockers=", evidence)

    def test_gold_tier_still_passes_with_empty_blockers(self):
        # Mutation killed: gold_ok derived from blockers instead of tier.
        out = _quality_json("Gold", 0.85, [])
        with mock.patch.object(lc, "run", return_value=(0, out, "")):
            _, ok, _, _ = _first_three_clauses("/tmp")[2]
        self.assertTrue(ok)

    def test_fail_closed_path_returns_six_tuple(self):
        # Mutation killed: the unverifiable branch left at the old 5-tuple
        # (would raise ValueError at the unpack site in _gather_clauses).
        with mock.patch.object(lc, "run", return_value=(1, "", "boom")):
            result = lc.clause_quality(Path("/tmp"))
        self.assertEqual(result, (False, [], None, None, None, []))

    def test_non_string_blockers_are_coerced_and_falsy_dropped(self):
        # Mutation killed: dropping the [str(b) ... if b] normalization.
        out = json.dumps({"tier": "Silver", "composite": 0.9,
                          "blockers": ["F1_3", None, 2], "dimensions": {}})
        with mock.patch.object(lc, "run", return_value=(0, out, "")):
            *_, blockers = lc.clause_quality(Path("/tmp"))
        self.assertEqual(blockers, ["F1_3", "2"])


if __name__ == "__main__":
    unittest.main()
