#!/usr/bin/env python3
"""W5 — Rewrite crate-path references inside files moved into touring-storage.

The W5 fusion moves 6 crates into submodules of `touring-storage`:

    touring-index           -> touring-storage/src/fts/
    touring-vfs             -> touring-storage/src/vfs/
    touring-incremental-salsa -> touring-storage/src/salsa/
    touring-vector-store    -> touring-storage/src/vec/
    touring-embeddings      -> touring-storage/src/embeddings/
    touring-search-fusion   -> touring-storage/src/hybrid_search/

A bare `crate::X` inside a moved file used to mean "this crate"; after the move
it must become `crate::<module>::X`. Additionally, `touring-search-fusion`
depended on three other crates that are *also* part of the fusion, so its
`touring_embeddings::` / `touring_vector_store::` / `touring_vfs::` references
become crate-local `crate::<module>::` paths.

The rewrite is **string- and comment-aware**: occurrences inside string
literals, raw strings, char literals, or comments are data / fixtures and are
left untouched. Idempotent: a `crate::` token that already carries the target
module prefix is skipped.

Usage:
    w5_rewrite_crate_paths.py <module> [<src_dir>]

`<module>` is one of: fts, vfs, salsa, vec, embeddings, hybrid_search.
`<src_dir>` defaults to crates/touring-storage/src/<module>.
"""

from __future__ import annotations

import sys
from pathlib import Path

# module -> cross-crate path map (intra-fusion deps only).
CROSS_CRATE: dict[str, dict[str, str]] = {
    "fts": {},
    "vfs": {},
    "salsa": {},
    "vec": {},
    "embeddings": {},
    "hybrid_search": {
        "touring_embeddings::": "crate::embeddings::",
        "touring_vector_store::": "crate::vec::",
        "touring_vfs::": "crate::vfs::",
    },
    # Integration tests relocated into touring-storage/tests/: the origin-crate
    # path becomes the fused-crate module path. Test files have no `crate::`
    # tokens, so the bare-crate rewrite branch never fires for this module.
    "tests": {
        "touring_vfs::": "touring_storage::vfs::",
        "touring_vector_store::": "touring_storage::vec::",
        "touring_embeddings::": "touring_storage::embeddings::",
        "touring_incremental_salsa::": "touring_storage::salsa::",
        "touring_search_fusion::": "touring_storage::hybrid_search::",
    },
}


def rewrite_source(text: str, module: str, cross: dict[str, str]) -> tuple[str, int]:
    """Return (new_text, num_rewrites). Only rewrites code-context tokens."""
    crate_prefix = f"crate::{module}::"
    already = f"crate::{module}::"
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
            if i + 1 < n and (text[i + 1].isalpha() or text[i + 1] == "_"):
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

        # Cross-crate path rewrite (intra-fusion deps).
        matched_cross = False
        for src_path, dst_path in cross.items():
            if text.startswith(src_path, i):
                out.append(dst_path)
                i += len(src_path)
                rewrites += 1
                matched_cross = True
                break
        if matched_cross:
            continue

        # Code context — check for crate:: token at this position.
        if text.startswith("crate::", i):
            after = text[i + 7 :]
            if after.startswith(already):
                # Already prefixed (idempotent re-run) — skip.
                out.append("crate::")
                i += 7
                continue
            out.append(crate_prefix)
            i += 7
            rewrites += 1
            continue

        out.append(ch)
        i += 1

    return "".join(out), rewrites


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2

    module = sys.argv[1]
    if module not in CROSS_CRATE:
        print(
            f"ERROR: unknown module '{module}' "
            f"(expected one of {sorted(CROSS_CRATE)})",
            file=sys.stderr,
        )
        return 2

    src_dir = Path(
        sys.argv[2]
        if len(sys.argv) > 2
        else f"crates/touring-storage/src/{module}"
    )
    if not src_dir.is_dir():
        print(f"ERROR: {src_dir} not found — run from workspace root", file=sys.stderr)
        return 2

    cross = CROSS_CRATE[module]
    total = 0
    files_changed = 0
    for path in sorted(src_dir.rglob("*.rs")):
        original = path.read_text(encoding="utf-8")
        new, count = rewrite_source(original, module, cross)
        if count > 0:
            path.write_text(new, encoding="utf-8")
            files_changed += 1
            total += count
            print(f"  {path.relative_to(src_dir)}: {count} rewrites")

    print(f"\nTOTAL [{module}]: {total} rewrites across {files_changed} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
