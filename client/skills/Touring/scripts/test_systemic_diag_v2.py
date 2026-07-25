#!/usr/bin/env python3
"""Unit + chain + e2e tests for systemic_diag_v2.py.

Verifies the systemic diagnostic's effectiveness (risk-fusion correctness),
functionality (path-scoping in every mode), robustness (error paths), and
completeness (lossless matrix). Stdlib `unittest` + `mock` only — no external
deps, no real cargo/touring-quality calls (both mocked), each test independent
and idempotent.

Run: python3 -m unittest test_systemic_diag_v2 -v
"""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

# ── Import the module under test ──────────────────────────────────────────────
# The module runs a `Path(QBIN).is_file()` guard at import; patch it so the suite
# is portable (does not require the touring-quality binary to be installed).
MODPATH = str(Path(__file__).resolve().parent / "systemic_diag_v2.py")
with mock.patch("pathlib.Path.is_file", return_value=True):
    _spec = importlib.util.spec_from_file_location("systemic_diag_v2", MODPATH)
    mod = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(mod)


# ── Fixtures / factories ──────────────────────────────────────────────────────
def make_score_json(fails=(), warns=(), na=(), composite=0.9, tier="Platinum"):
    """A complete 50-dim touring-quality score object with chosen statuses."""
    dims = {}
    for k in mod.NAME:
        if k in fails:
            dims[k] = {"value": 0.1, "status": "Fail"}
        elif k in warns:
            dims[k] = {"value": 0.6, "status": "Warn"}
        elif k in na:
            dims[k] = {"value": 1.0, "status": "NotApplicable"}
        else:
            dims[k] = {"value": 1.0, "status": "Pass"}
    return {"composite": composite, "tier": tier, "dimensions": dims}


def make_ok_entry(fails=(), warns=(), composite=0.9, tier="Platinum"):
    """An entry as produced by score_all_dims() (the `ok` map value)."""
    j = make_score_json(fails, warns, composite=composite, tier=tier)
    return {"composite": j["composite"], "tier": j["tier"], "dims": j["dimensions"]}


def fake_metadata(crates):
    """cargo-metadata JSON for a synthetic workspace. crates = {name: [int_deps]}."""
    return {"packages": [
        {"name": n, "manifest_path": str(mod.ROOT / f"crates/{n}/Cargo.toml"),
         "dependencies": [{"name": d} for d in deps]}
        for n, deps in crates.items()]}


def make_fake_run(metadata=None, scores=None):
    """A drop-in for mod.run: dispatch cargo-metadata vs touring-quality score.

    `metadata` / each `scores[target]` may be a dict (→ JSON), a str (raw stdout,
    to simulate garbled output), or an Exception instance (→ raised).
    """
    scores = scores or {}

    def _run(cmd, timeout=200):
        payload = metadata if cmd[0] == "cargo" else scores.get(cmd[2])
        if isinstance(payload, Exception):
            raise payload
        stdout = payload if isinstance(payload, str) else json.dumps(payload)
        return SimpleNamespace(stdout=stdout, returncode=0, stderr="")

    return _run


# ── UNIT: enforcement classification (the authoritative weight map) ───────────
class TestEnforcement(unittest.TestCase):
    def test_block_dims_are_block(self):
        for d in ("F2_1", "F2_4", "F2_5", "F2_6", "F4_3", "F4_5"):
            self.assertEqual(mod.enforcement(d), "Block", d)

    def test_warn_dims_are_warn(self):
        for d in ("F1_1", "F1_6", "F3_1", "F3_7"):
            self.assertEqual(mod.enforcement(d), "Warn", d)

    def test_other_dims_are_advisory(self):
        for d in ("F1_9", "F1_8", "F2_13", "F3_11", "F4_7", "F4_12"):
            self.assertEqual(mod.enforcement(d), "Advisory", d)

    def test_block_and_warn_sets_are_disjoint_and_sized(self):
        self.assertEqual(len(mod.BLOCK), 6)
        self.assertEqual(len(mod.WARN), 13)
        self.assertEqual(mod.BLOCK & mod.WARN, set())


