"""Keys per node type used by every library spec/fragment + per-key counts.
Usage: spec_keys.py <library_dir> <out.json>   (read-only on the library)"""
import sys, json, tomllib, collections, pathlib

lib = pathlib.Path(sys.argv[1])
by_type = collections.defaultdict(collections.Counter)  # type -> key -> count
files = sorted(list(lib.glob("*.toml")) + list((lib / "fragments").glob("*.toml")))
nodes_total = 0
per_file = {}
for f in files:
    if f.name in ("tiers.toml", "adw-curation.toml") and False:
        continue
    try:
        data = tomllib.loads(f.read_text(encoding="utf-8"))
    except Exception as e:  # noqa
        per_file[f.name] = f"PARSE-ERROR {e}"
        continue
    nodes = data.get("node") or {}
    if not isinstance(nodes, dict):
        continue
    per_file[str(f.relative_to(lib))] = len(nodes)
    for name, body in nodes.items():
        if not isinstance(body, dict):
            continue
        nodes_total += 1
        t = body.get("type", "?")
        for k in body:
            by_type[t][k] += 1
all_keys = collections.Counter()
for t, c in by_type.items():
    all_keys.update(c)
out = {
    "files": per_file,
    "nodes_total": nodes_total,
    "by_type": {t: dict(c) for t, c in by_type.items()},
    "all_keys": dict(all_keys),
}
json.dump(out, open(sys.argv[2], "w"), indent=1, sort_keys=True)
print("spec files:", len([k for k, v in per_file.items() if isinstance(v, int)]), "nodes:", nodes_total)
print("distinct keys in specs:", len(all_keys))
for t, c in sorted(by_type.items()):
    print(f"  {t}: {sum(c.values())} key-uses, {len(c)} distinct: {sorted(c)}")
