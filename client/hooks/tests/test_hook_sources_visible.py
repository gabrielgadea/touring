"""test_hook_sources_visible.py — a hook source never carries an invisible character.

`agent_model_gate.py` held a literal U+FEFF inside its frontmatter regex (`r"\\A<BOM>?---…"`, an optional
byte-order mark, intended) and its test held another in a string.  Both are invisible in an editor and in a
diff: a reviewer reading `\\A?---` sees a quantifier on nothing, and a tool that strips BOMs (or a copy through
a channel that does) silently turns the optional mark into a different pattern.  The escape `\\ufeff` says
the same thing where everyone can read it (the `re` module interprets it inside a raw string).

The guard is the Unicode category Cf (format controls: BOM, zero-width space/joiner, bidi overrides such as
U+202E, soft hyphen, word joiner).  Write them as escapes.
"""
from __future__ import annotations

import unicodedata

from helpers import CLIENT_HOOKS

SUFFIXES = {".py", ".sh"}


def _invisible_characters() -> list[str]:
    found = []
    for path in sorted(CLIENT_HOOKS.rglob("*")):
        if not path.is_file() or path.suffix not in SUFFIXES or "_aposentados" in path.parts:
            continue
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            for col, char in enumerate(line, start=1):
                if unicodedata.category(char) == "Cf":
                    found.append(f"{path.relative_to(CLIENT_HOOKS)}:{lineno}:{col} U+{ord(char):04X} {unicodedata.name(char, '?')}")
    return found


def test_hook_sources_hold_no_invisible_characters() -> None:
    found = _invisible_characters()
    assert not found, "write these as escapes (\\ufeff, \\u200b, …):\n  " + "\n  ".join(found)


def test_the_guard_sees_what_it_guards_against(tmp_path) -> None:
    """Control: the scan really flags a BOM and a bidi override when one is present."""
    sample = tmp_path / "x.py"
    sample.write_text('PATTERN = r"\\A\ufeff?---"\nNAME = "a\u202eb"\n', encoding="utf-8")
    flagged = [c for line in sample.read_text(encoding="utf-8").splitlines() for c in line if unicodedata.category(c) == "Cf"]
    assert flagged == ["\ufeff", "\u202e"]
