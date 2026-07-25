#!/usr/bin/env python3
"""Shared helpers for TACO-cross-audit.

Pure standard library — no third-party dependencies. Three responsibilities:

1. Wrap the ``touring`` CLI with a fail-open fallback.
2. Walk a code tree, yielding source files by extension.
3. Run a shell command and capture stdout/stderr/exit code — the building block
   of the proof discipline (a claim needs an executed command behind it).

Importable as a module; running it directly performs an environment self-check.
"""
from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any, Iterator, NamedTuple

# Code-file extensions the audit walks.
CODE_EXTENSIONS: frozenset[str] = frozenset({
    ".rs", ".py", ".ts", ".tsx", ".js", ".jsx", ".go", ".c", ".cpp", ".cc",
    ".h", ".hpp", ".java", ".kt", ".swift", ".rb", ".sh",
})

# Directories never worth auditing — build output, caches, vendored deps.
# NOTE: ".claude" is deliberately absent — it is the ancestor of every skill,
# so skipping it would make walk_code_files skip the entire target tree when
# the audited project lives under ~/.claude/. Caches there are excluded by
# extension (CODE_EXTENSIONS) instead.
SKIP_DIRS: frozenset[str] = frozenset({
    "target", "node_modules", ".git", "__pycache__", ".venv", "venv",
    "dist", "build", ".ruff_cache", ".mypy_cache", ".pytest_cache",
    ".touring", ".touring-cache",
})


class CommandResult(NamedTuple):
    """The captured outcome of a shell command — the unit of executed proof."""

    command: str
    exit_code: int
    stdout: str
    stderr: str
    timed_out: bool


def run(command: list[str], cwd: Path | None = None,
        timeout: float = 120.0) -> CommandResult:
    """Run a command and capture its outcome without raising.

    A non-zero exit is data, not an exception — the audit must *observe*
    failure, not crash on it.
    """
    try:
        proc = subprocess.run(
            command, cwd=cwd, capture_output=True, text=True, timeout=timeout,
        )
        return CommandResult(" ".join(command), proc.returncode,
                             proc.stdout, proc.stderr, False)
    except subprocess.TimeoutExpired:
        return CommandResult(" ".join(command), -1, "", "timed out", True)
    except (FileNotFoundError, OSError) as exc:
        return CommandResult(" ".join(command), -1, "", str(exc), False)


def touring(*args: str, timeout: float = 10.0) -> dict[str, Any] | list[Any] | None:
    """Run a ``touring`` CLI command and parse its JSON output.

    Returns ``None`` when touring is absent, the daemon is down, the call times
    out, or the output is not JSON — callers treat ``None`` as ``daemon_degraded``.
    """
    result = run(["touring", *args], timeout=timeout)
    if result.exit_code != 0 or not result.stdout.strip():
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def touring_available() -> bool:
    """Return True when the touring daemon answers a doctor probe."""
    return touring("doctor", "-j", timeout=8.0) is not None


def walk_code_files(root: Path,
                    extensions: frozenset[str] = CODE_EXTENSIONS) -> Iterator[Path]:
    """Yield every source file under ``root``, skipping build/cache directories."""
    if root.is_file():
        if root.suffix in extensions:
            yield root
        return
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if path.suffix in extensions:
            yield path


def detect_project_kind(root: Path) -> str:
    """Identify a project's toolchain from its manifest files.

    Returns one of: ``rust``, ``python``, ``node``, ``unknown``.
    """
    if (root / "Cargo.toml").is_file():
        return "rust"
    if (root / "pyproject.toml").is_file() or (root / "setup.py").is_file():
        return "python"
    if (root / "package.json").is_file():
        return "node"
    return "unknown"


def _self_check() -> int:
    """Smoke-test the helpers against the current directory."""
    here = Path.cwd()
    files = list(walk_code_files(here))
    print("lib.py self-check:")
    print(f"  code files under cwd: {len(files)}")
    print(f"  project kind:         {detect_project_kind(here)}")
    print(f"  touring available:    {touring_available()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(_self_check())
