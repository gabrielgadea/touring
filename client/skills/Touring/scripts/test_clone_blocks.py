#!/usr/bin/env python3
"""Unit + e2e tests for clone_blocks.py (the F1_3 Type-1 clone detector).

The detector drove every dedup decision this session, so its correctness matters:
does it find real 6-line duplicate blocks, exclude #[cfg(test)] regions (as F1_3
does), and filter trivial lines? Stdlib unittest only; find_clones reads a file,
so real temp files stand in for inputs (no external process to mock).

Run: python3 -m unittest test_clone_blocks -v
"""
import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest import mock

MODPATH = str(Path(__file__).resolve().parent / "clone_blocks.py")
_spec = importlib.util.spec_from_file_location("clone_blocks", MODPATH)
mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mod)


def write_tmp(text: str) -> str:
    """Write `text` to a NamedTemporaryFile and return its path (caller-owned)."""
    fh = tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False)
    fh.write(text)
    fh.close()
    return fh.name


# A 6-line body duplicated twice (lines 1-6 and 8-13), each line non-trivial.
DUP_BLOCK = "\n".join(f"stmt_{i}();" for i in range(6))
DUP_FILE = DUP_BLOCK + "\nseparator_line_x();\n" + DUP_BLOCK + "\n"


class TestStripTests(unittest.TestCase):
    def test_production_lines_keep_1_based_linenos(self):
        out = mod.strip_tests(["fn a() {}", "fn b() {}"])
        self.assertEqual(out, [(1, "fn a() {}"), (2, "fn b() {}")])

    def test_cfg_test_region_is_excluded(self):
        src = ["fn prod() {}", "#[cfg(test)]", "mod tests {", "fn t() {}", "}"]
        kept = [t for _, t in mod.strip_tests(src)]
        self.assertIn("fn prod() {}", kept)
        self.assertNotIn("fn t() {}", kept)

    def test_nested_braces_in_test_region_tracked(self):
        src = ["#[cfg(test)]", "mod tests {", "fn t() { if x { y(); } }", "}", "fn after() {}"]
        kept = [t for _, t in mod.strip_tests(src)]
        self.assertIn("fn after() {}", kept)  # region closed correctly at its `}`
        self.assertNotIn("fn t() { if x { y(); } }", kept)


class TestFindClones(unittest.TestCase):
    def test_detects_a_duplicated_6_line_block(self):
        clones = mod.find_clones(write_tmp(DUP_FILE))
        self.assertTrue(clones)
        _, locs = clones[0]
        self.assertEqual(len(locs), 2)  # the block appears at two start lines

    def test_no_duplication_returns_empty(self):
        uniq = "\n".join(f"unique_{i}();" for i in range(12))
        self.assertEqual(mod.find_clones(write_tmp(uniq)), [])

    def test_duplication_inside_cfg_test_is_not_reported(self):
        src = "#[cfg(test)]\nmod tests {\n" + DUP_FILE + "}\n"
        self.assertEqual(mod.find_clones(write_tmp(src)), [])

    def test_trivial_brace_and_blank_lines_are_ignored(self):
        # 6 identical braces would be a clone if not filtered; they must be skipped.
        braces = "\n".join(["{"] * 14)
        self.assertEqual(mod.find_clones(write_tmp(braces)), [])

    def test_blocks_are_ranked_by_frequency_desc(self):
        triple = DUP_BLOCK + "\nsep_a();\n" + DUP_BLOCK + "\nsep_b();\n" + DUP_BLOCK
        clones = mod.find_clones(write_tmp(triple))
        self.assertGreaterEqual(len(clones[0][1]), 3)  # most-frequent block first


class TestMain(unittest.TestCase):
    def test_main_prints_digest_without_error(self):
        path = write_tmp(DUP_FILE)
        printed = []
        with mock.patch("builtins.print", side_effect=lambda *a, **k: printed.append(" ".join(map(str, a)))):
            mod.main([path])
        self.assertIn("dup blocks", "\n".join(printed))

    def test_main_on_empty_arglist_is_a_noop(self):
        with mock.patch("builtins.print") as p:
            mod.main([])
        p.assert_not_called()


if __name__ == "__main__":
    unittest.main(verbosity=2)
