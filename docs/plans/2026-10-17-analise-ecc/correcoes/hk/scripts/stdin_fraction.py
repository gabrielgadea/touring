"""Fraction of hook invocations whose stdin was not read cleanly, per class (guard vs advisory).

Source: the per-invocation trace `TOURING_HOOK_TRACE_FILE` (armed in ~/.claude/settings.json), one
JSON line per `touring-hook` process with `stdin_state`. Run it BEFORE and AFTER deploying C4 and
compare; `--since-ms` isolates the post-deploy window.

  python3 -I stdin_fraction.py [--trace PATH] [--since-ms EPOCH_MS]

States: ok | empty | terminal | unread are readable; `ok-recovered` is a used-but-degraded read
(timeout/IO error with a whole JSON in hand); timeout | read-error | parse-error | too-large are
UNREADABLE: a guard denies them (`exit_reason = stdin-unreadable-deny`), an advisory runs on `{}`.
"""
import argparse
import collections
import json
import os
import sys

GUARDS = {"pre-bash", "pre-edit", "pre-edit-prevention", "pre-read", "pre-tool-use", "pre-write"}
UNREADABLE = {"timeout", "read-error", "parse-error", "too-large"}
CLEAN = {"ok", "unread", "terminal"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--trace", default=os.path.expanduser("~/.claude/touring/hook_trace.jsonl"))
    ap.add_argument("--since-ms", type=int, default=0)
    args = ap.parse_args()
    by_class = {"guard": collections.Counter(), "advisory": collections.Counter()}
    denied = 0
    lines = bad = 0
    with open(args.trace, errors="replace") as fh:
        for line in fh:
            try:
                rec = json.loads(line)
            except ValueError:
                bad += 1
                continue
            if rec.get("ts_ms", 0) < args.since_ms:
                continue
            lines += 1
            klass = "guard" if rec.get("hook") in GUARDS else "advisory"
            by_class[klass][rec.get("stdin_state", "?")] += 1
            if rec.get("exit_reason") == "stdin-unreadable-deny":
                denied += 1
    out = {"lines": lines, "unparseable": bad, "guard_denies_on_unreadable_stdin": denied}
    for klass, states in by_class.items():
        total = sum(states.values())
        unreadable = sum(v for k, v in states.items() if k in UNREADABLE)
        degraded = sum(v for k, v in states.items() if k not in CLEAN)
        out[klass] = {
            "invocations": total,
            "states": dict(states),
            "unreadable": unreadable,
            "unreadable_fraction": round(unreadable / total, 6) if total else None,
            "degraded_fraction": round(degraded / total, 6) if total else None,
        }
    json.dump(out, sys.stdout, indent=1)
    sys.stdout.write("\n")


main()
