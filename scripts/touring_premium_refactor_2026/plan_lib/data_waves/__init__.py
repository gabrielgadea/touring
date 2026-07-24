"""data_waves — 15-wave refactor plan data (W0..W14).

Thin orchestrator. The historical plan data was originally in a single 1904-line
file (generate_plan.py:412-2305) with 4 wave-range helpers (W0-W3, W4-W7,
W8-W11, W12-W14). The helpers have been split into ``data_waves/w*_w*.py``
so each file's MI stays high (REGRA #0 potentialization — preserve every byte,
improve maintainability through focused modules).

Public surface:
    WAVES: list[Wave] — populated by ``register_waves()``
    register_waves() -> None — populates WAVES by calling the four helpers
    _register_w0_w3 / _register_w4_w7 / _register_w8_w11 / _register_w12_w14
        — individual wave-range helpers (also re-exported)
"""
from __future__ import annotations

# Define WAVES FIRST (before importing the helpers) so each w*_w*.py can
# resolve ``from . import WAVES`` without a circular import error.
WAVES: list = []   # populated by register_waves() below

# Pull the dataclass into the package namespace so the w*_w*.py helpers can
# reference it as ``Wave`` directly without re-importing.
from ..dataclasses import Subtask, Wave  # noqa: F401 — re-export for compat

# Now import the helpers — they each do ``from . import WAVES`` which is
# already defined above.
from .w0_w3 import _register_w0_w3
from .w4_w7 import _register_w4_w7
from .w8_w11 import _register_w8_w11
from .w12_w14 import _register_w12_w14


def register_waves() -> None:
    """Populate the WAVES list. Called at module load."""
    if WAVES:
        return
    _register_w0_w3()
    _register_w4_w7()
    _register_w8_w11()
    _register_w12_w14()


__all__ = ["WAVES", "register_waves",
           "_register_w0_w3", "_register_w4_w7", "_register_w8_w11", "_register_w12_w14"]