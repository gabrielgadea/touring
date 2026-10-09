"""cross_audit knows whether the bundle was ASKED for (Gabriel, canvas 1 option (a), 05/10/2026).

Until 06/10/2026 the clause answered `None, "skipped"` whenever `audit-plan-completion.sh` was missing, and the
judge fabricated a bundle dir when `--bundle` was absent — so the clause could not tell "no artifact audit was
asked for" from "it was asked for and the script is not there". The second is a FAIL that names the file; the
first stays N/A and says so.
"""
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import loop_converged as lc  # noqa: E402

STUBS = {
    "clause_judge_intact": (True, "stub", None),
    "clause_dag_done": (True, "stub", None),
    "clause_not_truncated": (True, "stub"),
    "clause_orphans": (True, "stub"),
}


class ClauseProvenance(unittest.TestCase):
    def test_requested_and_missing_is_a_fail_that_names_the_file(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            ok, evidence = lc.clause_cross_audit(Path(d), requested=True)
        self.assertIs(ok, False)
        self.assertIn("audit-plan-completion.sh", evidence)
        self.assertIn(d, evidence)

    def test_not_requested_is_na_and_says_why(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            ok, evidence = lc.clause_cross_audit(Path(d), requested=False)
        self.assertIsNone(ok)
        self.assertIn("--bundle", evidence)
        self.assertNotIn("skipped", evidence)

    def test_present_script_decides_by_exit_code_either_way(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "audit-plan-completion.sh").write_text("exit 0\n", encoding="utf-8")
            for requested in (True, False):
                with mock.patch.object(lc, "run", return_value=(0, "", "")):
                    self.assertIs(lc.clause_cross_audit(Path(d), requested=requested)[0], True)
                with mock.patch.object(lc, "run", return_value=(1, "", "")):
                    self.assertIs(lc.clause_cross_audit(Path(d), requested=requested)[0], False)


class NoSilentDefault(unittest.TestCase):
    def test_a_caller_that_forgets_the_provenance_fails_loud(self) -> None:
        """Review touring-b4: a default of False would fail open (silent N/A)."""
        with self.assertRaises(TypeError):
            lc.evaluate("t", Path("."), Path("."), False)
        with self.assertRaises(TypeError):
            next(lc._gather_clauses("t", Path("."), Path("."), False, False))


class MainThreadsProvenance(unittest.TestCase):
    def _main(self, argv: list[str], baseline: Path) -> tuple[int, dict]:
        patches = [mock.patch.object(lc, name, return_value=value) for name, value in STUBS.items()]
        patches.append(mock.patch.object(lc, "clause_quality", return_value=(True, [], "Gold", 0.9, False, [])))
        patches.append(mock.patch.object(lc, "_global_baseline_dir", return_value=baseline))
        out = StringIO()
        for p in patches:
            p.start()
        try:
            with redirect_stdout(out):
                rc = lc.main(argv)
        finally:
            for p in patches:
                p.stop()
        return rc, json.loads(out.getvalue())

    def test_bundle_without_script_fails_and_the_next_action_names_the_file(self) -> None:
        with tempfile.TemporaryDirectory() as scope, tempfile.TemporaryDirectory() as bundle:
            rc, rep = self._main(["--task", "t", "--scope", scope, "--bundle", bundle, "--json"], Path(bundle))
        self.assertEqual(rc, 1)
        self.assertEqual(rep["clauses"]["cross_audit"]["result"], "FAIL")
        self.assertEqual(rep["unmet"], ["cross_audit"])
        self.assertIn("audit-plan-completion.sh", rep["next_action"])

    def test_no_bundle_keeps_na_and_converges(self) -> None:
        with tempfile.TemporaryDirectory() as scope, tempfile.TemporaryDirectory() as base:
            rc, rep = self._main(["--task", "t", "--scope", scope, "--json"], Path(base))
        self.assertEqual(rc, 0)
        self.assertEqual(rep["clauses"]["cross_audit"]["result"], "N/A")
        self.assertIn("--bundle", rep["clauses"]["cross_audit"]["evidence"])


if __name__ == "__main__":
    unittest.main()
