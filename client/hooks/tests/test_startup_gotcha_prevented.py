"""The startup panel never prints a fabricated `prevented_errors` (C7 of the ECC analysis, 17/10/2026).

No writer increments `gotchas.prevented_errors`: 13 gotchas, 388.840 hits and 0 prevented, because `hit_count`
counts warnings shown and nothing records an error that did not happen.  The binary before the C7 fix reports 0, the
one after reports null with `prevented_measured: false`; both must read "não medido".  Only a value the producer
declares measured is printed.
"""
from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

HOOK = Path(__file__).resolve().parents[1] / "session_startup_intelligence.py"


def _load():
    spec = importlib.util.spec_from_file_location("ssi_prevented_under_test", HOOK)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


SSI = _load()


@pytest.mark.parametrize("gotcha", [
    {"resolved": 2, "prevented_errors": 0, "total": 13},  # the binary before C7: a zero nobody measured
    {"prevented_errors": None, "prevented_measured": False},  # the binary after C7
    {"prevented_errors": 5, "prevented_measured": False},  # a value the producer itself disowns
    {"prevented_measured": True},  # measured, but the value is missing
    {},
], ids=["old-zero", "new-null", "disowned-value", "measured-without-value", "empty"])
def test_an_unmeasured_prevented_reads_not_measured(gotcha: dict) -> None:
    assert SSI.prevented_text(gotcha) == "não medido"


def test_a_measured_value_is_printed() -> None:
    assert SSI.prevented_text({"prevented_errors": 3, "prevented_measured": True}) == "**3**"
    assert SSI.prevented_text({"prevented_errors": 0, "prevented_measured": True}) == "**0**"  # a measured zero is a fact


def test_the_panel_line_goes_through_the_helper() -> None:
    panel = SSI.fmt_global({"evolution": {"gotcha_coverage": {"resolved": 2, "prevented_errors": 0, "total": 13}}})
    assert "prevented_errors=não medido" in panel, panel
    assert "prevented_errors=**0**" not in panel, panel
