#!/usr/bin/env python3
"""Guards for `harvest_candidates` — the ladder bodies nobody is told about.

The gap these tests encode (measured 2026-09-23): `--harvest` has 0 uses in
the whole journal, because surfacing candidates depended on someone going to
look for them. The ruler is honest in both directions: provisional climbers
with a real success rate surface with the exact next command, and an
absent/corrupt db answers `[]` — never an error, never a fabricated list.
"""
from __future__ import annotations

import importlib.util
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "loop_phase_close", Path(__file__).with_name("loop_phase_close.py")
)
pc = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pc)


def _db(root: Path, rows: list[tuple]) -> None:
    db = root / ".claude" / "touring"
    db.mkdir(parents=True)
    conn = sqlite3.connect(db / "memory.db")
    conn.execute(
        "CREATE TABLE snippet_stats (entry_key TEXT, executions INTEGER, "
        "successes INTEGER, trust_level TEXT, sig_hash TEXT)"
    )
    conn.executemany(
        "INSERT INTO snippet_stats VALUES (?, ?, ?, ?, 'sig')", rows
    )
    conn.commit()
    conn.close()


class HarvestCandidatesTest(unittest.TestCase):
    def test_missing_db_is_empty_list_never_an_error(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(pc.harvest_candidates(d), [])

    def test_provisional_climbers_surface_ordered_with_the_next_command(self):
        with tempfile.TemporaryDirectory() as d:
            _db(Path(d), [
                ("snippet:quente", 7, 7, "provisional"),
                ("snippet:frio-baixa-exec", 2, 2, "provisional"),
                ("snippet:ruim-taxa", 9, 4, "provisional"),
                ("snippet:ja-trusted", 200, 200, "trusted"),
            ])
            got = pc.harvest_candidates(d)
        self.assertEqual([c["key"] for c in got], ["snippet:quente"],
                         "só o provisional quente, exec≥3, taxa≥0.9")
        self.assertEqual(got[0]["success_rate"], 1.0)
        self.assertIn("harvest", got[0]["cmd"])

    def test_limit_and_corrupt_db_fail_open(self):
        with tempfile.TemporaryDirectory() as d:
            db = Path(d) / ".claude" / "touring"
            db.mkdir(parents=True)
            (db / "memory.db").write_text("not a sqlite db")
            self.assertEqual(pc.harvest_candidates(d), [])

    def test_nested_cwd_walks_up_to_the_project_marker(self):
        """The cross-audit finding (2026-09-23): answering [] from a nested cwd
        is the `count is cwd-sensitive` defect — the function must climb to the
        project marker like the daemon does, never measure an empty db silently."""
        with tempfile.TemporaryDirectory() as d:
            _db(Path(d), [("snippet:quente", 7, 7, "provisional")])
            nested = Path(d) / "a" / "b" / "c"
            nested.mkdir(parents=True)
            got = pc.harvest_candidates(nested)
        self.assertEqual([c["key"] for c in got], ["snippet:quente"],
                         "a nested cwd resolves the project's ladder by walking up")

    def test_the_project_root_wins_over_a_nearer_stray_db(self):
        """The E2E caught this (2026-09-23): `crates/.claude/touring/memory.db`
        is a lost, 0-entry db that shadowed the real ladder one level up. A
        project ROOT (.git / .touring pin) names the db under IT; a stray
        .claude in between never shadows it."""
        with tempfile.TemporaryDirectory() as d:
            _db(Path(d), [("snippet:real", 7, 7, "provisional")])
            (Path(d) / ".git").mkdir()                       # the project root marker
            stray = Path(d) / "crates"
            _db(stray, [])                                   # the lost, empty shadow db
            nested = stray / "touring-cli" / "src"
            nested.mkdir(parents=True)
            got = pc.harvest_candidates(nested)
        self.assertEqual([c["key"] for c in got], ["snippet:real"],
                         "the marker root's ladder wins over the nearer stray")


if __name__ == "__main__":
    sys.exit(unittest.main())