# ── UNIT: per-dimension workspace health aggregation ─────────────────────────
class TestDimHealth(unittest.TestCase):
    def test_counts_statuses_per_dim_across_crates(self):
        ok = {"a": make_ok_entry(fails=["F2_5"]), "b": make_ok_entry()}  # b passes F2_5
        h = mod.dim_health(ok)
        self.assertEqual(h["F2_5"], {"Fail": 1, "Pass": 1})

    def test_empty_input_yields_empty_per_dim_counts(self):
        h = mod.dim_health({})
        self.assertEqual(h["F1_1"], {})  # every dim present, no counts

    def test_not_applicable_is_counted_distinctly(self):
        ok = {"a": make_ok_entry(), "b": make_ok_entry()}
        ok["a"]["dims"]["F4_8"]["status"] = "NotApplicable"
        h = mod.dim_health(ok)
        self.assertEqual(h["F4_8"].get("NotApplicable"), 1)

    def test_dim_absent_from_a_crate_is_not_counted(self):
        # A partial score (crate missing some dims) must not inflate any count.
        ok = {"a": {"composite": 0.9, "tier": "Platinum", "dims": {}}}
        self.assertEqual(mod.dim_health(ok)["F1_1"], {})


class TestImportGuard(unittest.TestCase):
    def test_missing_binary_aborts_import_with_clear_message(self):
        with mock.patch("pathlib.Path.is_file", return_value=False):
            spec = importlib.util.spec_from_file_location("sdv2_noqbin", MODPATH)
            fresh = importlib.util.module_from_spec(spec)
            with self.assertRaises(SystemExit):
                spec.loader.exec_module(fresh)


# ── UNIT: fused risk (effectiveness — the core value proposition) ────────────
class TestCrateRisk(unittest.TestCase):
    def test_block_fail_weight_and_blast_amplification(self):
        # 1 BLOCK fail (weight 2.0), blast 10 → load 2.0, risk 2.0*(1+10/10)=4.0
        ok = {"a": make_ok_entry(fails=["F2_5"], composite=0.5, tier="Unranked")}
        r = mod.crate_risk(ok, {"a": 10})["a"]
        self.assertAlmostEqual(r["risk"], 4.0)
        self.assertEqual(r["block_fails"], ["F2_5"])
        self.assertEqual(r["n_fail"], 1)
        self.assertEqual(r["blast_fan_in"], 10)

    def test_higher_blast_ranks_higher_for_same_defect(self):
        ok = {"a": make_ok_entry(fails=["F2_5"])}
        low = mod.crate_risk(ok, {"a": 0})["a"]["risk"]
        high = mod.crate_risk(ok, {"a": 20})["a"]["risk"]
        self.assertGreater(high, low)  # architecture amplifies security severity

    def test_warn_contributes_half_weight(self):
        # 1 WARN dim (weight 1.5), no blast → load 0.5*1.5 = 0.75
        ok = {"a": make_ok_entry(warns=["F1_1"])}
        self.assertAlmostEqual(mod.crate_risk(ok, {"a": 0})["a"]["risk"], 0.75)

    def test_block_fails_filters_to_block_dims_only(self):
        ok = {"a": make_ok_entry(fails=["F2_5", "F1_3"])}  # F1_3 is WARN-class, not BLOCK
        self.assertEqual(mod.crate_risk(ok, {"a": 0})["a"]["block_fails"], ["F2_5"])

    def test_missing_blast_defaults_to_zero(self):
        ok = {"a": make_ok_entry(fails=["F2_5"])}
        self.assertEqual(mod.crate_risk(ok, {})["a"]["blast_fan_in"], 0)


