#!/usr/bin/env python3
"""Export proofs for the 10 ACO model wrappers (24/09/2026).

Gabriel's order («exporte todos os tipos CLK»): the eight types of
`touring_intelligence::rl::aco::models` plus `RlmMemory` and
`JaccardComputer`, importable at the level each consumer imports.

The string contracts are MEASURED in analise's
`scripts/aco/tests/test_aco_rust_parity.py` — enums stringify
lowercase/snake_case (Complexity carries the accent: Critical → "crítica"),
`from_str` round-trips them and raises ValueError on an unknown name.

Runtime is the BUILT debug cdylib loaded through the replicated installed
layout (the same trick as `test_clk_stub_parity.py`) — the suite runs in CI
without installing the wheel. Each test pins one type's surface, so removing
a class from `register()` or changing a parity string fails exactly here
(mutation-proven).
"""

from __future__ import annotations

import shutil
import sys
import tempfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
SO = ROOT / "target" / "debug" / "libclaude_learning_kernel.so"

pytestmark = pytest.mark.skipif(
    not SO.is_file(), reason="needs the built cdylib (cargo build -p touring-python)"
)

# The parity contract of analise's test_aco_rust_parity.py — measured there,
# never re-derived. variant name → the string str() must return.
ENUM_PARITY: dict[str, dict[str, str]] = {
    "GeneratorType": {
        "Template": "template",
        "Transformer": "transformer",
        "Composer": "composer",
        "Validator": "validator",
    },
    "Complexity": {
        "Trivial": "trivial",
        "Moderate": "moderada",
        "High": "alta",
        "Critical": "crítica",
    },
    "DriftLevel": {
        "Ok": "ok",
        "Warning": "warning",
        "Critical": "critical",
        "Halt": "halt",
    },
    "ValidationStatus": {
        "Pass": "pass",
        "Warn": "warn",
        "Fail": "fail",
    },
    "ExecutionStatus": {
        "Pending": "pending",
        "Running": "running",
        "Success": "success",
        "ValidationFailed": "validation_failed",
        "ExecutionFailed": "execution_failed",
        "PreconditionFailed": "precondition_failed",
        "RollbackExecuted": "rollback_executed",
        "RollbackFailed": "rollback_failed",
    },
}


def _load_clk():
    """Import the debug cdylib through the replicated installed layout."""
    tmp = Path(tempfile.mkdtemp(prefix="clk-export-"))
    pkg = tmp / "claude_learning_kernel"
    pkg.mkdir()
    (pkg / "__init__.py").write_text(
        "from .claude_learning_kernel import *\n\n"
        "__doc__ = claude_learning_kernel.__doc__\n"
        "if hasattr(claude_learning_kernel, \"__all__\"):\n"
        "    __all__ = claude_learning_kernel.__all__\n",
        encoding="utf-8",
    )
    shutil.copy(SO, pkg / "claude_learning_kernel.so")
    sys.path.insert(0, str(tmp))
    for stale in ("claude_learning_kernel", "claude_learning_kernel.claude_learning_kernel"):
        sys.modules.pop(stale, None)
    import claude_learning_kernel as clk

    return clk


@pytest.fixture(scope="module")
def clk():
    return _load_clk()


# ── The 5 enums: package level, parity strings, roundtrip, named refusal ────


def test_the_five_enums_are_exported_at_package_level(clk):
    for name in ENUM_PARITY:
        assert hasattr(clk, name), f"{name} missing from the package surface"


def test_enum_variants_stringify_with_the_aco_parity_strings(clk):
    for enum_name, variants in ENUM_PARITY.items():
        enum = getattr(clk, enum_name)
        for variant_name, expected in variants.items():
            variant = getattr(enum, variant_name)
            assert str(variant) == expected, (
                f"{enum_name}.{variant_name}: str() = {str(variant)!r}, "
                f"the aco parity contract says {expected!r}"
            )


