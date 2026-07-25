#!/usr/bin/env python3
"""Shared helpers for TACO-skilling.

Pure standard library — no third-party dependencies — so a copy is portable to
any generated skill. Two responsibilities:

1. Parse Claude Code JSONL session transcripts (``~/.claude/projects/*/*.jsonl``).
2. Wrap the ``touring`` CLI with a fail-open fallback, so callers degrade
   gracefully when the daemon is down rather than crashing.

Importable as a module; running it directly performs an environment self-check.
"""
from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any, Iterator

HOME = Path.home()
SKILLS_DIR = HOME / ".claude" / "skills"
PROJECTS_DIR = HOME / ".claude" / "projects"


# --- transcript parsing ---------------------------------------------------

def iter_jsonl(path: Path) -> Iterator[dict[str, Any]]:
    """Yield each line of a JSONL file as a dict, skipping malformed lines.

    Fail-open: a corrupt or truncated line is skipped, not raised — session
    transcripts are frequently appended to while being read.
    """
    try:
        with path.open("r", encoding="utf-8", errors="replace") as handle:
            for line in handle:
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(obj, dict):
                    yield obj
    except OSError:
        return


def iter_transcripts(projects_dir: Path | None = None) -> Iterator[Path]:
    """Yield every Claude Code session transcript (``*.jsonl``) path."""
    root = projects_dir or PROJECTS_DIR
    if not root.is_dir():
        return
    yield from sorted(root.glob("*/*.jsonl"))


def message_text(entry: dict[str, Any]) -> str:
    """Extract human-readable text from a transcript entry, best-effort.

    Transcript schemas vary across Claude Code versions; this reads the common
    shapes (a string ``content``, or a list of content blocks) and returns ``""``
    when it cannot find text rather than raising.
    """
    msg = entry.get("message")
    if not isinstance(msg, dict):
        return ""
    content = msg.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for block in content:
            if isinstance(block, dict) and isinstance(block.get("text"), str):
                parts.append(block["text"])
        return "\n".join(parts)
    return ""


# --- touring CLI wrapper --------------------------------------------------

def touring(*args: str, timeout: float = 10.0) -> dict[str, Any] | list[Any] | None:
    """Run a ``touring`` CLI command and parse its JSON output.

    Returns the parsed JSON, or ``None`` when touring is absent, the daemon is
    down, the call times out, or the output is not JSON. Callers treat ``None``
    as ``daemon_degraded`` and fall back to filesystem scans.
    """
    try:
        proc = subprocess.run(
            ["touring", *args],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
        return None
    out = proc.stdout.strip()
    if not out:
        return None
    try:
        return json.loads(out)
    except json.JSONDecodeError:
        return None


def touring_available() -> bool:
    """Return True when the touring daemon answers a doctor probe."""
    return touring("doctor", "-j", timeout=8.0) is not None


# --- skill inventory ------------------------------------------------------

def read_frontmatter(skill_md: Path) -> dict[str, str]:
    """Parse the YAML frontmatter of a SKILL.md into a flat ``key: value`` dict.

    A deliberately minimal parser — it handles the single-line ``key: value``
    shape skills use, plus folded continuation lines, and avoids a PyYAML
    dependency. Block scalars are not supported; skills keep name/description
    on single (possibly long) lines.
    """
    fields: dict[str, str] = {}
    try:
        text = skill_md.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return fields
    if not text.startswith("---"):
        return fields
    end = text.find("\n---", 3)
    if end == -1:
        return fields
    block = text[3:end]
    current_key: str | None = None
    for raw in block.splitlines():
        if raw and not raw[0].isspace() and ":" in raw:
            key, _, value = raw.partition(":")
            current_key = key.strip()
            fields[current_key] = value.strip()
        elif current_key and raw.strip():
            fields[current_key] = (fields[current_key] + " " + raw.strip()).strip()
    return fields


def body_line_count(skill_md: Path) -> int:
    """Count SKILL.md body lines, excluding the frontmatter block."""
    try:
        text = skill_md.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return 0
    if text.startswith("---"):
        end = text.find("\n---", 3)
        if end != -1:
            newline = text.find("\n", end + 1)
            text = text[newline + 1:] if newline != -1 else ""
    return len(text.splitlines())


def list_skills(skills_dir: Path | None = None) -> list[dict[str, str]]:
    """Return every installed skill as a ``{name, dir, description}`` dict.

    A directory counts as a skill when it contains a SKILL.md.
    """
    root = skills_dir or SKILLS_DIR
    skills: list[dict[str, str]] = []
    if not root.is_dir():
        return skills
    for entry in sorted(root.iterdir()):
        skill_md = entry / "SKILL.md"
        if not skill_md.is_file():
            continue
        frontmatter = read_frontmatter(skill_md)
        skills.append({
            "name": frontmatter.get("name", entry.name),
            "dir": str(entry),
            "description": frontmatter.get("description", ""),
        })
    return skills


def _self_check() -> int:
    """Smoke-test the helpers against the live environment."""
    skills = list_skills()
    transcripts = list(iter_transcripts())
    print("lib.py self-check:")
    print(f"  skills discovered: {len(skills)}")
    print(f"  transcripts found: {len(transcripts)}")
    print(f"  touring available: {touring_available()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(_self_check())
