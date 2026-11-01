#!/usr/bin/env python3
"""W1.3 — Find + propose removal of `pub use` statements referencing deleted crates.

After W1.1+W1.2 delete dead crates, dangling `pub use foo::Bar` statements
break compilation. This script:
  1. Loads list of deleted crates (from w1_audit_dead_code output)
  2. Greps every src/*.rs for `pub use <deleted_crate_module>::`
  3. Emits a removal patch (one entry per line to delete)
  4. With --apply, actually removes those lines (atomic, idempotent)

Outputs:
  - data/w1-dangling-reexports.json
  - staging/w1-reexport-removal-plan.md

Usage
-----
    python3 w1_clean_reexports.py                  # report
    python3 w1_clean_reexports.py --apply          # remove dangling lines
    python3 w1_clean_reexports.py --crates touring-foo,touring-bar
"""
from __future__ import annotations

import json
import re
from collections import defaultdict

import script_common as sc

_SCRIPT = "w1_clean_reexports"

_ROOT, _DATA_DIR, _STAGING_DIR, _CRATES_DIR = sc.ROOT, sc.DATA_DIR, sc.STAGING_DIR, sc.CRATES_DIR

DEFAULT_DELETED = [
    "touring-semantic-spike", "touring-wasm-client",
    "touring-wasm-common", "touring-wasm-server",
    "touring-loom-proofs",
]


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--apply", action="store_true",
                   help="DESTRUCTIVE — remove lines from .rs files.")
    p.add_argument("--crates", type=str, default="",
                   help="Comma-separated deleted crate names.")
    return sc.with_output_dir_and_verbose(p, _DATA_DIR)


def deleted_modules(crates: list[str]) -> list[str]:
    """Convert kebab crate names to snake_case Rust module names."""
    return [c.replace("-", "_") for c in crates]


def scan_dangling(modules: list[str]) -> dict[str, list[dict]]:
    """Find all `pub use <module>::*` lines across the workspace."""
    patterns = [re.compile(rf"^\s*pub\s+use\s+{re.escape(mod)}(::|\s|;)")
                 for mod in modules]
    findings: dict[str, list[dict]] = defaultdict(list)
    for crate_dir, rs, content in sc.crate_rust_sources(_CRATES_DIR):
        for lineno, line in enumerate(content.splitlines(), start=1):
            for pat in patterns:
                if pat.search(line):
                    findings[str(rs.relative_to(_ROOT))].append({
                        "line": lineno, "text": line.strip(),
                    })
                    break
    return dict(findings)


def apply_removal(findings: dict[str, list[dict]]) -> dict:
    """Remove dangling lines in-place (idempotent)."""
    applied: dict[str, int] = {}
    for rel_path, hits in findings.items():
        abs_path = _ROOT / rel_path
        try:
            lines = abs_path.read_text(encoding="utf-8",
                                         errors="replace").splitlines(keepends=True)
        except OSError:
            applied[rel_path] = -1
            continue
        hit_linenos = {h["line"] for h in hits}
        new_lines = [ln for i, ln in enumerate(lines, start=1)
                      if i not in hit_linenos]
        abs_path.write_text("".join(new_lines), encoding="utf-8")
        applied[rel_path] = len(hit_linenos)
    return applied


def render_md(report: dict) -> str:
    """Markdown removal plan."""
    lines = ["# W1.3 — Dangling `pub use` Removal Plan", "",
             f"Generated: {report['timestamp']}",
             f"Deleted crate modules: {', '.join(report['deleted_modules'])}",
             f"Files with dangling re-exports: {report['totals']['files']}",
             f"Total lines to remove: {report['totals']['lines']}",
             "", "## Removal steps", ""]
    for file, hits in report["findings"].items():
        lines.append(f"### `{file}`")
        for h in hits:
            lines.append(f"  - L{h['line']}: `{h['text']}`")
        lines.append("")
    return "\n".join(lines)


def run(args: sc.Namespace) -> dict:
    """Scan + optionally apply."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    crates = args.crates.split(",") if args.crates else DEFAULT_DELETED
    modules = deleted_modules(crates)
    findings = scan_dangling(modules)
    applied: dict[str, int] = {}
    if args.apply:
        applied = apply_removal(findings)
    report = {
        "script": "w1_clean_reexports",
        "wave": "W1", "subtask_refs": ["W1.3"],
        "timestamp": sc.now_iso(),
        "deleted_modules": modules,
        "apply": args.apply,
        "totals": {
            "files": len(findings),
            "lines": sum(len(hits) for hits in findings.values()),
            "applied_total": sum(v for v in applied.values() if v > 0),
        },
        "findings": findings, "applied": applied,
    }
    json_path = args.output_dir / "w1-dangling-reexports.json"
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    md_path = _STAGING_DIR / "w1-reexport-removal-plan.md"
    md_path.write_text(render_md(report), encoding="utf-8")
    return {**report, "status": "OK",
             "json_path": str(json_path.relative_to(_ROOT)),
             "md_path": str(md_path.relative_to(_ROOT))}


def _run_and_report(args: sc.Namespace) -> int:
    """Run w1_clean_reexports, print its summary; the return value is the exit code."""
    result = run(args)
    sc.emit_json({
        "status": result["status"], "apply": result["apply"],
        "totals": result["totals"],
        "json_path": result["json_path"], "md_path": result["md_path"],
    })
    return 0


def main() -> int:
    """Entry point of w1_clean_reexports."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
