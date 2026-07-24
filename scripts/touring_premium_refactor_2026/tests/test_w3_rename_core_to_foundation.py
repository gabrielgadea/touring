"""Tests for w3_rename_core_to_foundation.py.

Strategy: mock workspace with synthetic touring-core consumers + validate
that scan_consumers detects use statements correctly + estimate_hours computes
proportional results.

Coverage:
  - USE_RE matches `use touring_core::X` patterns
  - CARGO_DEP_RE detects `touring-core = ...` in Cargo.toml
  - scan_consumers returns sub_modules_used sorted by frequency
  - estimate_hours scales linearly with file counts
  - render_ast_grep_script generates idempotent script
"""
from __future__ import annotations

import re
import textwrap
from pathlib import Path

import pytest

import w3_rename_core_to_foundation as mod


class TestRegexes:
    """Use + Cargo dep + extern regex patterns."""

    def test_simple_use(self) -> None:
        assert mod.USE_RE.findall("use touring_core::TouringConfig;")
        assert mod.USE_RE.findall("use touring_core::diagnostic::Code;")

    def test_use_with_double_colon(self) -> None:
        matches = mod.USE_RE.findall("use touring_core::error::TouringError;")
        assert ("error", "TouringError") in matches

    def test_no_match_other_crate(self) -> None:
        assert not mod.USE_RE.findall("use touring_other::Foo;")

    def test_extern_crate(self) -> None:
        assert mod.EXTERN_RE.search("extern crate touring_core;")
        assert not mod.EXTERN_RE.search("extern crate touring_other;")

    def test_cargo_dep_re(self) -> None:
        assert mod.CARGO_DEP_RE.search('\ntouring-core = "1.0"\n')
        assert not mod.CARGO_DEP_RE.search('\ntouring-other = "1.0"\n')


@pytest.fixture
def workspace_with_core_consumers(monkeypatch: pytest.MonkeyPatch,
                                    tmp_path: Path) -> Path:
    """Mock workspace with 1 producer (touring-core) + 2 consumers."""
    crates = tmp_path / "crates"
    crates.mkdir()
    # touring-core (producer)
    core = crates / "touring-core"
    (core / "src").mkdir(parents=True)
    (core / "Cargo.toml").write_text('[package]\nname = "touring-core"\n',
                                       encoding="utf-8")
    (core / "src" / "lib.rs").write_text(
        "pub struct TouringConfig;\npub enum TouringError {}\n",
        encoding="utf-8")
    # touring-consumer-a
    a = crates / "touring-consumer-a"
    (a / "src").mkdir(parents=True)
    (a / "Cargo.toml").write_text(
        '[package]\nname = "touring-consumer-a"\n[dependencies]\n'
        'touring-core = { path = "../touring-core" }\n',
        encoding="utf-8")
    (a / "src" / "lib.rs").write_text(
        textwrap.dedent("""
            use touring_core::TouringConfig;
            use touring_core::error::TouringError;
            fn use_core() -> TouringConfig { TouringConfig }
        """).lstrip(),
        encoding="utf-8")
    # touring-consumer-b
    b = crates / "touring-consumer-b"
    (b / "src").mkdir(parents=True)
    (b / "Cargo.toml").write_text(
        '[package]\nname = "touring-consumer-b"\n[dependencies]\n'
        'touring-core = "0.1"\n', encoding="utf-8")
    (b / "src" / "lib.rs").write_text(
        "use touring_core::diagnostic::Code;\n", encoding="utf-8")
    monkeypatch.setattr(mod, "_ROOT", tmp_path)
    monkeypatch.setattr(mod, "_CRATES_DIR", crates)
    monkeypatch.setattr(mod, "_STAGING_DIR",
                        tmp_path / "scripts" / "tpr" / "staging")
    return tmp_path


class TestScanConsumers:
    """scan_consumers detects refs across multiple crates."""

    def test_detects_both_consumers(self, workspace_with_core_consumers: Path) -> None:
        result = mod.scan_consumers()
        assert "touring-consumer-a" in result["consumers"]
        assert "touring-consumer-b" in result["consumers"]

    def test_cargo_decls_detected(self, workspace_with_core_consumers: Path) -> None:
        result = mod.scan_consumers()
        # Both consumers declare touring-core in Cargo.toml
        assert len(result["cargo_decl_files"]) >= 2

    def test_sub_modules_aggregated(self, workspace_with_core_consumers: Path) -> None:
        result = mod.scan_consumers()
        # consumer-a uses TouringConfig + error
        # consumer-b uses diagnostic
        assert "TouringConfig" in result["sub_modules_used"]
        assert "diagnostic" in result["sub_modules_used"]


class TestEstimateHours:
    """estimate_hours scales with file counts."""

    def test_zero_files(self) -> None:
        scan = {"cargo_decl_files": [], "consumers": {},
                 "sub_modules_used": {}}
        est = mod.estimate_hours(scan)
        # Only the fixed cargo_check buffer
        assert est["total_hours"] >= 1.0
        assert est["totals"]["consumer_files"] == 0

    def test_scaling(self) -> None:
        scan = {
            "cargo_decl_files": ["a", "b", "c"],
            "consumers": {
                "crate1": [{"use_count": 5}, {"use_count": 3}],
                "crate2": [{"use_count": 2}],
            },
            "sub_modules_used": {},
        }
        est = mod.estimate_hours(scan)
        assert est["totals"]["cargo_files"] == 3
        assert est["totals"]["consumer_files"] == 3  # 2 + 1
        assert est["totals"]["total_use_statements"] == 10  # 5+3+2
        # cargo_files * 0.05 + consumer_files * 0.12 + 1.0 = 0.15 + 0.36 + 1.0 = 1.51
        assert abs(est["total_hours"] - 1.51) < 0.01


class TestAstGrepScript:
    """render_ast_grep_script emits valid bash."""

    def test_idempotent_marker(self) -> None:
        scan = {"consumers": {}}
        script = mod.render_ast_grep_script(scan)
        assert "set -euo pipefail" in script
        assert "Idempotent" in script

    def test_includes_consumer_files(self) -> None:
        scan = {"consumers": {
            "crate1": [{"file": "crates/crate1/src/foo.rs", "use_count": 1}],
        }}
        script = mod.render_ast_grep_script(scan)
        assert "crates/crate1/src/foo.rs" in script
        assert "touring_core" in script
        assert "touring_foundation" in script

    def test_safe_with_empty_consumers(self) -> None:
        scan = {"consumers": {}}
        script = mod.render_ast_grep_script(scan)
        # Should still emit header + phase markers
        assert "Phase 1" in script
        assert "cargo check" in script


class TestNameMapping:
    """Old/New crate + module name constants are correct."""

    def test_module_names(self) -> None:
        assert mod.OLD_MOD == "touring_core"
        assert mod.NEW_MOD == "touring_foundation"

    def test_crate_names(self) -> None:
        assert mod.OLD_NAME == "touring-core"
        assert mod.NEW_NAME == "touring-foundation"
