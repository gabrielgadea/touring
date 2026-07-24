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

import sys
from pathlib import Path

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


def rewrite_source(text: str, module: str, cross: dict[str, str]) -> tuple[str, int]:
    """Return (new_text, num_rewrites). Only rewrites code-context tokens."""
    crate_prefix = f"crate::{module}::"
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

        # Code context — check for `crate::` at this position.
        if text.startswith("crate::", i):
            after = text[i + 7:]
            if after.startswith(f"{module}::"):
                # Already prefixed (idempotent re-run) — emit as-is.
                out.append("crate::")
                i += 7
                continue
            # Also skip if it already has any of the known module prefixes
            already_prefixed = any(
                after.startswith(f"{m}::")
                for m in ALL_MODULES
            )
            if already_prefixed:
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

    if module == "tests":
        default_dir = "crates/touring-bindings/tests"
    elif module == "web_server":
        default_dir = "crates/touring-bindings/src/web/server"
    else:
        default_dir = f"crates/touring-bindings/src/{module}"

    src_dir = Path(sys.argv[2] if len(sys.argv) > 2 else default_dir)
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
