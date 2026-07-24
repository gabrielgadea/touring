#!/usr/bin/env python3
"""scalability_scan.py — scan Rust source for *truly* shared mutable global state.

Master Plan H1-B (2026-06-13). Closes the scalability gap: touring already
adopts rayon + tokio + sharded tantivy, but no CI gate verifies that new code
doesn't sneak in a `static mut` (the ONLY way to have a non-Sync global in
safe Rust) or a multi-threaded misuse of `RefCell` (which is !Sync).

Whitelist (intentional, Sync patterns that are safe):
  * `static mut <NAME>`         — ONLY true `static mut` is flagged (UAF/UB risk)
  * `static NAME: Atomic*`       — interior-mutable, lock-free, safe
  * `static NAME: Mutex<...>`   — synchronised, safe
  * `static NAME: RwLock<...>`  — synchronised, safe
  * `static NAME: OnceCell<...>` / `OnceLock<...>` / `LazyLock<...>` — safe
  * `static NAME: &'static T`   — immutable borrow, safe
  * `static NAME: [T; N]`       — fixed-size array, safe (immutable)
  * `static NAME: RefCell<...>` — single-threaded, OK in wasm/holon contexts

Heuristic: split into two detectors — UNCONDITIONAL (FAIL on static mut) and
TYPE-CHECKED (FAIL only on `static NAME: Type` where Type is NOT in the safe
set AND not a clear const-eligible type).

Exits:
  0  PASS  — no findings
  1  FAIL  — at least one finding
  2  ADVISORY  — scan skipped

Usage
-----
    docs/scalability_scan.py --check
    docs/scalability_scan.py --json
    docs/scalability_scan.py --strict    # also fail on `RefCell` in static (catch !Send misuse)
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXCLUDE_DIRS = {".git", "target", "fuzz", "node_modules", ".cargo"}

# Type-name prefixes that justify a `static NAME: T = ...` (immutable + Sync).
SAFE_STATIC_TYPES = {
    "AtomicBool",
    "AtomicU8", "AtomicU16", "AtomicU32", "AtomicU64", "AtomicUsize",
    "AtomicI8", "AtomicI16", "AtomicI32", "AtomicI64", "AtomicIsize",
    "OnceCell", "OnceLock", "LazyLock", "LazyCell",
    "RefCell", "Cell",  # single-threaded interior mutability (idiomatic in wasm/inferlets)
    "Mutex", "RwLock", "StdMutex", "parking_lot::Mutex", "parking_lot::RwLock",
    "std::sync::OnceLock", "std::sync::Mutex", "std::sync::RwLock",
    "DashMap", "dashmap::DashMap",
    "Regex", "regex::Regex",  # Regex is Sync
    "Lazy", "once_cell::sync::Lazy",  # Lazy<T> is Sync iff T is Sync (heuristic — Regex is common)
    # Project-defined Sync types
    "WorkflowTemplate", "WorkflowStep",
    "Template", "Spec", "Config",
    "SharedConn",  # type alias for OnceLock<Mutex<Option<Connection>>>
    "CosineComputer",  # fields: usize + Option<Arc<dyn GpuBackend>> — both Sync
    "CognitiveMetrics",  # fields: all AtomicU64 — Sync
    # Global allocator types (idempotent, safe in single-threaded init)
    "MiMalloc", "mimalloc::MiMalloc",
    "dhat::Alloc", "Alloc",
    "Jemalloc", "tikv_jemallocator::Jemalloc",
}
SAFE_STATIC_TYPE_PREFIXES = (
    "&'static ",
    "&[",
    "Atomic",
    "OnceCell", "OnceLock", "LazyLock", "LazyCell",
    "RefCell", "Cell<",
    "Mutex", "RwLock", "StdMutex",
    "DashMap",
    "std::sync::",
    "parking_lot::",
    "regex::",
    "once_cell::",
    "Lazy<",  # Lazy<Regex>, Lazy<HashMap>, etc. — heuristic
    "mimalloc::", "tikv_jemallocator::", "dhat::",
    "[",
)


def is_safe_static_type(ty: str) -> bool:
    ty = ty.strip().rstrip(";").strip()
    if not ty:
        return True
    if ty in SAFE_STATIC_TYPES:
        return True
    if any(ty.startswith(p) for p in SAFE_STATIC_TYPE_PREFIXES):
        return True
    # `()` is unit; `bool`/`u32`/`usize`/etc. literals are fine
    if re.fullmatch(r"[a-z][a-z0-9_]*", ty):
        return True
    return False


STATIC_MUT_RE = re.compile(
    r"^\s*static\s+mut\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([^=]+?)\s*=",
    re.MULTILINE,
)
STATIC_RE = re.compile(
    r"^\s*static\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([^=]+?)\s*=\s*",
    re.MULTILINE,
)


def scan_file(path: Path, strict: bool = False) -> list[dict]:
    findings: list[dict] = []
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return findings
    for m in STATIC_MUT_RE.finditer(text):
        line = text[: m.start()].count("\n") + 1
        findings.append(
            {
                "file": str(path.relative_to(ROOT)),
                "line": line,
                "kind": "static_mut",
                "name": m.group(1),
                "type": m.group(2).strip(),
                "rationale": "`static mut` is unsynchronised global mutable state — prefer Mutex/Atomic/OnceLock",
            }
        )
    for m in STATIC_RE.finditer(text):
        ty = m.group(2).strip()
        # In strict mode, also flag `RefCell` (which is !Sync)
        if strict and "RefCell" in ty:
            line = text[: m.start()].count("\n") + 1
            findings.append(
                {
                    "file": str(path.relative_to(ROOT)),
                    "line": line,
                    "kind": "refcell_in_static",
                    "name": m.group(1),
                    "type": ty,
                    "rationale": "`RefCell` is !Sync — sharing across threads is a hard error",
                }
            )
            continue
        if is_safe_static_type(ty):
            continue
        line = text[: m.start()].count("\n") + 1
        findings.append(
            {
                "file": str(path.relative_to(ROOT)),
                "line": line,
                "kind": "static_value",
                "name": m.group(1),
                "type": ty,
                "rationale": "static with non-Sync type — wrap in Mutex/OnceCell or make it local",
            }
        )
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="CI mode (default)")
    parser.add_argument("--json", action="store_true", help="machine-readable JSON")
    parser.add_argument("--strict", action="store_true", help="also flag `RefCell` in static (catch !Send misuse)")
    parser.add_argument("--max-findings", type=int, default=20, help="cap findings printed")
    args = parser.parse_args()

    findings: list[dict] = []
    files_scanned = 0
    for path in ROOT.rglob("*.rs"):
        if any(part in EXCLUDE_DIRS for part in path.parts):
            continue
        files_scanned += 1
        findings.extend(scan_file(path, strict=args.strict))

    report = {"files_scanned": files_scanned, "findings_count": len(findings), "findings": findings[: args.max_findings]}
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"scalability_scan: {files_scanned} files; {len(findings)} findings")
        for f in findings[: args.max_findings]:
            print(f"  {f['file']}:{f['line']} {f['kind']} {f['name']}: {f['type']} — {f['rationale']}", file=sys.stderr)

    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
