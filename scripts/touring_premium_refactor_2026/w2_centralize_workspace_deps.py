#!/usr/bin/env python3
"""W2.1 — Find all unique external deps + propose [workspace.dependencies].

Parses every crates/*/Cargo.toml, extracts external deps with their versions,
and consolidates into a single [workspace.dependencies] section. Conflicting
versions are reported with all variants for manual resolution.

Output:
  - scripts/touring_premium_refactor_2026/staging/w2-workspace-deps.toml (proposal)
  - scripts/touring_premium_refactor_2026/staging/w2-dep-conflicts.json (if any)

Usage
-----
    python3 w2_centralize_workspace_deps.py              # report
    python3 w2_centralize_workspace_deps.py --apply      # write into root Cargo.toml
    python3 w2_centralize_workspace_deps.py --strict     # require major-version match
"""
from __future__ import annotations

import argparse
import json
import logging
import re
from collections import defaultdict
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

# Match `dep = "1.2"`, `dep = "1.2.3"`, `dep = { version = "1.2", ... }`
SIMPLE_DEP_RE = re.compile(r'^\s*([a-zA-Z][a-zA-Z0-9_-]+)\s*=\s*"([^"]+)"\s*$')
COMPLEX_DEP_RE = re.compile(
    r'^\s*([a-zA-Z][a-zA-Z0-9_-]+)\s*=\s*\{[^}]*version\s*=\s*"([^"]+)"'
)
SECTION_RE = re.compile(r'^\s*\[(.+?)\]\s*$')


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w2_centralize_workspace_deps", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--apply", action="store_true",
                   help="DESTRUCTIVE — modify root Cargo.toml [workspace.dependencies].")
    p.add_argument("--strict", action="store_true",
                   help="Fail if multiple major versions detected.")
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def parse_cargo_deps(cargo_path: Path) -> dict[str, list[tuple[str, str]]]:
    """Return {dep_name: [(version, source_file), ...]} for [dependencies], [dev-deps], [build-deps]."""
    deps: dict[str, list[tuple[str, str]]] = defaultdict(list)
    if not cargo_path.exists():
        return deps
    in_dep_section = False
    section = ""
    src_rel = str(cargo_path.relative_to(_ROOT))
    for line in cargo_path.read_text(encoding="utf-8", errors="replace").splitlines():
        if (m := SECTION_RE.match(line)):
            section = m.group(1)
            in_dep_section = section in (
                "dependencies", "dev-dependencies", "build-dependencies"
            )
            continue
        if not in_dep_section:
            continue
        if (m := SIMPLE_DEP_RE.match(line)):
            name, ver = m.group(1), m.group(2)
            if name.startswith("touring-"):  # internal — skip
                continue
            deps[name].append((ver, src_rel))
        elif (m := COMPLEX_DEP_RE.match(line)):
            name, ver = m.group(1), m.group(2)
            if name.startswith("touring-"):
                continue
            deps[name].append((ver, src_rel))
    return deps


def scan_workspace() -> dict[str, list[tuple[str, str]]]:
    """Scan every crates/*/Cargo.toml + return merged deps map."""
    merged: dict[str, list[tuple[str, str]]] = defaultdict(list)
    for cargo in _CRATES_DIR.glob("touring-*/Cargo.toml"):
        for name, versions in parse_cargo_deps(cargo).items():
            merged[name].extend(versions)
    return merged


def detect_conflicts(merged: dict[str, list[tuple[str, str]]]) -> dict[str, list]:
    """Return {dep: [versions]} where multiple distinct versions exist."""
    conflicts = {}
    for name, version_sources in merged.items():
        unique_versions = sorted({v for v, _ in version_sources})
        if len(unique_versions) > 1:
            conflicts[name] = [
                {"version": v,
                 "sources": [src for ver, src in version_sources if ver == v]}
                for v in unique_versions
            ]
    return conflicts


def propose_workspace_deps(merged: dict[str, list[tuple[str, str]]]) -> str:
    """Render proposed [workspace.dependencies] section in TOML."""
    lines = ["[workspace.dependencies]"]
    for name in sorted(merged):
        versions = sorted({v for v, _ in merged[name]})
        # Pick newest version heuristically (string-sort approximates semver for X.Y.Z)
        chosen = versions[-1]
        lines.append(f'{name} = "{chosen}"')
    return "\n".join(lines) + "\n"


def _section_bounds(content: str) -> tuple[int, int] | None:
    """Return (start_after_header, end_before_next_section) of `[workspace.dependencies]`.

    `start_after_header` is the offset right after the `[workspace.dependencies]\\n`
    line. `end_before_next_section` is the offset where the next top-level
    `[<other>]` section begins (or `len(content)` if it is the last section).

    Returns `None` when the section is absent.
    """
    header_re = re.compile(r"^\[workspace\.dependencies\]\s*\n", re.MULTILINE)
    header_match = header_re.search(content)
    if header_match is None:
        return None
    start = header_match.end()
    # Find next `[<something>]` header (not a `[[double-bracket]]` array-of-tables
    # which has different semantics — but for safety we accept both as "end").
    next_re = re.compile(r"^\[", re.MULTILINE)
    next_match = next_re.search(content, start)
    end = next_match.start() if next_match else len(content)
    return (start, end)


