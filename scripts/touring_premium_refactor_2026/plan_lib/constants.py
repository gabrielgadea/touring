"""Paths, root directories, and immutable constants.

Extracted from generate_plan.py:49-63.
"""
from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path

# ─── Paths / Constants ───────────────────────────────────────────────────────

_ROOT = Path(__file__).resolve().parents[2]               # ~/.claude/rust/
_PLANS_DIR = _ROOT / "docs" / "plans" / "touring-premium-refactor-2026"
_SCRIPTS_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026"
_DATA_DIR = _SCRIPTS_DIR / "data"
_TESTS_DIR = _SCRIPTS_DIR / "tests"
_CHECKPOINTS_DIR = _ROOT / ".claude" / "checkpoints"

_TODAY = datetime.now(UTC).strftime("%Y-%m-%d")
_TODAY_COMPACT = datetime.now(UTC).strftime("%Y%m%d")
_VERSION = "1.0.0"
_PLAN_NAME = "touring-premium-refactor-2026"
_AUTHOR_GABRIEL = "Gabriel Gadea (architect)"
_AUTHOR_TACO = "TACO (orchestrator)"
