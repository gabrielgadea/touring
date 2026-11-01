#!/usr/bin/env python3
"""judge_attest.py — the integrity ledger for the loop's own graders.

Law L2 says code, never the LLM, ends the loop: `loop_converged.py` is the only
thing allowed to say "done". Law L3 says the verdict is an artifact:
`loop_outer_gate.py` + `flow_manifests.json` decide whether a turn may end.

Those three files are writable by the agent they judge, and until now nothing
recorded when they changed. So a verdict issued by a weakened judge was
indistinguishable from a verdict issued by the judge of record.

That is not a hypothetical. The Darwin Godel Machine paper (Zhang et al., ICLR
2026, arXiv:2505.22954, Appendix H) ran a self-improving agent against a
hallucination detector and reported node 114: a *perfect* score reached in two
modifications by deleting the markers the detector counted -- while under an
explicit instruction not to touch them. Their mitigation is stated plainly:
"the hallucination checking functions are hidden from the coding agent during
self-modification. We found that objective hacking occurs more frequently when
these functions are not hidden."

This repository has the same failure in its own history, twice, without any
malice: `find-code` shipped fabricated results with two tests asserting the bug,
and `an_unlabelled_dag_reads_as_ungated_implementation` encoded the very defect
it was meant to catch. Drift, not sabotage, is the threat model.

WHAT THIS BUYS, STATED HONESTLY
-------------------------------
It does not prevent objective hacking. Nothing living on a writable filesystem
can prevent an adversarial writer, and claiming otherwise would sell security
that does not exist. What it does is destroy the *deniability*: weakening the
rubric stops being an invisible side effect and becomes a dated, written act.
That is the same shape as `adw new --verdict` (does not stop you creating a
redundant flow; stops you creating one without judging) and `when_not_to_use`
(does not stop a recommendation; lets the portfolio rule a flow out).

WHAT BLOCKS AND WHAT ONLY SPEAKS
--------------------------------
  * a clause that VANISHED blocks. That is node 114 exactly -- the count lost an
    entry. Removing a clause on purpose is legitimate; doing it silently is not,
    so the remedy is one command (`--attest`), which dates the decision.
  * a file that CHANGED does not block, it stamps the verdict. Blocking here
    would break every legitimate improvement of the judge and teach the operator
    to re-attest reflexively -- the "gate whose failure has no automated remedy"
    this project already learned to avoid (2026-08-19). Silence is the enemy,
    not change.

Disarming this costs two coordinated edits in two files (the judge computes it,
and the Stop-side gate reads it), each leaving the other's trace. Not
inviolable; merely no longer accidental.

Usage:
    judge_attest.py [--json] [--quiet]     # report drift; exit 1 if blocking
    judge_attest.py --attest [--why "..."] # record the current state (human act)

Exit codes: 0 clean (or advisory-only drift) / 1 blocking drift / 2 usage error.
"""
from __future__ import annotations

import argparse
import ast
import datetime
import hashlib
import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ATTEST_PATH = HERE / "judge.attest.json"

#: The graders, by the law each one enforces. Relative to this file's directory
#: so the ledger travels with the skill rather than with one machine's layout.
JUDGE_FILES = {
    "loop_converged.py": "L2 — the only thing allowed to say 'done'",
    "hooks/loop_outer_gate.py": "L3 — the artifact manifest gate",
    "hooks/flow_manifests.json": "L3 — the artifacts each flow owes",
}

#: The function whose `yield`s ARE the rubric. Parsed as an AST rather than
#: grepped: a regex over source is exactly the brittle instrument this project
#: keeps getting burned by, and a rubric read wrongly is worse than none.
CLAUSE_SOURCE = ("loop_converged.py", "_gather_clauses")

SCHEMA_VERSION = 1


def sha256_of(path: Path) -> str | None:
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError:
        return None


def enforced_clauses(path: Path, func_name: str) -> list[str] | None:
    """The clause names actually yielded by ``func_name``, in source order.

    Returns None when the function cannot be found or parsed -- which is itself
    reportable drift, never a silent empty list. An empty rubric that reads as
    "no clauses removed" would invert this module's whole purpose.
    """
    try:
        tree = ast.parse(path.read_text(encoding="utf-8"))
    except (OSError, SyntaxError):
        return None
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef) and node.name == func_name:
            # Sorted by line, then de-duplicated: `ast.walk` is breadth-first, so
            # a yield nested in an `if` surfaces after its top-level siblings, and
            # the same clause name is yielded once per branch (cargo_green is
            # yielded twice — Rust scope and not). The rubric is the SET of names
            # a verdict can carry, so a name appearing under two conditions is one
            # clause, and a ledger a human reads must list it in source order.
            seen, names = set(), []
            found = [(sub.lineno, sub.value.elts[0].value)
                     for sub in ast.walk(node)
                     if isinstance(sub, ast.Yield) and isinstance(sub.value, ast.Tuple)
                     and sub.value.elts
                     and isinstance(sub.value.elts[0], ast.Constant)
                     and isinstance(sub.value.elts[0].value, str)]
            for _, name in sorted(found):
                if name not in seen:
                    seen.add(name)
                    names.append(name)
            return names
    return None


def inspect_judge(root: Path = HERE) -> dict:
    """The graders' current state: one digest per file, plus the live rubric."""
    files = {}
    for rel in JUDGE_FILES:
        files[rel] = sha256_of(root / rel)
    clause_file, clause_fn = CLAUSE_SOURCE
    return {"files": files, "clauses": enforced_clauses(root / clause_file, clause_fn)}


