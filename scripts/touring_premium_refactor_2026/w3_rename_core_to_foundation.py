#!/usr/bin/env python3
"""W3.1 — Plan rename of touring-core → touring-foundation (Layer 0 of new arch).

Strategy: NO DESTRUCTIVE OPERATION. This script ONLY produces a migration plan.
Actual rename happens in 3 steps (manual / staged):
  1. Add `touring-foundation` as alias in workspace.members (new directory, new Cargo.toml)
  2. Update all `use touring_core::*` → `use touring_foundation::*` (ast-grep rewrite)
  3. Remove old `touring-core` directory once consumers are migrated

This script:
  - Catalogs ALL `use touring_core::*` references across workspace
  - Counts consumers per module of touring-core
  - Emits a ast-grep rewrite spec + migration checklist
  - Estimates engineer-hours (per file)

Outputs:
  - data/w3-touring-core-consumer-map.json
  - staging/w3-rename-migration-plan.md
  - staging/w3-ast-grep-rewrites.sh    (idempotent rewrite commands)

Usage
-----
    python3 w3_rename_core_to_foundation.py
    python3 w3_rename_core_to_foundation.py --emit-script
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
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

USE_RE = re.compile(r"\b(?:use\s+)?touring_core::(\w+)(?:::(\w+))?")
EXTERN_RE = re.compile(r"\bextern\s+crate\s+touring_core\b")
CARGO_DEP_RE = re.compile(r'^\s*touring-core\s*=', re.MULTILINE)

OLD_NAME = "touring-core"
NEW_NAME = "touring-foundation"
OLD_MOD = "touring_core"
NEW_MOD = "touring_foundation"


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w3_rename_core_to_foundation", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--emit-script", action="store_true",
                   help="Emit staging/w3-ast-grep-rewrites.sh.")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def scan_consumers() -> dict:
    """Catalog all references to touring-core across workspace."""
    consumers: dict[str, list[dict]] = defaultdict(list)
    cargo_decl_files: list[str] = []
    sub_modules_used: dict[str, int] = defaultdict(int)
    for crate_dir in sorted(_CRATES_DIR.glob("touring-*")):
        if crate_dir.name == OLD_NAME:
            continue
        cargo = crate_dir / "Cargo.toml"
        if cargo.exists():
            try:
                if CARGO_DEP_RE.search(cargo.read_text(encoding="utf-8",
                                                        errors="replace")):
                    cargo_decl_files.append(str(cargo.relative_to(_ROOT)))
            except OSError:
                pass
        src = crate_dir / "src"
        if not src.exists():
            continue
        for rs_file in src.rglob("*.rs"):
            try:
                content = rs_file.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            use_matches = USE_RE.findall(content)
            extern_match = bool(EXTERN_RE.search(content))
            if not use_matches and not extern_match:
                continue
            sample_lines = []
            for n, line in enumerate(content.splitlines(), start=1):
                if "touring_core" in line:
                    sample_lines.append({"line": n, "text": line.strip()[:100]})
                if len(sample_lines) >= 3:
                    break
            consumers[crate_dir.name].append({
                "file": str(rs_file.relative_to(_ROOT)),
                "use_count": len(use_matches),
                "has_extern_crate": extern_match,
                "sample_lines": sample_lines,
            })
            for sub_mod, _ in use_matches:
                if sub_mod:
                    sub_modules_used[sub_mod] += 1
    return {
        "cargo_decl_files": cargo_decl_files,
        "consumers": dict(consumers),
        "sub_modules_used": dict(sorted(sub_modules_used.items(),
                                          key=lambda x: -x[1])),
    }


def estimate_hours(scan: dict) -> dict:
    """Engineer-hour estimate per migration phase."""
    cargo_files = len(scan["cargo_decl_files"])
    consumer_files = sum(len(files) for files in scan["consumers"].values())
    total_uses = sum(f["use_count"] for files in scan["consumers"].values()
                       for f in files)
    return {
        "cargo_toml_update_hours": round(cargo_files * 0.05, 2),
        "ast_grep_rewrite_hours": round(consumer_files * 0.02, 2),
        "manual_review_hours": round(consumer_files * 0.10, 2),
        "cargo_check_iterations_hours": 1.0,
        "total_hours": round(
            cargo_files * 0.05 + consumer_files * 0.12 + 1.0, 2),
        "total_engineer_days": round(
            (cargo_files * 0.05 + consumer_files * 0.12 + 1.0) / 8.0, 2),
        "totals": {
            "cargo_files": cargo_files,
            "consumer_files": consumer_files,
            "total_use_statements": total_uses,
        },
    }


def render_markdown_plan(scan: dict, est: dict) -> str:
    """Human-readable migration plan."""
    lines = [
        "# W3.1 — Rename touring-core → touring-foundation",
        "",
        f"Generated: {datetime.now(UTC).isoformat()}",
        "",
        "## Scope",
        "",
        f"- **Cargo.toml files** declaring `touring-core`: **{est['totals']['cargo_files']}**",
        f"- **Source files** using `touring_core::`: **{est['totals']['consumer_files']}**",
        f"- **Total use statements**: **{est['totals']['total_use_statements']}**",
        "",
        "## Sub-modules of touring-core used by consumers",
        "",
        "| Sub-module | Use count |",
        "|------------|-----------|",
    ]
    for mod, count in list(scan["sub_modules_used"].items())[:15]:
        lines.append(f"| `touring_core::{mod}` | {count} |")
    lines.extend([
        "",
        "## Estimate",
        "",
        f"- Cargo.toml updates: **{est['cargo_toml_update_hours']}h**",
        f"- ast-grep rewrites: **{est['ast_grep_rewrite_hours']}h**",
        f"- Manual review: **{est['manual_review_hours']}h**",
        f"- cargo check loops: **{est['cargo_check_iterations_hours']}h**",
        f"- **Total**: **{est['total_hours']}h ({est['total_engineer_days']} engineer-days)**",
        "",
        "## Migration steps (manual / staged, NOT DESTRUCTIVE)",
        "",
        f"1. **Create new crate** `crates/{NEW_NAME}/` (copy `touring-core/src/`)",
        f"2. **Update workspace.members** in root `Cargo.toml`: add `crates/{NEW_NAME}`",
        f"3. **Update consumer Cargo.toml** ({est['totals']['cargo_files']} files):",
        "",
        "```bash",
        "for f in $(jq -r '.cargo_decl_files[]' data/w3-touring-core-consumer-map.json); do",
        f"  sed -i 's/touring-core/{NEW_NAME}/g' \"$f\"",
        "done",
        "```",
        "",
        f"4. **Update source files** ({est['totals']['consumer_files']} files) via ast-grep",
        "5. **Validate**: `cargo check --workspace` exit 0",
        f"6. **Remove old crate**: `rm -rf crates/{OLD_NAME}` only after step 5",
        "",
        "## Consumer breakdown (top 10 by use count)",
        "",
        "| Crate | Files | Total uses |",
        "|-------|-------|------------|",
    ])
    ranked = sorted(
        scan["consumers"].items(),
        key=lambda kv: -sum(f["use_count"] for f in kv[1]),
    )
    for crate, files in ranked[:10]:
        total = sum(f["use_count"] for f in files)
        lines.append(f"| `{crate}` | {len(files)} | {total} |")
    return "\n".join(lines) + "\n"


def render_ast_grep_script(scan: dict) -> str:
    """Generate idempotent ast-grep rewrite commands."""
    lines = [
        "#!/usr/bin/env bash",
        "# AUTO-GENERATED — W3.1 ast-grep rewrites",
        f"# Generated: {datetime.now(UTC).isoformat()}",
        "# Idempotent: safe to re-run.",
        "set -euo pipefail",
        "",
        "echo 'Phase 1: Rewriting use statements...'",
        "",
    ]
    for crate, files in sorted(scan["consumers"].items()):
        for f in files:
            lines.append(
                f"touring ast grep '{f['file']}' '{OLD_MOD}' "
                f"--rewrite '{NEW_MOD}' --lang rust || true"
            )
    lines.extend([
        "",
        "echo 'Phase 1 done.'",
        "echo 'Phase 2: cargo check --workspace'",
        "cargo check --workspace --message-format=short 2>&1 | tail -20",
    ])
    return "\n".join(lines) + "\n"


def run(args: argparse.Namespace) -> dict:
    """Scan + estimate + render."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    scan = scan_consumers()
    est = estimate_hours(scan)
    json_path = args.output_dir / "w3-touring-core-consumer-map.json"
    json_path.write_text(json.dumps({**scan, "estimate": est},
                                       indent=2, ensure_ascii=False),
                          encoding="utf-8")
    md_path = _STAGING_DIR / "w3-rename-migration-plan.md"
    md_path.write_text(render_markdown_plan(scan, est), encoding="utf-8")
    script_path: str | None = None
    if args.emit_script:
        sp = _STAGING_DIR / "w3-ast-grep-rewrites.sh"
        sp.write_text(render_ast_grep_script(scan), encoding="utf-8")
        sp.chmod(0o755)
        script_path = str(sp.relative_to(_ROOT))
    return {
        "script": "w3_rename_core_to_foundation",
        "wave": "W3", "subtask_refs": ["W3.1"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "totals": est["totals"],
        "estimate": {k: v for k, v in est.items() if k != "totals"},
        "top_sub_modules": list(scan["sub_modules_used"].items())[:5],
        "json_path": str(json_path.relative_to(_ROOT)),
        "md_path": str(md_path.relative_to(_ROOT)),
        "script_path": script_path,
    }


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        import sys
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False))
        sys.stdout.write("\n")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
