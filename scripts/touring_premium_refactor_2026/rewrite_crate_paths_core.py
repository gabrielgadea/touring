"""Shared engine of the W4-W7 ``w<N>_rewrite_crate_paths.py`` scripts.

Each fusion wave moved crates into submodules of a fused crate, so path references inside
the moved files must be rewritten. What differs per wave is only the *rules* (which tokens
become what); the Rust-aware scanner that decides *where* a rule may fire — comments,
string literals, raw strings, char literals and lifetimes are data and stay untouched —
and the directory walk are identical, and live here once.

``rewrite_code_context`` is the scanner, ``rewrite_fused_source`` the rule set shared by
W5/W6/W7, ``rewrite_tree`` the directory walk and ``run_fused_cli`` the module-argument CLI.
"""
from __future__ import annotations

import sys
from collections.abc import Callable
from pathlib import Path

# (replacement, characters consumed, rewrites counted) — or None to leave the position alone.
Rewrite = tuple[str, int, int]
RewriteAt = Callable[[str, int], Rewrite | None]


def rewrite_code_context(text: str, rewrite_at: RewriteAt) -> tuple[str, int]:
    """Return (new_text, num_rewrites). Only code-context positions reach ``rewrite_at``."""
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

        # Code context — let the wave's rules decide.
        hit = rewrite_at(text, i)
        if hit is not None:
            replacement, consumed, counted = hit
            out.append(replacement)
            i += consumed
            rewrites += counted
            continue

        out.append(ch)
        i += 1

    return "".join(out), rewrites


def rewrite_fused_source(
    text: str,
    module: str,
    cross: dict[str, str],
    is_prefixed: Callable[[str, str], bool],
) -> tuple[str, int]:
    """The W5/W6/W7 rule set: intra-fusion ``cross`` paths, then a bare ``crate::`` token.

    A bare ``crate::`` becomes ``crate::<module>::`` unless ``is_prefixed(after, module)`` —
    ``after`` being the text following the ``crate::`` — says it already carries a prefix
    (the idempotence guard, which differs per wave).
    """
    crate_prefix = f"crate::{module}::"

    def rewrite_at(text: str, i: int) -> Rewrite | None:
        # Cross-crate path rewrite (intra-fusion deps).
        for src_path, dst_path in cross.items():
            if text.startswith(src_path, i):
                return dst_path, len(src_path), 1
        if text.startswith("crate::", i):
            if is_prefixed(text[i + 7 :], module):
                return "crate::", 7, 0
            return crate_prefix, 7, 1
        return None

    return rewrite_code_context(text, rewrite_at)


def rewrite_tree(src_dir: Path, rewrite_file: Callable[[str], tuple[str, int]]) -> tuple[int, int]:
    """Rewrite every ``*.rs`` under ``src_dir`` in place; return (total rewrites, files changed)."""
    total = 0
    files_changed = 0
    for path in sorted(src_dir.rglob("*.rs")):
        original = path.read_text(encoding="utf-8")
        new, count = rewrite_file(original)
        if count > 0:
            path.write_text(new, encoding="utf-8")
            files_changed += 1
            total += count
            print(f"  {path.relative_to(src_dir)}: {count} rewrites")
    return total, files_changed


def run_fused_cli(
    usage: str,
    cross_crate: dict[str, dict[str, str]],
    default_dir: Callable[[str], str],
    rewrite_source: Callable[[str, str, dict[str, str]], tuple[str, int]],
) -> int:
    """``<module> [<src_dir>]`` CLI of the W5/W6/W7 scripts. Returns the process exit code."""
    if len(sys.argv) < 2:
        print(usage, file=sys.stderr)
        return 2

    module = sys.argv[1]
    if module not in cross_crate:
        print(
            f"ERROR: unknown module '{module}' "
            f"(expected one of {sorted(cross_crate)})",
            file=sys.stderr,
        )
        return 2

    src_dir = Path(sys.argv[2] if len(sys.argv) > 2 else default_dir(module))
    if not src_dir.is_dir():
        print(f"ERROR: {src_dir} not found — run from workspace root", file=sys.stderr)
        return 2

    cross = cross_crate[module]
    total, files_changed = rewrite_tree(src_dir, lambda text: rewrite_source(text, module, cross))
    print(f"\nTOTAL [{module}]: {total} rewrites across {files_changed} files")
    return 0
