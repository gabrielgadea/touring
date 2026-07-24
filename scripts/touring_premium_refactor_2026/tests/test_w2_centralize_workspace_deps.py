"""Tests for w2_centralize_workspace_deps.py.

Strategy: build a mock workspace with known deps + version conflicts,
exercise parse_cargo_deps, scan_workspace, detect_conflicts, and run().

Coverage:
  - SIMPLE_DEP_RE matches `dep = "1.0"` form
  - COMPLEX_DEP_RE matches `dep = { version = "1.0", ... }` form
  - touring-* internal deps are excluded
  - dev-dependencies + build-dependencies are scanned
  - Version conflicts are detected correctly
  - Proposal TOML is valid + sorted alphabetically
"""
from __future__ import annotations

import json
import textwrap
from pathlib import Path

import pytest

import w2_centralize_workspace_deps as mod


class TestRegexes:
    """Parser regexes match expected dependency forms."""

    def test_simple_dep_matches(self) -> None:
        m = mod.SIMPLE_DEP_RE.match('serde = "1.0"')
        assert m
        assert m.group(1) == "serde"
        assert m.group(2) == "1.0"

    def test_simple_dep_with_patch(self) -> None:
        m = mod.SIMPLE_DEP_RE.match('  tokio = "1.35.0"  ')
        assert m
        assert m.group(2) == "1.35.0"

    def test_complex_dep_matches(self) -> None:
        m = mod.COMPLEX_DEP_RE.match(
            'tokio = { version = "1.35", features = ["full"] }'
        )
        assert m
        assert m.group(1) == "tokio"
        assert m.group(2) == "1.35"

    def test_section_re(self) -> None:
        m = mod.SECTION_RE.match("[dependencies]")
        assert m
        assert m.group(1) == "dependencies"

    def test_section_re_dev_deps(self) -> None:
        m = mod.SECTION_RE.match("[dev-dependencies]")
        assert m.group(1) == "dev-dependencies"


