#!/usr/bin/env python3
"""Every `touring …` command cited in docs/agents/*.md must exist in the binary.

Why (retro 11/11/2026): docs/agents/ tells the engineering skills which commands to run
against the issue tracker (`touring decompose`). While writing it, `touring decompose list`
was guessed and does not exist; only running `--help` by hand caught it. A doc that names a
subcommand or a flag the CLI does not have fails silently in the agent that follows it.

What is checked, per inline-code or fenced span that starts with `touring `:
  1. the subcommand path (`decompose`, `decompose claim`, …) answers `--help` with exit 0;
  2. every `--flag` written in that span appears in that subcommand's help text.

The binary is `$TOURING_BIN`, else the newest of target/{release,debug}/touring.

Wiring: the file name puts it in the rite glob of `scripts/propagate-release.sh`
(`test_propagate_release*.py`). The scanner tests run in gate 1/6; the per-command test is
`post_build`, so it runs right after the release build, against the binary being shipped.
Run by hand: `python3 -m pytest scripts/test_propagate_release_docs_agents.py -q -rs`.
Without any binary the per-command test SKIPS with the reason rather than pass.
"""
from __future__ import annotations

import os
import re
import subprocess
from functools import lru_cache
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
DOCS = sorted((REPO_ROOT / "docs" / "agents").glob("*.md"))

_SPAN = re.compile(r"```[^\n]*\n(.*?)```|`([^`\n]+)`", re.S)
_COMMAND = re.compile(r"(?:^|(?<=[\s(]))touring((?:\s+[a-z][a-z0-9-]*)+)((?:[^\n`]|\\\n)*)", re.M)
_FLAG = re.compile(r"(?<![\w-])(--[a-z][a-z0-9-]*)")
#: How many words after `touring` are tried as a subcommand path. The CLI nests two levels
#: (`decompose claim`); the third word is kept so `resolve` can tell an argument from a
#: subcommand by asking the binary, instead of guessing from the doc.
_MAX_DEPTH = 3


def _binary() -> Path | None:
    env = os.environ.get("TOURING_BIN")
    if env:
        return Path(env) if Path(env).is_file() else None
    built = [p for p in (REPO_ROOT / "target" / k / "touring" for k in ("release", "debug")) if p.is_file()]
    return max(built, key=lambda p: p.stat().st_mtime) if built else None


@lru_cache(maxsize=None)
def _help(binary: str, path: tuple[str, ...]) -> tuple[int, str]:
    proc = subprocess.run([binary, *path, "--help"], capture_output=True, text=True, timeout=30, check=False)
    return proc.returncode, proc.stdout + proc.stderr


def cited_commands(text: str) -> list[tuple[tuple[str, ...], tuple[str, ...]]]:
    """(subcommand words, flags) for every `touring …` invocation inside a code span."""
    found = []
    for fenced, inline in _SPAN.findall(text):
        span = (fenced or inline).replace("\\\n", " ")
        for m in _COMMAND.finditer(span):
            words = tuple(m.group(1).split())[:_MAX_DEPTH]
            flags = tuple(dict.fromkeys(_FLAG.findall(m.group(2))))
            found.append((words, flags))
    return found


def resolve(binary: str, words: tuple[str, ...]) -> tuple[str, ...] | None:
    """Longest prefix of `words` that is a real subcommand path; None when not even the first is.

    `touring decompose get <task_id>` cites `decompose get`; a trailing lowercase word such as
    a task type (`create feature`) is an argument, so the path shrinks until `--help` exits 0.
    """
    for depth in range(len(words), 0, -1):
        if _help(binary, words[:depth])[0] == 0:
            return words[:depth]
    return None


def _cases() -> list[tuple[str, tuple[str, ...], tuple[str, ...]]]:
    return [(doc.name, words, flags) for doc in DOCS for words, flags in cited_commands(doc.read_text(encoding="utf-8"))]


def test_the_docs_cite_at_least_one_command() -> None:
    """A scanner that finds nothing would make every other test here pass vacuously."""
    assert DOCS, "docs/agents/*.md not found"
    assert any(words[0] == "decompose" for _, words, _ in _cases()), "no `touring decompose …` citation was parsed"


def test_scanner_reads_inline_and_fenced_spans() -> None:
    text = "use `touring decompose claim <t> --owner <id> --subtask S-01`.\n```\ntouring decompose add t S \\\n  --depends-on=S-00\n```\n"
    assert cited_commands(text) == [
        (("decompose", "claim"), ("--owner", "--subtask")),
        (("decompose", "add", "t"), ("--depends-on",)),
    ]


@pytest.mark.post_build
@pytest.mark.parametrize("doc,words,flags", _cases(), ids=lambda v: " ".join(v) if isinstance(v, tuple) else v)
def test_cited_command_exists(doc: str, words: tuple[str, ...], flags: tuple[str, ...]) -> None:
    binary = _binary()
    if binary is None:
        pytest.skip("no touring binary (set TOURING_BIN or build target/{release,debug}/touring)")
    path = resolve(str(binary), words)
    assert path is not None and len(path) >= min(2, len(words)), (
        f"{doc}: `touring {' '.join(words)}` — `{binary.name} {' '.join(words[:2])} --help` "
        f"did not exit 0; the subcommand does not exist. Fix the doc or the CLI."
    )
    help_text = _help(str(binary), path)[1]
    missing = [f for f in flags if f != "--help" and f not in help_text]
    assert not missing, (
        f"{doc}: `touring {' '.join(path)}` has no {missing}; "
        f"its flags are {sorted(set(_FLAG.findall(help_text)))}."
    )
