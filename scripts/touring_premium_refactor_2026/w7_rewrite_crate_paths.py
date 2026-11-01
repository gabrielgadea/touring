#!/usr/bin/env python3
"""W7 — Rewrite crate-path references inside files moved into touring-bindings.

The W7 fusion moves 7 crates into submodules of `touring-bindings`:

    touring-python       -> touring-bindings/src/python/
    touring-wasm         -> touring-bindings/src/wasm/
    touring-capnp-server -> touring-bindings/src/capnp/
    touring-web          -> touring-bindings/src/web/
    touring-web-server   -> touring-bindings/src/web/server/
    touring-desktop-ui   -> touring-bindings/src/desktop/
    touring-geopostgis   -> touring-bindings/src/postgis/

A bare `crate::X` inside a moved file used to mean "this crate"; after the
move it must become `crate::<module>::X`.

Additionally, cross-fusion deps are rewritten:
  - web/server (web-server) uses: touring_web -> crate::web

The rewrite is **string- and comment-aware**: occurrences inside string
literals, raw strings, char literals, or comments are data/fixtures and are
left untouched. Idempotent: a `crate::` token that already carries the target
module prefix is skipped.

Usage:
    w7_rewrite_crate_paths.py <module> [<src_dir>]

`<module>` is one of: python, wasm, capnp, web, web_server, desktop, postgis, tests.
`<src_dir>` defaults to crates/touring-bindings/src/<module>.
"""

from __future__ import annotations

import rewrite_crate_paths_core as core

# All W7 module names (for idempotency check)
ALL_MODULES = ("python", "wasm", "capnp", "web", "desktop", "postgis")

# module -> cross-crate path map (intra-fusion deps only).
CROSS_CRATE: dict[str, dict[str, str]] = {
    "python":     {},
    "wasm":       {},
    "capnp":      {},
    "web":        {},
    # web_server is a sub-module of web; it previously imported touring_web::
    "web_server": {
        "touring_web::": "crate::web::",
    },
    "desktop":    {},
    "postgis":    {},
    # Integration tests: rewrite external crate names to fused paths
    "tests": {
        "touring_python::":       "touring_bindings::python::",
        "touring_wasm::":         "touring_bindings::wasm::",
        "touring_capnp_server::": "touring_bindings::capnp::",
        "touring_web::":          "touring_bindings::web::",
        "touring_web_server::":   "touring_bindings::web::server::",
        "touring_desktop_ui::":   "touring_bindings::desktop::",
        "touring_geopostgis::":   "touring_bindings::postgis::",
    },
}


def _is_prefixed(after: str, module: str) -> bool:
    return after.startswith(f"{module}::") or any(after.startswith(f"{m}::") for m in ALL_MODULES)


def _default_dir(module: str) -> str:
    if module == "tests":
        return "crates/touring-bindings/tests"
    if module == "web_server":
        return "crates/touring-bindings/src/web/server"
    return f"crates/touring-bindings/src/{module}"


def rewrite_source(text: str, module: str, cross: dict[str, str]) -> tuple[str, int]:
    """Return (new_text, num_rewrites). Only rewrites code-context tokens."""
    return core.rewrite_fused_source(text, module, cross, _is_prefixed)


def main() -> int:
    return core.run_fused_cli(__doc__, CROSS_CRATE, _default_dir, rewrite_source)


if __name__ == "__main__":
    raise SystemExit(main())
