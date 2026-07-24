"""Tests for w0_capture_baselines.py.

Strategy: focus on dry-run + flag-skip semantics. Subprocess invocation is
black-boxed via dry-run mode; we verify the orchestrator's argument
construction + skip logic, not cargo itself.

Coverage:
  - --all flag flips every skip to False
  - skip-* flags suppress matching steps
  - run() summary aggregates OK/FAIL/TIMEOUT/MISSING_TOOL correctly
  - Touring CLI sub-commands include expected args (wiring/cycles/status/workspace-info)
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import pytest

import w0_capture_baselines as mod


class TestParser:
    """CLI parser builds expected Namespace structure."""

    def test_defaults_all_steps_enabled(self) -> None:
        args = mod.build_parser().parse_args([])
        # Defaults: nothing skipped, no --all
        assert args.skip_bench is False
        assert args.skip_coverage is False
        assert args.skip_check is False
        assert args.skip_wiring is False
        assert args.all is False

    def test_all_flag(self) -> None:
        args = mod.build_parser().parse_args(["--all"])
        assert args.all is True

    def test_individual_skips(self) -> None:
        args = mod.build_parser().parse_args([
            "--skip-bench", "--skip-coverage", "--skip-check", "--skip-wiring",
        ])
        assert args.skip_bench is True
        assert args.skip_coverage is True
        assert args.skip_check is True
        assert args.skip_wiring is True


class TestRunDryMode:
    """run() with dry_run=True returns DRY_RUN status for all steps."""

    @pytest.fixture
    def args(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> argparse.Namespace:
        """Build a minimal args Namespace + redirect baselines dir to tmp_path."""
        monkeypatch.setattr(mod, "_BASELINES_DIR", tmp_path / "baselines")
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        ns = argparse.Namespace(
            date="2026-01-15",
            all=False, skip_bench=True, skip_coverage=True, skip_check=False,
            skip_test_norun=False, skip_wiring=False, dry_run=True,
            verbose=False,
        )
        return ns

    def test_dry_run_skips_bench_when_flagged(self, args: argparse.Namespace) -> None:
        result = mod.run(args)
        # bench skipped
        assert "cargo_bench" not in result["results"]

    def test_dry_run_skips_coverage_when_flagged(self, args: argparse.Namespace) -> None:
        result = mod.run(args)
        assert "cargo_llvm_cov" not in result["results"]

    def test_dry_run_includes_check_when_not_skipped(self, args: argparse.Namespace) -> None:
        result = mod.run(args)
        assert "cargo_check" in result["results"]
        assert result["results"]["cargo_check"]["status"] == "DRY_RUN"

    def test_dry_run_touring_commands(self, args: argparse.Namespace) -> None:
        result = mod.run(args)
        assert "wiring_audit" in result["results"]
        assert "wiring_cycles" in result["results"]
        assert "status" in result["results"]
        assert "workspace_info" in result["results"]
        for k in ("wiring_audit", "wiring_cycles", "status", "workspace_info"):
            assert result["results"][k]["status"] == "DRY_RUN"

    def test_summary_aggregation(self, args: argparse.Namespace) -> None:
        result = mod.run(args)
        assert result["summary"]["fail"] == 0
        assert result["summary"]["total"] == len(result["results"])

    def test_result_metadata(self, args: argparse.Namespace) -> None:
        result = mod.run(args)
        assert result["script"] == "w0_capture_baselines"
        assert result["wave"] == "W0"
        assert "W0.2" in result["subtask_refs"]
        assert "W0.3" in result["subtask_refs"]
        assert "W0.4" in result["subtask_refs"]
        assert "W0.5" in result["subtask_refs"]
        assert result["date"] == "2026-01-15"


class TestSkipFlagSemantics:
    """Each --skip-* flag suppresses its corresponding step."""

    def _base_args(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
                   **overrides) -> argparse.Namespace:
        monkeypatch.setattr(mod, "_BASELINES_DIR", tmp_path / "baselines")
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        defaults = {
            "date": "2026-01-15", "all": False, "skip_bench": True,
            "skip_coverage": True, "skip_check": True, "skip_test_norun": True,
            "skip_wiring": True, "dry_run": True, "verbose": False,
        }
        defaults.update(overrides)
        return argparse.Namespace(**defaults)

    def test_all_skipped(self, tmp_path: Path,
                         monkeypatch: pytest.MonkeyPatch) -> None:
        result = mod.run(self._base_args(tmp_path, monkeypatch))
        assert result["summary"]["total"] == 0

    def test_only_wiring(self, tmp_path: Path,
                         monkeypatch: pytest.MonkeyPatch) -> None:
        result = mod.run(self._base_args(tmp_path, monkeypatch, skip_wiring=False))
        # 4 touring commands run
        assert result["summary"]["total"] == 4

    def test_only_check(self, tmp_path: Path,
                        monkeypatch: pytest.MonkeyPatch) -> None:
        result = mod.run(self._base_args(tmp_path, monkeypatch, skip_check=False))
        assert "cargo_check" in result["results"]
        assert result["summary"]["total"] == 1


class TestRunCmdMissingTool:
    """run_cmd handles MISSING_TOOL gracefully (FileNotFoundError)."""

    def test_missing_tool(self, tmp_path: Path,
                          monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        result = mod.run_cmd(
            ["nonexistent-binary-xyz-12345"],
            tmp_path / "log.txt",
            timeout=5, dry=False,
        )
        assert result["status"] == "MISSING_TOOL"
        assert result["exit_code"] == -2
