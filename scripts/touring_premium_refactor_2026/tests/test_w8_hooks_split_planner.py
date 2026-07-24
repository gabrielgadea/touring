"""Tests for w8_hooks_split_planner.py (v5 — leaf invariant enforced).

Coverage:
  - classify_file routes lib.rs to facade
  - LEAF_VIOLATORS get relocated correctly
  - SHARED_FILES → SHARED_BUCKET
  - EXPLICIT_MAP entries route to their bucket
  - detect_leaf_violations finds outgoing crate:: refs from shared
  - detect_cycles distinguishes trivial 2-step from real ≥3-step
"""
from __future__ import annotations

import textwrap
from pathlib import Path

import pytest

import w8_hooks_split_planner as mod


@pytest.fixture
def fake_hooks_src(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Path:
    """Fake hooks src tree with files matching various rules."""
    src = tmp_path / "crates" / "touring-hooks" / "src"
    src.mkdir(parents=True)
    (src / "lib.rs").write_text("pub mod x;\n", encoding="utf-8")
    (src / "knowledge.rs").write_text(
        "pub struct DB;\n", encoding="utf-8")
    (src / "errors.rs").write_text(
        "pub enum Err {}\n", encoding="utf-8")
    # leaf violator: file in SHARED_FILES but imports from non-shared
    (src / "task_digest.rs").write_text(
        "use crate::runtime::HookRuntime;\npub struct Digest;\n",
        encoding="utf-8")
    # cli prefix
    (src / "cli_handler.rs").write_text("pub fn handle() {}\n", encoding="utf-8")
    monkeypatch.setattr(mod, "_ROOT", tmp_path)
    monkeypatch.setattr(mod, "_HOOKS_SRC", src)
    monkeypatch.setattr(mod, "_STAGING_DIR", tmp_path / "staging")
    monkeypatch.setattr(mod, "_DATA_DIR", tmp_path / "data")
    return src


class TestClassifyFile:
    """classify_file routes each file to correct bucket."""

    def test_facade_exempt(self, fake_hooks_src: Path) -> None:
        bucket, ev = mod.classify_file(fake_hooks_src / "lib.rs",
                                         fake_hooks_src)
        assert bucket == mod.FACADE_BUCKET
        assert "façade-exempt" in ev

    def test_leaf_violator_relocated(self, fake_hooks_src: Path) -> None:
        bucket, ev = mod.classify_file(fake_hooks_src / "task_digest.rs",
                                         fake_hooks_src)
        # task_digest is in LEAF_VIOLATORS → goes to touring-hooks-tools
        assert bucket == mod.LEAF_VIOLATORS["task_digest.rs"]
        assert "leaf-violator-relocated" in ev

    def test_explicit_map(self, fake_hooks_src: Path) -> None:
        bucket, ev = mod.classify_file(fake_hooks_src / "knowledge.rs",
                                         fake_hooks_src)
        assert bucket == mod.EXPLICIT_MAP["knowledge.rs"]
        assert "explicit-map" in ev

    def test_shared_whitelist(self, fake_hooks_src: Path) -> None:
        bucket, ev = mod.classify_file(fake_hooks_src / "errors.rs",
                                         fake_hooks_src)
        assert bucket == mod.SHARED_BUCKET
        assert ev == "shared-whitelist"

    def test_basename_regex(self, fake_hooks_src: Path) -> None:
        bucket, ev = mod.classify_file(fake_hooks_src / "cli_handler.rs",
                                         fake_hooks_src)
        assert bucket == "touring-hooks-cli"
        assert "basename-substr" in ev


class TestLeafViolations:
    """detect_leaf_violations finds outgoing crate:: refs from SHARED."""

    def test_no_violations_for_pure_shared(self, fake_hooks_src: Path) -> None:
        # errors.rs is shared + has no outgoing deps → no violation
        files = [fake_hooks_src / "errors.rs"]
        file_to_bucket = {files[0]: mod.SHARED_BUCKET}
        mod_to_bucket = {"errors": mod.SHARED_BUCKET}
        violations = mod.detect_leaf_violations(files, file_to_bucket,
                                                  mod_to_bucket)
        assert violations == []

    def test_detect_violation(self, fake_hooks_src: Path) -> None:
        # Add a file that's misclassified as shared but has crate:: refs to core
        violator = fake_hooks_src / "fake_shared.rs"
        violator.write_text(
            "use crate::knowledge::DB;\npub struct Bad;\n",
            encoding="utf-8")
        # Force into SHARED for the test
        files = [violator]
        file_to_bucket = {violator: mod.SHARED_BUCKET}
        mod_to_bucket = {"knowledge": "touring-hooks-core"}
        violations = mod.detect_leaf_violations(files, file_to_bucket,
                                                  mod_to_bucket)
        assert len(violations) == 1
        assert violations[0]["file"].endswith("fake_shared.rs")
        assert violations[0]["violations"][0]["target_module"] == "knowledge"
        assert violations[0]["violations"][0]["target_bucket"] == "touring-hooks-core"


class TestDetectCycles:
    """detect_cycles distinguishes trivial vs real."""

    def test_trivial_two_step(self) -> None:
        # A → B and B → A = trivial 2-step
        edges = {("A", "B"): 5, ("B", "A"): 3}
        result = mod.detect_cycles(edges)
        assert result["trivial_pair_count"] == 1
        assert result["real_cycle_count"] == 0

    def test_real_three_step(self) -> None:
        # A → B → C → A = real 3-step
        edges = {("A", "B"): 1, ("B", "C"): 1, ("C", "A"): 1}
        result = mod.detect_cycles(edges)
        assert result["real_cycle_count"] >= 1
        # Should not double-count as trivial
        assert result["trivial_pair_count"] == 0

    def test_no_cycles_in_dag(self) -> None:
        edges = {("A", "B"): 1, ("B", "C"): 1, ("A", "C"): 1}
        result = mod.detect_cycles(edges)
        assert result["trivial_pair_count"] == 0
        assert result["real_cycle_count"] == 0


class TestRun:
    """run() emits expected outputs."""

    def test_run_emits_files(self, fake_hooks_src: Path,
                              tmp_path: Path) -> None:
        import argparse
        args = argparse.Namespace(emit_cargo=False, emit_evidence=True,
                                    strict_acyclic=False,
                                    output_dir=tmp_path / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["status"] == "OK"
        assert result["version"] == "v5-leaf-enforced"
        # Files emitted
        assert "plan_path" in result
        assert "md_path" in result
        assert result["evidence_path"] is not None