def test_enum_from_str_roundtrips_every_parity_string(clk):
    for enum_name, variants in ENUM_PARITY.items():
        enum = getattr(clk, enum_name)
        for variant_name, parity in variants.items():
            parsed = enum.from_str(parity)
            assert parsed == getattr(enum, variant_name), (
                f"{enum_name}.from_str({parity!r}) did not round-trip to {variant_name}"
            )


def test_enum_from_str_rejects_unknown_names_with_value_error(clk):
    for enum_name in ENUM_PARITY:
        enum = getattr(clk, enum_name)
        with pytest.raises(ValueError, match=f"Unknown {enum_name}"):
            enum.from_str("not-a-variant")


# ── The 3 structs ────────────────────────────────────────────────────────────


def test_generator_outputs_defaults_and_kwargs_construction(clk):
    empty = clk.GeneratorOutputs()
    assert (empty.execution_script, empty.validation_script, empty.rollback_script) == ("", "", "")
    full = clk.GeneratorOutputs(
        execution_script="run.sh", validation_script="check.sh", rollback_script="undo.sh"
    )
    assert full.execution_script == "run.sh"
    assert full.validation_script == "check.sh"
    assert full.rollback_script == "undo.sh"


def test_generator_contract_is_complete_follows_the_rust_method(clk):
    assert clk.GeneratorContract().is_complete() is False
    partial = clk.GeneratorContract(precondition="pre", postcondition="post")
    assert partial.is_complete() is False
    full = clk.GeneratorContract(precondition="pre", postcondition="post", invariant="inv")
    assert full.is_complete() is True
    assert (full.precondition, full.postcondition, full.invariant) == ("pre", "post", "inv")


def test_validation_result_takes_the_enum_status_and_optional_remediation(clk):
    bare = clk.ValidationResult(
        "execution", "check-syntax", clk.ValidationStatus.Fail, "0 errors", "2 errors", "syntax broke"
    )
    assert bare.phase == "execution"
    assert bare.check_name == "check-syntax"
    assert bare.status == clk.ValidationStatus.Fail
    assert bare.remediation is None
    with_fix = clk.ValidationResult(
        "validation",
        "check-output",
        clk.ValidationStatus.Warn,
        "json",
        "text",
        "wrong shape",
        remediation="retry with the schema",
    )
    assert with_fix.status == clk.ValidationStatus.Warn
    assert with_fix.remediation == "retry with the schema"


# ── RlmMemory — the special-care type (the coordinator's verbatim questions) ─


def test_rlm_memory_explicit_path_creates_the_db_and_absent_key_reads_none(clk, tmp_path):
    db = tmp_path / "mem" / "memory.db"
    mem = clk.RlmMemory(str(db))
    assert db.is_file(), "the constructor must open-or-create the db file"
    # Absent key → None, never raises (the backend's fallback must keep working).
    assert mem.read("skill_ranking") is None
    assert mem.read("skill_ranking", "core") is None
    assert mem.read("skill_ranking", "ephemeral") is None


def test_rlm_memory_default_constructor_resolves_inside_the_cwd(clk, tmp_path, monkeypatch):
    # Omitted path → the canonical `.claude/touring/memory.db` of the "project"
    # found by walk-up from the cwd. Under a bare tmp dir there is no ancestor
    # marker, so it must create a FRESH db there — never touch a real project db.
    monkeypatch.chdir(tmp_path)
    mem = clk.RlmMemory()
    expected = tmp_path / ".claude" / "touring" / "memory.db"
    assert expected.is_file(), "the default path must materialize under the cwd walk-up"
    assert mem.read("absent") is None


# ── JaccardComputer — in the simd submodule, not the package level ───────────


def test_jaccard_computer_lives_in_the_simd_submodule(clk):
    assert hasattr(clk.simd, "JaccardComputer")
    assert not hasattr(clk, "JaccardComputer"), "JaccardComputer belongs to simd, not the package"
    computer = clk.simd.JaccardComputer()
    assert computer.jaccard([1, 2, 3], [2, 3, 4]) == pytest.approx(0.5)
    assert computer.jaccard([1, 2, 3], [1, 2, 3]) == pytest.approx(1.0)
    assert computer.jaccard([1, 2], [3, 4]) == pytest.approx(0.0)


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
