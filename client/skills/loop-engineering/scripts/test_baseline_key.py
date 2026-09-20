"""The bundle-less baseline is keyed by the RESOLVED scope, and a first run records, never approves."""
from __future__ import annotations

import os
import unittest
from pathlib import Path

import loop_converged as lc


class TestGlobalBaselineKey(unittest.TestCase):
    def test_three_spellings_of_one_scope_share_one_baseline_dir(self) -> None:
        here = Path(os.getcwd())
        rel = Path("scripts/eleitoral")
        spellings = [rel, Path("scripts/eleitoral/"), here / "scripts" / "eleitoral", Path("./scripts/../scripts/eleitoral")]
        dirs = {lc._global_baseline_dir(s) for s in spellings}
        self.assertEqual(len(dirs), 1, dirs)

    def test_a_different_scope_gets_a_different_dir(self) -> None:
        self.assertNotEqual(lc._global_baseline_dir(Path("scripts/eleitoral")), lc._global_baseline_dir(Path("scripts/memoria")))

    def test_the_dir_lives_under_the_home_baselines_not_in_the_judged_tree(self) -> None:
        d = lc._global_baseline_dir(Path("scripts/eleitoral"))
        self.assertTrue(str(d).startswith(str(Path.home() / ".claude" / "loop-engineering" / "baselines")))


if __name__ == "__main__":
    unittest.main()
