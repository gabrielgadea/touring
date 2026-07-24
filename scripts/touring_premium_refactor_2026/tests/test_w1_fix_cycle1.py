"""Tests for w1_fix_cycle1.py.

Strategy: build synthetic file_tools/project_tools content via fixture +
exercise analyze_cross_refs, diagnose, and propose_fix_* strategies.

Coverage:
  - analyze_cross_refs handles missing files
  - diagnose returns cycle_evidence with truthy/falsy combinations
  - propose_fix_shared lists extracted candidates
  - propose_fix_trait/merge return non-empty markdown
  - run() emits diagnosis.json + proposal-<strategy>.md
"""
from __future__ import annotations

import json
from pathlib import Path

import pytest

import w1_fix_cycle1 as mod


@pytest.fixture
def synthetic_tools(monkeypatch: pytest.MonkeyPatch, tmp_path: Path):
    """Build synthetic file_tools.rs + project_tools.rs with a known cycle."""
    tools = tmp_path / "crates" / "touring-server" / "src" / "tools"
    tools.mkdir(parents=True)
    (tools / "file_tools.rs").write_text("""\
use super::project_tools::ProjectCtx;
use crate::tools::project_tools::ProjectConfig;

pub struct FileTool { ctx: ProjectCtx }
impl FileTool {
    pub fn use_config(_c: ProjectConfig) {}
}
""", encoding="utf-8")
    (tools / "project_tools.rs").write_text("""\
use super::file_tools::FileTool;

pub struct ProjectCtx;
pub struct ProjectConfig;

impl ProjectCtx {
    pub fn make_tool() -> FileTool { unimplemented!() }
}
""", encoding="utf-8")
    monkeypatch.setattr(mod, "_ROOT", tmp_path)
    monkeypatch.setattr(mod, "_FILE_TOOLS", tools / "file_tools.rs")
    monkeypatch.setattr(mod, "_PROJECT_TOOLS", tools / "project_tools.rs")
    monkeypatch.setattr(mod, "_STAGING_DIR",
                        tmp_path / "scripts" / "tpr" / "staging")
    return tmp_path


@pytest.fixture
def synthetic_no_cycle(monkeypatch: pytest.MonkeyPatch, tmp_path: Path):
    """Tools that reference shared but not each other."""
    tools = tmp_path / "crates" / "touring-server" / "src" / "tools"
    tools.mkdir(parents=True)
    (tools / "file_tools.rs").write_text(
        "use crate::tools::shared::SharedCtx;\npub struct FileTool;\n",
        encoding="utf-8")
    (tools / "project_tools.rs").write_text(
        "use crate::tools::shared::SharedCtx;\npub struct ProjectTool;\n",
        encoding="utf-8")
    monkeypatch.setattr(mod, "_ROOT", tmp_path)
    monkeypatch.setattr(mod, "_FILE_TOOLS", tools / "file_tools.rs")
    monkeypatch.setattr(mod, "_PROJECT_TOOLS", tools / "project_tools.rs")
    monkeypatch.setattr(mod, "_STAGING_DIR",
                        tmp_path / "scripts" / "tpr" / "staging")
    return tmp_path


class TestAnalyzeCrossRefs:
    """analyze_cross_refs extracts use statements."""

    def test_missing_file(self, tmp_path: Path) -> None:
        result = mod.analyze_cross_refs(tmp_path / "nonexistent.rs")
        assert result["exists"] is False

    def test_finds_imports(self, synthetic_tools: Path) -> None:
        result = mod.analyze_cross_refs(mod._FILE_TOOLS)
        assert result["exists"] is True
        assert "project_tools" in result["imports_via_super_or_tools"]
        assert result["loc"] > 0


class TestDiagnose:
    """diagnose returns full cycle evidence."""

    def test_real_cycle_detected(self, synthetic_tools: Path) -> None:
        result = mod.diagnose()
        assert result["cycle_evidence"]["file_tools_uses_project_tools"] is True
        assert result["cycle_evidence"]["project_tools_uses_file_tools"] is True
        assert result["cycle_evidence"]["is_real_cycle"] is True

    def test_no_cycle_when_using_shared(self, synthetic_no_cycle: Path) -> None:
        result = mod.diagnose()
        # Neither file references the other directly
        assert result["cycle_evidence"]["file_tools_uses_project_tools"] is False
        assert result["cycle_evidence"]["project_tools_uses_file_tools"] is False
        assert result["cycle_evidence"]["is_real_cycle"] is False


class TestStrategyProposers:
    """All 3 strategies emit non-empty markdown."""

    def test_propose_shared_lists_candidates(self, synthetic_tools: Path) -> None:
        diag = mod.diagnose()
        out = mod.propose_fix_shared(diag)
        assert "shared" in out.lower()
        assert "Steps:" in out
        # Should mention items observed
        assert "ProjectCtx" in out or "FileTool" in out

    def test_propose_trait(self, synthetic_tools: Path) -> None:
        out = mod.propose_fix_trait(mod.diagnose())
        assert "trait" in out.lower()
        assert "ToolHandler" in out

    def test_propose_merge(self, synthetic_tools: Path) -> None:
        out = mod.propose_fix_merge(mod.diagnose())
        assert "merge" in out.lower()
        assert "Combine" in out or "combine" in out


class TestRun:
    """run() writes diagnosis + proposal files."""

    def test_run_writes_files(self, synthetic_tools: Path) -> None:
        result = mod.run(strategy="shared", apply=False)
        assert result["status"] == "OK"
        diag_path = Path(synthetic_tools) / result["diagnosis_file"]
        prop_path = Path(synthetic_tools) / result["proposal_file"]
        assert diag_path.exists()
        assert prop_path.exists()
        # diagnosis is valid JSON
        data = json.loads(diag_path.read_text(encoding="utf-8"))
        assert "cycle_evidence" in data

    def test_run_each_strategy(self, synthetic_tools: Path) -> None:
        for strat in ("shared", "trait", "merge"):
            result = mod.run(strategy=strat, apply=False)
            assert result["strategy"] == strat
            assert result["status"] == "OK"


class TestRegexes:
    """Regexes match expected patterns."""

    def test_cross_ref_re_matches_super(self) -> None:
        assert mod.CROSS_REF_RE.findall("use super::other_mod;")
        assert mod.CROSS_REF_RE.findall("use crate::tools::other_mod;")

    def test_type_use_re_matches_qualified(self) -> None:
        matches = mod.TYPE_USE_RE.findall("let x = super::other::Foo;")
        assert ("other", "Foo") in matches
