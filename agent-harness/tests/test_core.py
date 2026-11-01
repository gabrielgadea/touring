"""Unit tests for cli-anything-touring core modules.

Tests Tier 1 (SQLite direct) operations against real databases.
Run from a project directory with .claude/data/ or .claude/touring/ databases.
"""

from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import sys
import uuid
from pathlib import Path

HARNESS = Path(__file__).resolve().parents[1]
PROJECT = HARNESS.parent  # the touring checkout: its `.claude/touring/` holds the real stores
sys.path.insert(0, str(HARNESS))


def run_cli(*args: str, cwd: Path = PROJECT) -> tuple[int, str, str]:
    """Run the harness CLI as a module of THIS checkout, from the project root.

    It called the `cli-anything-touring` executable, which nothing installs: all 19 tests died with
    FileNotFoundError, so the suite never ran and a regression from 02/10/2026 (`KeyError: 'v5'` in
    `index status`) went unseen (cross-audit 04/10/2026, X3 and X15). The working directory is pinned
    because the stores are found by walking up from it.
    """
    env = {**os.environ, "PYTHONPATH": os.pathsep.join(p for p in (str(HARNESS), os.environ.get("PYTHONPATH")) if p)}
    result = subprocess.run(
        [sys.executable, "-m", "cli_anything.touring.touring_cli", *args],
        capture_output=True,
        text=True,
        timeout=30,
        cwd=cwd,
        env=env,
    )
    return result.returncode, result.stdout.strip(), result.stderr.strip()


def test_every_detected_schema_has_a_file_column(tmp_path: Path) -> None:
    """The F2.1 whitelist knew `legacy` and `current`; the detector answers `legacy` or `v5`, so
    `index status` raised `KeyError: 'v5'` on every current store (cross-audit 04/10/2026, X3)."""
    from cli_anything.touring.core import index as ix

    for ddl, expected in [
        ("CREATE TABLE symbols (name, kind, file, line, language)", "file"),
        ("CREATE TABLE symbols (name, file_path, line)", "file_path"),
    ]:
        conn = sqlite3.connect(tmp_path / f"{expected}.db")
        conn.execute(ddl)
        assert ix.FILE_COLUMN[ix._detect_schema(conn)] == expected
        conn.close()


def parse_json(stdout: str) -> dict | list:
    """Parse JSON from stdout."""
    return json.loads(stdout)


# ─── Health ───────────────────────────────────────────────────

def test_version() -> None:
    code, out, _ = run_cli("version")
    assert code == 0
    assert "cli-anything-touring v" in out


def test_health_json() -> None:
    code, out, _ = run_cli("health", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, dict)
    assert "healthy" in data
    assert "databases" in data


# ─── Index ────────────────────────────────────────────────────

def test_index_search_json() -> None:
    code, out, _ = run_cli("index", "search", "Pipeline", "-n", "3", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, list)
    assert len(data) <= 3
    if data:
        assert "name" in data[0]
        assert "file" in data[0]


def test_index_search_text() -> None:
    code, out, _ = run_cli("index", "search", "Pipeline", "-n", "3")
    assert code == 0
    assert "Pipeline" in out or "Found" in out


def test_index_status_json() -> None:
    code, out, _ = run_cli("index", "status", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, dict)
    assert data["total_symbols"] > 50000  # Expect 60k+


def test_index_find_json() -> None:
    code, out, _ = run_cli("index", "find", "Pipeline", "-n", "5", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, list)


def test_index_files_json() -> None:
    code, out, _ = run_cli("index", "files", "-n", "5", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, list)
    if data:
        assert "file" in data[0]
        assert "symbol_count" in data[0]


# ─── Memory ───────────────────────────────────────────────────

def test_memory_stats_json() -> None:
    code, out, _ = run_cli("memory", "stats", "-j")
    assert code == 0
    data = parse_json(out)
    assert data["total_entries"] > 100  # Expect at least some entries
    assert "by_tier" in data


def test_memory_recall_json() -> None:
    code, out, _ = run_cli("memory", "recall", "insight", "--tier", "all", "-n", "3", "--type", "insight", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, list)


def test_memory_list_json() -> None:
    code, out, _ = run_cli("memory", "list", "--tier", "reference", "-n", "5", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, list)
    assert len(data) <= 5


