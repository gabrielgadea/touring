#!/usr/bin/env python3
"""W6 — Rewrite crate-path references inside files moved into touring-intelligence.

The W6 fusion moves 4 crates into submodules of `touring-intelligence`:

    touring-cognitive  -> touring-intelligence/src/reasoning/
    touring-learning   -> touring-intelligence/src/rl/
    touring-antt       -> touring-intelligence/src/ann/
    touring-index      -> touring-intelligence/src/index/

A bare `crate::X` inside a moved file used to mean "this crate"; after the
move it must become `crate::<module>::X`.

Additionally, cross-fusion deps are rewritten:
  - reasoning (cognitive) uses: touring_learning, touring_antt
  - ann (antt) uses:            touring_learning
  - index uses:                 touring_learning

The rewrite is **string- and comment-aware**: occurrences inside string
literals, raw strings, char literals, or comments are data/fixtures and are
left untouched. Idempotent: a `crate::` token that already carries the target
module prefix is skipped.

Usage:
    w6_rewrite_crate_paths.py <module> [<src_dir>]

`<module>` is one of: reasoning, rl, ann, index, tests.
`<src_dir>` defaults to crates/touring-intelligence/src/<module>.
"""

from __future__ import annotations

import rewrite_crate_paths_core as core

# module -> cross-crate path map (intra-fusion deps only).
# Keys are the OLD external crate path; values are the new crate-local path.
CROSS_CRATE: dict[str, dict[str, str]] = {
    # reasoning (ex-cognitive): used touring_learning + touring_antt
    "reasoning": {
        "touring_learning::": "crate::rl::",
        "touring_antt::":     "crate::ann::",
    },
    # rl (ex-learning): no intra-fusion deps
    "rl": {},
    # ann (ex-antt): used touring_learning
    "ann": {
        "touring_learning::": "crate::rl::",
    },
    # index (ex-index): used touring_learning
    "index": {
        "touring_learning::": "crate::rl::",
    },
    # Integration tests: rewrite external crate names to fused paths
    "tests": {
        "touring_cognitive::":  "touring_intelligence::reasoning::",
        "touring_learning::":   "touring_intelligence::rl::",
        "touring_antt::":       "touring_intelligence::ann::",
        "touring_index::":      "touring_intelligence::index::",
    },
}

# The four modules of the W6 fusion: a `crate::` already followed by any of them was
# rewritten in a previous pass, so it is left alone (handles inter-module refs too).
ALL_MODULES = ("reasoning", "rl", "ann", "index")


def _is_prefixed(after: str, module: str) -> bool:
    return after.startswith(f"{module}::") or any(after.startswith(f"{m}::") for m in ALL_MODULES)


def _default_dir(module: str) -> str:
    if module == "tests":
        return "crates/touring-intelligence/tests"
    return f"crates/touring-intelligence/src/{module}"


def rewrite_source(text: str, module: str, cross: dict[str, str]) -> tuple[str, int]:
    """Return (new_text, num_rewrites). Only rewrites code-context tokens."""
    return core.rewrite_fused_source(text, module, cross, _is_prefixed)


def main() -> int:
    return core.run_fused_cli(__doc__, CROSS_CRATE, _default_dir, rewrite_source)


if __name__ == "__main__":
    raise SystemExit(main())
