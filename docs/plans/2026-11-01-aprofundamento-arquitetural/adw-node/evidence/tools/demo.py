"""Crafted specs, one defect each: what do the OLD and the NEW lint say?

    demo.py <old adw.py> <new adw.py> <library> <out.json>
"""
import json, os, pathlib, subprocess, sys, tempfile

OLD, NEW, LIB, OUT = sys.argv[1:5]
HEAD = '[adw]\nname = "{n}"\nentry = "{e}"\n'

SPECS = {
    # D1 - the shipped fragment's `env`, composed the way `adw new --use skilling-pack` does
    "d1_env_in_skilling_pack": HEAD.format(n="d1", e="skill") + '''
[[use]]
module = "skilling-pack"
as = "skill"
with = { procedure = "p", recurrence = "5" }
''',
    # D2 - prefix: blind_to `rev` vs a prompt that reads `review`
    "d2_blind_to_prefix": HEAD.format(n="d2", e="review") + '''
[node.review]
type = "code"
command = ["echo", "x"]
idempotent = true
on_pass = "crit"
[node.rev]
type = "code"
command = ["echo", "y"]
idempotent = true
on_pass = "__end__"
[node.crit]
type = "agent"
driver = "mock"
prompt = "judge {{nodes.review.summary}}"
allowed_tools = ["Read"]
[node.crit.persona]
role = "critic"
stance = "reject_by_default"
emits = "VERDICT=PASS|REJECT"
bar = "b"
blind_to = ["rev"]
''',
    # D3 - gate feedback: gate `check`, agent reads `check2`
    "d3_gate_feedback_prefix": HEAD.format(n="d3", e="work") + '''
[node.work]
type = "agent"
driver = "mock"
prompt = "fix it; last verdict: {{nodes.check2.summary}}"
allowed_tools = ["Read", "Edit"]
on_pass = "check"
[node.check]
type = "gate"
command = ["true"]
idempotent = true
on_pass = "__end__"
on_fail = "work"
[node.check2]
type = "gate"
command = ["true"]
idempotent = true
on_pass = "__end__"
''',
    # D4 - a read-only node consumed ONLY through full_ref
    "d4_consumed_via_full_ref": HEAD.format(n="d4", e="scan") + '''
[node.scan]
type = "agent"
driver = "mock"
prompt = "scan"
allowed_tools = ["Read", "Grep"]
on_pass = "use"
[node.use]
type = "agent"
driver = "mock"
prompt = "act on the whole report at {{nodes.scan.full_ref}}"
allowed_tools = ["Read", "Grep"]
''',
    # D5 - sandboxed gate interpolating agent text through the bare form
    "d5_sandbox_bare_ref": HEAD.format(n="d5", e="writer") + '''
[node.writer]
type = "agent"
driver = "mock"
prompt = "write"
allowed_tools = ["Read"]
on_pass = "check"
[node.check]
type = "gate"
command = ["bash", "-c", "echo {{nodes.writer}}"]
sandbox = true
idempotent = true
''',
    # D6 - typo'd key and a stringly-typed boolean
    "d6_typo_and_string_bool": HEAD.format(n="d6", e="run") + '''
[node.run]
type = "code"
command = ["true"]
idempotent = "false"
timeuot_ms = 1000
''',
}

result = {}
for name, text in SPECS.items():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        (root / ".touring" / "adw").mkdir(parents=True)
        spec_name = name.split("_")[0]
        (root / ".touring" / "adw" / f"{spec_name}.toml").write_text(text, encoding="utf-8")
        row = {}
        for label, path in (("old", OLD), ("new", NEW)):
            env = {**os.environ, "TOURING_ADW_LIBRARY": LIB}
            p = subprocess.run([sys.executable, "-I", path, "--root", str(root), "lint", spec_name],
                               capture_output=True, text=True, env=env)
            try:
                row[label] = {"rc": p.returncode, **json.loads(p.stdout)}
            except json.JSONDecodeError:
                row[label] = {"rc": p.returncode, "raw": p.stdout[:300], "err": p.stderr[-300:]}
        result[name] = row

# D1 b - what the runner actually does with `env`: run the fragment's own node
sys.path.insert(0, str(pathlib.Path(OLD).parent))
import importlib.util
spec = importlib.util.spec_from_file_location("adw_old_for_demo", OLD)
adw = importlib.util.module_from_spec(spec); sys.modules["adw_old_for_demo"] = adw; spec.loader.exec_module(adw)
node = adw.Node(name="should_skill", type="code", raw={
    "type": "code",
    "command": ["bash", "-c", 'test "${RECURRENCE:-0}" -ge 2'],
    "env": {"RECURRENCE": "5"},
})
os.environ.pop("RECURRENCE", None)
ran = adw.run_code_node(node, {}, {})
result["d1_runtime_evidence"] = {"recurrence_declared_in_env": 5, "exit_code": ran.exit_code,
                                 "meaning": "exit 0 would mean 'recurred, capture a skill'; 1 = 'one-off'"}
json.dump(result, open(OUT, "w"), indent=1, ensure_ascii=False)
for name, row in result.items():
    print(f"== {name}")
    if "old" not in row:
        print("  ", row)
        continue
    for label in ("old", "new"):
        r = row[label]
        print(f"  {label}: rc={r['rc']} errors={r.get('errors')} warnings={r.get('warnings')}")
