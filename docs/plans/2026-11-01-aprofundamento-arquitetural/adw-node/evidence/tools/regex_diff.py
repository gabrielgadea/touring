"""Differential test of the node-reference parsers in adw.py.

For each input `{{nodes.<body>}}` it asks every parser "which node does this reference?"
and compares to the RUNTIME (render_template) as ground truth - the runner is what really
executes, the lints only reason about it.

Usage: regex_diff.py <path/to/adw.py> [--json out.json]
"""
import importlib.util, json, re, sys, itertools

path = sys.argv[1]
out_json = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None
spec = importlib.util.spec_from_file_location("adw_under_test", path)
adw = importlib.util.module_from_spec(spec)
sys.modules["adw_under_test"] = adw
spec.loader.exec_module(adw)


class Facts(dict):
    def __init__(self, node):
        self.node = node

    def get(self, k, d=None):
        return f"<{self.node}:facts:{k}>"


class Probe(dict):
    """results[node] -> a dict whose every field names (node, field)."""

    def get(self, node, default=None):
        class Entry(dict):
            def get(s, field, d=None):
                if field == "facts":
                    return Facts(node)
                return f"<{node}:{field}>"
        return Entry()


MARK = re.compile(r"<(.+?):(summary|full_ref|omitted_bytes|facts)(?::(.*?))?>")


def runtime_node(text):
    got = adw.render_template(text, Probe(), {})
    m = MARK.search(got)
    return (m.group(1), m.group(2)) if m else None


def lint_data_node(text):  # NODE_REF_RE / node_refs (whichever the file has)
    if hasattr(adw, "node_refs"):
        refs = adw.node_refs(text)
        return refs[0].node if refs else None
    found = adw.NODE_REF_RE.findall(text)
    return found[0] if found else None


def agent_text_node(text):
    if hasattr(adw, "_AGENT_TEXT_REF_RE"):
        f = adw._AGENT_TEXT_REF_RE.findall(text)
        return f[0] if f else None
    # new design: agent text = summary|full_ref of a ref
    if hasattr(adw, "node_refs"):
        refs = [r for r in adw.node_refs(text) if r.field in ("summary", "full_ref")]
        return refs[0].node if refs else None
    return None


def namespaced(text, local):
    out = adw._namespace_node_refs(text, "NS", {local})
    return out


def substring_reads(text, name):  # _lint_persona / _lint_gate_feedback: f"nodes.{name}" in prompt
    return f"nodes.{name}" in text


BODIES = []
for n in ("a", "rev", "review", "a.b", "x-y", "A1"):
    for suffix in ("", ".summary", ".full_ref", ".omitted_bytes", ".facts.K", ".facts.summary"):
        BODIES.append(n + suffix)
BODIES += ["a.summary.summary", "a.full_ref.summary", "summary", "a.facts.", "a.bogus"]

rows = []
disagree = []
for body in BODIES:
    text = "{{nodes." + body + "}}"
    rt = runtime_node(text)
    rt_node = rt[0] if rt else None
    row = {
        "input": text,
        "runtime": rt,
        "data_graph": lint_data_node(text),
        "agent_text": agent_text_node(text),
    }
    # the namespacer: which local name does it rewrite?  (local = runtime's node head)
    if rt_node:
        head = rt_node.split(".")[0]
        row["namespaced"] = namespaced(text, head) != text
    rows.append(row)
    problems = []
    if row["data_graph"] != rt_node:
        problems.append(f"data_graph sees {row['data_graph']!r}")
    # agent_text only claims summary/full_ref; bare is summary at runtime
    expect_agent = rt_node if (rt and rt[1] in ("summary", "full_ref")) else None
    if row["agent_text"] != expect_agent:
        problems.append(f"agent_text sees {row['agent_text']!r}, runtime says {expect_agent!r}")
    if problems:
        disagree.append((text, rt, problems))

# substring parsers: prefix confusion
prefix = []
for hidden, real in (("rev", "review"), ("a", "ab"), ("check", "check2")):
    prompt = "{{nodes." + real + ".summary}}"
    claimed = substring_reads(prompt, hidden)
    truth = (runtime_node(prompt) or (None,))[0] == hidden
    if claimed != truth:
        prefix.append((hidden, real, claimed, truth))
# substring without braces: not a reference at runtime
braceless = substring_reads("see nodes.gate in the notes", "gate")

print(f"file: {path}")
print(f"inputs: {len(BODIES)}   disagreements (non-runtime view): {len(disagree)}")
for text, rt, problems in disagree:
    print(f"  {text:38s} runtime={rt}  ->  {'; '.join(problems)}")
print(f"substring tests (nodes.{{name}} in prompt) wrong on prefix names: {len(prefix)}")
for hidden, real, claimed, truth in prefix:
    print(f"  hidden={hidden!r} prompt reads {real!r}: substring says {claimed}, runtime says {truth}")
print(f"substring accepts a brace-less mention as a read: {braceless}")
if out_json:
    json.dump({"rows": rows, "disagree": [[t, r, p] for t, r, p in disagree], "prefix": prefix,
               "braceless": braceless}, open(out_json, "w"), indent=1)
