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

AST_DIR = Path("crates/touring-code/src/ast")


def rewrite_source(text: str) -> tuple[str, int]:
    """Return (new_text, num_rewrites). Only rewrites code-context tokens."""
    out: list[str] = []
    i = 0
    n = len(text)
    rewrites = 0

    while i < n:
        ch = text[i]

        # Line comment — copy verbatim to end of line.
        if ch == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            if j == -1:
                j = n
            out.append(text[i:j])
            i = j
            continue

        # Block comment — copy verbatim to closing */.
        if ch == "/" and i + 1 < n and text[i + 1] == "*":
            j = text.find("*/", i + 2)
            j = n if j == -1 else j + 2
            out.append(text[i:j])
            i = j
            continue

        # Raw string: r"...", r#"..."#, r##"..."##, ...
        if ch == "r" and i + 1 < n and text[i + 1] in '"#':
            k = i + 1
            hashes = 0
            while k < n and text[k] == "#":
                hashes += 1
                k += 1
            if k < n and text[k] == '"':
                closer = '"' + "#" * hashes
                j = text.find(closer, k + 1)
                j = n if j == -1 else j + len(closer)
                out.append(text[i:j])
                i = j
                continue

        # Regular string literal "..." with \-escapes.
        if ch == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(text[i:j])
            i = j
            continue

        # Char literal 'x' or '\n' — but NOT a lifetime ('a).
        if ch == "'":
            # Lifetime: ' followed by ident char and not a closing '.
            if i + 1 < n and (text[i + 1].isalpha() or text[i + 1] == "_"):
                # Could be lifetime or char like 'a'. Char literal closes with '
                # within 1-2 chars. Lifetime does not.
                if i + 2 < n and text[i + 2] == "'":
                    out.append(text[i : i + 3])  # char literal 'a'
                    i += 3
                    continue
                # treat as lifetime — copy the tick only.
                out.append(ch)
                i += 1
                continue
            if i + 1 < n and text[i + 1] == "\\":
                # escaped char literal '\n' '\'' '\\'
                j = i + 2
                while j < n and text[j] != "'":
                    j += 1
                j = min(j + 1, n)
                out.append(text[i:j])
                i = j
                continue

        # Code context — check for crate:: token at this position.
        if text.startswith("crate::", i):
            after = text[i + 7 :]
            if after.startswith("ast::") or after.startswith("polyglot::"):
                out.append("crate::")
                i += 7
                continue
            out.append("crate::ast::")
            i += 7
            rewrites += 1
            continue

        if text.startswith("touring_ast_polyglot::", i):
            out.append("crate::polyglot::")
            i += len("touring_ast_polyglot::")
            rewrites += 1
            continue

        out.append(ch)
        i += 1

    return "".join(out), rewrites


def main() -> int:
    if not AST_DIR.is_dir():
        print(f"ERROR: {AST_DIR} not found — run from workspace root", file=sys.stderr)
        return 2

    total = 0
    files_changed = 0
    for path in sorted(AST_DIR.rglob("*.rs")):
        original = path.read_text(encoding="utf-8")
        new, count = rewrite_source(original)
        if count > 0:
            path.write_text(new, encoding="utf-8")
            files_changed += 1
            total += count
            print(f"  {path.relative_to(AST_DIR)}: {count} rewrites")

    print(f"\nTOTAL: {total} rewrites across {files_changed} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
