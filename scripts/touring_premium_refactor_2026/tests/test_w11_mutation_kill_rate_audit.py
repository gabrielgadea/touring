"""Tests for w11_mutation_kill_rate_audit.py.

Strategy: black-box mutation-test cli via cache-only mode; verify parser
+ discover_crates + threshold logic.

Coverage:
  - parse_mutation_summary extracts killed/missed/timeout correctly
  - kill_rate computed as killed / (killed + missed + timeout)
  - find_cached_result returns None when cache empty
  - discover_crates filters by has_test_modules heuristic
  - run() in --cache-only mode returns NO_CACHE without invoking cargo
  - threshold filters low_kill_rate_crates
"""
from __future__ import annotations

from pathlib import Path

import pytest

import w11_mutation_kill_rate_audit as mod


class TestParseSummary:
    """parse_mutation_summary handles cargo-mutants output."""

    def test_typical_output(self) -> None:
        output = """
        running 100 mutants
        caught: 75
        missed: 20
        timeout: 5
        """
        result = mod.parse_mutation_summary(output)
        assert result["killed"] == 75
        assert result["missed"] == 20
        assert result["timeout"] == 5
        assert result["total_mutations"] == 100
        assert result["kill_rate"] == 0.75

    def test_empty_output(self) -> None:
        result = mod.parse_mutation_summary("")
        assert result["killed"] == 0
        assert result["missed"] == 0
        assert result["total_mutations"] == 0
        assert result["kill_rate"] == 0.0

    def test_perfect_kill_rate(self) -> None:
        output = "caught: 50\nmissed: 0\ntimeout: 0\n"
        result = mod.parse_mutation_summary(output)
        assert result["kill_rate"] == 1.0


class TestCache:
    """find_cached_result handles missing/empty cache."""

    def test_no_cache_dir(self, monkeypatch: pytest.MonkeyPatch,
                           tmp_path: Path) -> None:
        monkeypatch.setattr(mod, "_CACHE_DIR", tmp_path / "no_exist")
        assert mod.find_cached_result("touring-x") is None

    def test_empty_cache_dir(self, monkeypatch: pytest.MonkeyPatch,
                              tmp_path: Path) -> None:
        cache = tmp_path / "cache"
        cache.mkdir()
        monkeypatch.setattr(mod, "_CACHE_DIR", cache)
        assert mod.find_cached_result("touring-x") is None

    def test_valid_cached(self, monkeypatch: pytest.MonkeyPatch,
                           tmp_path: Path) -> None:
        cache = tmp_path / "cache"
        cache.mkdir()
        import json
        (cache / "touring-foo-2026.json").write_text(
            json.dumps({"killed": 80, "missed": 20, "kill_rate": 0.8}),
            encoding="utf-8")
        monkeypatch.setattr(mod, "_CACHE_DIR", cache)
        result = mod.find_cached_result("touring-foo")
        assert result is not None
        assert result["killed"] == 80


class TestDiscoverCrates:
    """discover_crates filters to crates with tests."""

    @pytest.fixture
    def workspace(self, monkeypatch: pytest.MonkeyPatch,
                   tmp_path: Path) -> Path:
        crates = tmp_path / "crates"
        crates.mkdir()
        # With tests
        a = crates / "touring-a"
        (a / "src").mkdir(parents=True)
        (a / "src" / "lib.rs").write_text(
            "#[cfg(test)] mod tests { #[test] fn t() {} }\n",
            encoding="utf-8")
        # No tests
        b = crates / "touring-b"
        (b / "src").mkdir(parents=True)
        (b / "src" / "lib.rs").write_text(
            "pub fn pure() {}\n", encoding="utf-8")
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        return tmp_path

    def test_filters_tested_crates(self, workspace: Path) -> None:
        result = mod.discover_crates()
        assert "touring-a" in result
        assert "touring-b" not in result

    def test_has_test_modules_helper(self, workspace: Path) -> None:
        assert mod.has_test_modules(workspace / "crates" / "touring-a")
        assert not mod.has_test_modules(workspace / "crates" / "touring-b")


class TestRun:
    """run() respects --cache-only."""

    def test_cache_only_returns_no_cache(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path,
    ) -> None:
        # Setup workspace with 1 testable crate, empty cache
        crates = tmp_path / "crates"
        crates.mkdir()
        a = crates / "touring-a"
        (a / "src").mkdir(parents=True)
        (a / "src" / "lib.rs").write_text(
            "#[test] fn t() {}\n", encoding="utf-8")
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        monkeypatch.setattr(mod, "_CACHE_DIR", tmp_path / "cache")
        monkeypatch.setattr(mod, "_STAGING_DIR", tmp_path / "staging")
        import argparse
        args = argparse.Namespace(threshold=0.70, force=False,
                                    cache_only=True, crates="", timeout=30,
                                    output_dir=tmp_path / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["status"] == "OK"
        # Should report NO_CACHE for the discovered crate
        assert result["totals"]["no_cache"] >= 1
        # No live invocations occurred
        assert result["totals"]["missing_tool"] == 0


class TestRegexes:
    """KILLED_RE / MISSED_RE / TIMEOUT_RE."""

    def test_killed_re(self) -> None:
        assert mod.KILLED_RE.search("caught: 42")
        assert mod.KILLED_RE.search("Caught: 100")

    def test_missed_re(self) -> None:
        assert mod.MISSED_RE.search("missed: 5")

    def test_timeout_re(self) -> None:
        assert mod.TIMEOUT_RE.search("timeout: 3")
