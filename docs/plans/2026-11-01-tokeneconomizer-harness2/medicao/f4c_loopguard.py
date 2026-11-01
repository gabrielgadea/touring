"""F4c — counterfactual: TokenEconomizer's heuristic stuck-detector (loopguard/rules.py
DEFAULT_THRESHOLDS) replayed over each user turn of the main transcripts.

A turn = tool calls between two genuine user prompts. The detector fires at the first tool
call where ALL hold: calls >= min_turns(20); (max edits to one file >= 4 OR distinct edited
files >= 3); error outputs >= 6; stagnation (share of failures repeating an earlier failure
text, normalized) >= 0.20. Reported: turns, turns flagged, tool calls AFTER the flag (the
upper bound a redirect could save), and how many flagged turns already got a Touring deny
(our per-call gates) before the flag. No outcome label exists offline: this bounds
opportunity, it does not prove waste. Aggregates only.
"""
import glob
import json
import os
import re
import sys
from collections import Counter

ROOT = os.path.expanduser("~/.claude/projects/" + os.getcwd().replace("/", "-"))  # run from the project root
TH = {"min_turns": 20, "edit_churn": 4, "distinct_edit_files": 3, "error_outputs": 6, "stagnation": 0.20}
EDIT_TOOLS = {"Edit", "Write", "MultiEdit", "NotebookEdit"}
DENY_MARK = re.compile(r"PreToolUse:\w+ hook error|permissionDecision\W+deny")
NORM = [(re.compile(r"/[\w.\-/]+"), "P"), (re.compile(r"\d+"), "N"), (re.compile(r"\s+"), " ")]


def norm(t):
    for rx, rep in NORM:
        t = rx.sub(rep, t)
    return t[:160]


def text_of(c):
    if isinstance(c, str):
        return c
    if isinstance(c, list):
        return " ".join(x.get("text", "") for x in c if isinstance(x, dict))
    return ""


class Turn:
    def __init__(self):
        self.calls, self.edits, self.errors, self.repeats = 0, Counter(), 0, 0
        self.seen_err, self.flag_at, self.deny_before_flag = set(), None, False
        self.pending = {}

    def use(self, block):
        self.calls += 1
        if block.get("name") in EDIT_TOOLS:
            fp = (block.get("input") or {}).get("file_path", "?")
            self.edits[fp] += 1
        self.pending[block.get("id")] = True

    def result(self, block):
        if not block.get("is_error"):
            return
        self.errors += 1
        t = text_of(block.get("content"))
        if DENY_MARK.search(t) and self.flag_at is None:
            self.deny_before_flag = True
        k = norm(t)
        if k in self.seen_err:
            self.repeats += 1
        self.seen_err.add(k)

    def check(self):
        if self.flag_at is not None or self.calls < TH["min_turns"]:
            return
        churn = (max(self.edits.values(), default=0) >= TH["edit_churn"]
                 or len(self.edits) >= TH["distinct_edit_files"])
        stag = self.errors and self.repeats / self.errors >= TH["stagnation"]
        if churn and self.errors >= TH["error_outputs"] and stag:
            self.flag_at = self.calls


def turns_of(path):
    turns, cur = [], Turn()
    with open(path, encoding="utf-8", errors="ignore") as fh:
        for raw in fh:
            try:
                r = json.loads(raw)
            except ValueError:
                continue
            content = (r.get("message") or {}).get("content")
            if r.get("type") == "user" and isinstance(content, str) and not r.get("isMeta"):
                turns.append(cur)
                cur = Turn()
                continue
            if not isinstance(content, list):
                continue
            for b in content:
                if not isinstance(b, dict):
                    continue
                if b.get("type") == "tool_use":
                    cur.use(b)
                elif b.get("type") == "tool_result":
                    cur.result(b)
                    cur.check()
    turns.append(cur)
    return [t for t in turns if t.calls]


def main():
    all_turns = [t for f in glob.glob(os.path.join(ROOT, "*.jsonl")) for t in turns_of(f)]
    flagged = [t for t in all_turns if t.flag_at is not None]
    calls_total = sum(t.calls for t in all_turns)
    after = sum(t.calls - t.flag_at for t in flagged)
    big = [t for t in all_turns if t.calls >= TH["min_turns"]]
    print(json.dumps({
        "turns_with_tool_calls": len(all_turns),
        "turns_ge_20_calls": len(big),
        "turns_flagged": len(flagged),
        "share_flagged_of_ge20": round(len(flagged) / len(big), 3) if big else None,
        "tool_calls_total": calls_total,
        "tool_calls_after_flag": after,
        "share_calls_after_flag": round(after / calls_total, 4) if calls_total else None,
        "flagged_with_touring_deny_before_flag": sum(t.deny_before_flag for t in flagged),
        "median_calls_flagged_turn": sorted(t.calls for t in flagged)[len(flagged) // 2] if flagged else None,
    }))


if __name__ == "__main__":
    sys.exit(main())
