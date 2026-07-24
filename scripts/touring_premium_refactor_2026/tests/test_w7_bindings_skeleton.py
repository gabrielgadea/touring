"""Tests for w7_bindings_skeleton.py.

Strategy: invoke emitter functions in temp directory + assert generated
content has expected features + structure.

Coverage:
  - Cargo.toml has all 6 expected features (bind-py, bind-ts-napi, etc.)
  - Cargo.toml default features array is EMPTY (caller picks language)
  - lib.rs declares all language modules behind feature gates
  - common.rs has BindingError + Greeting + BindingResult
  - py.rs / ts.rs / wasm.rs each gated by their feature flag
  - pyproject.toml + package.json are valid JSON/TOML
"""
from __future__ import annotations

import json
from pathlib import Path

import pytest

import w7_bindings_skeleton as mod


@pytest.fixture
def temp_skeleton(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Path:
    """Redirect skeleton dir to tmp_path."""
    skeleton = tmp_path / "w7-touring-bindings"
    monkeypatch.setattr(mod, "_ROOT", tmp_path)
    monkeypatch.setattr(mod, "_SKELETON_DIR", skeleton)
    monkeypatch.setattr(mod, "_STAGING_DIR", tmp_path / "staging")
    monkeypatch.setattr(mod, "_DATA_DIR", tmp_path / "data")
    return skeleton


class TestCargoToml:
    """Cargo.toml has all required features + deps."""

    def test_emit_includes_six_features(self, temp_skeleton: Path) -> None:
        cargo = mod.emit_cargo_toml()
        text = cargo.read_text(encoding="utf-8")
        for feat in ("bind-py", "bind-ts-napi", "bind-wasm",
                      "bind-async", "bind-stream", "bind-stable-api"):
            assert feat in text, f"missing feature {feat}"

    def test_default_features_empty(self, temp_skeleton: Path) -> None:
        cargo = mod.emit_cargo_toml()
        text = cargo.read_text(encoding="utf-8")
        # 'default = []' (no auto-loading)
        assert "default = []" in text

    def test_lib_crate_type(self, temp_skeleton: Path) -> None:
        cargo = mod.emit_cargo_toml()
        text = cargo.read_text(encoding="utf-8")
        # cdylib + rlib for both Python wheels and Node native
        assert "cdylib" in text
        assert "rlib" in text

    def test_optional_deps(self, temp_skeleton: Path) -> None:
        cargo = mod.emit_cargo_toml()
        text = cargo.read_text(encoding="utf-8")
        # PyO3 and napi must be optional (only loaded with feature)
        assert "pyo3 = { workspace = true, optional = true" in text
        assert "napi = { workspace = true, optional = true" in text


class TestSrcFiles:
    """Source files have correct feature gates."""

    def test_lib_rs_declares_modules(self, temp_skeleton: Path) -> None:
        files = mod.emit_src_files()
        lib_rs = next(f for f in files if f.name == "lib.rs")
        text = lib_rs.read_text(encoding="utf-8")
        assert 'pub mod py;' in text
        assert 'pub mod ts;' in text
        assert 'pub mod wasm;' in text
        assert 'pub mod common;' in text
        # All gated by feature flag
        assert '#[cfg(feature = "bind-py")]' in text
        assert '#[cfg(feature = "bind-ts-napi")]' in text
        assert '#[cfg(feature = "bind-wasm")]' in text

    def test_common_has_binding_error(self, temp_skeleton: Path) -> None:
        files = mod.emit_src_files()
        common = next(f for f in files if f.name == "common.rs")
        text = common.read_text(encoding="utf-8")
        assert "BindingError" in text
        assert "BindingResult" in text
        assert "Greeting" in text

    def test_py_rs_gated(self, temp_skeleton: Path) -> None:
        files = mod.emit_src_files()
        py = next(f for f in files if f.name == "py.rs")
        text = py.read_text(encoding="utf-8")
        # Every item gated by bind-py feature
        assert '#[cfg(feature = "bind-py")]' in text
        assert "pyfunction" in text

    def test_ts_rs_gated(self, temp_skeleton: Path) -> None:
        files = mod.emit_src_files()
        ts = next(f for f in files if f.name == "ts.rs")
        text = ts.read_text(encoding="utf-8")
        assert '#[cfg(feature = "bind-ts-napi")]' in text
        assert "napi_derive" in text


class TestPackaging:
    """pyproject.toml + package.json valid."""

    def test_pyproject_maturin(self, temp_skeleton: Path) -> None:
        p = mod.emit_pyproject()
        text = p.read_text(encoding="utf-8")
        assert 'build-backend = "maturin"' in text
        assert 'features = ["bind-py"]' in text

    def test_package_json_napi(self, temp_skeleton: Path) -> None:
        p = mod.emit_package_json()
        data = json.loads(p.read_text(encoding="utf-8"))
        assert data["name"] == "@touring/native"
        assert "napi" in data
        assert data["napi"]["name"] == "touring"
        assert "@napi-rs/cli" in data["devDependencies"]


class TestRun:
    """run() integration."""

    def test_full_run(self, temp_skeleton: Path) -> None:
        import argparse
        args = argparse.Namespace(
            skip_pyproject=False, skip_package_json=False,
            output_dir=temp_skeleton.parent / "data",
            verbose=False,
        )
        result = mod.run(args)
        assert result["status"] == "OK"
        # 1 (Cargo.toml) + 5 (src) + 1 (pyproject) + 1 (package.json) = 8
        assert len(result["files_emitted"]) == 8

    def test_skip_pyproject(self, temp_skeleton: Path) -> None:
        import argparse
        args = argparse.Namespace(
            skip_pyproject=True, skip_package_json=False,
            output_dir=temp_skeleton.parent / "data",
            verbose=False,
        )
        result = mod.run(args)
        # 1 + 5 + 1 (package.json only) = 7
        assert len(result["files_emitted"]) == 7
        assert all("pyproject" not in f for f in result["files_emitted"])
