"""Run the NEW schema over every [node.*] table of every spec AND fragment of every corpus.

    corpus_schema.py <new adw.py> <library> <roots>
"""
import importlib.util, pathlib, sys, tomllib, collections

path, lib, roots = sys.argv[1:4]
spec = importlib.util.spec_from_file_location("adw_c", path)
adw = importlib.util.module_from_spec(spec)
sys.modules["adw_c"] = adw
spec.loader.exec_module(adw)

bases = [("library", pathlib.Path(lib))] + [(p.parent.parent.name, p) for p in sorted(pathlib.Path(roots).glob("*/.touring/adw"))]
total_nodes = 0
files = 0
findings = collections.defaultdict(list)
seen = set()
for corpus, base in bases:
    for f in sorted(list(base.glob("*.toml")) + list((base / "fragments").glob("*.toml"))):
        try:
            data = tomllib.loads(f.read_text(encoding="utf-8"))
        except Exception:
            continue
        nodes = data.get("node")
        if not isinstance(nodes, dict):
            continue
        files += 1
        for name, body in nodes.items():
            if not isinstance(body, dict):
                continue
            total_nodes += 1
            errors, warnings = adw.schema_findings(adw.Node(name=name, type=body.get("type", ""), raw=body))
            for m in errors + warnings:
                findings[(corpus, f.relative_to(base).as_posix(), m)].append(name)
print(f"files with [node.*]: {files}   nodes checked: {total_nodes}")
print(f"schema findings: {len(findings)} distinct")
for (corpus, rel, msg), names in sorted(findings.items()):
    print(f"  [{corpus}] {rel}: {msg}")