def _scratch_project(tmp_path: Path) -> Path:
    """A project of its own holding an EMPTY `memory_entries` with the live table's schema.

    Once the suite ran (X15), this test wrote its row into the real `memory.db` of the checkout on
    every run (blind review of the 04/10 fixes). The stores are found by walking up from the cwd to a
    `CLAUDE.md`, so a scratch root with one is where the harness writes. Triggers stay out: they feed
    the FTS tables, which the harness never reads.
    """
    live = sqlite3.connect(f"file:{PROJECT / '.claude' / 'touring' / 'memory.db'}?mode=ro", uri=True)
    schema = [sql for (sql,) in live.execute(
        "SELECT sql FROM sqlite_master WHERE tbl_name = 'memory_entries' "
        "AND type IN ('table', 'index') AND sql IS NOT NULL ORDER BY type DESC")]
    live.close()
    (tmp_path / "CLAUDE.md").write_text("# scratch\n", encoding="utf-8")
    store = tmp_path / ".claude" / "touring"
    store.mkdir(parents=True)
    with sqlite3.connect(store / "memory.db") as conn:
        for sql in schema:
            conn.execute(sql)
    return tmp_path


def _live_rows(key: str) -> int:
    live = sqlite3.connect(f"file:{PROJECT / '.claude' / 'touring' / 'memory.db'}?mode=ro", uri=True)
    try:
        return live.execute("SELECT COUNT(*) FROM memory_entries WHERE key = ?", (key,)).fetchone()[0]
    finally:
        live.close()


def test_memory_store_and_recall(tmp_path: Path) -> None:
    # A key no run ever used: with a fixed key the live row an older run left made the guard below
    # pass while this run wrote there again (an upsert keeps the count at 1).
    key = f"test:cli-anything:e2e:{uuid.uuid4().hex}"
    scratch = _scratch_project(tmp_path)
    live_before = _live_rows(key)
    assert live_before == 0

    code, out, err = run_cli(
        "memory", "store", key, "E2E test entry from cli-anything-touring",
        "--tier", "ephemeral", "--type", "test", "-j", cwd=scratch,
    )
    assert code == 0, err
    assert parse_json(out)["key"] == key

    code, out, err = run_cli("memory", "recall", "cli-anything:e2e", "--tier", "all", "-n", "1", "-j",
                             cwd=scratch)
    assert code == 0, err
    data = parse_json(out)
    assert isinstance(data, list) and data and data[0]["key"] == key

    with sqlite3.connect(scratch / ".claude" / "touring" / "memory.db") as conn:
        assert conn.execute("SELECT COUNT(*) FROM memory_entries WHERE key = ?", (key,)).fetchone()[0] == 1
    assert _live_rows(key) == live_before, "the test must never write into the checkout's live store"


# ─── AST ──────────────────────────────────────────────────────

def test_ast_find_json() -> None:
    code, out, _ = run_cli("ast", "find", "Pipeline", "-n", "3", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, list)


def test_ast_blast_json() -> None:
    code, out, _ = run_cli("ast", "blast", "pipeline.rs", "-j")
    assert code == 0
    data = parse_json(out)
    assert "blast_radius" in data


# ─── Evolution ────────────────────────────────────────────────

def test_evolution_insights_json() -> None:
    code, out, _ = run_cli("evolution", "insights", "-j")
    assert code == 0
    data = parse_json(out)
    assert "wilson_top" in data
    assert "drift_metrics" in data
    assert "knowledge_stats" in data


def test_evolution_drift_json() -> None:
    code, out, _ = run_cli("evolution", "drift", "-j")
    assert code == 0
    data = parse_json(out)
    assert "metrics" in data
    assert "total_tracked" in data


def test_evolution_tools_json() -> None:
    code, out, _ = run_cli("evolution", "tools", "-j")
    assert code == 0
    data = parse_json(out)
    assert "top_commands" in data


# ─── Cortex (Tier 2) ─────────────────────────────────────────

def test_cortex_classify_json() -> None:
    code, out, _ = run_cli("cortex", "classify", "fix the bug in parser.rs", "-j")
    assert code == 0
    data = parse_json(out)
    assert "metadata" in data or "cila_level" in str(data)


def test_cortex_pii_json() -> None:
    code, out, _ = run_cli("cortex", "pii", "test text without PII", "-j")
    assert code == 0
    # PII scan may return empty or minimal result for clean text


# ─── JSON determinism (cache-friendliness) ────────────────────

def test_json_sort_keys() -> None:
    """Verify that JSON output has sorted keys (cache-friendly)."""
    code, out, _ = run_cli("index", "status", "-j")
    assert code == 0
    data = parse_json(out)
    # Re-serialize with sort_keys and compare
    canonical = json.dumps(data, sort_keys=True)
    reserialized = json.dumps(json.loads(out), sort_keys=True)
    assert canonical == reserialized


if __name__ == "__main__":
    import traceback

    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    passed = 0
    failed = 0

    for test_fn in tests:
        name = test_fn.__name__
        try:
            test_fn()
            print(f"  PASS {name}")
            passed += 1
        except Exception as e:
            print(f"  FAIL {name}: {e}")
            traceback.print_exc()
            failed += 1

    total = passed + failed
    print(f"\n{'='*50}")
    print(f"Results: {passed}/{total} PASS, {failed} FAIL")
    sys.exit(1 if failed > 0 else 0)