def _existing_deps_in_section(section_body: str) -> set[str]:
    """Extract dep names already declared in a `[workspace.dependencies]` body."""
    names: set[str] = set()
    for line in section_body.splitlines():
        m = SIMPLE_DEP_RE.match(line) or COMPLEX_DEP_RE.match(line)
        if m:
            names.add(m.group(1))
    return names


def apply_to_root_cargo_toml(
    merged: dict[str, list[tuple[str, str]]], root_cargo: Path
) -> dict:
    """Merge missing deps into root Cargo.toml `[workspace.dependencies]`.

    Behavior:
      - If section absent → append full proposal at EOF (preserves existing
        sections).
      - If section present → identify deps NOT already declared, append them
        as a marked block at the END of the section (before next `[...]`).
      - Existing entries are NEVER touched (preserves `features=[...]` overrides).

    Atomicity:
      - Writes `<Cargo.toml>.w2-bak.<ISO_TS>` backup BEFORE mutating.
      - On any exception, original is left intact (we write backup, then write
        new content — backup acts as snapshot).

    Returns:
      Dict with `applied`, `skipped_existing`, `missing_names`, `backup_path`,
      `section_existed`, `status` keys.
    """
    if not root_cargo.exists():
        return {
            "applied": 0,
            "skipped_existing": 0,
            "missing_names": [],
            "backup_path": None,
            "section_existed": False,
            "status": "ROOT_CARGO_MISSING",
        }

    original = root_cargo.read_text(encoding="utf-8")
    bounds = _section_bounds(original)
    section_existed = bounds is not None

    existing_deps: set[str] = set()
    if bounds is not None:
        section_body = original[bounds[0] : bounds[1]]
        existing_deps = _existing_deps_in_section(section_body)

    # Identify deps to append (in proposal but not already in section)
    missing: list[tuple[str, str]] = []
    for name in sorted(merged):
        if name in existing_deps:
            continue
        versions = sorted({v for v, _ in merged[name]})
        chosen = versions[-1]
        missing.append((name, chosen))

    if not missing:
        return {
            "applied": 0,
            "skipped_existing": len(existing_deps),
            "missing_names": [],
            "backup_path": None,
            "section_existed": section_existed,
            "status": "NO_OP",
        }

    # Build the block to insert.
    ts = datetime.now(UTC).isoformat()
    block_lines = [
        "",
        f"# Auto-merged by w2_centralize_workspace_deps.py at {ts}",
        f"# {len(missing)} dep(s) added; existing entries preserved with their features.",
    ]
    block_lines.extend(f'{name} = "{ver}"' for name, ver in missing)
    block_lines.append("")
    block = "\n".join(block_lines)

    if bounds is None:
        # No section — append at EOF as fresh section.
        proposal = propose_workspace_deps(merged)
        new_content = (
            original.rstrip()
            + "\n\n"
            + f"# Auto-created by w2_centralize_workspace_deps.py at {ts}\n"
            + proposal
        )
    else:
        # Section exists — splice block right BEFORE next `[<section>]`.
        end = bounds[1]
        new_content = original[:end] + block + original[end:]

    # Backup BEFORE write (atomicity boundary).
    ts_safe = ts.replace(":", "").replace("-", "").split(".")[0]
    backup_path = root_cargo.with_name(f"{root_cargo.name}.w2-bak.{ts_safe}")
    backup_path.write_text(original, encoding="utf-8")
    root_cargo.write_text(new_content, encoding="utf-8")

    return {
        "applied": len(missing),
        "skipped_existing": len(existing_deps),
        "missing_names": [name for name, _ in missing],
        "backup_path": str(backup_path),
        "section_existed": section_existed,
        "status": "APPLIED",
    }


def run(apply: bool, strict: bool) -> dict:
    """Scan + propose; optionally apply."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    merged = scan_workspace()
    conflicts = detect_conflicts(merged)
    proposal_path = _STAGING_DIR / "w2-workspace-deps.toml"
    proposal_path.write_text(propose_workspace_deps(merged), encoding="utf-8")
    conflicts_path = _STAGING_DIR / "w2-dep-conflicts.json"
    conflicts_path.write_text(
        json.dumps(conflicts, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    if strict and conflicts:
        LOGGER.error("strict mode: %d dep conflicts detected", len(conflicts))
        return {"status": "STRICT_FAIL", "conflicts": len(conflicts),
                "details": str(conflicts_path)}

    apply_result: dict = {}
    if apply:
        root_cargo = _ROOT / "Cargo.toml"
        apply_result = apply_to_root_cargo_toml(merged, root_cargo)
        LOGGER.info(
            "apply: status=%s applied=%d skipped_existing=%d",
            apply_result.get("status"),
            apply_result.get("applied", 0),
            apply_result.get("skipped_existing", 0),
        )

    return {
        "status": "OK",
        "timestamp": datetime.now(UTC).isoformat(),
        "total_unique_deps": len(merged),
        "conflicts_count": len(conflicts),
        "proposal_file": str(proposal_path.relative_to(_ROOT)),
        "conflicts_file": str(conflicts_path.relative_to(_ROOT)),
        "top_5_unique": sorted(merged.keys())[:5],
        "apply": apply_result if apply else None,
    }


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args.apply, args.strict)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0 if result["status"] == "OK" else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
