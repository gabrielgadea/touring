#!/usr/bin/env python3
"""Tool-channel trio (efficacy / efficiency / effectiveness) measured over CC transcripts.

Versioned successor of the /tmp prototype used in rounds 4-5 (26/10/2026), whose
numbers were partly instrument artefacts (REVISAO-2026-10-27.md, R1):

* Bash was classed by its FIRST token, so ``cd X && cargo test`` counted as ``cd``
  and ``cat > f <<EOF`` (a write, empty output = success) counted as a "50%-empty
  cat". Here the class is the EFFECTIVE verb: prefixes (``cd``, ``export``,
  ``set``, ``source``, ``VAR=val``, ``timeout N``, ``env``, ``time``, ``nice``)
  are peeled, and ``cat`` is split into ``cat-read`` / ``cat-write``.
* "retry" was tool+target, so two different Edits of one file were a retry. Here a
  retry is the byte-identical call (same tool, same input) within the last
  ``WINDOW`` calls of the session -- the predicate of gate G6.
* a hook denial is not a tool failure: it is counted as ``negado``, apart from
  ``erro``; ``grep``/``rg`` exit 1 with no output is "no match" (``vazio``).

Usage:
    python3 scripts/measure_tool_trio.py [--sessions 200] [--json] [--dirs a,b]
"""
from __future__ import annotations

import argparse
import collections
import glob
import json
import os
import re
import sys
from dataclasses import dataclass, field

WINDOW = 8
EMPTY_BYTES = 50
DEFAULT_DIRS = (
    "-home-gabrielgadea-projects-touring",
    "-home-gabrielgadea-projects-analise",
    "-home-gabrielgadea-projects-konverter",
)
PREFIX_VERBS = {"cd", "export", "set", "source", ".", "pushd", "popd", "true", "unset", "shopt"}
WRAPPERS = {"env", "time", "nice", "nohup", "command", "exec"}
DECORATIVE_VERBS = {"echo", "printf"}
NO_MATCH_VERBS = {"grep", "egrep", "fgrep", "rg", "ugrep"}
SUBCOMMAND_VERBS = {"touring", "git", "cargo", "scripts/hw-slot"}
_SEGMENT_SPLIT = re.compile(r"&&|\|\||;|\n")
_ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
_CAT_WRITE = re.compile(r"^cat\b[^|]*(?:>|<<)")
_HOOK_DENIAL = re.compile(r"hook error|PreToolUse:\w+ hook|blocked by .*hook", re.IGNORECASE)
_EXIT_ONE_ONLY = re.compile(r"^\s*(?:Exit code 1)?\s*$")


def _first_tokens(segment: str) -> list[str]:
    """Tokens of one shell segment with assignments and wrappers peeled."""
    toks = segment.strip().split()
    while toks:
        head = toks[0]
        if _ASSIGN.match(head) or head in WRAPPERS:
            toks = toks[1:]
        elif head == "timeout":
            toks = toks[1:]
            while toks and (toks[0].startswith("-") or re.match(r"^\d+[smh]?$", toks[0])):
                toks = toks[1:]
        else:
            break
    return toks


def classify_bash(command: str) -> str:
    """Effective verb of a Bash command (the action, not its prefix).

    ``echo``/``printf`` lead many multi-step scripts as section headers; they are
    the verb only when nothing else runs after them.
    """
    segments = [_first_tokens(s) for s in _SEGMENT_SPLIT.split(command or "")]
    segments = [t for t in segments if t and t[0] not in PREFIX_VERBS and not t[0].startswith("#")]
    actions = [t for t in segments if t[0] not in DECORATIVE_VERBS]
    chosen = actions[:1] or segments[:1]
    return _verb_of(chosen[0]) if chosen else "(prefixo-só)"


