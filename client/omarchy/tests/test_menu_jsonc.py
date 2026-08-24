"""Tests for client/omarchy/extensions/omarchy-menu.jsonc."""

from __future__ import annotations

import json
from pathlib import Path

OMARCHY_DIR = Path(__file__).parent.parent
MENU_FILE = OMARCHY_DIR / "extensions" / "omarchy-menu.jsonc"
DECK_FILE = OMARCHY_DIR / "skills-deck" / "deck.json"

KNOWN_BINARIES = {"omarchy-skill-run", "omarchy-launch-or-focus-tui", "omarchy-launch-webapp"}
REQUIRED_IDS = {"skills.audit", "skills.moc", "skills.inbox", "skills.herdr"}


def _strip_jsonc_comments(text: str) -> str:
    """Strip // line comments from JSONC (handles // not inside double-quoted strings)."""
    result: list[str] = []
    for line in text.splitlines():
        in_string = False
        i = 0
        out: list[str] = []
        while i < len(line):
            ch = line[i]
            if ch == "\\" and in_string and i + 1 < len(line):
                out.append(ch)
                out.append(line[i + 1])
                i += 2
                continue
            if ch == '"':
                in_string = not in_string
            if not in_string and line[i : i + 2] == "//":
                break
            out.append(ch)
            i += 1
        result.append("".join(out))
    return "\n".join(result)


def _load_menu() -> dict:
    raw = MENU_FILE.read_text(encoding="utf-8")
    stripped = _strip_jsonc_comments(raw)
    return json.loads(stripped)


def test_strip_comments_yields_valid_json() -> None:
    """Stripping // comments from the JSONC must produce parseable JSON."""
    data = _load_menu()
    assert isinstance(data, dict), "Top-level must be an object"
    assert len(data) > 0, "Menu must have at least one entry"


def test_required_ids_present() -> None:
    """skills.audit, .moc, .inbox, .herdr must all be present."""
    data = _load_menu()
    missing = REQUIRED_IDS - set(data.keys())
    assert not missing, f"Missing required menu ids: {missing}"


def test_every_action_binary_is_known() -> None:
    """The first token of every 'action' value must be a known binary."""
    data = _load_menu()
    unknown: list[str] = []
    for key, entry in data.items():
        action = entry.get("action")
        if not action:
            continue  # submenu rows have no action
        # Strip leading flags (--herdr, --dry-run) to get the binary name
        tokens = action.split()
        binary = next((t for t in tokens if not t.startswith("-")), "")
        if binary not in KNOWN_BINARIES:
            unknown.append(f"{key!r}: {binary!r}")
    assert not unknown, f"Unknown action binaries: {unknown}"


def test_no_bypass_permissions_anywhere() -> None:
    """bypassPermissions must not appear in menu or deck.json."""
    menu_raw = MENU_FILE.read_text(encoding="utf-8")
    assert "bypassPermissions" not in menu_raw, \
        "bypassPermissions found in omarchy-menu.jsonc"

    deck_raw = DECK_FILE.read_text(encoding="utf-8")
    assert "bypassPermissions" not in deck_raw, \
        "bypassPermissions found in deck.json"