# ── UNIT: path scoping (functionality — every mode) ──────────────────────────
class TestResolveTargets(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name).resolve()
        (self.root / "crates/a/src").mkdir(parents=True)
        (self.root / "crates/b").mkdir(parents=True)
        (self.root / "crates/a/lib.rs").write_text("// fake")
        self.members = ["a", "b"]
        self.mdir = {"a": "crates/a", "b": "crates/b"}
        self.fan_in = {"a": 3, "b": 0}
        self._patch = mock.patch.object(mod, "ROOT", self.root)
        self._patch.start()

    def tearDown(self):
        self._patch.stop()
        self._tmp.cleanup()

    def _resolve(self, target):
        return mod.resolve_targets(target, self.members, self.mdir, self.fan_in)

    def test_none_target_returns_all_members_at_crate_scope(self):
        units = self._resolve(None)
        self.assertEqual({u[0] for u in units}, {"a", "b"})
        self.assertTrue(all(u[2] == "crate" for u in units))
        self.assertEqual(dict((u[0], u[3]) for u in units), {"a": 3, "b": 0})

    def test_exact_crate_dir_returns_that_crate(self):
        units = self._resolve(str(self.root / "crates/a"))
        self.assertEqual(units, [("a", "crates/a", "crate", 3)])

    def test_folder_of_crates_returns_all_contained_crates(self):
        units = self._resolve(str(self.root / "crates"))
        self.assertEqual({u[0] for u in units}, {"a", "b"})

    def test_subfolder_of_crate_scores_at_path_scope_with_owner_blast(self):
        units = self._resolve(str(self.root / "crates/a/src"))
        self.assertEqual(units, [("crates/a/src", "crates/a/src", "path", 3)])

    def test_single_file_scores_at_file_scope(self):
        units = self._resolve(str(self.root / "crates/a/lib.rs"))
        self.assertEqual(units[0][2], "file")

    def test_path_outside_workspace_exits(self):
        with tempfile.TemporaryDirectory() as other:
            with self.assertRaises(SystemExit):
                self._resolve(other)

    def test_nonexistent_path_exits(self):
        with self.assertRaises(SystemExit):
            self._resolve(str(self.root / "nope"))


# ── UNIT + ERROR: cargo graph + score, incl. robustness (F1_6 lesson) ────────
class TestWorkspaceFanIn(unittest.TestCase):
    def test_builds_members_and_fan_in_from_metadata(self):
        with mock.patch.object(mod, "run", make_fake_run(metadata=fake_metadata({"a": ["b"], "b": []}))):
            members, mdir, fan_in = mod.workspace_fan_in()
        self.assertEqual(members, ["a", "b"])
        self.assertEqual(fan_in["b"], 1)  # a depends on b
        self.assertEqual(fan_in.get("a", 0), 0)

    def test_external_deps_are_excluded_from_fan_in(self):
        meta = fake_metadata({"a": []})
        meta["packages"][0]["dependencies"] = [{"name": "serde"}, {"name": "tokio"}]
        with mock.patch.object(mod, "run", make_fake_run(metadata=meta)):
            _, _, fan_in = mod.workspace_fan_in()
        self.assertEqual(dict(fan_in), {})  # non-members never counted

    def test_missing_cargo_binary_exits_cleanly(self):
        with mock.patch.object(mod, "run", make_fake_run(metadata=FileNotFoundError("cargo"))):
            with self.assertRaises(SystemExit):
                mod.workspace_fan_in()

    def test_garbled_metadata_exits_cleanly(self):
        with mock.patch.object(mod, "run", make_fake_run(metadata="not json")):
            with self.assertRaises(SystemExit):
                mod.workspace_fan_in()


class TestScoreAllDims(unittest.TestCase):
    def test_happy_path_returns_dims_composite_tier(self):
        with mock.patch.object(mod, "run", make_fake_run(scores={"crates/a": make_score_json(fails=["F2_5"])})):
            out = mod.score_all_dims("crates/a")
        self.assertEqual(out["tier"], "Platinum")
        self.assertEqual(out["dims"]["F2_5"]["status"], "Fail")

    def test_scope_argument_is_forwarded(self):
        captured = {}

        def spy(cmd, timeout=200):
            captured["cmd"] = cmd
            return SimpleNamespace(stdout=json.dumps(make_score_json()), returncode=0, stderr="")

        with mock.patch.object(mod, "run", spy):
            mod.score_all_dims("crates/a/src", scope="path")
        self.assertIn("path", captured["cmd"])

    def test_missing_binary_degrades_to_error_row(self):
        with mock.patch.object(mod, "run", make_fake_run(scores={"crates/a": OSError("no bin")})):
            out = mod.score_all_dims("crates/a")
        self.assertIn("error", out)
        self.assertNotIn("dims", out)

    def test_garbled_output_degrades_to_error_row(self):
        with mock.patch.object(mod, "run", make_fake_run(scores={"crates/a": "boom"})):
            self.assertIn("error", mod.score_all_dims("crates/a"))

    def test_missing_dimensions_key_degrades_to_error_row(self):
        with mock.patch.object(mod, "run", make_fake_run(scores={"crates/a": {"composite": 1}})):
            self.assertIn("error", mod.score_all_dims("crates/a"))


