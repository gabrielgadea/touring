"""F4a — counterfactual: how many transcript-mined lessons would survive a
TokenEconomizer-style confirmation rule (same fact seen in >= k distinct sessions)?

Re-implements touring-server/src/ingest/transcript_miner.rs::extract_error_resolution_pairs
(per tool_name, contiguous failed chain closed by a success within RESOLUTION_SCAN_WINDOW=3;
error_text truncated to 500 chars; dedup key = (tool_name, first 200 chars of error_text)).
Prints aggregates only — never transcript text (public repo rule).
"""
import glob
import json
import os
import re
import sys
from collections import defaultdict

ROOT = os.path.expanduser("~/.claude/projects/" + os.getcwd().replace("/", "-"))  # run from the project root
WINDOW, ERR_MAX, PREFIX = 3, 500, 200
NORMALIZE = os.environ.get("F4A_NORMALIZE") == "1" or "--normalize" in sys.argv
_NORM = [(re.compile(r"/[\w.\-/]+"), "<path>"), (re.compile(r"0x[0-9a-f]+|\b[0-9a-f]{8,}\b"), "<hex>"),
         (re.compile(r"\d+"), "<n>"), (re.compile(r"\s+"), " ")]


def norm(t):
    if not NORMALIZE:
        return t
    for rx, rep in _NORM:
        t = rx.sub(rep, t)
    return t.strip()


def flatten(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join(flatten(c.get("text", "")) if isinstance(c, dict) else str(c) for c in content)
    return str(content)


def _blocks(raw):
    """Content blocks of one transcript line, only from user/assistant records."""
    try:
        rec = json.loads(raw)
    except ValueError:
        return []
    if rec.get("type") not in ("user", "assistant"):
        return []
    content = (rec.get("message") or {}).get("content")
    return [b for b in content if isinstance(b, dict)] if isinstance(content, list) else []


def read_tool_events(path):
    """(tool_name -> ordered unique tool_use ids, tool_use_id -> (is_error, text)).

    Errata 01/11/2026 (found by the Rust port, commit 9e22fd31): the first version read
    tool_use blocks from every record type and kept duplicated ids, +35 pairs on 215
    transcripts. Only user/assistant records count, and an id is taken once."""
    results, order, seen = {}, defaultdict(list), set()
    with open(path, encoding="utf-8", errors="ignore") as fh:
        for raw in fh:
            for b in _blocks(raw):
                if b.get("type") == "tool_use" and b.get("id") not in seen:
                    seen.add(b.get("id"))
                    order[b.get("name", "?")].append(b.get("id"))
                elif b.get("type") == "tool_result":
                    results[b.get("tool_use_id")] = (bool(b.get("is_error")),
                                                     flatten(b.get("content", ""))[:ERR_MAX])
    return order, results


def pairs_of(path):
    order, results = read_tool_events(path)
    out = []
    for tool, ids in order.items():
        start, n = None, 0
        for i in ids:
            if i not in results:
                start, n = None, 0
                continue
            err, text = results[i]
            if err:
                if start is None:
                    start = text
                n += 1
            else:
                if start is not None and n <= WINDOW:
                    out.append((tool, norm(start)[:PREFIX]))
                start, n = None, 0
    return out


def main():
    files = sorted(glob.glob(os.path.join(ROOT, "*.jsonl")))
    sessions_by_key = defaultdict(set)
    per_session = []
    for f in files:
        sid = os.path.basename(f)[:-6]
        ps = pairs_of(f)
        per_session.append(len(ps))
        for k in ps:
            sessions_by_key[k].add(sid)
    keys = len(sessions_by_key)
    dist = defaultdict(int)
    for s in sessions_by_key.values():
        dist[min(len(s), 5)] += 1
    by_tool = defaultdict(lambda: [0, 0])
    for (tool, _), s in sessions_by_key.items():
        cls = "mcp" if tool.startswith("mcp__") else tool
        by_tool[cls][0] += 1
        by_tool[cls][1] += len(s) >= 3
    print(json.dumps({
        "sessions": len(files),
        "pairs_total": sum(per_session),
        "distinct_keys": keys,
        "keys_by_distinct_sessions(5=5+)": dict(sorted(dist.items())),
        "keys_confirmed_k2": sum(v for k, v in dist.items() if k >= 2),
        "keys_confirmed_k3": sum(v for k, v in dist.items() if k >= 3),
        "share_confirmed_k3": round(sum(v for k, v in dist.items() if k >= 3) / keys, 4) if keys else None,
        "by_tool_[keys,confirmed_k3]": {k: v for k, v in sorted(by_tool.items(), key=lambda x: -x[1][0])[:8]},
    }, ensure_ascii=False))


if __name__ == "__main__":
    sys.exit(main())
