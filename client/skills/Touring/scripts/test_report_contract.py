#!/usr/bin/env python3
"""Unit tests for report_contract — the premium-elite reporting-contract module.

Design trace:
  REQUIREMENT the footer names every mandatory report section -> BOUNDARY the 7
    skeleton labels must ALL appear -> TEST assert each label present.
  REQUIREMENT the footer is specific, not a generic banner -> BOUNDARY each
    `presents` row + the diagnostic name + the artifact path must survive into the
    text -> TEST assert each present.
  REQUIREMENT digests stay diffable -> BOUNDARY no clock/randomness -> TEST same
    inputs render byte-identical.

Stdlib only (unittest). No filesystem, no subprocess, no daemon.
"""
import importlib.util
import io
import unittest
from contextlib import redirect_stdout
from pathlib import Path

MODPATH = str(Path(__file__).resolve().parent / "report_contract.py")
_spec = importlib.util.spec_from_file_location("report_contract", MODPATH)
rc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rc)


class TestSkeleton(unittest.TestCase):
    def test_skeleton_has_exactly_seven_sections(self):
        self.assertEqual(len(rc.SKELETON), 7)

    def test_skeleton_labels_are_the_elite_report_sections(self):
        labels = [lbl for lbl, _ in rc.SKELETON]
        self.assertEqual(
            labels,
            ["VERDICT", "SCORECARD", "FINDINGS", "FUSED RISK",
             "ROOT-CAUSE", "PROVENANCE", "ACTIONS"],
        )

    def test_every_skeleton_section_has_a_nonempty_intent(self):
        for label, intent in rc.SKELETON:
            self.assertTrue(intent.strip(), f"{label} has empty intent")


class TestContractFooter(unittest.TestCase):
    def setUp(self):
        self.artifact = "/tmp/out/systemic_diag_v2_matrix.json"
        self.diagnostic = "systemic_diag_v2 — 50 dims × workspace × architecture"
        self.presents = ["full per-dim table", "fused risk ranking", "what the aggregate hid"]
        self.txt = rc.contract_footer(self.artifact, self.diagnostic, self.presents)

    def test_footer_names_all_seven_skeleton_sections(self):
        for label, _ in rc.SKELETON:
            self.assertIn(label, self.txt)

    def test_footer_carries_the_artifact_path(self):
        self.assertIn(self.artifact, self.txt)

    def test_footer_carries_the_diagnostic_subject(self):
        self.assertIn(self.diagnostic, self.txt)

    def test_footer_carries_every_presents_row(self):
        for p in self.presents:
            self.assertIn(p, self.txt)

    def test_footer_states_the_mandate_and_the_violation(self):
        self.assertIn("MANDATORY", self.txt)
        self.assertIn("violation", self.txt)
        self.assertIn(f"v{rc.CONTRACT_VERSION}", self.txt)

    def test_footer_demands_provenance_from_source(self):
        # provenance is non-negotiable — the footer must say so in words
        self.assertIn("source", self.txt.lower())

    def test_footer_is_deterministic(self):
        again = rc.contract_footer(self.artifact, self.diagnostic, self.presents)
        self.assertEqual(self.txt, again)

    def test_empty_presents_still_renders_the_skeleton(self):
        txt = rc.contract_footer(self.artifact, self.diagnostic, [])
        for label, _ in rc.SKELETON:
            self.assertIn(label, txt)


class TestPrintContract(unittest.TestCase):
    def test_print_contract_writes_the_footer_to_stdout(self):
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc.print_contract("/tmp/a.json", "diag", ["row one"])
        out = buf.getvalue()
        self.assertIn("REPORTING CONTRACT", out)
        self.assertIn("row one", out)
        self.assertIn("/tmp/a.json", out)


if __name__ == "__main__":
    unittest.main()