def _verb_of(toks: list[str]) -> str:
    """Class label of one peeled segment: cat split by direction, subcommand kept."""
    verb = os.path.basename(toks[0]) if "/" in toks[0] and toks[0] != "scripts/hw-slot" else toks[0]
    if verb == "cat":
        return "cat-write" if _CAT_WRITE.match(" ".join(toks)) else "cat-read"
    if verb in SUBCOMMAND_VERBS and len(toks) > 1 and not toks[1].startswith("-"):
        return f"{verb} {toks[1]}"
    return verb


def tool_class(name: str, tool_input: dict) -> str:
    if name == "Bash":
        return "Bash:" + classify_bash(tool_input.get("command", ""))
    return name


def result_text(content) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        # Non-text blocks (tool_reference, image) are content too: count their JSON.
        return "".join(
            c["text"] if "text" in c else json.dumps(c, ensure_ascii=False)
            for c in content
            if isinstance(c, dict)
        )
    return ""


def outcome(cls: str, text: str, is_error: bool) -> str:
    """One of: negado, erro, vazio, ok."""
    stripped = text.strip()
    if is_error and _HOOK_DENIAL.search(text):
        return "negado"
    verb = cls.split(":", 1)[1] if cls.startswith("Bash:") else ""
    if is_error and verb in NO_MATCH_VERBS and _EXIT_ONE_ONLY.match(stripped):
        return "vazio"
    if is_error:
        return "erro"
    if cls == "Bash:cat-write":
        return "ok"  # a heredoc write prints nothing on success
    if len(stripped.encode()) < EMPTY_BYTES or stripped in ("[]", "{}", "null", "(No content)"):
        return "vazio"
    return "ok"


@dataclass
class Stats:
    calls: int = 0
    retry: int = 0
    result_bytes: int = 0
    outcomes: collections.Counter = field(default_factory=collections.Counter)

    def row(self) -> dict:
        n = max(self.calls, 1)
        pct = {k: round(100 * self.outcomes[k] / n, 1) for k in ("erro", "vazio", "negado")}
        return {
            "calls": self.calls,
            **{f"{k}%": v for k, v in pct.items()},
            "retry%": round(100 * self.retry / n, 1),
            "eficacia%": round(100 - pct["erro"] - pct["vazio"] - pct["negado"], 1),
            "result_bytes": self.result_bytes,
        }


def measure_session(records, stats: dict[str, Stats]) -> None:
    """Fold one session's records into ``stats`` (keyed by tool class)."""
    pending: dict[str, tuple[str, str]] = {}
    window: collections.deque = collections.deque(maxlen=WINDOW)
    for rec in records:
        msg = rec.get("message") or {}
        content = msg.get("content")
        if not isinstance(content, list):
            continue
        for c in content:
            if not isinstance(c, dict):
                continue
            if rec.get("type") == "assistant" and c.get("type") == "tool_use":
                name, inp = c.get("name", ""), c.get("input") or {}
                cls = tool_class(name, inp)
                sig = name + "\x00" + json.dumps(inp, sort_keys=True, ensure_ascii=False)
                st = stats.setdefault(cls, Stats())
                st.calls += 1
                if sig in window:
                    st.retry += 1
                window.append(sig)
                pending[c.get("id", "")] = (cls, sig)
            elif rec.get("type") == "user" and c.get("type") == "tool_result":
                hit = pending.pop(c.get("tool_use_id", ""), None)
                if not hit:
                    continue
                text = result_text(c.get("content"))
                st = stats[hit[0]]
                st.result_bytes += len(text.encode())
                st.outcomes[outcome(hit[0], text, bool(c.get("is_error")))] += 1


_GATE = re.compile(r"hook error:\s*\[([^\]]+)\]")
_WRITERS = {"Edit", "Write", "MultiEdit", "NotebookEdit"}


def gate_of(text: str) -> str:
    """Short name of the gate that denied a call (`G10`, `OUTER`, …), or `outro`."""
    m = _GATE.search(text)
    return m.group(1).split()[0] if m else "outro"


