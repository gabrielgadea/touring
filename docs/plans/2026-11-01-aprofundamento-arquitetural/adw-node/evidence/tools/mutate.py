"""Mutation run over the NEW adw.py: every mutant must be killed by the shipped tests.

    mutate.py <new scripts dir> <work dir> <out.json>

A mutant = one exact text replacement (asserted to match exactly once). It is written to a
private copy of the scripts dir together with the three test files, and the suite runs
there. KILLED = pytest exits non-zero. The first failing test id is recorded.
"""
import json, pathlib, shutil, subprocess, sys

SRC, WORK, OUT = map(pathlib.Path, sys.argv[1:4])
FILES = ["adw.py", "test_adw.py", "test_adw_gate_rejection.py", "test_adw_node.py"]

M = []


def mut(mid, desc, old, new):
    M.append((mid, desc, old, new))


mut("M01", "schema row dropped: `retries`",
    '    "retries": _field(int, "code", "gate", "probe", "agent"),\n', "")
mut("M02", "kind check always true",
    "    if isinstance(value, bool):\n        return bool in kinds\n    return isinstance(value, kinds)", "    return True")
mut("M03", "boolean accepted as integer",
    "    if isinstance(value, bool):\n        return bool in kinds\n", "")
mut("M04", "regex variant: hyphen dropped from the node-name class",
    'REF_RE = re.compile(r"\\{\\{(nodes|vars)\\.([\\w.-]+?)\\}\\}")',
    'REF_RE = re.compile(r"\\{\\{(nodes|vars)\\.([\\w.]+?)\\}\\}")')
mut("M05", "parser order: facts yields to a trailing `.summary`",
    "    if FACTS_SEP in body:", '    if FACTS_SEP in body and not body.endswith(".summary"):')
mut("M06", "RESULT_FIELDS loses omitted_bytes",
    'RESULT_FIELDS = ("summary", "full_ref", "omitted_bytes")', 'RESULT_FIELDS = ("summary", "full_ref")')
mut("M07", "accessor default changed: bare reference is full_ref",
    '    field: str = "summary"  # a RESULT_FIELDS member, or "facts"',
    '    field: str = "full_ref"  # a RESULT_FIELDS member, or "facts"')
mut("M08", "unknown keys accepted silently",
    "        field_def = NODE_FIELDS.get(key)\n", "        field_def = NODE_FIELDS.get(key, NodeField((object,), dedicated=True))\n")
mut("M09", "session vocabulary shrinks",
    'choices=("fresh", "resume_on_fail")', 'choices=("fresh",)')
mut("M10", "namespacer rewrites vars too",
    '            if kind == "nodes" and head in local_names:', "            if head in local_names:")
mut("M11", "sandbox text lint: only full_ref counts as text",
    '_TEXT_FIELDS = frozenset({"summary", "full_ref"})', '_TEXT_FIELDS = frozenset({"full_ref"})')
mut("M12", "placement warning removed",
    "    if field_def.types and node.type not in field_def.types:\n", "    if False:\n")
mut("M13", "refused rows stop refusing",
    "    if field_def.refused:\n", "    if False:\n")
mut("M14", "control_waived back to boolean only",
    '"control_waived": _field((bool, str), "gate"),', '"control_waived": _field(bool, "gate"),')
mut("M15", "blind_to back to a substring test",
    '    read_by_prompt = {ref.node for ref in node_refs(prompt)}\n    for hidden in persona.get("blind_to") or []:\n'
    "        if hidden in read_by_prompt:",
    '    for hidden in persona.get("blind_to") or []:\n        if f"nodes.{hidden}" in prompt:')
mut("M16", "gate feedback back to a substring test",
    '        if node.name in {r.node for r in node_refs(str(target.raw.get("prompt", "")))}:',
    '        if f"nodes.{node.name}" in str(target.raw.get("prompt", "")):')
mut("M17", "persona row loses `dedicated` (double report)",
    '"persona": _field(dict, "agent", dedicated=True),', '"persona": _field(dict, "agent"),')
