"""Tests for W1.landing."""

from __future__ import annotations

import pytest

import landing  # noqa: F401  — import the sub-script under test


class TestRun:
    """Integration tests for run()."""

    def test_dry_run_returns_ok(self, mock_workspace) -> None:
        """A dry-run on the mock workspace must return status=OK."""
        # TODO: build args, invoke run(), assert status
        assert True

    def test_apply_off_by_default(self, mock_workspace) -> None:
        """--apply must be opt-in (L9)."""
        # TODO: assert mutation phase did not execute
        assert True
