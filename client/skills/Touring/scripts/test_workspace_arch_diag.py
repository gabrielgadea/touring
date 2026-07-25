#!/usr/bin/env python3
"""Unit + chain + e2e tests for workspace_arch_diag.py (the inter-crate DAG diagnostic).

The diagnostic's value is graph correctness: does Tarjan find real dependency
cycles, does `depth_of` compute layers without looping on a cycle, do fan-in/out
and roles come out right? Stdlib unittest + mock; cargo and the filesystem are
mocked (no real workspace scan).

Run: python3 -m unittest test_workspace_arch_diag -v
"""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

MODPATH = str(Path(__file__).resolve().parent / "workspace_arch_diag.py")
_spec = importlib.util.spec_from_file_location("workspace_arch_diag", MODPATH)
mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mod)


def fake_meta(crates: dict) -> dict:
    """cargo-metadata for a synthetic workspace. crates = {name: [internal_deps]}."""
    return {"packages": [
        {"name": n, "manifest_path": f"/ws/crates/{n}/Cargo.toml",
         "dependencies": [{"name": d} for d in deps]}
        for n, deps in crates.items()]}


# ── UNIT: Tarjan strongly-connected components (cycle detection) ──────────────
class TestTarjanSCC(unittest.TestCase):
    def _sccs(self, edges):
        return mod.tarjan_scc(list(edges), edges)

    def test_acyclic_graph_has_only_singleton_components(self):
        edges = {"a": {"b"}, "b": {"c"}, "c": set()}
        self.assertTrue(all(len(c) == 1 for c in self._sccs(edges)))

    def test_two_node_cycle_is_one_component(self):
        edges = {"a": {"b"}, "b": {"a"}}
        multi = [sorted(c) for c in self._sccs(edges) if len(c) > 1]
        self.assertEqual(multi, [["a", "b"]])

    def test_three_node_cycle_is_one_component(self):
        edges = {"a": {"b"}, "b": {"c"}, "c": {"a"}}
        multi = [c for c in self._sccs(edges) if len(c) > 1]
        self.assertEqual(len(multi), 1)
        self.assertEqual(len(multi[0]), 3)

    def test_self_loop_is_not_counted_as_a_multi_node_cycle(self):
        edges = {"a": {"a"}}
        self.assertTrue(all(len(c) == 1 for c in self._sccs(edges)))

    def test_disconnected_components_are_separate(self):
        edges = {"a": {"b"}, "b": set(), "x": {"y"}, "y": set()}
        self.assertEqual(len(self._sccs(edges)), 4)


# ── UNIT: layer depth (longest dependency chain, cycle-safe) ─────────────────
class TestDepthOf(unittest.TestCase):
    def test_leaf_has_depth_zero(self):
        self.assertEqual(mod.depth_of("c", {"c": set()}, {}), 0)

    def test_chain_depth_is_longest_path(self):
        edges = {"a": {"b"}, "b": {"c"}, "c": set()}
        self.assertEqual(mod.depth_of("a", edges, {}), 2)
        self.assertEqual(mod.depth_of("b", edges, {}), 1)

    def test_cycle_does_not_infinite_loop(self):
        edges = {"a": {"b"}, "b": {"a"}}
        # memo cycle-guard must make this terminate with a finite depth.
        self.assertIsInstance(mod.depth_of("a", edges, {}), int)


# ── UNIT: role classification (5 branches) ───────────────────────────────────
class TestRole(unittest.TestCase):
    def test_all_five_roles(self):
        self.assertEqual(mod.role(0, 0), "isolated")
        self.assertEqual(mod.role(3, 0), "foundation-leaf")
        self.assertEqual(mod.role(0, 3), "top/orchestrator")
        self.assertEqual(mod.role(6, 2), "hub")
        self.assertEqual(mod.role(2, 2), "intermediate")


