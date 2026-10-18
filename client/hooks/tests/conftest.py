"""conftest.py — pytest fixtures for client/hooks tests."""
from __future__ import annotations

import sys
from pathlib import Path

# Add the tests directory to sys.path so helpers.py is importable. A bare insert, not one under an `if`:
# the linter exempts an import that follows it (E402), so no suppression is needed for the order.
_TESTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_TESTS_DIR))

from helpers import *  # re-export for pytest collection
