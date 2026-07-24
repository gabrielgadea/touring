"""Pytest config + shared fixtures for touring_premium_refactor_2026 tests.

Allows tests to import sibling scripts via `import w0_snapshot_tar`.
Provides temporary directory fixtures + mock workspace builders.
"""
from __future__ import annotations

import sys
import textwrap
from pathlib import Path

import pytest

# Add scripts/touring_premium_refactor_2026/ to sys.path so tests can
# `import w0_snapshot_tar` etc.
_SCRIPTS_DIR = Path(__file__).resolve().parent.parent
if str(_SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS_DIR))


@pytest.fixture
def mock_workspace(tmp_path: Path) -> Path:
    """Build a minimal mock Touring workspace for integration tests.

    Layout:
      <root>/
        Cargo.toml              (workspace.members = [...])
        crates/
          touring-foo/
            Cargo.toml
            src/lib.rs
          touring-empty/
            Cargo.toml
            src/lib.rs (0 bytes)
          touring-archive/
            Cargo.toml
            ARCHITECTURE.md (archived)
            src/lib.rs
        docs/baselines/
    """
    root = tmp_path / "workspace"
    root.mkdir()
    crates = root / "crates"
    crates.mkdir()
    (root / "docs" / "baselines").mkdir(parents=True)
    # Root Cargo.toml
    (root / "Cargo.toml").write_text(textwrap.dedent("""
        [workspace]
        members = [
            "crates/touring-foo",
            "crates/touring-empty",
            "crates/touring-archive",
        ]
        resolver = "2"
    """).lstrip(), encoding="utf-8")
    # touring-foo (real crate)
    foo = crates / "touring-foo"
    (foo / "src").mkdir(parents=True)
    (foo / "Cargo.toml").write_text(textwrap.dedent("""
        [package]
        name = "touring-foo"
        version = "0.1.0"
        edition = "2021"

        [dependencies]
        serde = "1.0"
        tokio = { version = "1.35", features = ["full"] }
    """).lstrip(), encoding="utf-8")
    (foo / "src" / "lib.rs").write_text(textwrap.dedent("""
        //! touring-foo library
        pub fn add(a: i32, b: i32) -> i32 { a + b }
        pub struct Foo { pub value: i32 }
        pub trait Bar { fn bar(&self) -> i32; }
    """).lstrip(), encoding="utf-8")
    # touring-empty (dead crate, 0 LOC)
    empty = crates / "touring-empty"
    (empty / "src").mkdir(parents=True)
    (empty / "Cargo.toml").write_text(textwrap.dedent("""
        [package]
        name = "touring-empty"
        version = "0.1.0"
    """).lstrip(), encoding="utf-8")
    (empty / "src" / "lib.rs").write_text("", encoding="utf-8")
    # touring-archive (archived dead crate)
    arch = crates / "touring-archive"
    (arch / "src").mkdir(parents=True)
    (arch / "Cargo.toml").write_text(textwrap.dedent("""
        [package]
        name = "touring-archive"
        version = "0.1.0"

        [dependencies]
        serde = "1.0.180"
    """).lstrip(), encoding="utf-8")
    (arch / "ARCHITECTURE.md").write_text(
        "# Archived\n\nThis crate is archived.\n", encoding="utf-8")
    (arch / "src" / "lib.rs").write_text(
        "// archived stub\n", encoding="utf-8")
    return root


@pytest.fixture
def patched_root(monkeypatch: pytest.MonkeyPatch, mock_workspace: Path):
    """Patch _ROOT in target script modules to point at mock_workspace.

    Use as: `def test_x(patched_root): patched_root('w1_audit_dead_code')`
    """
    def _patch(module_name: str) -> None:
        import importlib
        mod = importlib.import_module(module_name)
        monkeypatch.setattr(mod, "_ROOT", mock_workspace, raising=True)
        # Re-derive dependent paths
        if hasattr(mod, "_CRATES_DIR"):
            monkeypatch.setattr(mod, "_CRATES_DIR",
                                mock_workspace / "crates", raising=True)
        if hasattr(mod, "_BASELINES_DIR"):
            monkeypatch.setattr(mod, "_BASELINES_DIR",
                                mock_workspace / "docs" / "baselines", raising=True)
        if hasattr(mod, "_STAGING_DIR"):
            staging = mock_workspace / "scripts" / "touring_premium_refactor_2026" / "staging"
            staging.mkdir(parents=True, exist_ok=True)
            monkeypatch.setattr(mod, "_STAGING_DIR", staging, raising=True)
        if hasattr(mod, "_DATA_DIR"):
            data = mock_workspace / "scripts" / "touring_premium_refactor_2026" / "data"
            data.mkdir(parents=True, exist_ok=True)
            monkeypatch.setattr(mod, "_DATA_DIR", data, raising=True)
        if hasattr(mod, "_WORKSPACE_CARGO"):
            monkeypatch.setattr(mod, "_WORKSPACE_CARGO",
                                mock_workspace / "Cargo.toml", raising=True)
    return _patch
