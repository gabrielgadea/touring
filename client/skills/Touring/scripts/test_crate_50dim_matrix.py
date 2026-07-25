#!/usr/bin/env python3
"""Unit + chain + e2e tests for crate_50dim_matrix.py (the lossless 50-dim matrix).

Verifies the primary matrix tool: score() error-resilience, digest/non_artifact
filtering correctness, target enumeration (whole crate tree), and lossless
artifact writing (JSON raw + wide TSV na-marking + long format). Stdlib unittest
+ mock; touring-quality mocked, crate trees are real temp dirs.

Run: python3 -m unittest test_crate_50dim_matrix -v
"""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

MODPATH = str(Path(__file__).resolve().parent / "crate_50dim_matrix.py")
with mock.patch("pathlib.Path.is_file", return_value=True):
    _spec = importlib.util.spec_from_file_location("crate_50dim_matrix", MODPATH)
    mod = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(mod)

DIMS = ["F1_3", "F2_5", "F3_11", "F4_8"]  # a small representative dim set


def make_score(root="crates/x", scope="crate", statuses=None):
    """A complete touring-quality score object with chosen per-dim statuses."""
    statuses = statuses or {d: "Pass" for d in DIMS}
    dims = {d: {"value": 0.1 if s == "Fail" else 1.0, "status": s,
                "evidence": f"ev\t{d}\nline2", "suggestions": [f"fix {d}"], "latency_ms": 0}
            for d, s in statuses.items()}
    fails = [d for d, s in statuses.items() if s == "Fail"]
    return {"scope_kind": scope, "root": root, "file_count": 1, "total_loc": 100,
            "composite": 0.9, "tier": "Platinum", "blockers": fails,
            "warnings": [], "schema_version": 1, "dimensions": dims}


# ── UNIT + ERROR: score() resilience ─────────────────────────────────────────
class TestScore(unittest.TestCase):
    def test_happy_path_returns_complete_object(self):
        payload = SimpleNamespace(returncode=0, stdout=json.dumps(make_score()), stderr="")
        with mock.patch.object(mod.subprocess, "run", return_value=payload):
            self.assertIn("dimensions", mod.score("crates/x", "crate"))

    def test_missing_binary_degrades_to_error_dict(self):
        with mock.patch.object(mod.subprocess, "run", side_effect=OSError("no bin")):
            self.assertIn("error", mod.score("crates/x", "crate"))

    def test_garbled_stdout_degrades_to_error_dict(self):
        payload = SimpleNamespace(returncode=0, stdout="not json", stderr="")
        with mock.patch.object(mod.subprocess, "run", return_value=payload):
            self.assertIn("error", mod.score("crates/x", "crate"))

    def test_nonzero_exit_without_dims_records_error(self):
        payload = SimpleNamespace(returncode=2, stdout="{}", stderr="bad target")
        with mock.patch.object(mod.subprocess, "run", return_value=payload):
            self.assertIn("error", mod.score("crates/x", "crate"))


# ── UNIT: digest / non_artifact_fails / clean ────────────────────────────────
class TestDigest(unittest.TestCase):
    def test_extracts_fails_warns_na_and_metadata(self):
        d = make_score(statuses={"F2_5": "Fail", "F1_3": "Warn", "F4_8": "NotApplicable", "F3_11": "Pass"})
        dg = mod.digest(d)
        self.assertEqual(dg["fails"], ["F2_5"])
        self.assertEqual(dg["warns"], ["F1_3"])
        self.assertEqual(dg["n_na"], 1)
        self.assertEqual(dg["blockers"], ["F2_5"])
        self.assertEqual(dg["tier"], "Platinum")

    def test_error_row_passes_through(self):
        dg = mod.digest({"root": "crates/x", "scope_kind": "crate", "error": "boom"})
        self.assertEqual(dg["error"], "boom")

    def test_missing_dimensions_is_treated_as_error(self):
        self.assertIn("error", mod.digest({"root": "x", "scope_kind": "crate"}))


class TestNonArtifactFails(unittest.TestCase):
    def test_strips_scope_artifact_dims(self):
        # F3_11 (readme) is a scope-artifact FP; F1_3 is a real fail.
        self.assertEqual(mod.non_artifact_fails({"fails": ["F3_11", "F1_3"]}), ["F1_3"])

    def test_empty_fails_yields_empty(self):
        self.assertEqual(mod.non_artifact_fails({"fails": []}), [])


