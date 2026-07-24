#!/usr/bin/env python3
"""gen_reference.py - generate docs/reference/*.md from the Touring source of truth.

Dogfooding documentation generation (Master Plan H1 - D.W2.P1.T4): MCP tool names,
daemon hook names, and generator kinds are extracted deterministically from the
code/CLI, never hand-written, so the reference docs cannot drift from reality.

Hardened 2026-06-11: the daemon-lib-rearch moved ALL_DAEMON_HOOK_NAMES out of
touring-hooks; sources are now discovered across crates, generator kinds fall
back to source parsing when the daemon CLI is unavailable (CI), and any section
that resolves to 0 items aborts loudly instead of silently writing an empty doc.

Usage:
    docs/gen_reference.py             regenerate docs/reference/{generators,mcp-tools,hooks}.md
    docs/gen_reference.py --validate  exit 1 if regenerating would change any file (CI drift gate)
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REF = ROOT / "docs" / "reference"


def generator_kinds() -> list[str]:
    """Generator kinds via `touring generate list-kinds -j`, falling back to
    parsing the GeneratorKind enum source when the daemon CLI is unavailable."""
    try:
        out = subprocess.run(
            ["touring", "generate", "list-kinds", "-j"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=20,
        )
        data = json.loads(out.stdout)
        if isinstance(data, list):
            kinds = sorted(str(x.get("kind", x.get("name", x)) if isinstance(x, dict) else x) for x in data)
            if kinds:
                return kinds
        if isinstance(data, dict):
            for key in ("kinds", "generator_kinds", "items"):
                if isinstance(data.get(key), list):
                    kinds = sorted(
                        str(x.get("kind", x.get("name", x)) if isinstance(x, dict) else x) for x in data[key]
                    )
                    if kinds:
                        return kinds
    except Exception:
        pass
    # Daemon-free fallback (CI): parse the GeneratorKind enum variants.
    gen_src = ROOT / "crates" / "touring-generator" / "src"
    for path in sorted(gen_src.rglob("*.rs")):
        try:
            text = path.read_text(errors="ignore")
        except Exception:
            continue
        m = re.search(r"pub enum GeneratorKind\s*\{(.*?)\n\}", text, re.S)
        if m:
            variants = re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\b", m.group(1), re.M)
            if variants:
                return sorted(set(variants))
    return []


def mcp_tools() -> list[str]:
    """MCP tool names extracted from touring-server handler string literals."""
    names: set[str] = set()
    srv = ROOT / "crates" / "touring-server" / "src"
    pat = re.compile(r'"(touring_[a-z0-9_]+|ctx_[a-z0-9_]+)"')
    for path in srv.rglob("*.rs"):
        try:
            names.update(pat.findall(path.read_text(errors="ignore")))
        except Exception:
            continue
    return sorted(names)


def hook_names() -> list[str]:
    """Lifecycle hook names parsed from the ALL_DAEMON_HOOK_NAMES constant.

    The constant moved crates during the daemon-lib-rearch (2026-06-11):
    known homes are checked first, then a workspace-wide scan."""
    candidates = [
        ROOT / "crates" / "touring-hooks-core" / "src" / "inventory_registry.rs",
        ROOT / "crates" / "touring-hooks" / "src" / "hook_registry.rs",
    ]
    seen = set(candidates)
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        if "target" in path.parts or path in seen:
            continue
        if path.name in ("inventory_registry.rs", "hook_registry.rs"):
            candidates.append(path)
    for reg in candidates:
        try:
            text = reg.read_text(errors="ignore")
        except Exception:
            continue
        m = re.search(r"ALL_DAEMON_HOOK_NAMES[^=]*=\s*&\[(.*?)\];", text, re.S)
        if m:
            names = sorted(set(re.findall(r'"([a-z0-9\-_]+)"', m.group(1))))
            if names:
                return names
    return []


def crate_modules() -> list[str]:
    """Top-level module surface per crate: `<crate>::<module>` for each immediate
    child module under `crates/<crate>/src` (deterministic, daemon-free).

    Closes the missing 4th reference subcommand of Master Plan D.W2.P1.T4
    ("modules — por crate"). A module is an immediate `*.rs` file (except
    lib/main/mod) or a sub-directory that is itself a module (`mod.rs` present,
    or a sibling `<name>.rs` declaring it)."""
    items: list[str] = []
    crates_dir = ROOT / "crates"
    if not crates_dir.is_dir():
        return items
    for crate_dir in sorted(p for p in crates_dir.iterdir() if p.is_dir()):
        src = crate_dir / "src"
        if not src.is_dir():
            continue
        mods: set[str] = set()
        for child in src.iterdir():
            name = child.name
            if child.is_file() and name.endswith(".rs") and name not in ("lib.rs", "main.rs", "mod.rs"):
                mods.add(name[:-3])
            elif child.is_dir() and ((child / "mod.rs").exists() or (src / f"{name}.rs").exists()):
                mods.add(name)
        items.extend(f"{crate_dir.name}::{mod}" for mod in sorted(mods))
    return items


def render(title: str, intro: str, items: list[str]) -> str:
    lines = [
        f"# {title}",
        "",
        "> Auto-generated by `docs/gen_reference.py` from the source of truth. Do not edit by hand.",
        "",
        intro,
        "",
        f"**Count: {len(items)}**",
        "",
    ]
    lines.extend(f"- `{item}`" for item in items)
    lines.append("")
    return "\n".join(lines)


SECTIONS = [
    ("generators.md", "Touring Generator Kinds",
     "Code-generation kinds available via `touring generate render <kind>`.", generator_kinds),
    ("mcp-tools.md", "Touring MCP Tools",
     "MCP tool names defined in `touring-server` (string-literal extraction). The default "
     "build exposes essentially all of them; a curated 22-tool surface is opt-in behind "
     "`--features mcp-curated` (default OFF), so this count is the full defined set, not the "
     "curated surface.", mcp_tools),
    ("hooks.md", "Touring Daemon Hooks",
     "Lifecycle hook names registered in `ALL_DAEMON_HOOK_NAMES`.", hook_names),
    ("modules.md", "Touring Crate Modules",
     "Top-level module surface per crate (`<crate>::<module>`), scanned from `crates/*/src` "
     "(closes Master Plan D.W2.P1.T4's 4th reference subcommand).", crate_modules),
]


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate docs/reference/*.md from the source of truth.")
    ap.add_argument("--validate", action="store_true",
                    help="exit 1 if any reference file would change (CI drift gate)")
    args = ap.parse_args()

    REF.mkdir(parents=True, exist_ok=True)
    drift: list[str] = []
    for fname, title, intro, fn in SECTIONS:
        items = fn()
        if not items:
            print(
                f"ERROR: {fname}: source parse returned 0 items - the source of truth "
                "moved or is unreadable; refusing to write/validate an empty reference",
                file=sys.stderr,
            )
            return 2
        content = render(title, intro, items)
        target = REF / fname
        old = target.read_text(errors="ignore") if target.exists() else ""
        if args.validate:
            if old != content:
                drift.append(fname)
        else:
            target.write_text(content)
            print(f"wrote {target.relative_to(ROOT)} ({len(items)} items)")

    if args.validate:
        if drift:
            print(f"DRIFT: {', '.join(drift)} out of sync - run docs/gen_reference.py", file=sys.stderr)
            return 1
        print("OK: reference docs in sync")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
