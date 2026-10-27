"""conftest.py — pytest fixtures for client/hooks tests."""
from __future__ import annotations

import sys
from pathlib import Path

# Add the tests directory to sys.path so helpers.py is importable. A bare insert, not one under an `if`:
# the linter exempts an import that follows it (E402), so no suppression is needed for the order.
_TESTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_TESTS_DIR))

from helpers import *  # re-export for pytest collection

import pytest


@pytest.fixture(autouse=True)
def _isolate_injection_dedup(monkeypatch: pytest.MonkeyPatch) -> None:
    """Keep the cross-process injection dedup out of the per-hook tests.

    27/10/2026: once `touring-hook dedup-stdin` went live, hooks run here wrote
    their dedup state under the CC session's `CLAUDE_PROJECT_DIR` (inherited by
    every subprocess), so tests sharing a fixed session id ("sess-a") saw the
    previous run's emission as a repetition and lost their output. These tests
    exercise each hook's own logic; the dedup has its own suites
    (`crates/touring-hooks-shared/src/injection_dedup.rs`,
    `scripts/test_injection_dedup.py`). A test that wants the dedup can
    `monkeypatch.delenv("TOURING_DEDUP_DISABLED")`.
    """
    monkeypatch.setenv("TOURING_DEDUP_DISABLED", "1")
    monkeypatch.delenv("CLAUDE_PROJECT_DIR", raising=False)
