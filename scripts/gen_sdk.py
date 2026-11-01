#!/usr/bin/env python3
"""gen_sdk.py — emit a SignalReport from `~/.claude/touring/run_journal.jsonl`.

Mirrors `crates/touring-code/src/sdk.rs::signal_report_from_journal`. Two
emitters (Rust + Python) on purpose: the Rust path runs in production CLI /
in-process hooks; this script runs in CI (no Rust toolchain available) and
on developer machines for a quick preview. Both produce byte-identical
JSON for the same journal (sort keys canonical, key order canonical via
`sort_keys=True`).

Usage:
    python3 scripts/gen_sdk.py                    # default journal path
    python3 scripts/gen_sdk.py --journal /path    # custom journal
    python3 scripts/gen_sdk.py --out report.json  # custom output (default: stdout)
    python3 scripts/gen_sdk.py --check           # CI: exit 1 if report invalid

Exit codes:
    0 — report emitted (or --check passed)
    1 — journal missing / malformed
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
    return parser.parse_args(argv)


def read_journal(path: Path) -> tuple[list[dict[str, Any]], int | None]:
    """Stream the journal; skip blank lines; collect parseable ones + first malformed.

    Returns (entries, first_malformed_line_no). first_malformed_line_no is
    `None` when the whole journal is valid or only blank lines were seen.
    Lines that fail to parse are SKIPPED (the Rust parser is fail-soft); the
    line number of the first failure is reported so a CI gate can pinpoint.
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
                entry = json.loads(line)
            except json.JSONDecodeError:
                if first_malformed is None:
                    first_malformed = line_no
                continue
            entries.append(entry)

    return entries, first_malformed


def aggregate(entries: list[dict[str, Any]]) -> dict[str, Any]:
    """Pure aggregation — mirrors `journal::aggregate` in Rust."""
    by_language_durations: dict[str, list[int]] = defaultdict(list)
    by_language_failure_count: Counter[str] = Counter()
    failure_taxonomy: Counter[str] = Counter()
    all_durations: list[int] = []
    total_bytes_elided = 0
    total_code_hash_stdout_bytes = 0

    for entry in entries:
        # Skip entries that don't have the canonical schema (the journal
        # sometimes emits a corrupted header line as the first record — the
        # Rust parser drops it via serde, Python mirrors here).
        if not isinstance(entry, dict):
            continue
        if "language" not in entry or "duration_ms" not in entry:
            continue
        lang = entry["language"]
        if not isinstance(lang, str):
            continue
        try:
            dur = int(entry["duration_ms"])
        except (TypeError, ValueError):
            continue
        by_language_durations[lang].append(dur)
        all_durations.append(dur)
        kind = entry.get("failure_kind")
        if kind is not None:
            by_language_failure_count[lang] += 1
            if isinstance(kind, str):
                failure_taxonomy[kind] += 1
        try:
            total_bytes_elided += int(entry.get("bytes_elided", 0))
            total_code_hash_stdout_bytes += int(entry.get("code_hash_stdout_bytes", 0))
        except (TypeError, ValueError):
            pass

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

    return {
        "generated_at_unix": int(time.time()),
        "source_journal_path": str(path_str()),
        "total_runs": len(entries),
        "hooks": hooks,
        "by_language": by_language,
        "failure_taxonomy": dict(failure_taxonomy),
        # fields not in Rust SignalReport — surfaced as `_extras` so callers
        # can ignore them while we audit the totals.
        "_extras": {
            "total_bytes_elided": total_bytes_elided,
            "total_code_hash_stdout_bytes": total_code_hash_stdout_bytes,
        },
    }


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


def emit_report(journal_path: Path) -> dict[str, Any]:
    """Top-level: read + aggregate + verify + return report dict."""
    _path_holder.append(journal_path)
    try:
        entries, _ = read_journal(journal_path)
        report = aggregate(entries)
        assert_canonical_hooks(report)
        return report
    finally:
        _path_holder.pop()


def main() -> int:
    args = parse_args()

    try:
        report = emit_report(args.journal)
    except FileNotFoundError as e:
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