#!/usr/bin/env python3
"""Shared CLI primitives for the Touring arsenal crate/workspace diagnostics.

Canonical, DRY helpers imported by crate_arch_diag.py, crate_50dim_matrix.py and
workspace_arch_diag.py so the whole arsenal parses flags + resolves a crate
name/path identically — no per-script drift. Sibling module, imported the same
way as report_contract (after each script's `sys.path.insert(parent)`).
"""
import os
import shutil
from pathlib import Path

# Workspace root for bare crate-name lookup. The canonical source moved from
# `~/.claude/rust` to `~/projects/touring` (per-project toolchain topology); the
# legacy path is kept as a fallback so an older layout still resolves.
_RUST_ROOT_CANDIDATES = (
    Path.home() / "projects/touring",
    Path.home() / ".claude/rust",
)
RUST_ROOT = next(
    (p for p in _RUST_ROOT_CANDIDATES if (p / "crates").is_dir()),
    _RUST_ROOT_CANDIDATES[0],
)


def resolve_quality_bin() -> str | None:
    """Resolve the `touring-quality` binary, or None when it is not installed.

    Resolution order: the `TOURING_QUALITY_BIN` override, then `PATH` (where
    `update-touring` puts the `~/.local/bin` symlink), then the release target of
    each known workspace root. Returning None instead of a stale absolute path
    lets callers fail loud with an actionable message rather than silently
    scoring nothing — the failure mode when `~/.claude/rust` went away.
    """
    override = os.environ.get("TOURING_QUALITY_BIN")
    if override and Path(override).is_file():
        return override
    on_path = shutil.which("touring-quality")
    if on_path:
        return on_path
    for root in _RUST_ROOT_CANDIDATES:
        candidate = root / "target/release/touring-quality"
        if candidate.is_file():
            return str(candidate)
    return None


def require_quality_bin() -> str:
    """`resolve_quality_bin()` or exit with the remediation command."""
    found = resolve_quality_bin()
    if not found:
        raise SystemExit(
            "touring-quality binary not found (checked $TOURING_QUALITY_BIN, PATH, "
            f"{', '.join(str(p / 'target/release') for p in _RUST_ROOT_CANDIDATES)}); "
            "run `update-touring` first"
        )
    return found


def resolve_crate(token: str) -> str | None:
    """Resolve a crate NAME or a rel/abs PATH to the crate dir that holds `src/`.

    A path form (contains '/') resolves first, as given — so the documented
    `<crate-rel-path>` contract never regresses. A bare crate name is looked up
    under `<cwd>/crates/` then the workspace `~/projects/touring/crates/`, making name
    resolution cwd-independent. Returns the resolved path string, or None when no
    candidate contains a `src/` dir — so callers can fail loud instead of scoring
    a phantom 0-file crate.
    """
    candidates = [Path(token)]  # as given: path form (rel/abs) resolves first
    if "/" not in token:  # bare crate name → workspace crates/ dirs
        candidates += [Path.cwd() / "crates" / token, RUST_ROOT / "crates" / token]
    for c in candidates:
        if (c / "src").is_dir():
            return str(c)
    return None


def split_flags(argv: list[str], bool_flags: dict[str, str]) -> tuple[list[str], set[str]]:
    """Split argv into (positionals, canonical-flag-set); fail loud on unknown flag.

    `bool_flags` maps each accepted flag token to its canonical name, e.g.
    {"-j": "json", "--json": "json"}. `-h`/`--help` are always accepted and map to
    "help" (the caller prints its own USAGE and exits). A token starting with '-'
    that is not recognized raises SystemExit — the prior scripts passed such tokens
    through as positional paths, producing phantom 0-file targets (the root defect).
    """
    positionals: list[str] = []
    flags: set[str] = set()
    for a in argv:
        if a in ("-h", "--help"):
            flags.add("help")
        elif a in bool_flags:
            flags.add(bool_flags[a])
        elif a.startswith("-"):
            raise SystemExit(f"unknown flag: {a}")
        else:
            positionals.append(a)
    return positionals, flags
