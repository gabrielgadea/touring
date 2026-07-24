#!/usr/bin/env python3
"""W2.3 — Convert literal dep versions in per-crate Cargo.toml → .workspace = true.

After W2.1 centralizes [workspace.dependencies], each crates/*/Cargo.toml
should reference workspace inheritance instead of pinning its own version.

Usage
-----
    python3 w2_propagate_workspace_inherit.py            # report
    python3 w2_propagate_workspace_inherit.py --apply    # rewrite in-place
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

SIMPLE_DEP_RE = re.compile(r'^(\s*)([a-zA-Z][a-zA-Z0-9_-]+)\s*=\s*"([^"]+)"\s*$')
COMPLEX_DEP_RE = re.compile(
    r'^(\s*)([a-zA-Z][a-zA-Z0-9_-]+)\s*=\s*\{([^}]*)\}\s*$'
)
SECTION_RE = re.compile(r'^\s*\[(.+?)\]\s*$')


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w2_propagate_workspace_inherit", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--apply", action="store_true")
    p.add_argument("--workspace-deps", type=Path,
                   default=_STAGING_DIR / "w2-workspace-deps.toml")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def load_workspace_deps(path: Path) -> set[str]:
    """Parse w2-workspace-deps.toml → set of dep names."""
    if not path.exists():
        return set()
    deps: set[str] = set()
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = SIMPLE_DEP_RE.match(line)
        if m and not m.group(2).startswith("["):
            deps.add(m.group(2))
    return deps


def rewrite_complex_dep(inner: str) -> str | None:
    """`{ version = "...", features = [...], ... }` → workspace=true."""
    if "version" not in inner:
        return None
    parts: list[str] = ["workspace = true"]
    if (m := re.search(r"features\s*=\s*\[[^\]]*\]", inner)):
        parts.append(m.group(0))
    if (m := re.search(r"optional\s*=\s*(true|false)", inner)):
        parts.append(m.group(0))
    if (m := re.search(r"default-features\s*=\s*(true|false)", inner)):
        parts.append(m.group(0))
    return "{ " + ", ".join(parts) + " }"


def analyze_cargo(cargo_path: Path,
                    workspace_deps: set[str]) -> list[dict]:
    """Find rewrite targets."""
    if not cargo_path.exists():
        return []
    proposals: list[dict] = []
    in_dep_section = False
    for lineno, line in enumerate(
        cargo_path.read_text(encoding="utf-8", errors="replace").splitlines(),
        start=1,
    ):
        if (m := SECTION_RE.match(line)):
            in_dep_section = m.group(1) in (
                "dependencies", "dev-dependencies", "build-dependencies"
            )
            continue
        if not in_dep_section:
            continue
        if (m := SIMPLE_DEP_RE.match(line)):
            indent, name, _ver = m.groups()
            if name in workspace_deps and not name.startswith("touring-"):
                proposals.append({"lineno": lineno, "current": line,
                                   "proposed": f'{indent}{name} = '
                                              '{ workspace = true }'})
        elif (m := COMPLEX_DEP_RE.match(line)):
            indent, name, inner = m.groups()
            if name in workspace_deps and not name.startswith("touring-"):
                rewritten = rewrite_complex_dep(inner)
                if rewritten:
                    proposals.append({"lineno": lineno, "current": line,
                                       "proposed": f"{indent}{name} = {rewritten}"})
    return proposals


def apply_rewrites(cargo_path: Path, proposals: list[dict]) -> int:
    """Apply rewrites in-place."""
    if not proposals:
        return 0
    lines = cargo_path.read_text(encoding="utf-8",
                                   errors="replace").splitlines(keepends=True)
    by_line = {p["lineno"]: p["proposed"] for p in proposals}
    new_lines: list[str] = []
    for i, ln in enumerate(lines, start=1):
        if i in by_line:
            new_lines.append(by_line[i] + ("\n" if ln.endswith("\n") else ""))
        else:
            new_lines.append(ln)
    cargo_path.write_text("".join(new_lines), encoding="utf-8")
    return len(proposals)


def run(args: argparse.Namespace) -> dict:
    """Scan + propose + optionally apply."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    workspace_deps = load_workspace_deps(args.workspace_deps)
    proposals_per_crate: dict[str, list[dict]] = {}
    applied: dict[str, int] = {}
    for cargo in sorted(_CRATES_DIR.glob("touring-*/Cargo.toml")):
        proposals = analyze_cargo(cargo, workspace_deps)
        if proposals:
            rel = str(cargo.relative_to(_ROOT))
            proposals_per_crate[rel] = proposals
            if args.apply:
                applied[rel] = apply_rewrites(cargo, proposals)
    report = {
        "script": "w2_propagate_workspace_inherit",
        "wave": "W2", "subtask_refs": ["W2.3"],
        "timestamp": datetime.now(UTC).isoformat(),
        "apply": args.apply,
        "workspace_deps_count": len(workspace_deps),
        "totals": {
            "crates_to_rewrite": len(proposals_per_crate),
            "total_rewrites": sum(len(v) for v in proposals_per_crate.values()),
            "applied": sum(applied.values()),
        },
        "proposals": proposals_per_crate,
    }
    json_path = args.output_dir / "w2-inherit-targets.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    return {**report, "status": "OK",
             "json_path": str(json_path.relative_to(_ROOT))}


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps({
            "status": result["status"], "apply": result["apply"],
            "totals": result["totals"],
            "workspace_deps_loaded": result["workspace_deps_count"],
            "json_path": result["json_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
