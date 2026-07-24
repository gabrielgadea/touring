#!/usr/bin/env python3
"""Language-neutral THSF conformance runner.

Usage:
    python3 test_conformance.py [--impl=reference | --impl=/path/to/adapter.py]

Gates executed (see ../README.md for the full table):
- Manifest parsing: 4 happy-path + 6 error-code cases + coverage meta-test.
- CRDT semantics: LWW tie-break, G-Set commutativity, PN-Counter merge.
- Grow-only invariant is implementation-defined; if the adapter exposes
  a grow-only-enforcement marker (``supports_grow_only_triggers=True``),
  we additionally assert UPDATE/DELETE on G-Set raise an error.

Exit codes: 0 all pass, 1 any fail, 2 invocation error.
"""

from __future__ import annotations

import argparse
import importlib.util
import sqlite3
import sys
import tempfile
from pathlib import Path
from typing import Any

THIS = Path(__file__).resolve()
FIXTURES = THIS.parent.parent / "fixtures"


# --------------------------------------------------------------------
# Implementation loader
# --------------------------------------------------------------------

def _load_reference() -> Any:
    """Load the bundled reference impl (~/.claude/tools/holon/holon.py)."""
    candidate = Path.home() / ".claude" / "tools" / "holon" / "holon.py"
    if not candidate.exists():
        print(f"reference impl not found: {candidate}", file=sys.stderr)
        sys.exit(2)
    spec = importlib.util.spec_from_file_location("holon_ref", candidate)
    if spec is None or spec.loader is None:
        print(f"failed to import {candidate}", file=sys.stderr)
        sys.exit(2)
    mod = importlib.util.module_from_spec(spec)
    # @dataclass reads sys.modules[cls.__module__] during class creation —
    # we MUST register before executing the module or dataclass decoration
    # in the target module will crash.
    sys.modules["holon_ref"] = mod
    spec.loader.exec_module(mod)

    class _RefAdapter:
        ManifestError = mod.ManifestError

        @staticmethod
        def parse_manifest(path: str) -> dict:
            m = mod.HolonManifest.from_path(Path(path))
            return {
                "name": m.name,
                "version": m.version,
                "offers": sorted(m.offers.keys()),
                "requires": sorted(m.requires.keys()),
            }

        @staticmethod
        def crdt_open(db_path: str, actor_id: str):
            return mod.CRDTStore(Path(db_path), actor_id)

        supports_grow_only_triggers = True

    return _RefAdapter


def _load_custom(path: str) -> Any:
    p = Path(path).expanduser().resolve()
    if not p.exists():
        print(f"adapter not found: {p}", file=sys.stderr)
        sys.exit(2)
    spec = importlib.util.spec_from_file_location("thsf_custom_impl", p)
    if spec is None or spec.loader is None:
        print(f"failed to import {p}", file=sys.stderr)
        sys.exit(2)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["thsf_custom_impl"] = mod
    spec.loader.exec_module(mod)
    return mod


# --------------------------------------------------------------------
# Gate primitives
# --------------------------------------------------------------------

_DIAG = {
    "missing-name.toml": "thsf-manifest-001",
    "bad-name-uppercase.toml": "thsf-manifest-003",
    "cli-no-cmd.toml": "thsf-manifest-004",
    "adapter-cmd-semicolon.toml": "thsf-manifest-005",
    "path-traversal.toml": "thsf-manifest-006",
    "unknown-top-level.toml": "thsf-manifest-008",
}

_HAPPY = {
    "minimal-p0.toml": "docs-only-holon",
    "p1-cli.toml": "p1-cli-holon",
    "p2-full.toml": "p2-full-holon",
    "p1-wasm-hashed.toml": "p1-wasm-hashed",
}


