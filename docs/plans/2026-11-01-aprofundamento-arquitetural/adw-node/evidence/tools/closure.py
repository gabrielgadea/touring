"""keys_runtime / keys_lint / keys_specs and the drift between them.

    closure.py <adw.py> <raw_reads.json> <library_dir> <roots_dir> <out.json>

keys_runtime : every constant key read from a node's raw table anywhere in adw.py
               (+ the keys read through a loop variable / named family, listed below)
keys_lint    : the same, restricted to functions reachable from lint_spec
keys_exec    : ... reachable from execute / cmd_run / cmd_test / cmd_campaign (what RUNS)
keys_specs   : keys used by [node.*] tables in every shipped/project spec and fragment
"""
import ast, json, sys, tomllib, pathlib, collections

adw_path, reads_path, lib, roots, out = sys.argv[1:6]
src = open(adw_path, encoding="utf-8").read()
tree = ast.parse(src)
reads = json.load(open(reads_path))["reads"]

# keys read through a loop variable (the AST scan reports them as <dyn>/<get>); each family
# is a named tuple/constant in adw.py, resolved here so the count is honest.
FAMILIES = {
    "_lint_node": {"good_input", "bad_input", "on_stable", "on_covered", "on_calibrated", "on_escalate"},
    "run_control_node": {"good_input", "bad_input"},
    "control_successors": {"on_escalate", "on_stable", "on_covered", "on_calibrated"},
    "_lint_parallel": {"on_pass", "on_fail"},
}

fn_keys = collections.defaultdict(set)
for line, fn, key, kind, ctx in reads:
    if key.startswith("<") or key == ":":
        continue
    fn_keys[fn].add(key)
for fn, ks in FAMILIES.items():
    fn_keys[fn] |= ks

defs = {n.name: n for n in ast.walk(tree) if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))}
calls = {}
for name, node in defs.items():
    called = set()
    for c in ast.walk(node):
        if isinstance(c, ast.Call):
            f = c.func
            if isinstance(f, ast.Name) and f.id in defs:
                called.add(f.id)
            elif isinstance(f, ast.Attribute) and f.attr in defs:
                called.add(f.attr)
        # functions passed as values (e.g. executor.submit(fn)) / referenced by name
        if isinstance(c, ast.Name) and c.id in defs and isinstance(c.ctx, ast.Load):
            called.add(c.id)
    calls[name] = called


def closure(roots_):
    seen, stack = set(), list(roots_)
    while stack:
        f = stack.pop()
        if f in seen or f not in defs:
            continue
        seen.add(f)
        stack.extend(calls.get(f, ()))
    return seen


def keys_of(fns):
    ks = set()
    for f in fns:
        ks |= fn_keys.get(f, set())
    return ks


lint_fns = closure(["lint_spec"])
exec_fns = closure(["execute", "cmd_run", "cmd_test", "cmd_campaign", "run_loop"])
keys_runtime = set().union(*fn_keys.values())
# the loader reads these off the TOML body (not through Node.raw)
LOADER = {"type", "on_pass", "on_fail", "on_dry"}
keys_runtime_all = keys_runtime | LOADER
keys_lint = keys_of(lint_fns) | LOADER  # _lint_node validates the edges
keys_exec = keys_of(exec_fns) | LOADER

spec_keys = collections.Counter()
files = []
for base in [pathlib.Path(lib)] + sorted(pathlib.Path(roots).glob("*/.touring/adw")):
    for f in sorted(list(base.glob("*.toml")) + list((base / "fragments").glob("*.toml"))):
        try:
            data = tomllib.loads(f.read_text(encoding="utf-8"))
        except Exception:
            continue
        nodes = data.get("node")
        if not isinstance(nodes, dict):
            continue
        files.append(str(f))
        for body in nodes.values():
            if isinstance(body, dict):
                spec_keys.update(body.keys())
keys_specs = set(spec_keys)

res = {
    "functions_touching_raw": len({fn for _, fn, k, *_ in reads if not k.startswith(":")}),
    "raw_reads": len([r for r in reads if r[2] != ":"]),
    "keys_runtime": sorted(keys_runtime_all),
    "keys_lint": sorted(keys_lint),
    "keys_exec": sorted(keys_exec),
    "keys_specs": sorted(keys_specs),
    "spec_files_scanned": len(set(files)),
    "drift": {
        "specs_not_read_by_runtime": sorted(keys_specs - keys_runtime_all),
        "runtime_not_in_any_spec": sorted(keys_runtime_all - keys_specs),
        "runtime_not_seen_by_lint": sorted(keys_runtime_all - keys_lint),
        "lint_not_in_exec_path": sorted(keys_lint - keys_exec),
        "exec_not_seen_by_lint": sorted(keys_exec - keys_lint),
    },
}
json.dump(res, open(out, "w"), indent=1)
print("raw reads:", res["raw_reads"], "| functions:", res["functions_touching_raw"])
print("keys_runtime:", len(res["keys_runtime"]), "keys_lint:", len(res["keys_lint"]), "keys_exec:", len(res["keys_exec"]),
      "keys_specs:", len(res["keys_specs"]), f"(from {res['spec_files_scanned']} files)")
for k, v in res["drift"].items():
    print(f"  {k}: {len(v)} {v}")