class TestParseCargoDeps:
    """parse_cargo_deps handles real Cargo.toml structures."""

    def test_basic_dependencies(self, tmp_path: Path,
                                 monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(textwrap.dedent("""
            [package]
            name = "x"

            [dependencies]
            serde = "1.0.180"
            tokio = { version = "1.35", features = ["full"] }
        """).lstrip(), encoding="utf-8")
        deps = mod.parse_cargo_deps(cargo)
        assert "serde" in deps
        assert "tokio" in deps
        assert deps["serde"][0][0] == "1.0.180"
        assert deps["tokio"][0][0] == "1.35"

    def test_skips_touring_internal(self, tmp_path: Path,
                                     monkeypatch: pytest.MonkeyPatch) -> None:
        """Internal touring-* deps must NOT appear in workspace.dependencies."""
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(textwrap.dedent("""
            [dependencies]
            touring-foo = { path = "../touring-foo" }
            touring-bar = "0.1"
            external-dep = "1.0"
        """).lstrip(), encoding="utf-8")
        deps = mod.parse_cargo_deps(cargo)
        assert "touring-foo" not in deps
        assert "touring-bar" not in deps
        assert "external-dep" in deps

    def test_scans_dev_and_build_deps(self, tmp_path: Path,
                                       monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(textwrap.dedent("""
            [dependencies]
            runtime-dep = "1.0"

            [dev-dependencies]
            pytest-rs = "0.5"

            [build-dependencies]
            cc = "1.0"
        """).lstrip(), encoding="utf-8")
        deps = mod.parse_cargo_deps(cargo)
        assert "runtime-dep" in deps
        assert "pytest-rs" in deps
        assert "cc" in deps

    def test_ignores_other_sections(self, tmp_path: Path,
                                     monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setattr(mod, "_ROOT", tmp_path)
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(textwrap.dedent("""
            [features]
            default = ["thing"]
            thing = "should_not_be_parsed_as_dep"

            [dependencies]
            real-dep = "1.0"
        """).lstrip(), encoding="utf-8")
        deps = mod.parse_cargo_deps(cargo)
        assert "real-dep" in deps
        assert "default" not in deps
        assert "thing" not in deps


class TestScanWorkspace:
    """scan_workspace aggregates deps across crates/."""

    def test_scan_mock_workspace(self, patched_root,
                                  mock_workspace: Path) -> None:
        patched_root("w2_centralize_workspace_deps")
        merged = mod.scan_workspace()
        # touring-foo has serde + tokio; touring-archive has serde
        assert "serde" in merged
        assert "tokio" in merged
        # 2 versions of serde: 1.0 (touring-foo) and 1.0.180 (touring-archive)
        serde_versions = {v for v, _ in merged["serde"]}
        assert "1.0" in serde_versions
        assert "1.0.180" in serde_versions


class TestDetectConflicts:
    """detect_conflicts finds version mismatches."""

    def test_no_conflict_single_version(self) -> None:
        merged = {"foo": [("1.0", "a/Cargo.toml"), ("1.0", "b/Cargo.toml")]}
        assert mod.detect_conflicts(merged) == {}

    def test_conflict_detected(self) -> None:
        merged = {"foo": [("1.0", "a/Cargo.toml"), ("2.0", "b/Cargo.toml")]}
        conflicts = mod.detect_conflicts(merged)
        assert "foo" in conflicts
        assert len(conflicts["foo"]) == 2
        versions = {v["version"] for v in conflicts["foo"]}
        assert versions == {"1.0", "2.0"}


class TestProposeWorkspaceDeps:
    """propose_workspace_deps emits valid sorted TOML."""

    def test_sorted_alphabetically(self) -> None:
        merged = {
            "zebra": [("1.0", "x")],
            "alpha": [("2.0", "y")],
            "middle": [("3.0", "z")],
        }
        out = mod.propose_workspace_deps(merged)
        lines = out.strip().splitlines()
        assert lines[0] == "[workspace.dependencies]"
        # alpha < middle < zebra
        dep_lines = lines[1:]
        assert dep_lines[0].startswith("alpha")
        assert dep_lines[1].startswith("middle")
        assert dep_lines[2].startswith("zebra")

    def test_picks_newest_version(self) -> None:
        """When multiple versions, lexically last is chosen (heuristic semver)."""
        merged = {"foo": [("1.0", "a"), ("1.5", "b"), ("1.2", "c")]}
        out = mod.propose_workspace_deps(merged)
        assert 'foo = "1.5"' in out


class TestRun:
    """run() integration: emits proposal + conflicts files."""

    def test_run_creates_outputs(self, patched_root,
                                  mock_workspace: Path) -> None:
        patched_root("w2_centralize_workspace_deps")
        result = mod.run(apply=False, strict=False)
        assert result["status"] == "OK"
        assert result["total_unique_deps"] >= 2  # serde + tokio at minimum
        # Proposal file exists
        proposal = mock_workspace / result["proposal_file"]
        assert proposal.exists()
        text = proposal.read_text(encoding="utf-8")
        assert text.startswith("[workspace.dependencies]")
        # Conflicts file exists
        conflicts = mock_workspace / result["conflicts_file"]
        assert conflicts.exists()
        json.loads(conflicts.read_text(encoding="utf-8"))  # valid JSON

    def test_strict_mode_fails_on_conflict(self, patched_root,
                                            mock_workspace: Path) -> None:
        patched_root("w2_centralize_workspace_deps")
        # Mock workspace has serde version conflict (1.0 vs 1.0.180)
        result = mod.run(apply=False, strict=True)
        assert result["status"] == "STRICT_FAIL"
        assert result["conflicts"] >= 1


# ─────────────────────────────────────────────────────────────────────────────
# Regression tests — apply mode (Bug fix 2026-05-12)
#
# Original bug: `--apply` was a stub that logged a warning and returned
# status=OK without modifying root Cargo.toml. When [workspace.dependencies]
# already had a pre-existing section (e.g., 106 hand-curated entries with
# `features=[...]`), the 99 deps from the proposal were silently dropped.
# Side effect: w2_propagate_workspace_inherit.py converted consumer deps to
# `.workspace = true`, which then failed `cargo check` with "dependency.X
# was not found in workspace.dependencies".
# ─────────────────────────────────────────────────────────────────────────────


class TestSectionBounds:
    """`_section_bounds` finds the [workspace.dependencies] section reliably."""

    def test_returns_none_when_section_absent(self) -> None:
        content = "[package]\nname = \"x\"\n"
        assert mod._section_bounds(content) is None

    def test_finds_section_at_eof(self) -> None:
        content = "[package]\nname = \"x\"\n\n[workspace.dependencies]\nfoo = \"1\"\n"
        bounds = mod._section_bounds(content)
        assert bounds is not None
        # body should contain `foo = "1"` and nothing else
        assert content[bounds[0] : bounds[1]].strip() == 'foo = "1"'

    def test_stops_at_next_section(self) -> None:
        content = (
            "[workspace.dependencies]\n"
            'foo = "1"\n'
            "\n"
            "[profile.dev]\n"
            "opt-level = 0\n"
        )
        bounds = mod._section_bounds(content)
        assert bounds is not None
        body = content[bounds[0] : bounds[1]]
        assert 'foo = "1"' in body
        assert "[profile.dev]" not in body
        assert "opt-level" not in body


class TestExistingDepsInSection:
    """`_existing_deps_in_section` extracts already-declared dep names."""

    def test_extracts_simple_and_complex_forms(self) -> None:
        body = textwrap.dedent("""
            serde = "1.0"
            tokio = { version = "1.35", features = ["full"] }
            # comment line
            another = "0.1"
        """)
        names = mod._existing_deps_in_section(body)
        assert names == {"serde", "tokio", "another"}

    def test_ignores_non_dep_lines(self) -> None:
        body = "# only comments\n\nfoo-bar = \"1.0\"\n"
        assert mod._existing_deps_in_section(body) == {"foo-bar"}


class TestApplyToRootCargoToml:
    """`apply_to_root_cargo_toml` is the actual mutation entry point.

    These tests prevent regression of the original stub-only behavior where
    `--apply` did nothing.
    """

    def _merged(self, *names_versions: tuple[str, str]) -> dict:
        return {
            name: [(ver, "crates/foo/Cargo.toml")]
            for name, ver in names_versions
        }

    def test_creates_section_when_absent(self, tmp_path: Path) -> None:
        """Fresh Cargo.toml with no [workspace.dependencies] → section appended."""
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text("[workspace]\nmembers = []\n", encoding="utf-8")
        result = mod.apply_to_root_cargo_toml(
            self._merged(("serde", "1.0"), ("tokio", "1.35")), cargo
        )
        assert result["status"] == "APPLIED"
        assert result["section_existed"] is False
        assert result["applied"] == 2
        text = cargo.read_text(encoding="utf-8")
        assert "[workspace.dependencies]" in text
        assert 'serde = "1.0"' in text
        assert 'tokio = "1.35"' in text

    def test_merges_missing_into_existing_section(self, tmp_path: Path) -> None:
        """Pre-existing section + new deps → appends missing only.

        This is the ORIGINAL BUG: section existed with 106 entries, the 99
        deps from the proposal were silently dropped.
        """
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(
            textwrap.dedent("""
                [workspace.dependencies]
                serde = { version = "1.0", features = ["derive"] }
                anyhow = "1.0"

                [profile.dev]
                opt-level = 0
            """).lstrip(),
            encoding="utf-8",
        )
        result = mod.apply_to_root_cargo_toml(
            self._merged(
                ("serde", "1.0"),          # already present — must skip
                ("anyhow", "1.0"),         # already present — must skip
                ("tokio", "1.35"),         # NEW — must add
                ("arc-swap", "1.6"),       # NEW — must add
            ),
            cargo,
        )
        assert result["status"] == "APPLIED"
        assert result["section_existed"] is True
        assert result["applied"] == 2
        assert result["skipped_existing"] == 2
        assert set(result["missing_names"]) == {"tokio", "arc-swap"}
        text = cargo.read_text(encoding="utf-8")
        # New deps were appended
        assert 'tokio = "1.35"' in text
        assert 'arc-swap = "1.6"' in text
        # Pre-existing rich entry survived
        assert 'serde = { version = "1.0", features = ["derive"] }' in text

    def test_preserves_features_on_existing_entries(self, tmp_path: Path) -> None:
        """Existing `{ version, features = [...] }` entries are NOT rewritten."""
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(
            textwrap.dedent("""
                [workspace.dependencies]
                tokio = { version = "1.35", features = ["full", "macros"] }
            """).lstrip(),
            encoding="utf-8",
        )
        mod.apply_to_root_cargo_toml(
            self._merged(("tokio", "1.40")),  # newer version proposed
            cargo,
        )
        after = cargo.read_text(encoding="utf-8")
        # The original line MUST be preserved verbatim
        assert 'tokio = { version = "1.35", features = ["full", "macros"] }' in after

    def test_creates_backup_before_mutation(self, tmp_path: Path) -> None:
        """A `.w2-bak.<TS>` backup is written before the new content."""
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text("[workspace]\nmembers = []\n", encoding="utf-8")
        result = mod.apply_to_root_cargo_toml(
            self._merged(("serde", "1.0")), cargo
        )
        assert result["status"] == "APPLIED"
        backup_path = Path(result["backup_path"])
        assert backup_path.exists()
        # Backup contains the ORIGINAL content (pre-mutation)
        assert "[workspace.dependencies]" not in backup_path.read_text(encoding="utf-8")
        assert "members = []" in backup_path.read_text(encoding="utf-8")

    def test_returns_no_op_when_nothing_missing(self, tmp_path: Path) -> None:
        """All proposed deps already present → status=NO_OP, no backup written."""
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text(
            "[workspace.dependencies]\nserde = \"1.0\"\n",
            encoding="utf-8",
        )
        result = mod.apply_to_root_cargo_toml(
            self._merged(("serde", "1.0")), cargo
        )
        assert result["status"] == "NO_OP"
        assert result["applied"] == 0
        assert result["backup_path"] is None

    def test_handles_missing_cargo_toml(self, tmp_path: Path) -> None:
        """Non-existent root Cargo.toml → status=ROOT_CARGO_MISSING, no crash."""
        cargo = tmp_path / "does-not-exist.toml"
        result = mod.apply_to_root_cargo_toml(
            self._merged(("serde", "1.0")), cargo
        )
        assert result["status"] == "ROOT_CARGO_MISSING"
        assert result["applied"] == 0
        assert not cargo.exists()  # not created as side effect
