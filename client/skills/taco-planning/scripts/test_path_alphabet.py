#!/usr/bin/env python3
"""Regression: the path detectors see the REAL path alphabet (-, ., digits, uppercase).

Origin (30/08/2026): every touring crate path carries a hyphen
(crates/touring-ceg/…), and the old [a-z_0-9/] classes either matched NOTHING
(_RE_FILE_LINE: a FACT-grade citation scored as INFERENCE) or captured a
MUTILATED path ("ceg/src/…"), which fails any existence check downstream.
Same family as the analise finding the peer session reported the same day:
_PARECE_CAMINHO rejected [ ] and recognized 0 of 505 real acervo paths.
The failure mode is silent by construction — the detector never errs, it
never sees. Hence: every assertion here is POSITIVE (the full path is the
match), never "no exception was raised".
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from confidence_tagger import _RE_FILE_LINE, classify_block  # noqa: E402
from gap_detector import _RE_FILE_CITATION  # noqa: E402
from lib import _PATH_RE  # noqa: E402

CITACAO = "veja `crates/touring-ceg/src/gateway/sandbox_executor.rs:530` para o root"


class PathAlphabet(unittest.TestCase):
    def test_file_line_sees_hyphenated_crate_paths(self):
        self.assertTrue(_RE_FILE_LINE.search(CITACAO),
                        "file:line citation with a hyphenated crate dir must match")

    def test_fact_grade_is_not_downgraded_by_the_alphabet(self):
        body = CITACAO + "\nverificado com touring ast blast"
        level, score = classify_block(body)
        self.assertEqual(level, "FACT", "file:line + touring cmd is the FACT contract")
        self.assertEqual(score, 1.0)

    def test_citation_captures_the_whole_path_never_a_mutilation(self):
        m = _RE_FILE_CITATION.search(CITACAO)
        self.assertIsNotNone(m)
        self.assertEqual(m.group(1), "crates/touring-ceg/src/gateway/sandbox_executor.rs",
                         "a partial capture (ceg/src/…) fails every existence check downstream")

    def test_path_re_sees_uppercase_hyphen_and_digits(self):
        intent = "config em Cargo.toml, manual em docs/code-mode.md, bundle docs/plans/2026-08-30-mundos-da-criacao/index.md"
        achados = {m.group(0) for m in _PATH_RE.finditer(intent)}
        self.assertIn("Cargo.toml", achados)
        self.assertIn("docs/code-mode.md", achados,
                      "the hyphen used to cut this down to 'mode.md'")
        self.assertIn("docs/plans/2026-08-30-mundos-da-criacao/index.md", achados)

    def test_path_re_sees_leading_dotdir_and_never_a_mid_path_prefix(self):
        # 26 real tracked paths live under dot-dirs (.cargo/, .github/, .holon/)
        # — found by the universe sweep, invisible to \b before ".".
        m = _PATH_RE.search("veja .cargo/config.toml aqui")
        self.assertIsNotNone(m)
        self.assertEqual(m.group(0), ".cargo/config.toml")
        # The lookbehind must forbid a match starting mid-path (the mutilation).
        m = _PATH_RE.search("veja crates/touring-ceg/src/lib.rs aqui")
        self.assertEqual(m.group(0), "crates/touring-ceg/src/lib.rs")

    def test_prose_still_does_not_match(self):
        # The widening must not turn ordinary prose into paths.
        for texto in ("isto é só prosa comum", "a versão 1.2 saiu ontem"):
            self.assertIsNone(_PATH_RE.search(texto), texto)
            self.assertIsNone(_RE_FILE_CITATION.search(texto), texto)


if __name__ == "__main__":
    unittest.main()
