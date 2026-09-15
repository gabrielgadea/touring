"""orphans_base must filter to the scope — a relative path is not "in scope" just because it is relative.

Origin (2026-09-14, measured on /home/gabrielgadea/projects/analise with --scope scripts/eleitoral): the
clause reported 6488 NEW orphans "in scope"; all 6488 lived in 818 files OUTSIDE the scope
(``.claude/config/checkpoint_config.py``, ``.claude/hooks/...``). ``module_file`` arrives relative to the
INDEX root, the clause joined it to the SCOPE, and ``Path.resolve()`` does not require existence — so
``relative_to(scope)`` accepted every relative path. The filter filtered nothing; the loop was held on
workspace churn it could never fix. Same family as the 2026-08-07 vacuity (a filter that matched
nothing), from the opposite side (a filter that matches everything).

Each test names the mutation that kills it.
"""
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import loop_converged as lc


def _orphans_json(*pairs):
    return json.dumps({"orphans": [{"module_file": f, "symbol_name": s} for f, s in pairs]})


class OrphanScopeTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        (self.root / ".touring").mkdir()
        self.scope = self.root / "scripts" / "eleitoral"
        (self.scope / "src").mkdir(parents=True)
        (self.scope / "a.py").write_text("A = 1\n")
        (self.scope / "src" / "b.rs").write_text("pub fn b() {}\n")
        (self.root / ".claude" / "config").mkdir(parents=True)
        (self.root / ".claude" / "config" / "checkpoint_config.py").write_text("X = 1\n")
        self.bundle = self.root / "bundle"
        self.bundle.mkdir()

    def tearDown(self):
        self._tmp.cleanup()

    def _run_clause(self, payload):
        with mock.patch.object(lc, "run", return_value=(0, payload, "")):
            return lc.clause_orphans(self.scope, self.bundle)

    def _baseline(self):
        return set((self.bundle / ".baseline" / "orphans-scoped.txt").read_text().splitlines())

    def test_root_relative_file_outside_scope_is_excluded(self):
        # kills: joining a root-relative path to the SCOPE (the 2026-09-14 defect)
        ok, _ = self._run_clause(
            _orphans_json((".claude/config/checkpoint_config.py", "CHECKPOINT_DIR"), ("scripts/eleitoral/a.py", "A"))
        )
        self.assertTrue(ok)
        self.assertEqual(self._baseline(), {"scripts/eleitoral/a.py::A"})

    def test_scope_relative_file_inside_scope_is_included(self):
        # kills: resolving only against the index root (the kazuba-geo-engine crate records "src/…")
        self._run_clause(_orphans_json(("src/b.rs", "b")))
        self.assertEqual(self._baseline(), {"src/b.rs::b"})

    def test_new_in_scope_orphan_still_fails_and_is_named(self):
        # kills: a fix that excludes everything (the 2026-08-07 vacuity, again)
        self._run_clause(_orphans_json(("scripts/eleitoral/a.py", "A")))
        ok, detail = self._run_clause(
            _orphans_json(("scripts/eleitoral/a.py", "A"), ("scripts/eleitoral/a.py", "NOVO"), (".claude/config/checkpoint_config.py", "X"))
        )
        self.assertFalse(ok)
        self.assertIn("a.py::NOVO", detail)
        self.assertNotIn("checkpoint_config", detail)

    def test_corpus_that_never_reaches_the_scope_is_unmeasured_not_zero(self):
        # kills: reporting PASS "scoped orphans=0" when the wiring corpus has no record under the scope's
        # top-level tree (measured 2026-09-14: 20830 orphans, 0 under scripts/ — the corpus never saw it)
        ok, detail = self._run_clause(_orphans_json((".claude/config/checkpoint_config.py", "CHECKPOINT_DIR")))
        self.assertIsNone(ok)
        self.assertIn("unmeasured", detail)
        self.assertFalse((self.bundle / ".baseline" / "orphans-scoped.txt").exists(), "no baseline from a blind run")

    def test_covered_tree_with_no_orphan_in_scope_is_a_real_zero(self):
        # positive control of the coverage check: records exist under scripts/, just not under the scope
        (self.root / "scripts" / "outro").mkdir(parents=True)
        (self.root / "scripts" / "outro" / "c.py").write_text("C = 1\n")
        ok, detail = self._run_clause(_orphans_json(("scripts/outro/c.py", "C")))
        self.assertTrue(ok)
        self.assertIn("scoped orphans=0", detail)

    def test_baseline_without_any_key_in_scope_is_rerecorded_not_compared(self):
        # kills: comparing against a baseline recorded while the corpus (or the old filter) never reached the scope.
        # Measured 2026-09-14 on analise/scripts/eleitoral: baseline 24860 names, none under the scope, so all 1329
        # in-scope orphans read NEW — 1200 of them older than the session. A baseline that names nothing in the scope
        # is not a baseline for it.
        base = self.bundle / ".baseline"
        base.mkdir()
        (base / "orphans-scoped.txt").write_text(".claude/config/checkpoint_config.py::CHECKPOINT_DIR\n")
        ok, detail = self._run_clause(_orphans_json(("scripts/eleitoral/a.py", "A")))
        self.assertTrue(ok)
        self.assertIn("re-recorded", detail)
        self.assertEqual(self._baseline(), {"scripts/eleitoral/a.py::A"})

    def test_baseline_with_a_key_in_scope_still_catches_the_new_orphan(self):
        # positive control: the rerecord path must not swallow a real baseline
        base = self.bundle / ".baseline"
        base.mkdir()
        (base / "orphans-scoped.txt").write_text("scripts/eleitoral/a.py::A\n")
        ok, detail = self._run_clause(_orphans_json(("scripts/eleitoral/a.py", "A"), ("scripts/eleitoral/a.py", "NOVO")))
        self.assertFalse(ok)
        self.assertIn("a.py::NOVO", detail)

    def test_deleted_file_resolves_against_the_index_root(self):
        # a file gone from disk cannot be located by existence; the .touring root decides
        ok, _ = self._run_clause(_orphans_json((".claude/hooks/apagado.py", "Y"), ("scripts/eleitoral/apagado.py", "Z")))
        self.assertTrue(ok)
        self.assertEqual(self._baseline(), {"scripts/eleitoral/apagado.py::Z"})


if __name__ == "__main__":
    unittest.main()