mut("M18", "did-you-mean cutoff 0 (suggests anything)", "cutoff=0.75", "cutoff=0.0")
mut("M19", "reserved key becomes an error",
    "\n    return [], warnings\n\n\ndef schema_findings", "\n    return warnings, []\n\n\ndef schema_findings")
mut("M20", "list element kind unchecked",
    "    elements_ok = not (field_def.of and isinstance(value, list)) or all(\n        isinstance(v, field_def.of) for v in value)\n",
    "    elements_ok = True\n")
mut("M21", "data graph ignores non-summary fields",
    "found.update(ref.node for ref in node_refs(text))",
    'found.update(r.node for r in node_refs(text) if r.field == "summary")')
mut("M22", "runtime ignores facts",
    '        if ref.field == "facts":\n            return str(entry.get("facts"', '        if False:\n            return str(entry.get("facts"')
mut("M23", "`dedicated` rows are validated again (double report)",
    "    if field_def.dedicated:\n        return [], warnings\n", "")
mut("M24", "lint_spec does not run the schema",
    "        schema_errors, schema_warnings = schema_findings(node)\n        errors.extend(schema_errors)\n"
    "        warnings.extend(schema_warnings)\n", "")
mut("M25", "dead row added to the table",
    '    "retries": _field(int, "code", "gate", "probe", "agent"),\n',
    '    "retries": _field(int, "code", "gate", "probe", "agent"),\n    "zzz_dead_row": _field(dict, "code"),\n')
mut("M26", "schema row dropped: `dry_rounds`", '    "dry_rounds": _field(int, "loop"),\n', "")
mut("M27", "RAW_EDGE row dropped: on_calibrated edges lose their row",
    'for edge in ("on_stable", "on_covered", "on_calibrated")},', 'for edge in ("on_stable", "on_covered")},')
mut("M28", "`timeout_ms` row loses the agent type (placement false warning)",
    '"timeout_ms": _field(int, *_EXEC, "agent"),', '"timeout_ms": _field(int, *_EXEC),')

results = []
WORK.mkdir(parents=True, exist_ok=True)
base_src = (SRC / "adw.py").read_text(encoding="utf-8")
for mid, desc, old, new in M:
    n = base_src.count(old)
    if n != 1:
        results.append({"id": mid, "desc": desc, "status": f"BAD-MUTANT (matches {n})"})
        print(f"{mid} BAD-MUTANT matches={n}  {desc}")
        continue
    d = WORK / mid
    d.mkdir(exist_ok=True)
    for f in FILES:
        shutil.copy2(SRC / f, d / f)
    (d / "adw.py").write_text(base_src.replace(old, new, 1), encoding="utf-8")
    p = subprocess.run([sys.executable, "-B", "-m", "pytest", "-x", "-q", "--tb=no", "-rf", "-p", "no:cacheprovider",
                        "test_adw_node.py", "test_adw.py", "test_adw_gate_rejection.py"],
                       cwd=d, capture_output=True, text=True, env={"PYTHONDONTWRITEBYTECODE": "1", **__import__("os").environ})
    failed = [l for l in p.stdout.splitlines() if l.startswith("FAILED")]
    killed = p.returncode != 0
    by = failed[0].split(" - ")[0].replace("FAILED ", "") if failed else (p.stdout.strip().splitlines() or ["?"])[-1][:100]
    results.append({"id": mid, "desc": desc, "status": "KILLED" if killed else "SURVIVED", "by": by})
    print(f"{mid} {'KILLED  ' if killed else 'SURVIVED'} {desc}  <- {by}")

killed = sum(1 for r in results if r["status"] == "KILLED")
print(f"\nmutants: {len(results)}  killed: {killed}  survived: {sum(1 for r in results if r['status']=='SURVIVED')}"
      f"  bad: {sum(1 for r in results if r['status'].startswith('BAD'))}")
json.dump(results, open(OUT, "w"), indent=1, ensure_ascii=False)
