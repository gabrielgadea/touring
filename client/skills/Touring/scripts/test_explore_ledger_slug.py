#!/usr/bin/env python3
"""Guards for the CCE ledger's filename derivation.

Regression (observed live 2026-08-25): `"ê".isalnum()` is True, so diacritics
went straight into the ledger filename. `…inteligência…` and `…inteligencia…`
— one question, typed twice — produced two ledgers in the same scope. One had
converged over 5 rounds with the external lens marked; the other had not, and
the Stop gate's glob happily accepted whichever was fresher.
"""
from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
_spec = importlib.util.spec_from_file_location(
    "explore_until_dry", HERE / "explore_until_dry.py")
eud = importlib.util.module_from_spec(_spec)
sys.modules["explore_until_dry"] = eud
_spec.loader.exec_module(eud)

ACCENTED = "autoresearch: camadas de inteligência e RL em loops"
PLAIN = "autoresearch: camadas de inteligencia e RL em loops"


class LedgerSlugTest(unittest.TestCase):
    def test_accent_variants_share_one_ledger(self):
        """The measured bug: one question, two ledgers, two verdicts."""
        self.assertEqual(eud._slugify(ACCENTED), eud._slugify(PLAIN))

    def test_folding_preserves_the_rest_of_the_slug(self):
        """Folding must not also eat words — only the marks."""
        self.assertEqual(eud._slugify("ação e Ç"), eud._slugify("acao e C"))
        self.assertIn("acao", eud._slugify("ação e Ç"))

    def test_a_legacy_accented_ledger_is_adopted_not_orphaned(self):
        """Migration: the folded path must not strand an existing history."""
        with TemporaryDirectory() as tmp:
            scope = Path(tmp)
            (scope / eud.LEDGER_DIRNAME).mkdir()
            legacy = (scope / eud.LEDGER_DIRNAME /
                      f"{eud._slugify(ACCENTED, fold=False)}.ledger.json")
            legacy.write_text(json.dumps({"topic": ACCENTED, "rounds": [1, 2, 3]}))

            resolved = eud.ledger_path_for(ACCENTED, scope, None)
            self.assertEqual(resolved, legacy,
                             "an existing accented ledger must be reused, not replaced")

    def test_without_a_legacy_file_the_folded_path_wins(self):
        with TemporaryDirectory() as tmp:
            scope = Path(tmp)
            (scope / eud.LEDGER_DIRNAME).mkdir()
            resolved = eud.ledger_path_for(ACCENTED, scope, None)
            self.assertEqual(resolved.name, f"{eud._slugify(ACCENTED)}.ledger.json")

    def test_explicit_path_still_overrides_everything(self):
        with TemporaryDirectory() as tmp:
            target = Path(tmp) / "chosen.ledger.json"
            self.assertEqual(
                eud.ledger_path_for(ACCENTED, Path(tmp), str(target)), target.resolve())


if __name__ == "__main__":
    unittest.main(verbosity=2)
