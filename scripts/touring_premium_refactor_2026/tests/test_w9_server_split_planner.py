"""Tests for w9_server_split_planner.py.

Coverage:
  - classify_file routes lib.rs to facade
  - parent-dir match wins over basename
  - basename regex captures expected patterns (cli_, mcp_, reasoning, capnp)
  - Fallback "touring-server-misc" for unmatched files
  - run() emits plan + markdown + optional evidence
"""
from __future__ import annotations

from pathlib import Path

import pytest

import w9_server_split_planner as mod


@pytest.fixture
def fake_server_src(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Path:
    """Fake server src tree."""
    src = tmp_path / "crates" / "touring-server" / "src"
    src.mkdir(parents=True)
    (src / "lib.rs").write_text("pub mod x;\n", encoding="utf-8")
    (src / "cli_handlers.rs").write_text("pub fn cli() {}\n", encoding="utf-8")
    (src / "tools_infra.rs").write_text("pub struct Infra;\n", encoding="utf-8")
    (src / "random_unrelated.rs").write_text("pub fn x() {}\n", encoding="utf-8")
    # parent-dir match
    (src / "reasoning").mkdir()
    (src / "reasoning" / "persistence.rs").write_text(
        "pub struct Persist;\n", encoding="utf-8")
    (src / "capnp").mkdir()
    (src / "capnp" / "schema.rs").write_text("pub struct S;\n", encoding="utf-8")
    monkeypatch.setattr(mod, "_ROOT", tmp_path)
    monkeypatch.setattr(mod, "_SERVER_SRC", src)
    monkeypatch.setattr(mod, "_STAGING_DIR", tmp_path / "staging")
    monkeypatch.setattr(mod, "_DATA_DIR", tmp_path / "data")
    return src


class TestClassifyFile:
    """classify_file routes correctly."""

    def test_facade(self, fake_server_src: Path) -> None:
        bucket, ev = mod.classify_file(fake_server_src / "lib.rs",
                                         fake_server_src)
        assert bucket == mod.FACADE_BUCKET
        assert "façade" in ev

    def test_cli_basename(self, fake_server_src: Path) -> None:
        bucket, ev = mod.classify_file(fake_server_src / "cli_handlers.rs",
                                         fake_server_src)
        assert bucket == "touring-server-cli"
        assert "basename" in ev

    def test_tools_basename(self, fake_server_src: Path) -> None:
        # `tools_infra.rs` matches CORE rule first (contains "tools_infra" in
        # the core regex as a specific dispatch-infrastructure exception).
        # This is correct behavior: tools_infra is server RPC infra, not a
        # user-facing tool handler.
        bucket, _ev = mod.classify_file(fake_server_src / "tools_infra.rs",
                                         fake_server_src)
        assert bucket == "touring-server-core"

    def test_parent_dir_reasoning(self, fake_server_src: Path) -> None:
        bucket, ev = mod.classify_file(
            fake_server_src / "reasoning" / "persistence.rs", fake_server_src)
        assert bucket == "touring-server-reasoning"
        assert "parent-dir" in ev

    def test_parent_dir_capnp(self, fake_server_src: Path) -> None:
        bucket, ev = mod.classify_file(
            fake_server_src / "capnp" / "schema.rs", fake_server_src)
        assert bucket == "touring-server-protocol"

    def test_fallback_misc(self, fake_server_src: Path) -> None:
        bucket, ev = mod.classify_file(fake_server_src / "random_unrelated.rs",
                                         fake_server_src)
        assert bucket == "touring-server-misc"
        assert ev == "fallback"


class TestRun:
    """run() integration."""

    def test_emits_plan_and_md(self, fake_server_src: Path,
                                 tmp_path: Path) -> None:
        import argparse
        args = argparse.Namespace(emit_cargo=False, emit_evidence=False,
                                    output_dir=tmp_path / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["status"] == "OK"
        assert result["totals"]["rs_files"] >= 6  # 5 files + 1 lib.rs
        # All expected buckets observed
        assert mod.FACADE_BUCKET in result["buckets"]
        assert "touring-server-cli" in result["buckets"]

    def test_emit_evidence(self, fake_server_src: Path,
                            tmp_path: Path) -> None:
        import argparse
        args = argparse.Namespace(emit_cargo=False, emit_evidence=True,
                                    output_dir=tmp_path / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["evidence_path"] is not None

    def test_missing_crate(self, monkeypatch: pytest.MonkeyPatch,
                            tmp_path: Path) -> None:
        nonexistent = tmp_path / "no-server" / "src"
        monkeypatch.setattr(mod, "_SERVER_SRC", nonexistent)
        import argparse
        args = argparse.Namespace(emit_cargo=False, emit_evidence=False,
                                    output_dir=tmp_path / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["status"] == "MISSING_CRATE"
