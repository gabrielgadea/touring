"""`gotcha stats`/`gotcha list` never report a fabricated prevented count (C7 of the ECC analysis, 17/10/2026).

No writer increments `gotchas.prevented_errors` (the Rust side declares `FileKnowledgeDB::PREVENTED_MEASURED = false`),
so the column is 0 by default and SUM() over it is a zero nobody measured.  The harness reports null plus the reason.
"""
from __future__ import annotations

import sqlite3
import sys
from contextlib import contextmanager
from pathlib import Path

import pytest
from typer.testing import CliRunner

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # the harness package, imported right below
from cli_anything.touring import touring_cli
from cli_anything.touring.core import gotcha


@pytest.fixture
def db(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    path = tmp_path / "knowledge.db"

    @contextmanager
    def connect(_name: str, *, readonly: bool = True):
        conn = sqlite3.connect(path)
        conn.row_factory = sqlite3.Row
        try:
            yield conn
        finally:
            conn.close()

    monkeypatch.setattr(gotcha, "connect", connect)
    gotcha.add("src/a.rs", "watch the lock", "warning")
    gotcha.add("src/b.rs", "watch the cache", "error")
    with connect("") as conn:  # 388.840 hits in the real table, 0 prevented: the shape of the real data
        conn.execute("UPDATE gotchas SET hit_count = 1000")
        conn.commit()
    return path


def test_stats_reports_null_and_the_reason_not_a_zero(db: Path) -> None:
    out = gotcha.stats()
    assert out["total"] == 2 and out["total_hits"] == 2000
    assert out["total_prevented"] is None, out
    assert out["prevented_measured"] is False
    assert "nenhum produtor" in out["prevented_note"]


def test_list_does_not_serve_the_default_zero_as_a_count(db: Path) -> None:
    rows = gotcha.list_all()
    assert len(rows) == 2
    assert all(r["prevented_errors"] is None for r in rows), rows


def test_the_cli_text_says_not_measured(db: Path) -> None:
    runner = CliRunner()
    stats = runner.invoke(touring_cli.app, ["gotcha", "stats"])
    assert stats.exit_code == 0, stats.output
    assert "Errors prevented: não medido" in stats.output, stats.output
    listing = runner.invoke(touring_cli.app, ["gotcha", "list"])
    assert listing.exit_code == 0, listing.output
    assert "prevented=0" not in listing.output and "prevented=não medido" in listing.output, listing.output
