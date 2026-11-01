"""Scaffolding shared by the standalone test scripts (``python3 test_x.py``, no pytest needed).

Each script reports its checks through :func:`check`, which prints one PASS/FAIL line and remembers
the failures, and ends with ``return finish()`` — the summary line and the process exit code.
Under pytest the ``test_*`` functions also run, but :func:`check` only reports there.
"""

from __future__ import annotations

FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    """Print ``[PASS|FAIL] name — detail`` and record the failed names."""
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))
    if not ok:
        FAILURES.append(name)


def finish() -> int:
    """Print the closing summary and return the exit code (1 when any check failed)."""
    print(f"\n{len(FAILURES)} falha(s): {FAILURES}" if FAILURES else "\nTODOS OS TESTES PASSARAM")
    return 1 if FAILURES else 0
