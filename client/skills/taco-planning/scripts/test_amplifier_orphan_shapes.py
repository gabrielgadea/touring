"""Regression: dimension_amplifier must accept every `wiring_orphans` shape.

The collector emits three shapes — list of dicts (Rust workspaces), list of
strings (polyglot projects such as analise) and an elided dict
(`{"_elided_array_len": N}`). The amplifier crashed on the last two with
`'str' object has no attribute 'get'` (25/09/2026) while dimension_scorer had
been fixed for them on 20/08. Absence is not zero: an elided list must surface as
`orphans_unmeasured`, never as "every orphan covered".
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import dimension_amplifier as amp  # noqa: E402

REPORT = {"name": "functionality", "current": 2.0}
PLAN = "the plan wires covered_fn into its consumer"


def _functionality_actions(orphans: object) -> list[dict]:
    """Run the amplifier for the functionality dimension with the given orphans."""
    return amp.amplify_dimension(REPORT, PLAN, {"wiring_orphans": orphans}, 7.0)


class OrphanShapes(unittest.TestCase):
    """One case per shape the collector is known to emit."""

    def test_list_of_dicts(self) -> None:
        acts = _functionality_actions([{"name": "covered_fn"}, {"name": "lonely_fn"}])
        self.assertEqual(acts[0]["uncovered_orphans"], ["lonely_fn"])

    def test_list_of_strings(self) -> None:
        acts = _functionality_actions(["covered_fn", "lonely_fn"])
        self.assertEqual(acts[0]["uncovered_orphans"], ["lonely_fn"])

    def test_elided_dict_is_unmeasured_not_covered(self) -> None:
        acts = _functionality_actions({"_elided_array_len": 7290})
        self.assertEqual(acts[0].get("orphans_unmeasured"), 7290)
        self.assertNotIn("uncovered_orphans", acts[0])

    def test_wrapped_dict(self) -> None:
        acts = _functionality_actions({"orphans": ["lonely_fn"]})
        self.assertEqual(acts[0]["uncovered_orphans"], ["lonely_fn"])


if __name__ == "__main__":
    unittest.main()
