"""Compare two replay JSONs: identical / different per (spec, command); for lint and
explain show the line-level delta (added / removed lines).

    compare.py old.json new.json [--verbose]
"""
import json, sys, difflib, collections
old = json.load(open(sys.argv[1]))
new = json.load(open(sys.argv[2]))
verbose = "--verbose" in sys.argv
assert old.keys() == new.keys(), "spec sets differ"
total = identical = 0
by_cmd = collections.defaultdict(lambda: [0, 0])
diffs = []
for key in sorted(old):
    for cmd in ("lint", "explain", "explain_cost", "mermaid", "test"):
        total += 1
        by_cmd[cmd][0] += 1
        a, b = old[key][cmd], new[key][cmd]
        if a == b:
            identical += 1
            by_cmd[cmd][1] += 1
            continue
        al, bl = a["stdout"].splitlines(), b["stdout"].splitlines()
        removed = [l for l in difflib.unified_diff(al, bl, lineterm="", n=0) if l.startswith("-") and not l.startswith("---")]
        added = [l for l in difflib.unified_diff(al, bl, lineterm="", n=0) if l.startswith("+") and not l.startswith("+++")]
        diffs.append((key, cmd, a["rc"], b["rc"], removed, added))
print(f"pairs: {total}  byte-identical: {identical}  different: {total - identical}")
for cmd, (n, ok) in by_cmd.items():
    print(f"  {cmd:13s} {ok}/{n} identical")
print()
for key, cmd, rca, rcb, removed, added in diffs:
    print(f"## {key} :: {cmd}  rc {rca}->{rcb}  (-{len(removed)} +{len(added)})")
    if verbose or cmd == "lint":
        for l in removed:
            print("   ", l[:230])
        for l in added:
            print("   ", l[:230])
