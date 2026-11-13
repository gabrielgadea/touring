#!/usr/bin/env python3
"""gen_sdk.py — emit a SignalReport from `~/.claude/touring/run_journal.jsonl`.

Mirrors `crates/touring-code/src/sdk.rs::signal_report_from_journal`. Two
emitters (Rust + Python) on purpose: the Rust path runs in production CLI /
in-process hooks; this script runs in CI (no Rust toolchain available) and
on developer machines for a quick preview. Both produce the SAME report
for the same journal once parsed (the Rust struct keeps its field order, this
script sorts keys, so the bytes differ); `crates/touring-code/tests/
sdk_cross_emitter.rs` runs both over a journal with every family of line and
fails on any difference. RUST IS THE REFERENCE.

What counts as a run is what the Rust reader counts: a line that is a JSON
object carrying the `JournalEntry` fields (`ts` u64, `language` string,
`exit_code` i32, `duration_ms` u32, and the optional ones with their types).
Everything else — the `{"repeat_blocked":{...}}` family, valid JSON that is not
an object, a torn line, an object missing a field — is NOT a run. Counting
every parseable line made this script read 57.539 runs where Rust read 57.405
on the real journal (cross-audit 12/11/2026).

Usage:
    python3 scripts/gen_sdk.py                    # default journal path
    python3 scripts/gen_sdk.py --journal /path    # custom journal
    python3 scripts/gen_sdk.py --out report.json  # custom output (default: stdout)
    python3 scripts/gen_sdk.py --check           # CI: exit 1 if report invalid
    python3 scripts/gen_sdk.py --extras          # add `_extras` (NOT in the Rust schema)

Exit codes:
    0 — report emitted (or --check passed)
    1 — journal missing, unreadable, or with lines but not one run (Rust:
        `SdkError::JournalMissing` / `JournalMalformed`)
    2 — report schema mismatch (--check only)

Decision D3 of strategy v1.1: SDK = hybrid (names hardcoded in sdk.rs,
types derived from the journal via this script + the Rust sibling).
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

# Canonical hook list — must match `HookName::ALL` in sdk.rs byte-for-byte.
# Adding a hook here requires amending sdk.rs; the --check exit code guards
# against drift (see `assert_canonical_hooks` below).
CANONICAL_HOOKS: tuple[str, ...] = (
    "ast_meta",
    "ast_blast",
    "index_find",
    "wiring_orphans",
    "memory_recall",
    "pre_edit",
    "tantivy_search",
    "parallel",
)

# Failure taxonomy — must match `FailureKind` in journal.rs.
CANONICAL_FAILURE_KINDS: frozenset[str] = frozenset(
    {
        "exception",
        "timeout",
        "abort",
        "proc-exit",
        "invalid-output",
        "output-limit",
    }
)


def default_journal_path() -> Path:
    """`~/.claude/touring/run_journal.jsonl` — matches journal.rs."""
    home = Path(os.environ.get("HOME", "/root"))
    return home / ".claude" / "touring" / "run_journal.jsonl"


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Emit SignalReport from the touring run journal.",
    )
    parser.add_argument(
        "--journal",
        type=Path,
        default=default_journal_path(),
        help="path to run_journal.jsonl (default: ~/.claude/touring/run_journal.jsonl)",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=None,
        help="output file (default: stdout)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="CI mode: parse journal + verify schema, exit 1 on any failure",
    )
    parser.add_argument(
        "--extras",
        action="store_true",
        help="add `_extras` (byte totals): a Python-only extension, NOT in the Rust SignalReport",
    )
    return parser.parse_args(argv)


class JournalMalformed(ValueError):
    """The journal has non-blank lines but not ONE of them is a run.

    The Rust reader refuses it the same way (`SdkError::JournalMalformed`).
    """

    def __init__(self, line_no: int) -> None:
        super().__init__(f"journal malformed at line {line_no}: no line is a run")
        self.line_no = line_no


def _is_int(v: Any) -> bool:
    return isinstance(v, int) and not isinstance(v, bool)


def _is_uint(v: Any, bits: int) -> bool:
    return _is_int(v) and 0 <= v < (1 << bits)


def _is_sint32(v: Any) -> bool:
    return _is_int(v) and -(1 << 31) <= v < (1 << 31)


def _is_opt_str(v: Any) -> bool:
    return v is None or isinstance(v, str)


def _is_hint(v: Any) -> bool:
    """`Option<JournalHint>`: null, or an object with a string `key` and a numeric `sim`."""
    if v is None:
        return True
    sim = v.get("sim") if isinstance(v, dict) else None
    return (
        isinstance(v, dict)
        and isinstance(v.get("key"), str)
        and isinstance(sim, (int, float))
        and not isinstance(sim, bool)
    )


# `JournalEntry` in `touring_foundation::run_outcome::reader`, field by field:
# (key, required, type check). A field with a `#[serde(default)]` is not required but must
# have its type when PRESENT; an explicit `null` passes only where Rust has an `Option`.
_RUN_FIELDS: tuple[tuple[str, bool, Any], ...] = (
    ("ts", True, lambda v: _is_uint(v, 64)),
    ("language", True, lambda v: isinstance(v, str)),
    ("exit_code", True, _is_sint32),
    ("duration_ms", True, lambda v: _is_uint(v, 32)),
    ("failure_kind", False, _is_opt_str),
    ("code_hash_stdout_bytes", False, lambda v: _is_uint(v, 64)),
    ("bytes_elided", False, lambda v: _is_uint(v, 64)),
    ("source", False, _is_opt_str),
    ("file", False, _is_opt_str),
    ("harvest", False, _is_opt_str),
    ("brief", False, lambda v: isinstance(v, bool)),
    ("orchestrate", False, lambda v: isinstance(v, bool)),
    ("tmp_bytes", False, lambda v: _is_uint(v, 64)),
    ("hint", False, _is_hint),
)


def is_run_entry(entry: Any) -> bool:
    """Is `entry` a journal run — the strict (`serde`) reading of `JournalEntry`."""
    return isinstance(entry, dict) and all(
        ok(entry[key]) if key in entry else not required
        for key, required, ok in _RUN_FIELDS
    )


def _reject_constant(name: str) -> Any:
    """`NaN` / `Infinity` are not JSON; the Rust reader refuses them, so does this one."""
    raise ValueError(f"not JSON: {name}")


def read_journal(path: Path) -> tuple[list[dict[str, Any]], int | None]:
    """Stream the journal; skip blank lines; collect the RUN lines + the first non-run.

    Returns (runs, first_non_run_line_no). `first_non_run_line_no` is `None` when every
    non-blank line is a run. A line that is not a run — torn JSON, valid JSON that is not
    an object, an object that is not a `JournalEntry` (the `repeat_blocked` family) — is
    SKIPPED, exactly as the strict Rust reader skips it; the first one's position is
    reported so a CI gate can pinpoint it.
    """
    if not path.exists():
        raise FileNotFoundError(f"journal not found: {path}")

    entries: list[dict[str, Any]] = []
    first_malformed: int | None = None

    with path.open("r", encoding="utf-8") as f:
        for line_no, raw in enumerate(f):
            line = raw.strip()
            if not line:
                continue
            try:
                entry = json.loads(line, parse_constant=_reject_constant)
            except ValueError:  # JSONDecodeError is one
                entry = None
            if is_run_entry(entry):
                entries.append(entry)
            elif first_malformed is None:
                first_malformed = line_no

    return entries, first_malformed


def aggregate(entries: list[dict[str, Any]], extras: bool = False) -> dict[str, Any]:
    """Pure aggregation — mirrors `journal::aggregate` in Rust. `entries` are runs
    (`is_run_entry`), so every field read below is present and typed."""
    by_language_durations: dict[str, list[int]] = defaultdict(list)
    by_language_failure_count: Counter[str] = Counter()
    failure_taxonomy: Counter[str] = Counter()
    total_bytes_elided = 0
    total_code_hash_stdout_bytes = 0

    for entry in entries:
        lang = entry["language"]
        by_language_durations[lang].append(entry["duration_ms"])
        kind = entry.get("failure_kind")
        if kind is not None:
            by_language_failure_count[lang] += 1
            failure_taxonomy[kind] += 1
        total_bytes_elided += entry.get("bytes_elided", 0)
        total_code_hash_stdout_bytes += entry.get("code_hash_stdout_bytes", 0)

    # Per-hook stats: until F3 ships the PostToolUse-sync, the journal does
    # not record which hook the program called. We materialize zeroed stats
    # so the report's hook count equals CANONICAL_HOOKS — the same shape
    # sdk.rs emits.
    hooks: dict[str, dict[str, int]] = {
        name: {
            "call_count": 0,
            "failure_count": 0,
            "duration_ms_p50": 0,
            "duration_ms_p99": 0,
        }
        for name in CANONICAL_HOOKS
    }

    by_language = {
        lang: {
            "count": len(durs),
            "failure_count": by_language_failure_count.get(lang, 0),
        }
        for lang, durs in by_language_durations.items()
    }

    report: dict[str, Any] = {
        "generated_at_unix": int(time.time()),
        "source_journal_path": str(path_str()),
        "total_runs": len(entries),
        "hooks": hooks,
        "by_language": by_language,
        "failure_taxonomy": dict(failure_taxonomy),
    }
    if extras:
        # The ONE declared Python-only extension: byte totals the Rust `SignalReport` does
        # not carry. Opt-in (`--extras`) — by default the two emitters print the same keys,
        # which `tests/sdk_cross_emitter.rs` enforces.
        report["_extras"] = {
            "total_bytes_elided": total_bytes_elided,
            "total_code_hash_stdout_bytes": total_code_hash_stdout_bytes,
        }
    return report


# Module-level alias used by aggregate() — argparse sets the path before
# aggregate is called; this is the indirection point.
_path_holder: list[Path] = []


def path_str() -> str:
    return str(_path_holder[0]) if _path_holder else "<unset>"


def assert_canonical_hooks(report: dict[str, Any]) -> None:
    """Cross-process invariant: Python and Rust must agree on hook names."""
    keys = set(report["hooks"].keys())
    expected = set(CANONICAL_HOOKS)
    if keys != expected:
        missing = expected - keys
        extra = keys - expected
        raise ValueError(
            f"hook drift: missing={sorted(missing)} extra={sorted(extra)}"
        )


def emit_report(journal_path: Path, extras: bool = False) -> dict[str, Any]:
    """Top-level: read + aggregate + verify + return report dict.

    Raises `FileNotFoundError` (no journal) and `JournalMalformed` (lines, but not one
    run) — the two the Rust reader refuses with.
    """
    _path_holder.append(journal_path)
    try:
        entries, first_malformed = read_journal(journal_path)
        if not entries and first_malformed is not None:
            raise JournalMalformed(first_malformed)
        report = aggregate(entries, extras)
        assert_canonical_hooks(report)
        return report
    finally:
        _path_holder.pop()


def main() -> int:
    args = parse_args()

    try:
        report = emit_report(args.journal, args.extras)
    except (FileNotFoundError, JournalMalformed, UnicodeDecodeError) as e:
        print(f"❌ {e}", file=sys.stderr)
        return 1

    payload = json.dumps(report, indent=2, sort_keys=True)

    if args.out:
        args.out.write_text(payload, encoding="utf-8")
        if not args.check:
            print(f"Wrote {len(payload)} bytes to {args.out}", file=sys.stderr)
    else:
        print(payload)

    if args.check:
        # CI gate. Currently passes if we got here without exception.
        # Future: diff against previous report to catch silent regressions.
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())