# ── UNIT: crate LOC counting (filesystem) ────────────────────────────────────
class TestCrateLoc(unittest.TestCase):
    def test_counts_lines_and_files_under_src(self):
        with tempfile.TemporaryDirectory() as t:
            src = Path(t) / "crates/x/src"
            src.mkdir(parents=True)
            (src / "a.rs").write_text("l1\nl2\nl3\n")
            (src / "b.rs").write_text("only\n")
            loc, n = mod.crate_loc(str(Path(t) / "crates/x/Cargo.toml"))
        self.assertEqual((loc, n), (4, 2))

    def test_absent_src_dir_yields_zero(self):
        with tempfile.TemporaryDirectory() as t:
            self.assertEqual(mod.crate_loc(str(Path(t) / "no/Cargo.toml")), (0, 0))


# ── CHAIN: build_graph fuses edges → fan → SCC → depth → roles ───────────────
class TestBuildGraph(unittest.TestCase):
    def test_acyclic_workspace_result_is_correct(self):
        meta = fake_meta({"a": ["b"], "b": [], "iso": []})
        with mock.patch.object(mod, "crate_loc", return_value=(50, 1)):
            r = mod.build_graph(meta)
        self.assertEqual(r["cycle_count"], 0)
        self.assertEqual(r["nodes"]["b"]["fan_in"], 1)      # a → b
        self.assertEqual(r["nodes"]["b"]["role"], "foundation-leaf")
        self.assertEqual(r["nodes"]["iso"]["role"], "isolated")
        self.assertEqual(r["max_depth"], 1)                 # a→b chain
        self.assertEqual(r["total_loc"], 150)               # 3 crates × 50

    def test_cycle_is_reported(self):
        meta = fake_meta({"a": ["b"], "b": ["a"]})
        with mock.patch.object(mod, "crate_loc", return_value=(1, 1)):
            r = mod.build_graph(meta)
        self.assertEqual(r["cycle_count"], 1)
        self.assertEqual(r["cycles"][0], ["a", "b"])

    def test_external_deps_do_not_count_as_edges(self):
        meta = fake_meta({"a": []})
        meta["packages"][0]["dependencies"] = [{"name": "serde"}]
        with mock.patch.object(mod, "crate_loc", return_value=(1, 1)):
            r = mod.build_graph(meta)
        self.assertEqual(r["internal_edge_count"], 0)


# ── UNIT + ERROR: cargo_metadata ─────────────────────────────────────────────
class TestCargoMetadata(unittest.TestCase):
    def test_happy_path_parses_json(self):
        payload = SimpleNamespace(returncode=0, stdout=json.dumps(fake_meta({"a": []})), stderr="")
        with mock.patch.object(mod.subprocess, "run", return_value=payload):
            self.assertIn("packages", mod.cargo_metadata(Path(".")))

    def test_nonzero_exit_aborts_cleanly(self):
        payload = SimpleNamespace(returncode=1, stdout="", stderr="boom")
        with mock.patch.object(mod.subprocess, "run", return_value=payload):
            with self.assertRaises(SystemExit):
                mod.cargo_metadata(Path("."))


# ── E2E: main() end-to-end with mocked cargo + filesystem ────────────────────
class TestE2E(unittest.TestCase):
    def test_main_writes_matrix_and_prints(self):
        meta = fake_meta({"a": ["b"], "b": []})
        printed: list[str] = []
        with tempfile.TemporaryDirectory() as t:
            with mock.patch.object(mod, "cargo_metadata", return_value=meta), \
                 mock.patch.object(mod, "crate_loc", return_value=(10, 1)), \
                 mock.patch.object(mod, "OUT", t), \
                 mock.patch("builtins.print", side_effect=lambda *a, **k: printed.append(" ".join(map(str, a)))):
                mod.main()
            written = json.loads((Path(t) / "workspace_arch_matrix.json").read_text())
        self.assertEqual(written["crate_count"], 2)
        self.assertEqual(written["nodes"]["b"]["fan_in"], 1)
        self.assertIn("REPORTING CONTRACT", "\n".join(printed))  # mandatory contract emitted


if __name__ == "__main__":
    unittest.main(verbosity=2)
