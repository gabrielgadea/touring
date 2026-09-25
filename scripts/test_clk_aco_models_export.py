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
# Contract (25/09/2026): the omitted-path form resolves the project's LIVE
# memory.db, which the daemon owns — so the default is READ-ONLY (no write
# PRAGMAs, no migrations, no DDL; a reader creates nothing). Writing requires
# `writable=True` with an explicit path — a writer names its database.


def test_rlm_memory_writable_requires_an_explicit_path(clk):
    with pytest.raises(ValueError, match="writable=True requires an explicit db_path"):
        clk.RlmMemory(writable=True)


def test_rlm_memory_writable_explicit_path_creates_and_absent_key_reads_none(clk, tmp_path):
    db = tmp_path / "mem" / "memory.db"
    mem = clk.RlmMemory(str(db), writable=True)
    assert db.is_file(), "the writable form opens-or-creates at the EXPLICIT path"
    assert mem.read("skill_ranking") is None
    assert mem.read("skill_ranking", "core") is None


def test_rlm_memory_default_is_readonly_and_creates_nothing(clk, tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    with pytest.raises(ValueError, match="read-only open"):
        clk.RlmMemory()
    assert not (tmp_path / ".claude" / "touring" / "memory.db").exists(), (
        "a reader never creates nor migrates another process's database"
    )


def test_rlm_memory_readonly_opens_an_existing_db_and_open_readonly_is_the_explicit_form(
    clk, tmp_path, monkeypatch
):
    db = tmp_path / ".claude" / "touring" / "memory.db"
    clk.RlmMemory(str(db), writable=True)  # the writer materializes it
    monkeypatch.chdir(tmp_path)
    mem = clk.RlmMemory()  # omitted path → canonical walk-up, READ-ONLY
    assert mem.read("absent") is None
    explicit = clk.RlmMemory.open_readonly(str(db))
    assert explicit.read("absent", "core") is None
    with pytest.raises(ValueError, match="does not exist"):
        clk.RlmMemory.open_readonly(str(tmp_path / "nope.db"))
    assert not (tmp_path / "nope.db").exists()


def test_rlm_memory_readonly_read_never_bumps_access_count(clk, tmp_path):
    import sqlite3

    db = tmp_path / "memory.db"
    writable = clk.RlmMemory(str(db), writable=True)
    raw = sqlite3.connect(str(db))
    raw.execute(
        "INSERT INTO memory_entries (key, tier, value, access_count) VALUES ('k', 'core', 'v', 0)"
    )
    raw.commit()
    reader = clk.RlmMemory.open_readonly(str(db))
    assert reader.read("k", "core") == "v"
    reader.read("k", "core")
    n = raw.execute("SELECT access_count FROM memory_entries WHERE key = 'k'").fetchone()[0]
    assert n == 0, "the read-only read is a pure SELECT — never the bump"
    assert writable.read("k", "core") == "v"
    n = raw.execute("SELECT access_count FROM memory_entries WHERE key = 'k'").fetchone()[0]
    assert n == 1, "the writable read keeps the bump (the control)"
    raw.close()


def test_rlm_memory_readonly_reader_beside_a_live_writer(clk, tmp_path):
    db = tmp_path / "memory.db"
    writer = clk.RlmMemory(str(db), writable=True)  # holds the WAL writer open
    reader = clk.RlmMemory.open_readonly(str(db))
    for _ in range(20):
        assert reader.read("skill_ranking") is None
    del writer  # the reader never blocked on it


def test_rlm_read_reaches_every_tier_without_a_list(clk, tmp_path):
    """25/09/2026 (a d4, no banco vivo do analise): 25,1% das linhas fora dos
    4 tiers do enum — `semantic` 4.689, `local` 179. A coluna é livre (DDL
    default 'local'); o read filtra pela string crua ou por nada."""
    import sqlite3

    db = tmp_path / "memory.db"
    clk.RlmMemory(str(db), writable=True)
    raw = sqlite3.connect(str(db))
    raw.executemany(
        "INSERT INTO memory_entries (key, tier, value) VALUES (?, ?, ?)",
        [("lesson", "semantic", "the lesson"),
         ("scratch", "local", "local note"),
         ("hot", "ephemeral", "hot note")],
    )
    raw.commit()
    mem = clk.RlmMemory.open_readonly(str(db))
    # Sem tier: a PK por chave (H1) devolve a única linha — sem lista a percorrer.
    assert mem.read("lesson") == "the lesson"
    assert mem.read("scratch") == "local note"
    assert mem.read("hot") == "hot note"
    # Tier exato — inclusive os que nenhum enum nomeia.
    assert mem.read("lesson", "semantic") == "the lesson"
    assert mem.read("scratch", "local") == "local note"
    # Tier inexistente → None honesto (o fail-soft de busca ampla morreu: um
    # tier digitado errado não parece válido).
    assert mem.read("lesson", "semantc") is None
    assert mem.read("absent") is None
    raw.close()


# ── search_index (the analise's indexing test contract, T1-T8) ───────────────


def test_search_index_submodule_is_registered(clk):
    pre = clk.search_index.SearchPreprocessor()
    assert pre is not None


def test_search_index_preprocess_contract_and_doc_type_by_path(clk):
    pre = clk.search_index.SearchPreprocessor()
    doc = pre.preprocess(".claude/skills/antt-legal/SKILL.md", "# Test\nUse when analyzing documents.")
    assert doc.path == ".claude/skills/antt-legal/SKILL.md"
    assert doc.name == "SKILL.md"
    assert doc.doc_type == "skill"
    assert isinstance(doc.content_tokens, list)
    assert isinstance(doc.symbols, list)
    assert isinstance(doc.triggers, list)
    assert isinstance(doc.dependencies, list)
    assert isinstance(doc.module, str)
    # T3: doc_type by path
    assert pre.preprocess(".claude/hooks/quality/gate.py", "content").doc_type == "hook"
    assert pre.preprocess(".claude/rules/00-core.md", "content").doc_type == "rule"
    assert pre.preprocess(".claude/agents/my-agent.md", "content").doc_type == "agent"


def test_search_index_batch_equals_individual(clk):
    pre = clk.search_index.SearchPreprocessor()
    docs = [
        (".claude/skills/skill-a/SKILL.md", "Use when doing task A."),
        (".claude/hooks/hook-b.py", "def main(): pass"),
        (".claude/rules/rule-c.md", "# Rule\nPriority: 100"),
    ]
    results = pre.preprocess_batch(docs)
    assert len(results) == 3
    assert [d.doc_type for d in results] == ["skill", "hook", "rule"]
    single = pre.preprocess(*docs[0])
    assert results[0].doc_type == single.doc_type
    assert results[0].module == single.module


# ── AcoGraph — the d4's behavior probe (25/09/2026, REGRA #21) ───────────────
# The four measured defects: the false «cycle detected» on any graph WITH an
# edge (inverted Kahn in-degree), the silent stub node for an unknown
# dependency, the unvalidated triad, and the silent Template fallback for an
# unknown generator_type string. All proofs run against the built module.


def _aco_node(clk, nid, deps=None, scripts=("run", "check", "undo"), contract=("pre", "post", "inv")):
    return clk.GeneratorNode(
        nid, "d", "template", [], [], [], scripts[0], scripts[1], scripts[2],
        contract[0], contract[1], contract[2], "", deps or [],
    )


def test_aco_graph_levels_parallelism_and_freeze_work_WITH_edges(clk):
    g = clk.AcoGraph()
    g.add_node(_aco_node(clk, "A"))
    g.add_node(_aco_node(clk, "B", ["A"]))
    g.add_node(_aco_node(clk, "C", ["A"]))
    g.add_node(_aco_node(clk, "D", ["B", "C"]))
    # The diamond: acyclic, 3 levels, B‖C parallel — the broken Kahn said
    # «cycle detected» here.
    assert g.get_all_levels() == [["A"], ["B", "C"], ["D"]]
    assert any(len(level) == 2 for level in g.detect_parallelizable())
    import json as _json

    frozen = _json.loads(g.freeze(None))
    assert len(frozen["nodes"]) == 4
    # linear of 3
    g2 = clk.AcoGraph()
    g2.add_node(_aco_node(clk, "A"))
    g2.add_node(_aco_node(clk, "B", ["A"]))
    g2.add_node(_aco_node(clk, "C", ["B"]))
    assert len(g2.get_all_levels()) == 3


def test_aco_graph_unknown_dependency_is_never_a_stub_and_is_named(clk):
    g = clk.AcoGraph()
    g.add_node(_aco_node(clk, "A", ["NONEXISTENT"]))
    assert len(g) == 1, "a phantom stub node was materialized"
    assert g.validate_dependencies() == ["A: depends on unknown node 'NONEXISTENT'"]
    with pytest.raises(ValueError, match="unresolved.*NONEXISTENT|NONEXISTENT.*unresolved"):
        g.freeze(None)


def test_aco_graph_validate_contracts_names_the_contract_and_the_triad(clk):
    g = clk.AcoGraph()
    g.add_node(_aco_node(clk, "X", None, scripts=("", "", ""), contract=("", "", "")))
    v = g.validate_contracts()
    for expected in [
        "X: missing precondition",
        "X: missing postcondition",
        "X: missing invariant",
        "X: missing execution_script (triad mandatory)",
        "X: missing validation_script (triad mandatory)",
        "X: missing rollback_script (triad mandatory)",
    ]:
        assert expected in v, f"{expected} ausente em {v}"


def test_aco_graph_unknown_generator_type_is_refused_with_the_valid_list(clk):
    g = clk.AcoGraph()
    bogus = clk.GeneratorNode(
        "Y", "d", "bogus-type", [], [], [], "a", "b", "c", "p", "q", "i", "", []
    )
    with pytest.raises(ValueError, match="unknown generator_type.*validator"):
        g.add_node(bogus)


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