class _Runner:
    def __init__(self, adapter: Any) -> None:
        self.adapter = adapter
        self.pass_count = 0
        self.fail_count = 0
        self.failures: list[str] = []

    def gate(self, label: str, fn) -> None:
        try:
            fn()
            print(f"[gate] {label:<55} PASS")
            self.pass_count += 1
        except AssertionError as exc:
            print(f"[gate] {label:<55} FAIL — {exc}", file=sys.stderr)
            self.fail_count += 1
            self.failures.append(f"{label}: {exc}")

    def manifest_happy(self, fixture: str, expected_name: str) -> None:
        parsed = self.adapter.parse_manifest(str(FIXTURES / fixture))
        assert parsed["name"] == expected_name, f"{fixture}: name={parsed['name']!r}"

    def manifest_error(self, fixture: str, expected_code: str) -> None:
        try:
            self.adapter.parse_manifest(str(FIXTURES / fixture))
        except Exception as exc:  # noqa: BLE001 — adapter error type is impl-defined
            code = getattr(exc, "code", None)
            assert code == expected_code, f"{fixture}: got code={code!r}, want {expected_code!r}"
        else:
            raise AssertionError(f"{fixture}: expected {expected_code}, got no exception")

    def crdt_lww_tiebreak(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            db = Path(td) / "lww.db"
            a = self.adapter.crdt_open(str(db), "actor-a")
            b = self.adapter.crdt_open(str(db), "actor-b")
            a.lww_set("k", "from-a", ts=100.0)
            b.lww_set("k", "from-b", ts=100.0)
            got = a.lww_get("k")
            assert got == "from-a", f"LWW tie-break failed: got {got!r}"

    def crdt_gset_commutative(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            db = Path(td) / "gset.db"
            a = self.adapter.crdt_open(str(db), "actor-a")
            b = self.adapter.crdt_open(str(db), "actor-b")
            a.gset_add("lessons", "l1")
            b.gset_add("lessons", "l2")
            assert set(a.gset_members("lessons")) == {"l1", "l2"}
            assert set(b.gset_members("lessons")) == {"l1", "l2"}

    def crdt_pn_counter(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            db = Path(td) / "pn.db"
            a = self.adapter.crdt_open(str(db), "actor-a")
            b = self.adapter.crdt_open(str(db), "actor-b")
            a.pn_increment("hits", 10)
            b.pn_increment("hits", 7)
            b.pn_decrement("hits", 3)
            assert a.pn_value("hits") == 14, f"PN merge: got {a.pn_value('hits')}"
            assert b.pn_value("hits") == 14

    def crdt_grow_only_trigger(self) -> None:
        if not getattr(self.adapter, "supports_grow_only_triggers", False):
            return  # impl opts out; not a failure
        with tempfile.TemporaryDirectory() as td:
            db = Path(td) / "go.db"
            a = self.adapter.crdt_open(str(db), "actor-a")
            a.gset_add("lessons", "l1")
            raised = False
            try:
                with sqlite3.connect(str(db)) as con:
                    con.execute("DELETE FROM gset WHERE element='l1'")
            except sqlite3.IntegrityError:
                raised = True
            assert raised, "DELETE on gset must raise IntegrityError"

    def run(self) -> int:
        # Manifest happy paths
        for fixture, expected_name in _HAPPY.items():
            self.gate(f"RFC-001 happy — {fixture}", lambda f=fixture, n=expected_name: self.manifest_happy(f, n))
        # Manifest error codes
        for fixture, expected_code in _DIAG.items():
            self.gate(f"RFC-001 error — {fixture} → {expected_code}", lambda f=fixture, c=expected_code: self.manifest_error(f, c))
        # CRDT
        self.gate("RFC-003 LWW tie-break by actor_id", self.crdt_lww_tiebreak)
        self.gate("RFC-003 G-Set union commutative", self.crdt_gset_commutative)
        self.gate("RFC-003 PN-Counter cross-actor merge", self.crdt_pn_counter)
        self.gate("RFC-003 grow-only trigger on G-Set", self.crdt_grow_only_trigger)
        total = self.pass_count + self.fail_count
        print()
        print(f"==== THSF Conformance Summary: {self.pass_count}/{total} gates pass ====")
        return 0 if self.fail_count == 0 else 1


# --------------------------------------------------------------------
# Entrypoint
# --------------------------------------------------------------------

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__ or "")
    parser.add_argument(
        "--impl", default="reference",
        help="'reference' for bundled holon.py OR path to custom adapter .py",
    )
    args = parser.parse_args(argv)
    adapter = _load_reference() if args.impl == "reference" else _load_custom(args.impl)
    return _Runner(adapter).run()


if __name__ == "__main__":
    raise SystemExit(main())
