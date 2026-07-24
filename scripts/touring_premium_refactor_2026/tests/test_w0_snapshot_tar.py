"""Tests for w0_snapshot_tar.py.

Strategy:
  - dry-run mode produces a valid command string and never executes tar
  - SHA-256 helper computes correct digest for known content
  - parser accepts expected flags (--dry-run, --output-dir, --date)
  - exclude/include lists are correctly serialized into command

Fixtures: standard `tmp_path` (no full workspace needed for these scoped tests).

Coverage:
  - happy path: build_parser, compute_sha256, run(dry_run=True)
  - edge case: empty file SHA-256 = e3b0c44...
  - error path: tar binary missing handled by caller
"""
from __future__ import annotations

import hashlib
from pathlib import Path

import pytest

import w0_snapshot_tar as mod


class TestComputeSha256:
    """SHA-256 chunked digest matches hashlib reference."""

    def test_empty_file(self, tmp_path: Path) -> None:
        f = tmp_path / "empty"
        f.write_bytes(b"")
        expected = hashlib.sha256(b"").hexdigest()
        assert mod.compute_sha256(f) == expected
        # The "empty SHA" constant
        assert expected == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

    def test_small_file(self, tmp_path: Path) -> None:
        f = tmp_path / "small"
        content = b"hello world\n"
        f.write_bytes(content)
        assert mod.compute_sha256(f) == hashlib.sha256(content).hexdigest()

    def test_large_file_chunked(self, tmp_path: Path) -> None:
        """File larger than chunk_size (1<<16=64KB) processed correctly."""
        f = tmp_path / "large"
        # 200KB of random-ish bytes
        content = bytes(range(256)) * 800
        f.write_bytes(content)
        assert mod.compute_sha256(f) == hashlib.sha256(content).hexdigest()


class TestParser:
    """CLI parser accepts and parses required flags."""

    def test_defaults(self) -> None:
        args = mod.build_parser().parse_args([])
        assert args.dry_run is False
        assert args.verbose is False
        # Date defaults to today (ISO format YYYY-MM-DD)
        assert len(args.date) == 10
        assert args.date[4] == "-" and args.date[7] == "-"

    def test_dry_run_flag(self) -> None:
        args = mod.build_parser().parse_args(["--dry-run"])
        assert args.dry_run is True

    def test_custom_date(self) -> None:
        args = mod.build_parser().parse_args(["--date", "2026-01-15"])
        assert args.date == "2026-01-15"

    def test_output_dir(self, tmp_path: Path) -> None:
        args = mod.build_parser().parse_args(["--output-dir", str(tmp_path)])
        assert args.output_dir == tmp_path


class TestRunDryMode:
    """run() with dry_run=True returns command without executing."""

    def test_dry_run_returns_command(self, tmp_path: Path) -> None:
        result = mod.run(tmp_path, "2026-01-15", dry_run=True)
        assert result["status"] == "DRY_RUN"
        assert "tar" in result["command"]
        assert "touring-snapshot-pre-refactor-2026-01-15.tar.gz" in result["command"]

    def test_dry_run_includes_excludes_and_paths(self, tmp_path: Path) -> None:
        result = mod.run(tmp_path, "2026-01-15", dry_run=True)
        cmd = result["command"]
        assert "--exclude=target" in cmd
        assert "--exclude=.git" in cmd
        assert "crates" in cmd
        assert "Cargo.toml" in cmd
        assert "Cargo.lock" in cmd

    def test_dry_run_does_not_create_file(self, tmp_path: Path) -> None:
        """dry-run must NOT actually write the tar archive."""
        result = mod.run(tmp_path, "2026-01-15", dry_run=True)
        archive_path = tmp_path / "touring-snapshot-pre-refactor-2026-01-15.tar.gz"
        assert not archive_path.exists()

    def test_output_dir_created(self, tmp_path: Path) -> None:
        """run() must create the output directory if missing."""
        nested = tmp_path / "nested" / "deep"
        assert not nested.exists()
        mod.run(nested, "2026-01-15", dry_run=True)
        assert nested.exists()


class TestExcludePatterns:
    """Exclude patterns cover known noise (target, .git, caches)."""

    def test_exclude_target(self) -> None:
        assert any("target" in p for p in mod.EXCLUDE_PATTERNS)

    def test_exclude_git(self) -> None:
        assert any(".git" in p for p in mod.EXCLUDE_PATTERNS)

    def test_exclude_pycache(self) -> None:
        assert any("__pycache__" in p for p in mod.EXCLUDE_PATTERNS)

    def test_include_paths(self) -> None:
        assert "crates" in mod.INCLUDE_PATHS
        assert "Cargo.toml" in mod.INCLUDE_PATHS
        assert "Cargo.lock" in mod.INCLUDE_PATHS