@dataclass
class Extras:
    """D1a: what the trio table does not show — deny→next action, sizes, superseded reads."""

    deny_next: collections.Counter = field(default_factory=collections.Counter)
    sizes: dict = field(default_factory=lambda: collections.defaultdict(list))
    read_bytes: int = 0
    read_superseded_bytes: int = 0


def _tool_events(records):
    """(kind, payload) in order: ('use', (id, name, input)) and ('result', (id, text, is_error))."""
    for rec in records:
        content = (rec.get("message") or {}).get("content")
        if not isinstance(content, list):
            continue
        for c in content:
            if not isinstance(c, dict):
                continue
            if rec.get("type") == "assistant" and c.get("type") == "tool_use":
                yield "use", (c.get("id", ""), c.get("name", ""), c.get("input") or {})
            elif rec.get("type") == "user" and c.get("type") == "tool_result":
                yield "result", (c.get("tool_use_id", ""), result_text(c.get("content")), bool(c.get("is_error")))


def measure_extras(records, ex: Extras) -> None:
    """Fold one session into ``ex`` with the same predicates as ``outcome``."""
    events = list(_tool_events(records))
    uses = {v[0]: (v[1], v[2]) for k, v in events if k == "use"}
    _fold_deny_next(events, uses, ex)
    _fold_sizes_and_reads(events, uses, ex)


def _signature(name: str, inp: dict) -> str:
    return name + "\x00" + json.dumps(inp, sort_keys=True, ensure_ascii=False)


def _fold_deny_next(events, uses, ex: Extras) -> None:
    """After a denial, the next call: repetiu (same call), negado-de-novo, recuperou, or abandonou."""
    pending: tuple[str, str] | None = None  # (gate, signature) of the denied call
    probe: tuple[str, str] | None = None  # (gate, id) of the call that followed it
    for kind, val in events:
        if kind == "use":
            uid, name, inp = val
            if pending is not None:
                if _signature(name, inp) == pending[1]:
                    ex.deny_next[(pending[0], "repetiu")] += 1
                else:
                    probe = (pending[0], uid)
                pending = None
            continue
        rid, text, is_err = val
        if rid not in uses:
            continue
        name, inp = uses[rid]
        verdict = outcome(tool_class(name, inp), text, is_err)
        if probe is not None and probe[1] == rid:
            ex.deny_next[(probe[0], "negado-de-novo" if verdict == "negado" else "recuperou")] += 1
            probe = None
        if verdict == "negado":
            pending = (gate_of(text), _signature(name, inp))
    if pending is not None:
        ex.deny_next[(pending[0], "abandonou")] += 1


def _fold_sizes_and_reads(events, uses, ex: Extras) -> None:
    """Result sizes per class; a Read is superseded when its file is touched again later."""
    last_touch: dict[str, int] = {}
    read_at: dict[str, tuple[int, str]] = {}  # result id -> (use position, file)
    for pos, (kind, val) in enumerate(events):
        if kind != "use":
            continue
        uid, name, inp = val
        target = inp.get("file_path") or inp.get("notebook_path")
        if target and (name in _WRITERS or name == "Read"):
            last_touch[target] = pos
            if name == "Read":
                read_at[uid] = (pos, target)
    for kind, (rid, text, _err) in ((k, v) for k, v in events if k == "result"):
        if rid not in uses:
            continue
        name, inp = uses[rid]
        size = len(text.encode())
        ex.sizes[tool_class(name, inp)].append(size)
        if rid in read_at:
            pos, target = read_at[rid]
            ex.read_bytes += size
            if last_touch.get(target, -1) > pos:
                ex.read_superseded_bytes += size


def size_summary(sizes: list[int]) -> dict:
    """p50/p95 and the share of results above 10 KB."""
    if not sizes:
        return {"n": 0}
    s = sorted(sizes)

    def pick(q: float) -> int:
        return s[min(len(s) - 1, int(q * len(s)))]

    return {"n": len(s), "p50": pick(0.5), "p95": pick(0.95),
            "gt10k%": round(100 * sum(1 for x in s if x > 10_000) / len(s), 1),
            "gt10k_bytes%": round(100 * sum(x for x in s if x > 10_000) / max(sum(s), 1), 1)}


