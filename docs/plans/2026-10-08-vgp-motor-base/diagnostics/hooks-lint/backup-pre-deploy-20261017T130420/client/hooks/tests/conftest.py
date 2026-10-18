"""conftest.py — pytest fixtures for client/hooks tests."""
from __future__ import annotations
import sys
from pathlib import Path

# Add the tests directory to sys.path so helpers.py is importable.
_TESTS_DIR = Path(__file__).resolve().parent
if str(_TESTS_DIR) not in sys.path:
    sys.path.insert(0, str(_TESTS_DIR))

from helpers import *  # noqa: E402, F401, F403  re-export for pytest collection (after the sys.path insert)
