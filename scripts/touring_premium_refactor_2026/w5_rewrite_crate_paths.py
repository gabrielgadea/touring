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

import rewrite_crate_paths_core as core

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


def _is_prefixed(after: str, module: str) -> bool:
    """Idempotence guard, kept exactly as shipped: it looks for ``crate::<module>::`` *after*
    a ``crate::`` token (a doubled prefix), so a plain re-run is not detected as done."""
    return after.startswith(f"crate::{module}::")


def rewrite_source(text: str, module: str, cross: dict[str, str]) -> tuple[str, int]:
    """Return (new_text, num_rewrites). Only rewrites code-context tokens."""
    return core.rewrite_fused_source(text, module, cross, _is_prefixed)


def main() -> int:
    return core.run_fused_cli(
        __doc__, CROSS_CRATE, lambda module: f"crates/touring-storage/src/{module}", rewrite_source,
    )


if __name__ == "__main__":
    raise SystemExit(main())
