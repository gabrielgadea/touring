"""F4d — counterfactual: TokenEconomizer tool pruning applied to our skill/agent/deferred-tool
surface, replayed chronologically over the main transcripts.

Rule (tokeneconomizer/plugin/fold.py:enough_evidence, config.py p=10 m=2 s=5): an item that
went unused is left out once its unused streak spans >= m sessions AND >= p prompts, or
>= s sessions. Applied per item over sessions in time order; a later use of an item that was
already pruned counts as a FALSE prune (the session would have needed it).
Announced surface comes from attachments: skill_listing.names, agent_listing_delta.addedTypes,
deferred_tools_delta.addedNames. Use comes from tool_use blocks (Skill.input.skill,
Agent.input.subagent_type, tool name). Aggregates only.
"""
import glob
import json
import os
import sys
from collections import defaultdict

ROOT = os.path.expanduser("~/.claude/projects/" + os.getcwd().replace("/", "-"))  # run from the project root
P, M, S = 10, 2, 5


def session_facts(path):
    ann = {"skill": set(), "agent": set(), "tool": set()}
    used = {"skill": set(), "agent": set(), "tool": set()}
    listing_bytes = {"skill": 0, "agent": 0, "tool": 0}
    prompts, first_ts = 0, None
    with open(path, encoding="utf-8", errors="ignore") as fh:
        for raw in fh:
            try:
                r = json.loads(raw)
            except ValueError:
                continue
            first_ts = first_ts or r.get("timestamp")
            t = r.get("type")
            if t == "attachment":
                a = r.get("attachment") or {}
                k = a.get("type")
                if k == "skill_listing":
                    ann["skill"] |= set(a.get("names") or [])
                    listing_bytes["skill"] += len(a.get("content") or "")
                elif k == "agent_listing_delta":
                    ann["agent"] |= set(a.get("addedTypes") or [])
                    listing_bytes["agent"] += sum(len(x) for x in a.get("addedLines") or [])
                elif k == "deferred_tools_delta":
                    ann["tool"] |= set(a.get("addedNames") or [])
                    listing_bytes["tool"] += sum(len(x) for x in a.get("addedLines") or [])
                continue
            msg = r.get("message") or {}
            content = msg.get("content")
            if t == "user" and isinstance(content, str) and not r.get("isMeta"):
                prompts += 1
            if t != "assistant" or not isinstance(content, list):
                continue
            for b in content:
                if not isinstance(b, dict) or b.get("type") != "tool_use":
                    continue
                name, inp = b.get("name", ""), b.get("input") or {}
                used["tool"].add(name)
                if name == "Skill" and inp.get("skill"):
                    used["skill"].add(inp["skill"])
                if name == "Agent":
                    used["agent"].add(inp.get("subagent_type") or "general-purpose")
    return first_ts or "", max(prompts, 1), ann, used, listing_bytes


def simulate(sessions, kind):
    streak_s, streak_p, pruned = defaultdict(int), defaultdict(int), set()
    false_prunes, ever_pruned = 0, set()
    for _, prompts, ann, used, _ in sessions:
        for item in ann[kind] | pruned:
            if item in used[kind]:
                if item in pruned:
                    false_prunes += 1
                    pruned.discard(item)
                streak_s[item], streak_p[item] = 0, 0
                continue
            if item not in ann[kind]:
                continue
            streak_s[item] += 1
            streak_p[item] += prompts
            if (streak_s[item] >= M and streak_p[item] >= P) or streak_s[item] >= S:
                pruned.add(item)
                ever_pruned.add(item)
    announced = set().union(*(s[2][kind] for s in sessions))
    used_any = set().union(*(s[3][kind] for s in sessions)) & announced
    return {
        "announced": len(announced),
        "used_at_least_once": len(used_any),
        "pruned_at_end": len(pruned),
        "share_pruned_at_end": round(len(pruned) / len(announced), 3) if announced else None,
        "false_prune_events": false_prunes,
        "items_ever_pruned": len(ever_pruned),
    }


def main():
    files = glob.glob(os.path.join(ROOT, "*.jsonl"))
    sessions = sorted((session_facts(f) for f in files), key=lambda s: s[0])
    out = {"sessions": len(sessions)}
    for kind in ("skill", "agent", "tool"):
        res = simulate(sessions, kind)
        lb = sum(s[4][kind] for s in sessions)
        res["listing_bytes_total"] = lb
        if res["share_pruned_at_end"] is not None:
            res["listing_bytes_saved_est"] = int(lb * res["share_pruned_at_end"])
        out[kind] = res
    print(json.dumps(out))


if __name__ == "__main__":
    sys.exit(main())
