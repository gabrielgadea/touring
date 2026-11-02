"""Per raw key: the set of defaults used across `.get(key, default)` sites, plus how many
sites read it as required (`raw[key]`) vs optional. Disagreement = the table is not single-source.
Usage: defaults.py <adw.py>"""
import ast, sys, collections
src = open(sys.argv[1], encoding="utf-8").read()
tree = ast.parse(src)
lines = src.splitlines()

parents = {}
for n in ast.walk(tree):
    for c in ast.iter_child_nodes(n):
        parents[c] = n

def is_raw(e):
    return isinstance(e, ast.Attribute) and e.attr == "raw" or (isinstance(e, ast.Name) and e.id == "raw")

sites = collections.defaultdict(list)
for n in ast.walk(tree):
    if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and n.func.attr == "get" and is_raw(n.func.value):
        if n.args and isinstance(n.args[0], ast.Constant) and isinstance(n.args[0].value, str):
            k = n.args[0].value
            d = ast.unparse(n.args[1]) if len(n.args) > 1 else "<none>"
            sites[k].append(("get", n.lineno, d))
    if isinstance(n, ast.Subscript) and is_raw(n.value) and isinstance(n.slice, ast.Constant):
        sites[n.slice.value].append(("sub", n.lineno, "<required>"))

multi = {}
for k, v in sorted(sites.items()):
    ds = collections.Counter(d for _, _, d in v)
    kinds = {t for t, _, _ in v}
    if len(ds) > 1:
        multi[k] = (ds, sorted(v, key=lambda x: x[1]))
print("keys with >1 distinct default/access style:", len(multi), "of", len(sites))
for k, (ds, v) in multi.items():
    print(f"  {k}: {dict(ds)}")
    for t, ln, d in v:
        print(f"      {ln:5d} {t:3s} default={d:20s} | {lines[ln-1].strip()[:90]}")
