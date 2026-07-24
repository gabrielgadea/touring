"""Tests for w1_audit_dead_code.py.

Strategy: full integration test using mock_workspace fixture. Verifies:
  - count_loc and count_pub heuristics work as expected
  - audit_crate classifies known dead crates correctly
  - --apply mode deletes directories + updates workspace Cargo.toml
  - report serialization is complete

Fixtures: mock_workspace (3 crates: foo/empty/archive).
"""
from __future__ import annotations

import json
from pathlib import Path

import pytest

import w1_audit_dead_code as mod


class TestCountHelpers:
    """count_loc + count_pub heuristics."""

    def test_count_loc_empty(self, tmp_path: Path) -> None:
        d = tmp_path / "empty"
        d.mkdir()
        (d / "lib.rs").write_text("", encoding="utf-8")
        assert mod.count_loc(d) == 0

    def test_count_loc_nonempty(self, tmp_path: Path) -> None:
        d = tmp_path / "with_content"
        d.mkdir()
        (d / "lib.rs").write_text("fn foo() {}\nfn bar() {}\n", encoding="utf-8")
        assert mod.count_loc(d) == 2

    def test_count_pub_basic(self, tmp_path: Path) -> None:
        d = tmp_path / "pubs"
        d.mkdir()
        (d / "lib.rs").write_text(
            "pub fn a() {}\npub struct B;\nfn private() {}\n", encoding="utf-8")
        assert mod.count_pub(d) == 2

    def test_count_pub_no_files(self, tmp_path: Path) -> None:
        d = tmp_path / "no_rs"
        d.mkdir()
        assert mod.count_pub(d) == 0


class TestAuditCrate:
    """audit_crate classifies workspace members correctly."""

    def test_audit_real_crate(self, patched_root, mock_workspace: Path) -> None:
        patched_root("w1_audit_dead_code")
        result = mod.audit_crate("touring-foo")
        assert result["exists"] is True
        assert result["loc_src"] > 0
        assert result["pub_count"] >= 3  # add, Foo, Bar
        assert result["candidate_for_deletion"] is False

    def test_audit_empty_crate(self, patched_root, mock_workspace: Path) -> None:
        patched_root("w1_audit_dead_code")
        result = mod.audit_crate("touring-empty")
        assert result["exists"] is True
        assert result["loc_src"] == 0
        assert result["pub_count"] == 0
        assert result["candidate_for_deletion"] is True

    def test_audit_archive_crate(self, patched_root, mock_workspace: Path) -> None:
        patched_root("w1_audit_dead_code")
        result = mod.audit_crate("touring-archive")
        assert result["exists"] is True
        assert result["has_architecture_md"] is True
        # Has ARCHITECTURE.md and pub_count == 0 → candidate
        assert result["candidate_for_deletion"] is True

    def test_audit_missing_crate(self, patched_root, mock_workspace: Path) -> None:
        patched_root("w1_audit_dead_code")
        result = mod.audit_crate("touring-nonexistent")
        assert result["exists"] is False


class TestRunReportOnly:
    """run() without --apply reports without deleting."""

    def test_report_default_known_dead_list(
        self, patched_root, mock_workspace: Path,
    ) -> None:
        patched_root("w1_audit_dead_code")
        out_dir = mock_workspace / "docs" / "baselines"
        # Use our 3 mock crates (override KNOWN_DEAD)
        result = mod.run(apply=False,
                         crate_list=["touring-foo", "touring-empty", "touring-archive"],
                         output_dir=out_dir)
        assert result["total_audited"] == 3
        assert result["candidates_for_deletion"] == 2  # empty + archive
        assert result["apply"] is False
        assert result["actions"] == []
        # Crates still exist (no apply)
        assert (mock_workspace / "crates" / "touring-empty").exists()

    def test_report_written_to_disk(
        self, patched_root, mock_workspace: Path,
    ) -> None:
        patched_root("w1_audit_dead_code")
        out_dir = mock_workspace / "docs" / "baselines"
        mod.run(apply=False, crate_list=["touring-foo"], output_dir=out_dir)
        files = list(out_dir.glob("dead-code-report-*.json"))
        assert len(files) == 1
        data = json.loads(files[0].read_text(encoding="utf-8"))
        assert data["script"] == "w1_audit_dead_code"
        assert data["wave"] == "W1"


class TestApplyMode:
    """run(apply=True) actually deletes directories + updates Cargo.toml."""

    def test_apply_deletes_crate(
        self, patched_root, mock_workspace: Path,
    ) -> None:
        patched_root("w1_audit_dead_code")
        out_dir = mock_workspace / "docs" / "baselines"
        empty_dir = mock_workspace / "crates" / "touring-empty"
        assert empty_dir.exists()
        result = mod.run(apply=True, crate_list=["touring-empty"],
                         output_dir=out_dir)
        assert not empty_dir.exists()
        assert any(a["status"] == "DELETED" for a in result["actions"])

    def test_apply_updates_workspace_cargo(
        self, patched_root, mock_workspace: Path,
    ) -> None:
        patched_root("w1_audit_dead_code")
        out_dir = mock_workspace / "docs" / "baselines"
        cargo_before = (mock_workspace / "Cargo.toml").read_text(encoding="utf-8")
        assert "touring-empty" in cargo_before
        mod.run(apply=True, crate_list=["touring-empty"], output_dir=out_dir)
        cargo_after = (mock_workspace / "Cargo.toml").read_text(encoding="utf-8")
        assert "touring-empty" not in cargo_after
        # Other members preserved
        assert "touring-foo" in cargo_after
        assert "touring-archive" in cargo_after

    def test_apply_idempotent_on_already_gone(
        self, patched_root, mock_workspace: Path,
    ) -> None:
        patched_root("w1_audit_dead_code")
        out_dir = mock_workspace / "docs" / "baselines"
        # First removal
        mod.run(apply=True, crate_list=["touring-empty"], output_dir=out_dir)
        # Second removal of the same crate → action shows already gone OR skipped
        result = mod.run(apply=True, crate_list=["touring-empty"], output_dir=out_dir)
        # Crate doesn't exist anymore — audit returns exists=False, no candidate
        assert result["candidates_for_deletion"] == 0


class TestKnownDeadList:
    """KNOWN_DEAD list contains the 4 forensically identified crates."""

    def test_known_dead_size(self) -> None:
        assert len(mod.KNOWN_DEAD) == 4

    def test_known_dead_contents(self) -> None:
        expected = {"touring-semantic-spike", "touring-wasm-client",
                    "touring-wasm-common", "touring-wasm-server"}
        assert set(mod.KNOWN_DEAD.keys()) == expected
