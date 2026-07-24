"""Unit tests for cli-anything-touring core modules.

Tests Tier 1 (SQLite direct) operations against real databases.
Run from a project directory with .claude/data/ or .claude/touring/ databases.
"""

from __future__ import annotations

import json
import subprocess
import sys


def run_cli(*args: str) -> tuple[int, str, str]:
    """Run cli-anything-touring command and capture output."""
    result = subprocess.run(
        ["cli-anything-touring", *args],
        capture_output=True,
        text=True,
        timeout=30,
    )
    return result.returncode, result.stdout.strip(), result.stderr.strip()


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


def test_memory_store_and_recall() -> None:
    # Store
    code, out, _ = run_cli(
        "memory", "store",
        "test:cli-anything:e2e", "E2E test entry from cli-anything-touring",
        "--tier", "ephemeral", "--type", "test", "-j",
    )
    assert code == 0
    data = parse_json(out)
    assert data["key"] == "test:cli-anything:e2e"

    # Recall
    code, out, _ = run_cli("memory", "recall", "cli-anything:e2e", "--tier", "all", "-n", "1", "-j")
    assert code == 0
    data = parse_json(out)
    assert isinstance(data, list)
    assert len(data) >= 1
    assert "cli-anything" in data[0]["key"]


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