def load_attestation(path: Path = ATTEST_PATH) -> dict | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def compare(attested: dict | None, current: dict) -> list[dict]:
    """Typed drift between the attested state and the live one.

    Each entry carries ``blocking`` so callers never re-derive severity and can
    never disagree about it -- the same reason the terminal-subtask vocabulary
    was hoisted into ``loop_marker``.
    """
    drift: list[dict] = []

    if current["clauses"] is None:
        drift.append({
            "kind": "rubric_unreadable",
            "detail": f"{CLAUSE_SOURCE[1]} not found or unparsable in {CLAUSE_SOURCE[0]}",
            "blocking": True,
        })

    if attested is None:
        drift.append({
            "kind": "unattested",
            "detail": "no judge.attest.json — run `judge_attest.py --attest` to record the judge of record",
            "blocking": False,
        })
        return drift

    old_files = attested.get("files") or {}
    for rel in JUDGE_FILES:
        now, before = current["files"].get(rel), old_files.get(rel)
        if now is None:
            drift.append({"kind": "file_missing", "detail": rel, "blocking": True})
        elif before is None:
            drift.append({"kind": "file_unattested", "detail": rel, "blocking": False})
        elif now != before:
            drift.append({
                "kind": "file_changed",
                "detail": f"{rel} ({before[:12]}… → {now[:12]}…)",
                "blocking": False,
            })

    before_clauses = attested.get("clauses")
    if current["clauses"] is not None and before_clauses is not None:
        for name in before_clauses:
            if name not in current["clauses"]:
                drift.append({
                    "kind": "clause_removed",
                    "detail": f"{name} — the rubric shrank; a verdict under it is not the verdict of record",
                    "blocking": True,
                })
        for name in current["clauses"]:
            if name not in before_clauses:
                drift.append({"kind": "clause_added", "detail": name, "blocking": False})
    return drift


def verdict(root: Path = HERE, attest_path: Path | None = None) -> dict:
    """The judge's self-report, shaped to be embedded in a convergence verdict."""
    current = inspect_judge(root)
    attested = load_attestation(attest_path or (root / ATTEST_PATH.name))
    drift = compare(attested, current)
    return {
        "attested": attested is not None,
        "attested_at": (attested or {}).get("attested_at"),
        "clauses_enforced": current["clauses"],
        "clauses_attested": (attested or {}).get("clauses"),
        "drift": drift,
        "blocking": any(d["blocking"] for d in drift),
        "clean": not drift,
    }


def write_attestation(root: Path = HERE, why: str | None = None) -> dict:
    current = inspect_judge(root)
    doc = {
        "schema_version": SCHEMA_VERSION,
        "attested_at": datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
        "attested_by": os.environ.get("USER") or "unknown",
        "why": why or "",
        "files": current["files"],
        "clauses": current["clauses"],
        "_note": "The judge of record. Rewriting this is a deliberate, dated act: "
                 "it declares that the graders changed on purpose. See judge_attest.py.",
    }
    (root / ATTEST_PATH.name).write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    return doc


def require_human_hand() -> bool:
    """Is a human actually here? Attesting is the one act that must not be delegated.

    The attestation is all that stands between the judge and the sessions it
    judges — and on 15-16/09/2026 it was rewritten twice in two days by agent
    sessions, neither time out of disobedience: the decision had been approved
    and the condition measured, so each session concluded it was allowed to act.
    A sentence in a docstring ("a human act") does not survive that kind of zeal,
    and the files carry no permission barrier either, since every session runs as
    the same Unix user.

    So the barrier is a terminal. An agent session has no controlling TTY; a
    human at a prompt does. This cannot be satisfied by exporting a variable,
    which is precisely why no override flag exists here. CI is not an exception:
    CI must never attest.
    """
    try:
        if sys.stdin.isatty() and sys.stdout.isatty():
            return True
    except (ValueError, OSError):
        pass
    print(
        "judge_attest: --attest needs a terminal, and this process has none.\n"
        "  Attesting declares that a HUMAN inspected the graders and accepts them as the\n"
        "  judge of record; it is the only defence against the judge being rewritten by\n"
        "  what it judges. There is deliberately no flag to bypass this.\n"
        "  Run it in a REAL terminal window (not through an agent, and not through\n"
        "  Claude Code's '!' prefix — that runs in the session's shell, which has no\n"
        "  TTY either, as measured on 16/09/2026):\n"
        "      python3 ~/.claude/skills/loop-engineering/scripts/judge_attest.py --attest --why '<reason>'",
        file=sys.stderr,
    )
    return False


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Integrity ledger for the loop's graders.")
    ap.add_argument("--attest", action="store_true",
                    help="record the current graders as the judge of record (a human act)")
    ap.add_argument("--why", default=None, help="why the graders changed (stored with --attest)")
    ap.add_argument("--json", action="store_true", help="emit JSON only")
    ap.add_argument("--quiet", action="store_true", help="no output, just the exit code")
    args = ap.parse_args(argv)

    if args.attest:
        if not require_human_hand():
            return 2
        doc = write_attestation(why=args.why)
        if not args.quiet:
            n = len(doc["clauses"] or [])
            print(json.dumps(doc, indent=2) if args.json
                  else f"attested {len(doc['files'])} grader files · {n} clauses · {doc['attested_at']}")
        return 0

    rep = verdict()
    if args.json:
        print(json.dumps(rep, indent=2))
    elif not args.quiet:
        if rep["clean"]:
            print(f"✅ judge of record intact · {len(rep['clauses_enforced'] or [])} clauses")
        else:
            for d in rep["drift"]:
                print(f"  {'❌' if d['blocking'] else '⚠️ '} {d['kind']:<18} {d['detail']}")
            if rep["blocking"]:
                print("  → the rubric changed in a way that invalidates a verdict; "
                      "fix it, or declare it: judge_attest.py --attest --why '<reason>'")
    return 1 if rep["blocking"] else 0


if __name__ == "__main__":
    sys.exit(main())
