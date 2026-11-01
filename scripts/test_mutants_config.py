#!/usr/bin/env python3
"""Invariants of `.cargo/mutants.toml` — the config that decides whether the
mutation harness can run at all.

Origin (2026-08-20): `additional_cargo_args = ["--all-targets"]` sat under a
comment reading "Skip bench/doctest targets". `--all-targets` INCLUDES benches,
so `nextest list` tried to enumerate the iai-callgrind harness, failed without
`iai-callgrind-runner` on PATH, and every workspace mutation run died at the
baseline — producing 0 mutants, which the CLI reported as `ok: true` and
`repo-score` consumed as the "testing" category. A 19-minute run yielding
nothing, read as a pass.
"""
from __future__ import annotations

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CONFIG = ROOT / ".cargo" / "mutants.toml"


def _args() -> list[str]:
    """The `additional_cargo_args` list, parsed without a TOML dependency."""
    text = CONFIG.read_text()
    marker = "additional_cargo_args"
    line = next(l for l in text.splitlines() if l.strip().startswith(marker))
    inner = line.split("[", 1)[1].rsplit("]", 1)[0]
    return [p.strip().strip('"') for p in inner.split(",") if p.strip()]


def test_config_exists() -> None:
    assert CONFIG.is_file(), f"missing {CONFIG}"


def test_bench_targets_are_never_listed() -> None:
    """`--all-targets` pulls benches into the test list and kills the baseline."""
    args = _args()
    assert "--all-targets" not in args, (
        "--all-targets INCLUDES bench targets; the iai-callgrind harness then "
        "fails `nextest list` and the mutation baseline dies with 0 mutants. "
        "Name the targets explicitly instead."
    )
    assert "--benches" not in args, "benches must not be in the test list"


def test_targets_are_named_explicitly() -> None:
    """Silence is not the same as exclusion — the targets must be stated."""
    args = _args()
    for expected in ("--lib", "--bins", "--tests"):
        assert expected in args, f"{expected} missing from additional_cargo_args"


def test_bench_glob_is_not_mistaken_for_a_test_filter() -> None:
    """`exclude_globs` skips MUTANT GENERATION, never the test list.

    The two are independent, and believing otherwise is what produced the
    original defect. Keep the glob (it is correct for its own purpose) and keep
    the explicit targets alongside it.
    """
    text = CONFIG.read_text()
    assert "**/benches/**" in text, "the mutant-generation glob was dropped"
    assert "--lib" in text, "explicit targets must accompany the glob"


NEXTEST = ROOT / ".config" / "nextest.toml"


def test_mutation_runs_use_the_dedicated_profile() -> None:
    """`ci` retries failures; under mutation a failure is the KILL signal."""
    args = _args_named("additional_cargo_test_args")
    assert "--profile" in args, "the nextest profile must be pinned"
    profile = args[args.index("--profile") + 1]
    assert profile == "mutants", (
        f"mutation runs must use the `mutants` profile, not `{profile}`: "
        "`ci` retries 3x with exponential backoff, which turns a caught "
        "mutant into a recorded timeout."
    )


def test_mutants_profile_never_retries() -> None:
    """A retried failure is a kill counted as a timeout."""
    text = NEXTEST.read_text()
    assert "[profile.mutants]" in text, "the `mutants` profile is missing"
    block = text.split("[profile.mutants]", 1)[1].split("\n[", 1)[0]
    assert "retries = 0" in block, (
        "the mutants profile must set `retries = 0` — inheriting the default "
        "profile's retries reintroduces the defect"
    )
    assert "fail-fast = true" in block, (
        "one failing test already proves the mutant caught; keep fail-fast"
    )


def _args_named(key: str) -> list[str]:
    text = CONFIG.read_text()
    line = next(l for l in text.splitlines() if l.strip().startswith(key))
    inner = line.split("[", 1)[1].rsplit("]", 1)[0]
    return [p.strip().strip('"') for p in inner.split(",") if p.strip()]


def main() -> int:
    failures = 0
    for name, fn in sorted(globals().items()):
        if not name.startswith("test_") or not callable(fn):
            continue
        try:
            fn()
            print(f"  ok   {name}")
        except AssertionError as exc:
            failures += 1
            print(f"  FAIL {name}: {exc}")
    print(f"{'FAILED' if failures else 'PASSED'} — {failures} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