# ── CHAIN: a defect propagates coherently across the pipeline stages ─────────
class TestChain(unittest.TestCase):
    def test_fail_propagates_from_score_to_dim_health_and_risk(self):
        ok = {
            "a": make_ok_entry(fails=["F2_5"], composite=0.5, tier="Unranked"),
            "b": make_ok_entry(fails=["F2_5", "F1_3"], warns=["F1_1"], composite=0.6, tier="Unranked"),
        }
        health = mod.dim_health(ok)
        risk = mod.crate_risk(ok, {"a": 5, "b": 1})
        # (1) dim_health reflects both crates failing F2_5
        self.assertEqual(health["F2_5"]["Fail"], 2)
        # (2) the same fail surfaces in each crate's block_fails
        self.assertIn("F2_5", risk["a"]["block_fails"])
        self.assertIn("F2_5", risk["b"]["block_fails"])
        # (3) blast amplification orders a (fewer defects but blast 5) sensibly vs b
        self.assertGreater(risk["a"]["risk"], 0)
        self.assertEqual(risk["b"]["n_warn"], 1)  # F1_1 warn threaded through


# ── E2E: main() end-to-end with mocked externals ─────────────────────────────
class TestE2E(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self._out = mock.patch.object(mod, "OUT", self._tmp.name)
        self._out.start()
        self.matrix_path = Path(self._tmp.name) / "systemic_diag_v2_matrix.json"

    def tearDown(self):
        self._out.stop()
        self._tmp.cleanup()

    def _run_main(self, argv, metadata, scores):
        with mock.patch.object(mod.sys, "argv", argv), \
             mock.patch.object(mod, "run", make_fake_run(metadata=metadata, scores=scores)), \
             mock.patch("builtins.print"):  # silence the digest
            mod.main()
        return json.loads(self.matrix_path.read_text())

    def test_workspace_mode_writes_complete_lossless_matrix(self):
        meta = fake_metadata({"a": ["b"], "b": []})
        scores = {"crates/a": make_score_json(fails=["F2_5"]),
                  "crates/b": make_score_json(fails=["F2_5", "F1_3"], warns=["F1_1"])}
        res = self._run_main(["prog"], meta, scores)
        self.assertEqual(res["unit_count"], 2)
        self.assertEqual(res["scored"], 2)
        self.assertEqual(res["dim_health"]["F2_5"], {"Fail": 2})
        self.assertEqual(res["crate_risk"]["b"]["blast_fan_in"], 1)  # a→b
        self.assertEqual(res["errors"], {})
        # lossless: every scored crate carries all 50 raw dims
        self.assertEqual(len(res["crate_dims"]["a"]), 50)
        self.assertIn("F2_5", res["crate_dims"]["a"])

    def test_unscoreable_crate_becomes_error_row_not_a_crash(self):
        meta = fake_metadata({"a": [], "b": []})
        scores = {"crates/a": make_score_json(), "crates/b": OSError("score died")}
        res = self._run_main(["prog"], meta, scores)
        self.assertEqual(res["scored"], 1)
        self.assertIn("b", res["errors"])
        self.assertNotIn("b", res["crate_risk"])

    def test_digest_prints_without_error(self):
        meta = fake_metadata({"a": []})
        scores = {"crates/a": make_score_json(fails=["F2_5"])}
        printed = []
        with mock.patch.object(mod.sys, "argv", ["prog"]), \
             mock.patch.object(mod, "run", make_fake_run(metadata=meta, scores=scores)), \
             mock.patch("builtins.print", side_effect=lambda *a, **k: printed.append(" ".join(map(str, a)))):
            mod.main()
        blob = "\n".join(printed)
        self.assertIn("SYSTEMIC DIAGNOSTIC", blob)
        self.assertIn("PER-DIMENSION WORKSPACE HEALTH", blob)
        # the reporting contract is mandatory — every digest emits it (compaction-proof)
        self.assertIn("REPORTING CONTRACT", blob)


if __name__ == "__main__":
    unittest.main(verbosity=2)
