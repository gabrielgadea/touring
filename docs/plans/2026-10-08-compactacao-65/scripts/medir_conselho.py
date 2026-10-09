"""Baseline for compaction work: (A) how CC 2.1.294 applies CLAUDE_AUTOCOMPACT_PCT_OVERRIDE, read from the
bundle itself; (B) hook advice that reaches the model, chars per tool call, grouped by the hook that emitted it.

Usage: python3 medir_conselho.py [n_transcripts] [--session <uuid>] [--no-bundle]
"""

import json
import os
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

PROJECT = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"
ADVICE = ("hook_additional_context", "hook_success", "async_hook_response")


def bundle_logic():
    exe = Path(os.path.realpath(os.popen("command -v claude").read().strip()))
    data = exe.read_bytes()
    print(f"### A. bundle {exe} ({len(data) / 1e6:.0f} MB)")
    seen = []
    for m in re.finditer(rb"AUTOCOMPACT_PCT_OVERRIDE", data):
        chunk = data[max(0, m.start() - 700):m.end() + 500].decode("utf-8", "replace")
        if any(chunk[600:900] == s for s in seen):
            continue
        seen.append(chunk[600:900])
        print("----\n" + re.sub(r"\s+", " ", chunk))
        if len(seen) >= 2:
            break


def hook_name(att):
    """Short hook identity: the script basename, or the touring-hook subcommand."""
    raw = next((att[k] for k in ("hookName", "command", "hookCommand") if isinstance(att.get(k), str)), "?")
    parts = [p for p in raw.split() if not p.startswith("python")]
    if not parts:
        return "?"
    base = re.sub(r"^.*/", "", parts[0])
    return f"{base} {parts[1]}" if base == "touring-hook" and len(parts) > 1 else base[:48]


def count_tool_uses(rec):
    return sum(1 for c in (rec.get("message") or {}).get("content") or []
               if isinstance(c, dict) and c.get("type") == "tool_use")


def scan_line(line, acc):
    if '"tool_use"' in line and '"assistant"' in line:
        rec = json.loads(line)
        acc["calls"] += count_tool_uses(rec)
        return
    if '"attachment"' not in line:
        return
    att = json.loads(line).get("attachment") or {}
    kind = att.get("type")
    if kind not in ADVICE:
        return
    acc["keys"][kind].update(att.keys())
    body = att.get("content")
    n = len(body if isinstance(body, str) else json.dumps(body, ensure_ascii=False))
    key = (kind, att.get("hookEvent") or "?", hook_name(att))
    acc["chars"][key] += n
    acc["blocks"][key] += 1


def advice_by_source(files):
    acc = {"chars": Counter(), "blocks": Counter(), "calls": 0, "keys": defaultdict(set)}
    for f in files:
        for line in f.open(encoding="utf-8", errors="replace"):
            try:
                scan_line(line, acc)
            except json.JSONDecodeError:
                continue
    return acc


def report(acc, nfiles):
    calls = max(acc["calls"], 1)
    print(f"\n### B. {nfiles} transcripts · {acc['calls']} tool calls")
    for kind, ks in acc["keys"].items():
        print(f"  keys[{kind}] = {sorted(ks)}")
    for kind in ADVICE:
        rows = sorted(((v, k) for k, v in acc["chars"].items() if k[0] == kind), reverse=True)
        tot = sum(v for v, _ in rows)
        print(f"\n  {kind}: {tot:,} chars · {tot / calls:,.0f} chars/call")
        for v, k in rows[:14]:
            print(f"    {v:>10,}  {acc['blocks'][k]:>6} blocos  {v / calls:>7,.0f}/call  {k[1]:<18} {k[2]}")


def main():
    args = sys.argv[1:]
    if "--no-bundle" not in args:
        bundle_logic()
    if "--session" in args:
        files = [PROJECT / f"{args[args.index('--session') + 1]}.jsonl"]
    else:
        n = int(args[0]) if args and args[0].isdigit() else 10
        files = sorted(PROJECT.glob("*.jsonl"), key=lambda f: f.stat().st_mtime, reverse=True)[:n]
    report(advice_by_source(files), len(files))


if __name__ == "__main__":
    main()
