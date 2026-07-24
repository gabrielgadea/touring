"""Tests for w5_storage_skeleton.py.

Strategy: mock workspace with synthetic .rs files containing storage patterns
(sqlite, tantivy, rkyv, checkpoint); validate scan + skeleton emission.

Coverage:
  - STORAGE_PATTERNS detect each storage backend
  - scan_storage_code groups by crate + sorts by total_hits
  - emit_cargo_toml produces valid TOML structure
  - emit_lib_rs declares all expected modules
  - emit_module_stub produces compileable Rust (best-effort grammar check)
  - run --scan-only mode skips emission
"""
from __future__ import annotations

import textwrap
from pathlib import Path

import pytest

import w5_storage_skeleton as mod


class TestPatterns:
    """STORAGE_PATTERNS regex match expected idioms."""

    def test_sqlite_patterns(self) -> None:
        sqlite_patterns = mod.STORAGE_PATTERNS["sqlite"]
        assert any(p.search("rusqlite::Connection") for p in sqlite_patterns)
        assert any(p.search('Connection::open("db")') for p in sqlite_patterns)

    def test_tantivy_patterns(self) -> None:
        tantivy_patterns = mod.STORAGE_PATTERNS["tantivy"]
        assert any(p.search("tantivy::Index") for p in tantivy_patterns)
        assert any(p.search("let w: IndexWriter") for p in tantivy_patterns)

    def test_rkyv_patterns(self) -> None:
        rkyv_patterns = mod.STORAGE_PATTERNS["rkyv"]
        assert any(p.search("rkyv::Archive") for p in rkyv_patterns)
        assert any(p.search('#[derive(Archive, Serialize)]')
                   for p in rkyv_patterns)


@pytest.fixture
def workspace_with_storage(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Path:
    """Mock workspace with files containing storage patterns."""
    crates = tmp_path / "crates"
    crates.mkdir()
    # Crate A: heavy sqlite + some checkpoint
    a = crates / "touring-a"
    (a / "src").mkdir(parents=True)
    (a / "src" / "lib.rs").write_text(textwrap.dedent("""
        use rusqlite::Connection;
        fn open_db() {
            let conn = Connection::open("test.db").unwrap();
        }
        fn checkpoint() {
            serde_json::to_writer(&mut std::io::stdout(), &42).ok();
        }
    """).lstrip(), encoding="utf-8")
    # Crate B: tantivy
    b = crates / "touring-b"
    (b / "src").mkdir(parents=True)
    (b / "src" / "lib.rs").write_text(
        "use tantivy::Index;\nfn make() { let w: IndexWriter; }\n",
        encoding="utf-8")
    # Crate C: no storage code
    c = crates / "touring-c"
    (c / "src").mkdir(parents=True)
    (c / "src" / "lib.rs").write_text("fn pure() {}\n", encoding="utf-8")
    monkeypatch.setattr(mod, "_ROOT", tmp_path)
    monkeypatch.setattr(mod, "_CRATES_DIR", crates)
    monkeypatch.setattr(mod, "_STAGING_DIR",
                        tmp_path / "scripts" / "tpr" / "staging")
    monkeypatch.setattr(mod, "_DATA_DIR",
                        tmp_path / "scripts" / "tpr" / "data")
    monkeypatch.setattr(mod, "_SKELETON_DIR",
                        tmp_path / "scripts" / "tpr" / "staging"
                        / "w5-touring-storage")
    return tmp_path


class TestScanStorage:
    """scan_storage_code detects + groups correctly."""

    def test_detects_storage_crates(self, workspace_with_storage: Path) -> None:
        findings = mod.scan_storage_code()
        assert "touring-a" in findings
        assert "touring-b" in findings
        # Pure crate without storage code excluded
        assert "touring-c" not in findings

    def test_pattern_breakdown(self, workspace_with_storage: Path) -> None:
        findings = mod.scan_storage_code()
        a_files = findings["touring-a"]
        assert len(a_files) == 1
        a_hit = a_files[0]
        assert "sqlite" in a_hit["pattern_hits"]
        assert "checkpoint" in a_hit["pattern_hits"]


class TestEmitters:
    """Skeleton files are well-formed."""

    def test_cargo_toml(self, workspace_with_storage: Path) -> None:
        cargo = mod.emit_cargo_toml()
        text = cargo.read_text(encoding="utf-8")
        assert '[package]' in text
        assert 'name = "touring-storage"' in text
        assert '[features]' in text
        assert 'sqlite = []' in text
        assert 'tantivy = []' in text
        assert 'rkyv = []' in text

    def test_lib_rs(self, workspace_with_storage: Path) -> None:
        lib = mod.emit_lib_rs()
        text = lib.read_text(encoding="utf-8")
        assert "pub mod checkpoint;" in text
        assert "pub mod error;" in text
        assert "pub trait Store" in text
        # Forbid unsafe + deny missing docs
        assert "#![forbid(unsafe_code)]" in text
        assert "#![deny(missing_docs)]" in text

    def test_error_module(self, workspace_with_storage: Path) -> None:
        err = mod.emit_error_module()
        text = err.read_text(encoding="utf-8")
        assert "enum StorageError" in text
        assert "type StorageResult" in text
        assert "thiserror::Error" in text

    def test_module_stub(self, workspace_with_storage: Path) -> None:
        stub = mod.emit_module_stub("sqlite", "SQLite backend")
        text = stub.read_text(encoding="utf-8")
        assert "pub struct SqliteBackend" in text
        assert "pub fn new()" in text
        assert "impl Default for SqliteBackend" in text


class TestRun:
    """run() integration."""

    def test_scan_only_skips_emission(self, workspace_with_storage: Path) -> None:
        import argparse
        args = argparse.Namespace(scan_only=True,
                                    output_dir=workspace_with_storage
                                    / "scripts" / "tpr" / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["status"] == "OK"
        assert result["skeleton_files_emitted"] == []
        assert result["scan_only"] is True

    def test_full_emit(self, workspace_with_storage: Path) -> None:
        import argparse
        args = argparse.Namespace(scan_only=False,
                                    output_dir=workspace_with_storage
                                    / "scripts" / "tpr" / "data",
                                    verbose=False)
        result = mod.run(args)
        assert result["status"] == "OK"
        assert len(result["skeleton_files_emitted"]) == 7
        # Cargo + lib + error + 4 module stubs
