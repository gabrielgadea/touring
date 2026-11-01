"""Tests for client/omarchy/skills-deck/ plugin manifest and deck.json schema."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

OMARCHY_DIR  = Path(__file__).parent.parent
PLUGIN_DIR   = OMARCHY_DIR / "skills-deck"
MANIFEST_FILE = PLUGIN_DIR / "manifest.json"
DECK_FILE     = PLUGIN_DIR / "deck.json"
VALIDATOR     = OMARCHY_DIR / "vendor" / "omarchy-plugin-validate"

REQUIRED_MANIFEST_FIELDS = {
    "schemaVersion", "id", "name", "version", "kinds", "entryPoints",
}
REQUIRED_TILE_FIELDS = {"label", "skill", "model", "effort", "mode", "cwd", "herdr"}
VALID_MODELS  = {"fable", "opus", "sonnet", "haiku"}
VALID_EFFORTS = {"low", "medium", "high", "xhigh", "max"}
VALID_MODES   = {"plan", "acceptEdits", "auto", "default"}


def test_vendor_validator_exit_0() -> None:
    """vendor/omarchy-plugin-validate must exit 0 for skills-deck/."""
    assert VALIDATOR.exists(), f"Validator not found: {VALIDATOR}"
    assert PLUGIN_DIR.is_dir(), f"Plugin dir not found: {PLUGIN_DIR}"
    result = subprocess.run(
        [str(VALIDATOR), str(PLUGIN_DIR)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, (
        f"omarchy-plugin-validate failed:\nstdout: {result.stdout}\nstderr: {result.stderr}"
    )


def test_manifest_fields() -> None:
    """manifest.json must have all required fields with correct values."""
    data = json.loads(MANIFEST_FILE.read_text(encoding="utf-8"))

    missing = REQUIRED_MANIFEST_FIELDS - set(data.keys())
    assert not missing, f"Missing manifest fields: {missing}"

    assert data["schemaVersion"] == 1, "schemaVersion must be the number 1"
    assert data["id"] == "gabriel.skills-deck", "id mismatch"
    assert not data["id"].startswith("omarchy."), "id must not use omarchy.* namespace"
    assert "bar-widget" in data["kinds"], "kinds must include bar-widget"
    assert "barWidget" in data["entryPoints"], "entryPoints must have barWidget"

    bar = data.get("barWidget", {})
    assert bar.get("allowMultiple") is False, "allowMultiple must be false"
    assert bar.get("defaultSection") in {"left", "center", "right"}, \
        "defaultSection must be left|center|right"
    assert bar.get("category"), "barWidget.category must be non-empty"

    # entry point file must exist
    entry = data["entryPoints"]["barWidget"]
    assert (PLUGIN_DIR / entry).is_file(), f"entryPoints.barWidget file not found: {entry}"


def test_deck_json_schema() -> None:
    """deck.json must be a non-empty array of valid tile objects."""
    tiles = json.loads(DECK_FILE.read_text(encoding="utf-8"))
    assert isinstance(tiles, list), "deck.json must be a JSON array"
    assert len(tiles) > 0, "deck.json must have at least one tile"

    for i, tile in enumerate(tiles):
        missing = REQUIRED_TILE_FIELDS - set(tile.keys())
        assert not missing, f"Tile [{i}] missing fields: {missing}"

        assert tile["model"]  in VALID_MODELS,  f"Tile [{i}] invalid model: {tile['model']!r}"
        assert tile["effort"] in VALID_EFFORTS, f"Tile [{i}] invalid effort: {tile['effort']!r}"
        assert tile["mode"]   in VALID_MODES,   f"Tile [{i}] invalid mode: {tile['mode']!r}"
        assert tile["mode"] != "bypassPermissions", \
            f"Tile [{i}]: bypassPermissions is not allowed"
        assert isinstance(tile["herdr"], bool),  f"Tile [{i}] herdr must be bool"
        assert isinstance(tile["label"], str) and tile["label"], \
            f"Tile [{i}] label must be a non-empty string"
