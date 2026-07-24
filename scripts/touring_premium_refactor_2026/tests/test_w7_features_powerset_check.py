"""Tests for w7_features_powerset_check.py.

Strategy: synthetic Cargo.toml files + mock subprocess to validate
feature extraction + combination generation. Cargo invocations are
black-boxed via --dry-run mode.

Coverage:
  - extract_features handles [features] section correctly
  - extract_features ignores RESERVED names (default)
  - extract_features ignores empty / missing [features]
  - generate_combinations respects max_cardinality
  - generate_combinations excludes empty sets
  - discover_multi_feature_crates filters by feature count
  - dry-run mode never invokes cargo
"""
from __future__ import annotations

import argparse
import textwrap
from pathlib import Path

import pytest

import w7_features_powerset_check as mod


class TestExtractFeatures:
    """extract_features parses [features] section."""

    def test_basic(self, tmp_path: Path) -> None:
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(textwrap.dedent("""
            [package]
            name = "x"
            [features]
            default = ["foo"]
            foo = []
            bar = ["foo"]
            baz = ["foo", "bar"]
        """).lstrip(), encoding="utf-8")
        features = mod.extract_features(cargo)
        assert "foo" in features
        assert "bar" in features
        assert "baz" in features
        assert "default" not in features  # reserved

    def test_missing_section(self, tmp_path: Path) -> None:
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text("[package]\nname = \"x\"\n", encoding="utf-8")
        assert mod.extract_features(cargo) == []

    def test_missing_file(self, tmp_path: Path) -> None:
        assert mod.extract_features(tmp_path / "nonexistent.toml") == []

    def test_section_ends_at_next_header(self, tmp_path: Path) -> None:
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(textwrap.dedent("""
            [features]
            real = []
            another = []

            [dependencies]
            serde = "1.0"
        """).lstrip(), encoding="utf-8")
        features = mod.extract_features(cargo)
        assert "real" in features
        assert "another" in features
        assert "serde" not in features


class TestGenerateCombinations:
    """generate_combinations enumerates subsets correctly."""

    def test_single_feature(self) -> None:
        result = mod.generate_combinations(["a"], max_cardinality=3)
        assert result == [["a"]]

    def test_two_features_card_1(self) -> None:
        result = mod.generate_combinations(["a", "b"], max_cardinality=1)
        assert sorted(result) == [["a"], ["b"]]

    def test_two_features_card_2(self) -> None:
        result = mod.generate_combinations(["a", "b"], max_cardinality=2)
        # 2 singletons + 1 pair = 3 combos
        assert len(result) == 3
        assert ["a", "b"] in result

    def test_three_features_card_3(self) -> None:
        result = mod.generate_combinations(["a", "b", "c"], max_cardinality=3)
        # C(3,1) + C(3,2) + C(3,3) = 3 + 3 + 1 = 7
        assert len(result) == 7

    def test_cardinality_above_feature_count(self) -> None:
        # If max_cardinality > len(features), don't error
        result = mod.generate_combinations(["a", "b"], max_cardinality=10)
        assert len(result) == 3  # only 2 features → 2+1 combos


class TestDiscoverMultiFeature:
    """discover_multi_feature_crates filters correctly."""

    @pytest.fixture
    def workspace(self, monkeypatch: pytest.MonkeyPatch,
                   tmp_path: Path) -> Path:
        crates = tmp_path / "crates"
        crates.mkdir()
        # Crate A: 3 features (qualifies)
        a = crates / "touring-a"
        (a / "src").mkdir(parents=True)
        (a / "Cargo.toml").write_text(textwrap.dedent("""
            [package]
            name = "touring-a"
            [features]
            default = []
            f1 = []
            f2 = []
            f3 = []
        """).lstrip(), encoding="utf-8")
        # Crate B: 1 feature (rejected)
        b = crates / "touring-b"
        (b / "src").mkdir(parents=True)
        (b / "Cargo.toml").write_text(textwrap.dedent("""
            [package]
            name = "touring-b"
            [features]
            single = []
        """).lstrip(), encoding="utf-8")
        # Crate C: 0 features (rejected)
        c = crates / "touring-c"
        (c / "src").mkdir(parents=True)
        (c / "Cargo.toml").write_text(
            '[package]\nname = "touring-c"\n', encoding="utf-8")
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        monkeypatch.setattr(mod, "_CRATES_DIR", crates)
        return tmp_path

    def test_filters_by_feature_count(self, workspace: Path) -> None:
        result = mod.discover_multi_feature_crates()
        assert "touring-a" in result
        assert "touring-b" not in result  # only 1 feature
        assert "touring-c" not in result  # 0 features

    def test_returns_feature_lists(self, workspace: Path) -> None:
        result = mod.discover_multi_feature_crates()
        assert set(result["touring-a"]) == {"f1", "f2", "f3"}


class TestRunDryMode:
    """run() with --dry-run never invokes cargo."""

    def test_dry_run_status(self, monkeypatch: pytest.MonkeyPatch,
                              tmp_path: Path) -> None:
        crates = tmp_path / "crates"
        crates.mkdir()
        a = crates / "touring-a"
        (a / "src").mkdir(parents=True)
        (a / "Cargo.toml").write_text(textwrap.dedent("""
            [package]
            name = "touring-a"
            [features]
            f1 = []
            f2 = []
        """).lstrip(), encoding="utf-8")
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        monkeypatch.setattr(mod, "_CRATES_DIR", crates)
        monkeypatch.setattr(mod, "_STAGING_DIR", tmp_path / "staging")
        args = argparse.Namespace(crate="", max_cardinality=2, dry_run=True,
                                    timeout=30,
                                    output_dir=tmp_path / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["status"] == "OK"
        # All results must be DRY_RUN
        for r in result["results"]:
            assert r.get("status") == "DRY_RUN"
        assert result["totals"]["dry_run"] > 0
        assert result["totals"]["ok"] == 0
        assert result["totals"]["fail"] == 0


class TestRunCargoCheck:
    """run_cargo_check handles MISSING_TOOL gracefully."""

    def test_missing_cargo(self, monkeypatch: pytest.MonkeyPatch,
                            tmp_path: Path) -> None:
        # Simulate missing cargo by setting PATH to empty
        monkeypatch.setenv("PATH", "/nonexistent")
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        result = mod.run_cargo_check("touring-x", ["foo"],
                                       timeout=5, dry_run=False)
        # Should return MISSING_TOOL or FAIL gracefully (not raise)
        assert result["status"] in ("MISSING_TOOL", "FAIL", "TIMEOUT")
