"""Scan adw.py: every read of a node's raw dict (direct `.raw[...]`/`.raw.get(..)` and
through local aliases `raw = x.raw`), with key, line and enclosing function."""
import ast, sys, json, collections
src = open(sys.argv[1], encoding="utf-8").read()
tree = ast.parse(src)

def enclosing(tree):
    parents = {}
    for n in ast.walk(tree):
        for c in ast.iter_child_nodes(n):
            parents[c] = n
    return parents
parents = enclosing(tree)
def func_of(n):
    while n in parents:
        n = parents[n]
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return n.name
    return "<module>"

def is_raw_expr(e, aliases):
    # x.raw  or alias Name
    if isinstance(e, ast.Attribute) and e.attr == "raw":
        return True
    if isinstance(e, ast.Name) and e.id in aliases:
        return True
    return False

reads = []   # (line, func, key, kind)
alias_by_func = collections.defaultdict(set)
# pass 1 aliases: NAME = <expr>.raw   or  NAME = <expr>.raw or {}  or  NAME = dict(<expr>.raw)
for n in ast.walk(tree):
    if isinstance(n, ast.Assign) and len(n.targets) == 1 and isinstance(n.targets[0], ast.Name):
        v = n.value
        cands = [v]
        if isinstance(v, ast.BoolOp): cands = v.values
        for c in cands:
            if isinstance(c, ast.Attribute) and c.attr == "raw":
                alias_by_func[func_of(n)].add(n.targets[0].id)
# function params named raw/body/node_raw treated as aliases too (recorded separately)
param_alias = collections.defaultdict(set)
for f in ast.walk(tree):
    if isinstance(f, (ast.FunctionDef, ast.AsyncFunctionDef)):
        for a in f.args.args + f.args.kwonlyargs:
            if a.arg in ("raw", "body", "node_raw", "braw"):
                param_alias[f.name].add(a.arg)

def key_of(sl):
    if isinstance(sl, ast.Constant) and isinstance(sl.value, str):
        return sl.value
    return None

for n in ast.walk(tree):
    fn = func_of(n)
    al = alias_by_func.get(fn, set()) | param_alias.get(fn, set())
    # subscripts
    if isinstance(n, ast.Subscript) and is_raw_expr(n.value, al):
        reads.append((n.lineno, fn, key_of(n.slice) or "<dyn>", "sub", type(n.ctx).__name__))
    # .get / .pop / .setdefault / .items / .keys
    if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and is_raw_expr(n.func.value, al):
        a = n.func.attr
        k = key_of(n.args[0]) if n.args else None
        reads.append((n.lineno, fn, k or f"<{a}>", a, "call"))
    # `"key" in raw`
    if isinstance(n, ast.Compare) and any(isinstance(o, (ast.In, ast.NotIn)) for o in n.ops):
        for comp in n.comparators:
            if is_raw_expr(comp, al):
                reads.append((n.lineno, fn, key_of(n.left) or "<dyn>", "in", "cmp"))
out = {"reads": reads, "aliases": {k: sorted(v) for k, v in alias_by_func.items()},
       "param_alias": {k: sorted(v) for k, v in param_alias.items()}}
json.dump(out, open(sys.argv[2], "w"), indent=0)
keys = collections.Counter(r[2] for r in reads)
print("total raw reads:", len(reads))
print("distinct keys (const):", len([k for k in keys if not k.startswith("<")]))
print("dynamic/other:", {k: v for k, v in keys.items() if k.startswith("<")})
print("functions touching raw:", len({r[1] for r in reads}))
print("alias funcs:", out["aliases"])
