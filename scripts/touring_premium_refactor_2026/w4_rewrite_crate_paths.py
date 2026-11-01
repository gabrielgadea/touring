#!/usr/bin/env python3
"""W4.2 — Rewrite `crate::` → `crate::ast::` inside the moved touring-ast files.

The moved files now live under `touring-code/src/ast/`, so a bare `crate::X`
(which used to mean `touring_ast::X`) must become `crate::ast::X`. The rewrite
is **string- and comment-aware**: occurrences inside string literals, raw
strings, char literals, or comments are test fixtures / data and must NOT be
touched (e.g. `wiring.rs` simulates parsed Rust source code as string data).

Also rewrites `touring_ast_polyglot::` → `crate::polyglot::` (the polyglot
crate was fused in W4.3).

Idempotent: skips `crate::ast::` and `crate::polyglot::` that already carry
the prefix.
"""

from __future__ import annotations

import sys
from pathlib import Path

import rewrite_crate_paths_core as core

AST_DIR = Path("crates/touring-code/src/ast")


def _rewrite_at(text: str, i: int) -> core.Rewrite | None:
    if text.startswith("crate::", i):
        after = text[i + 7 :]
        if after.startswith(("ast::", "polyglot::")):
            return "crate::", 7, 0
        return "crate::ast::", 7, 1
    if text.startswith("touring_ast_polyglot::", i):
        return "crate::polyglot::", len("touring_ast_polyglot::"), 1
    return None


def rewrite_source(text: str) -> tuple[str, int]:
    """Return (new_text, num_rewrites). Only rewrites code-context tokens."""
    return core.rewrite_code_context(text, _rewrite_at)


def main() -> int:
    if not AST_DIR.is_dir():
        print(f"ERROR: {AST_DIR} not found — run from workspace root", file=sys.stderr)
        return 2

    total, files_changed = core.rewrite_tree(AST_DIR, rewrite_source)
    print(f"\nTOTAL: {total} rewrites across {files_changed} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