class TestClean(unittest.TestCase):
    def test_flattens_tabs_newlines_carriage_returns(self):
        self.assertEqual(mod.clean("a\tb\nc\rd"), "a b c d")


# ── UNIT: target enumeration (whole crate tree) ──────────────────────────────
class TestEnumerateTargets(unittest.TestCase):
    def test_enumerates_files_and_containing_dirs_excluding_root(self):
        with tempfile.TemporaryDirectory() as t:
            crate = Path(t) / "crate"
            (crate / "src").mkdir(parents=True)
            (crate / "tests").mkdir(parents=True)
            (crate / "src/a.rs").write_text("x")
            (crate / "tests/b.rs").write_text("y")
            files, dirs = mod.enumerate_targets(str(crate))
        self.assertEqual(len(files), 2)
        self.assertEqual({Path(d).name for d in dirs}, {"src", "tests"})  # not the crate root

    def test_target_dir_is_skipped(self):
        with tempfile.TemporaryDirectory() as t:
            crate = Path(t) / "crate"
            (crate / "target/debug").mkdir(parents=True)
            (crate / "target/debug/x.rs").write_text("z")
            (crate / "src").mkdir()
            (crate / "src/real.rs").write_text("r")
            files, _ = mod.enumerate_targets(str(crate))
        self.assertEqual([Path(f).name for f in files], ["real.rs"])


# ── CHAIN: build_matrix scores every granularity ─────────────────────────────
class TestBuildMatrix(unittest.TestCase):
    def test_scores_crate_files_and_dirs(self):
        with tempfile.TemporaryDirectory() as t:
            crate = Path(t) / "crate"
            (crate / "src").mkdir(parents=True)
            (crate / "src/a.rs").write_text("x")
            with mock.patch.object(mod, "score", side_effect=lambda tgt, sc: make_score(tgt, sc)):
                res = mod.build_matrix(str(crate), "slug")
        self.assertEqual(res["aggregate"]["scope_kind"], "crate")
        self.assertEqual(len(res["files"]), 1)
        self.assertEqual(res["meta"]["file_scopes"], 1)


# ── UNIT: write_artifacts (lossless JSON + wide na-marking + long rows) ───────
class TestWriteArtifacts(unittest.TestCase):
    def test_writes_three_artifacts_and_counts_long_rows(self):
        res = {
            "aggregate": make_score("crates/x", "crate",
                                    {"F2_5": "Fail", "F4_8": "NotApplicable", "F1_3": "Pass", "F3_11": "Pass"}),
            "files": [make_score("crates/x/a.rs", "file")],
            "paths": [],
        }
        with tempfile.TemporaryDirectory() as t:
            rows = mod.write_artifacts(res, t, "slug")
            matrix = json.loads((Path(t) / "slug_50dim_matrix.json").read_text())
            wide = (Path(t) / "slug_50dim_matrix.tsv").read_text()
            long = (Path(t) / "slug_50dim_long.tsv").read_text().splitlines()
        self.assertEqual(rows, 2 * len(DIMS))                 # 2 scored targets × 4 dims
        self.assertEqual(matrix["aggregate"]["dimensions"]["F2_5"]["status"], "Fail")  # lossless raw
        self.assertIn("na", wide)                             # NotApplicable cell marked `na`
        self.assertEqual(len(long), 1 + rows)                 # header + one row per cell


# ── E2E: main() end-to-end with mocked score ─────────────────────────────────
class TestE2E(unittest.TestCase):
    def test_main_builds_and_writes_matrix(self):
        with tempfile.TemporaryDirectory() as t:
            crate = Path(t) / "crate"
            (crate / "src").mkdir(parents=True)
            (crate / "src/a.rs").write_text("x")
            out = Path(t) / "out"
            out.mkdir()
            printed: list[str] = []
            with mock.patch.object(mod, "ROOT", str(crate)), \
                 mock.patch.object(mod, "SLUG", "slug"), \
                 mock.patch.object(mod, "OUT", str(out)), \
                 mock.patch.object(mod, "score", side_effect=lambda tgt, sc: make_score(tgt, sc)), \
                 mock.patch("builtins.print", side_effect=lambda *a, **k: printed.append(" ".join(map(str, a)))):
                mod.main()
            self.assertTrue((out / "slug_50dim_matrix.json").exists())
            self.assertTrue((out / "slug_50dim_long.tsv").exists())
            self.assertIn("REPORTING CONTRACT", "\n".join(printed))  # mandatory contract emitted


if __name__ == "__main__":
    unittest.main(verbosity=2)