def extras_report(ex: Extras, rows: dict) -> dict:
    """The D1a section of the JSON report."""
    gates: dict[str, dict[str, int]] = collections.defaultdict(dict)
    for (gate, verdict), n in ex.deny_next.items():
        gates[gate][verdict] = n
    return {
        "deny_next_by_gate": dict(sorted(gates.items(), key=lambda kv: -sum(kv[1].values()))),
        "sizes": {k: size_summary(ex.sizes.get(k, [])) for k in rows},
        "read_bytes": ex.read_bytes,
        "read_superseded_bytes": ex.read_superseded_bytes,
        "read_superseded%": round(100 * ex.read_superseded_bytes / max(ex.read_bytes, 1), 1),
    }


def iter_records(path: str, until: str = "", since: str = ""):
    """Records of one transcript inside ``[since, until)`` (ISO UTC; empty = open end).

    A record without a timestamp is dropped under ``since`` (it cannot be placed in the
    window). A call whose use falls before ``since`` and whose result falls after it has
    no pair in the window and is not counted: an A3 arm window loses its boundary calls,
    never gains the other arm's.
    """
    with open(path, errors="ignore") as fh:
        for line in fh:
            try:
                rec = json.loads(line)
            except ValueError:
                continue
            ts = rec.get("timestamp", "")
            if until and ts >= until:
                return
            if since and ts < since:
                continue
            yield rec


def corpus(dirs, sessions: int) -> list[str]:
    base = os.path.expanduser("~/.claude/projects")
    files = [f for d in dirs for f in glob.glob(f"{base}/{d}/*.jsonl")]
    return sorted(files, key=os.path.getmtime, reverse=True)[:sessions]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--sessions", type=int, default=200)
    ap.add_argument("--dirs", default=",".join(DEFAULT_DIRS))
    ap.add_argument("--min-calls", type=int, default=40)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--until", default="", help="ISO UTC cut, e.g. 2026-10-27T13:29:00Z (pre-deploy baseline)")
    ap.add_argument("--since", default="", help="ISO UTC start of the window (inclusive), e.g. an A3 arm block")
    ap.add_argument("--extras", action="store_true", help="D1a: deny→next per gate, sizes, superseded reads")
    args = ap.parse_args(argv)
    if args.since and args.until and args.since >= args.until:
        print(f"janela vazia: --since {args.since} não é anterior a --until {args.until}", file=sys.stderr)
        return 2
    files = corpus(args.dirs.split(","), args.sessions)
    if not files:
        print("sem sessões no corpus", file=sys.stderr)
        return 1
    stats: dict[str, Stats] = {}
    ex = Extras()
    for f in files:
        measure_session(iter_records(f, args.until, args.since), stats)
        if args.extras:
            measure_extras(iter_records(f, args.until, args.since), ex)
    rows = {k: v.row() for k, v in stats.items() if v.calls >= args.min_calls}
    rows = dict(sorted(rows.items(), key=lambda kv: -kv[1]["calls"]))
    report = {
        "sessions": len(files),
        "window": WINDOW,
        "since": args.since or None,
        "until": args.until or None,
        "rows": rows,
    }
    if args.extras:
        report["extras"] = extras_report(ex, rows)
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=1))
        return 0
    print(f"sessões={len(files)} janela={WINDOW} min_calls={args.min_calls}")
    print("| classe | calls | erro% | vazio% | negado% | retry% | eficácia% | bytes resultado |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|")
    for k, r in rows.items():
        print(f"| {k} | {r['calls']} | {r['erro%']} | {r['vazio%']} | {r['negado%']} | {r['retry%']} | {r['eficacia%']} | {r['result_bytes']} |")
    return 0


if __name__ == "__main__":
    sys.exit(main())
