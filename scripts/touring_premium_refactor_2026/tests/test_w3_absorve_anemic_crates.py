"""Tests for w3_absorve_anemic_crates.py.

Strategy: mock workspace with mix of anemic + healthy crates, validate
audit_crate + is_anemic + propose_target heuristics.

Coverage:
  - count_src_loc/count_pub_items match expected values
  - build_dep_graph extracts dependencies correctly
  - is_anemic respects all 4 thresholds (loc/pub/fan-in/allow-list)
  - propose_target picks the right consumer
  - ALLOW_LIST guards intentionally-anemic crates
"""
from __future__ import annotations

import textwrap
from pathlib import Path

import pytest

import w3_absorve_anemic_crates as mod


@pytest.fixture
def workspace_mixed(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Path:
    """Mock workspace: 1 anemic + 1 healthy + 1 allow-listed."""
    crates = tmp_path / "crates"
    crates.mkdir()
    # Anemic crate (small LOC, 1 pub item)
    anemic = crates / "touring-tiny"
    (anemic / "src").mkdir(parents=True)
    (anemic / "Cargo.toml").write_text(
        '[package]\nname = "touring-tiny"\n', encoding="utf-8")
    (anemic / "src" / "lib.rs").write_text(
        "pub fn hello() {}\n", encoding="utf-8")
    # Healthy crate (1000+ LOC, many pubs)
    healthy = crates / "touring-large"
    (healthy / "src").mkdir(parents=True)
    (healthy / "Cargo.toml").write_text(
        '[package]\nname = "touring-large"\n[dependencies]\n'
        'touring-tiny = { path = "../touring-tiny" }\n',
        encoding="utf-8")
    body = "pub fn a() {}\n" * 100 + "pub struct B;\n" * 50
    (healthy / "src" / "lib.rs").write_text(body, encoding="utf-8")
    # Allow-listed
    allowed = crates / "touring-types"
    (allowed / "src").mkdir(parents=True)
    (allowed / "Cargo.toml").write_text(
        '[package]\nname = "touring-types"\n', encoding="utf-8")
    (allowed / "src" / "lib.rs").write_text(
        "pub struct A;\n", encoding="utf-8")
    monkeypatch.setattr(mod, "_ROOT", tmp_path)
    monkeypatch.setattr(mod, "_CRATES_DIR", crates)
    monkeypatch.setattr(mod, "_STAGING_DIR",
                        tmp_path / "scripts" / "tpr" / "staging")
    monkeypatch.setattr(mod, "_DATA_DIR",
                        tmp_path / "scripts" / "tpr" / "data")
    return tmp_path


class TestCountHelpers:
    """LOC + pub counters."""

    def test_count_src_loc_basic(self, workspace_mixed: Path) -> None:
        anemic = workspace_mixed / "crates" / "touring-tiny"
        assert mod.count_src_loc(anemic) == 1  # one line

    def test_count_src_loc_missing_src(self, tmp_path: Path) -> None:
        no_src = tmp_path / "no_src"
        no_src.mkdir()
        assert mod.count_src_loc(no_src) == 0

    def test_count_pub_items(self, workspace_mixed: Path) -> None:
        anemic = workspace_mixed / "crates" / "touring-tiny"
        assert mod.count_pub_items(anemic) == 1
        healthy = workspace_mixed / "crates" / "touring-large"
        assert mod.count_pub_items(healthy) == 150  # 100 fns + 50 structs


class TestBuildDepGraph:
    """build_dep_graph parses Cargo.toml dependencies."""

    def test_healthy_depends_on_tiny(self, workspace_mixed: Path) -> None:
        consumers = mod.build_dep_graph()
        assert "touring-large" in consumers.get("touring-tiny", set())

    def test_no_self_deps(self, workspace_mixed: Path) -> None:
        consumers = mod.build_dep_graph()
        # touring-tiny should NOT have itself as a consumer
        assert "touring-tiny" not in consumers.get("touring-tiny", set())


class TestAuditAndAnemia:
    """audit_crate + is_anemic logic."""

    def test_anemic_detected(self, workspace_mixed: Path) -> None:
        consumers = mod.build_dep_graph()
        anemic_path = workspace_mixed / "crates" / "touring-tiny"
        audit = mod.audit_crate(anemic_path, consumers)
        assert audit["src_loc"] < 500
        assert audit["pub_count"] < 10
        # is_anemic with default args
        import argparse
        args = argparse.Namespace(max_loc=500, max_pub=10, max_fan_in=2)
        assert mod.is_anemic(audit, args)

    def test_healthy_not_anemic(self, workspace_mixed: Path) -> None:
        consumers = mod.build_dep_graph()
        healthy_path = workspace_mixed / "crates" / "touring-large"
        audit = mod.audit_crate(healthy_path, consumers)
        assert audit["pub_count"] >= 10
        import argparse
        args = argparse.Namespace(max_loc=500, max_pub=10, max_fan_in=2)
        assert not mod.is_anemic(audit, args)

    def test_allow_list_protected(self, workspace_mixed: Path) -> None:
        consumers = mod.build_dep_graph()
        allowed_path = workspace_mixed / "crates" / "touring-types"
        audit = mod.audit_crate(allowed_path, consumers)
        assert audit["in_allow_list"] is True
        import argparse
        args = argparse.Namespace(max_loc=500, max_pub=10, max_fan_in=2)
        # Even though it's tiny, allow_list protects it
        assert not mod.is_anemic(audit, args)


class TestProposeTarget:
    """propose_target picks first consumer alphabetically."""

    def test_with_consumers(self) -> None:
        audit = {"consumers": ["touring-bbb", "touring-aaa", "touring-ccc"]}
        # Note: consumers list is already sorted by caller
        assert mod.propose_target({"consumers": sorted(audit["consumers"])}) == "touring-aaa"

    def test_no_consumers(self) -> None:
        assert mod.propose_target({"consumers": []}) is None


class TestRun:
    """run() integration."""

    def test_emits_files(self, workspace_mixed: Path) -> None:
        import argparse
        args = argparse.Namespace(max_loc=500, max_pub=10, max_fan_in=2,
                                    output_dir=workspace_mixed / "scripts" / "tpr" / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["status"] == "OK"
        assert result["totals"]["anemic"] >= 1
        # Allow-listed crate should NOT be in anemic list
        anemic_names = {a["crate"] for a in result["anemic_crates"]}
        assert "touring-types" not in anemic_names
        assert "touring-tiny" in anemic_names
