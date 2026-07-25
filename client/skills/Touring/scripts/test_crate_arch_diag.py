#!/usr/bin/env python3
"""Unit + chain + e2e tests for crate_arch_diag.py (crate-internal architecture).

Verifies the structural analysis: God-object detection (LOC > 3× median & > 800),
intra-crate module fan-in via the `use crate::<mod>` regex, and graceful handling
of the harness dim-score. Stdlib unittest + mock; touring-quality is mocked,
crate trees are real temp dirs.

Run: python3 -m unittest test_crate_arch_diag -v
"""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

MODPATH = str(Path(__file__).resolve().parent / "crate_arch_diag.py")
with mock.patch("pathlib.Path.is_file", return_value=True):
    _spec = importlib.util.spec_from_file_location("crate_arch_diag", MODPATH)
    mod = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(mod)


def make_crate(tmp: Path, files: dict) -> str:
    """Create tmp/crate/src/<name> for each {name: text}; return the crate rel path."""
    src = tmp / "crate/src"
    src.mkdir(parents=True)
    for name, text in files.items():
        (src / name).parent.mkdir(parents=True, exist_ok=True)
        (src / name).write_text(text)
    return str(tmp / "crate")


def score_json(fails=("F1_7",), composite=0.9, tier="Platinum"):
    dims = {d: {"value": 0.1 if d in fails else 1.0,
                "status": "Fail" if d in fails else "Pass",
                "evidence": f"ev-{d}", "suggestions": []} for d in mod.ARCH_DIMS}
    return {"composite": composite, "tier": tier, "dimensions": dims}


# ── UNIT: crate-internal structure (God-objects + fan-in) ────────────────────
class TestAnalyzeCrate(unittest.TestCase):
    def test_detects_god_object_over_absolute_and_median_thresholds(self):
        with tempfile.TemporaryDirectory() as t:
            crate = make_crate(Path(t), {
                "big.rs": "\n".join(["x();"] * 2000),
                "s1.rs": "a\n", "s2.rs": "b\n", "s3.rs": "c\n",
            })
            out = mod.analyze_crate(crate)
        self.assertEqual([g["file"] for g in out["god_objects"]], ["big.rs"])
        self.assertEqual(out["files"], 4)
        self.assertEqual(out["max_loc"], 2000)

    def test_no_god_object_when_all_files_small(self):
        with tempfile.TemporaryDirectory() as t:
            crate = make_crate(Path(t), {"a.rs": "a\n", "b.rs": "b\n"})
            self.assertEqual(mod.analyze_crate(crate)["god_objects"], [])

    def test_intra_crate_module_fanin_from_use_statements(self):
        with tempfile.TemporaryDirectory() as t:
            crate = make_crate(Path(t), {
                "a.rs": "use crate::alpha;\nlet x = crate::beta::go();\n",
                "b.rs": "use crate::alpha;\n",
            })
            fanin = dict(mod.analyze_crate(crate)["top_module_fanin"])
        self.assertEqual(fanin["alpha"], 2)  # referenced from both files
        self.assertEqual(fanin["beta"], 1)

    def test_empty_src_tree_is_handled(self):
        with tempfile.TemporaryDirectory() as t:
            (Path(t) / "crate/src").mkdir(parents=True)
            out = mod.analyze_crate(str(Path(t) / "crate"))
        self.assertEqual(out["files"], 0)
        self.assertEqual(out["god_objects"], [])


# ── UNIT + ERROR: harness dim score (mocked subprocess) ──────────────────────
class TestArchDims(unittest.TestCase):
    def test_happy_path_extracts_arch_dims(self):
        with mock.patch.object(mod.subprocess, "run",
                               return_value=SimpleNamespace(stdout=json.dumps(score_json()), returncode=0, stderr="")):
            out = mod.arch_dims("crates/x")
        self.assertEqual(out["tier"], "Platinum")
        self.assertIn("F1_7", out["arch"])
        self.assertEqual(out["arch"]["F1_7"]["status"], "Fail")

    def test_garbled_output_degrades_to_error(self):
        with mock.patch.object(mod.subprocess, "run",
                               return_value=SimpleNamespace(stdout="nope", returncode=0, stderr="")):
            self.assertIn("error", mod.arch_dims("crates/x"))

    def test_missing_binary_degrades_to_error(self):
        with mock.patch.object(mod.subprocess, "run", side_effect=OSError("no bin")):
            self.assertIn("error", mod.arch_dims("crates/x"))


# ── CHAIN + E2E: analyze() + main() ──────────────────────────────────────────
class TestChainAndE2E(unittest.TestCase):
    def test_analyze_fuses_struct_and_dims_per_crate(self):
        with tempfile.TemporaryDirectory() as t:
            crate = make_crate(Path(t), {"a.rs": "use crate::z;\n"})
            with mock.patch.object(mod, "arch_dims", return_value={"composite": 0.9, "tier": "Platinum", "arch": {}}):
                res = mod.analyze([crate])
        name = crate.rsplit("/", 1)[-1]
        self.assertIn(name, res)
        self.assertEqual(res[name]["struct"]["files"], 1)
        self.assertEqual(res[name]["dims"]["tier"], "Platinum")

    def test_digest_prints_god_objects_composite_and_arch_dims(self):
        # Exercises the happy print branches: composite present, arch-dim loop, God-objects listed.
        results = {"x": {
            "struct": {"files": 2, "total_loc": 3000, "median_loc": 10.0, "max_loc": 2000,
                       "god_objects": [{"file": "big.rs", "loc": 2000}],
                       "top_module_fanin": [("alpha", 3)]},
            "dims": {"composite": 0.9, "tier": "Platinum",
                     "arch": {"F1_7": {"value": 0.4, "status": "Warn", "evidence": "boundary gap"}}},
        }}
        printed = []
        with mock.patch("builtins.print", side_effect=lambda *a, **k: printed.append(" ".join(map(str, a)))):
            mod._print_digest(results)
        blob = "\n".join(printed)
        self.assertIn("GOD-OBJECTS", blob)
        self.assertIn("big.rs", blob)
        self.assertIn("F1_7", blob)
        self.assertIn("REPORTING CONTRACT", blob)  # mandatory contract emitted with every digest

    def test_digest_reports_no_god_objects_branch(self):
        results = {"x": {
            "struct": {"files": 1, "total_loc": 5, "median_loc": 5.0, "max_loc": 5,
                       "god_objects": [], "top_module_fanin": []},
            "dims": {"error": "score failed"},
        }}
        printed = []
        with mock.patch("builtins.print", side_effect=lambda *a, **k: printed.append(" ".join(map(str, a)))):
            mod._print_digest(results)
        self.assertIn("God-objects: none", "\n".join(printed))

    def test_main_writes_matrix_and_survives_dim_error(self):
        with tempfile.TemporaryDirectory() as t:
            crate = make_crate(Path(t), {"a.rs": "fn a() {}\n"})
            with mock.patch.object(mod.sys, "argv", ["prog", crate]), \
                 mock.patch.object(mod, "arch_dims", return_value={"error": "score failed"}), \
                 mock.patch.object(mod, "OUT", t), \
                 mock.patch("builtins.print"):
                mod.main()
            written = json.loads((Path(t) / "crate_arch_matrix.json").read_text())
        name = crate.rsplit("/", 1)[-1]
        self.assertIn("error", written[name]["dims"])  # error row survives, no crash


if __name__ == "__main__":
    unittest.main(verbosity=2)
