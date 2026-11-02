#!/usr/bin/env python3
"""Tests for the node schema (`NODE_FIELDS` / `schema_findings`) and the single
node-reference grammar (`REF_RE` / `parse_node_ref` / `node_refs`).

Origin — aprofundamento #9 (02/11/2026). `Node.raw` is the TOML table verbatim, so a
node's interface was implicit: a key the runner never reads lint-passed forever (`env`,
in the shipped `skilling-pack`), and `{{nodes.X}}` was parsed by four regexes and two
bare-substring tests that accepted four different languages. Each test below pins one
measured defect or one property that keeps the table honest.
"""

import ast
import re
import tomllib
from pathlib import Path

import pytest

import adw

SCRIPT = Path(adw.__file__).resolve()


def node(ntype: str, **raw) -> "adw.Node":
    return adw.Node(name="n", type=ntype, raw={"type": ntype, **raw})


@pytest.fixture()
def root(tmp_path: Path) -> Path:
    (tmp_path / ".touring" / "adw").mkdir(parents=True)
    return tmp_path


def lint(root: Path, text: str, name: str = "t") -> tuple[list[str], list[str]]:
    (adw.adw_dir(root) / f"{name}.toml").write_text(text, encoding="utf-8")
    return adw.lint_spec(adw.load_spec(root, name))


HEAD = '[adw]\nname = "t"\nentry = "{entry}"\n'


# ── the grammar ────────────────────────────────────────────────────────────────


@pytest.mark.parametrize("body, expected", [
    ("a", ("a", "summary", "")),
    ("a.summary", ("a", "summary", "")),
    ("a.full_ref", ("a", "full_ref", "")),
    ("a.omitted_bytes", ("a", "omitted_bytes", "")),
    ("a.facts.K", ("a", "facts", "K")),
    ("a.facts.x.y", ("a", "facts", "x.y")),
    ("a.facts.summary", ("a", "facts", "summary")),  # the fact is called `summary`
    ("p.q.r", ("p.q.r", "summary", "")),             # a namespaced node
    ("p.q.full_ref", ("p.q", "full_ref", "")),
    ("a.bogus", ("a.bogus", "summary", "")),          # an unknown suffix is part of the NAME
    ("x-y.full_ref", ("x-y", "full_ref", "")),
    ("summary", ("summary", "summary", "")),
])
def test_parse_node_ref_forms(body, expected):
    ref = adw.parse_node_ref(body)
    assert (ref.node, ref.field, ref.fact) == expected


def test_node_refs_reads_nodes_only_and_only_well_formed_references():
    text = ("{{nodes.a}} {{vars.a}} {{ nodes.b }} {{nodes.c} {nodes.d}} "
            "{{nodes.x-y.full_ref}} {{nodes.e.facts.K}} {{inputs.f}}")
    assert [(r.node, r.field) for r in adw.node_refs(text)] == [
        ("a", "summary"), ("x-y", "full_ref"), ("e", "facts")]


def test_render_template_resolves_every_form_through_the_same_parser():
    results = {"a": {"summary": "S", "full_ref": "F", "omitted_bytes": "O",
                     "facts": {"K": "fact-K", "summary": "fact-summary"}},
               "x-y": {"summary": "XY"}}
    out = adw.render_template(
        "{{nodes.a}}|{{nodes.a.summary}}|{{nodes.a.full_ref}}|{{nodes.a.omitted_bytes}}|"
        "{{nodes.a.facts.K}}|{{nodes.a.facts.summary}}|{{nodes.x-y}}|{{nodes.ghost.full_ref}}|"
        "{{vars.v}}|{{vars.missing}}", results, {"v": "VV"})
    assert out == "S|S|F|O|fact-K|fact-summary|XY||VV|"


@pytest.mark.parametrize("form", ["full_ref", "omitted_bytes", "facts.K"])
def test_the_data_graph_sees_every_result_field_of_a_node(form):
    """Measured 02/11: the lint read `{{nodes.X.full_ref}}` as a node called `X.full_ref`,
    so a node consumed only through full_ref/omitted_bytes/facts looked like dead weight."""
    reader = node("agent", prompt=f"use {{{{nodes.scan.{form}}}}}")
    assert adw.node_data_reads(reader) == {"scan"}


def test_a_node_consumed_only_through_full_ref_is_not_reported_dead(root):
    errors, warnings = lint(root, HEAD.format(entry="scan") + '''
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
''')
    assert errors == []
    assert not [w for w in warnings if "nothing downstream reads" in w or "fake waiting" in w]


def test_namespacer_rewrites_every_form_of_a_local_node_and_nothing_else():
    text = ("{{nodes.a}} {{nodes.a.summary}} {{nodes.a.full_ref}} {{nodes.a.omitted_bytes}} "
            "{{nodes.a.facts.K}} {{vars.a}} {{nodes.host.summary}} {{nodes.ab.summary}}")
    out = adw._namespace_node_refs(text, "ns", {"a"})
    assert out == ("{{nodes.ns.a}} {{nodes.ns.a.summary}} {{nodes.ns.a.full_ref}} "
                   "{{nodes.ns.a.omitted_bytes}} {{nodes.ns.a.facts.K}} {{vars.a}} "
                   "{{nodes.host.summary}} {{nodes.ab.summary}}")


# ── the lints that used `f"nodes.{name}" in text` ──────────────────────────────

CRITIC = HEAD.format(entry="review") + '''
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
prompt = "{prompt}"
allowed_tools = ["Read"]
[node.crit.persona]
role = "critic"
stance = "reject_by_default"
emits = "VERDICT=PASS|REJECT"
bar = "b"
blind_to = ["rev"]
'''


def critic(root, prompt):
    return lint(root, CRITIC.replace("{prompt}", prompt))


def test_blind_to_is_a_node_name_not_a_prefix(root):
    errors, _ = critic(root, "judge {{nodes.review.summary}}")  # reads `review`, blind to `rev`
    assert not [e for e in errors if "blindness" in e]


@pytest.mark.parametrize("ref", ["{{nodes.rev.summary}}", "{{nodes.rev}}", "{{nodes.rev.full_ref}}"])
def test_blind_to_still_catches_a_real_read_in_any_form(root, ref):
    errors, _ = critic(root, f"judge {ref}")
    assert [e for e in errors if "blindness" in e]


def test_a_mention_without_braces_is_not_a_read(root):
    errors, _ = critic(root, "do not look at nodes.rev")
    assert not [e for e in errors if "blindness" in e]


GATE_FLOW = HEAD.format(entry="work") + '''
[node.work]
type = "agent"
driver = "mock"
prompt = "fix it; last verdict: {ref}"
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
'''


def gate_warnings(root, ref):
    _, warnings = lint(root, GATE_FLOW.replace("{ref}", ref))
    return [w for w in warnings if "blind to the verdict" in w]


def test_gate_feedback_is_not_satisfied_by_a_prefix_name(root):
    assert gate_warnings(root, "{{nodes.check2.summary}}")  # reads `check2`, not `check`


@pytest.mark.parametrize("ref", ["{{nodes.check.summary}}", "{{nodes.check}}", "{{nodes.check.full_ref}}"])
def test_gate_feedback_is_satisfied_by_a_real_read_in_any_form(root, ref):
    assert gate_warnings(root, ref) == []


SANDBOX_FLOW = HEAD.format(entry="writer") + '''
[node.writer]
type = "agent"
driver = "mock"
prompt = "write"
allowed_tools = ["Read"]
on_pass = "check"
[node.check]
type = "gate"
command = ["bash", "-c", "echo {ref}"]
sandbox = true
idempotent = true
'''


@pytest.mark.parametrize("ref, flagged", [
    ("{{nodes.writer}}", True),                 # bare = summary = agent text
    ("{{nodes.writer.summary}}", True),
    ("{{nodes.writer.full_ref}}", True),
    ("{{nodes.writer.omitted_bytes}}", False),  # a number, not prose
])
def test_sandbox_text_lint_follows_the_runtime_meaning_of_the_reference(root, ref, flagged):
    _, warnings = lint(root, SANDBOX_FLOW.replace("{ref}", ref))
    assert bool([w for w in warnings if "texto de agente" in w]) is flagged


# ── the schema: keys ───────────────────────────────────────────────────────────


def test_an_unknown_key_is_an_error_with_a_suggestion():
    errors, _ = adw.schema_findings(node("code", command=["true"], timeuot_ms=10))
    assert len(errors) == 1
    assert "unknown key `timeuot_ms`" in errors[0] and "did you mean `timeout_ms`" in errors[0]


def test_a_key_the_runner_never_reads_is_refused_without_a_made_up_suggestion():
    """`env` is the shipped finding: `skilling-pack` sets RECURRENCE through it and the
    runner has never passed it to the subprocess."""
    errors, _ = adw.schema_findings(node("code", command=["true"], env={"RECURRENCE": "5"}))
    assert len(errors) == 1 and "unknown key `env`" in errors[0] and "did you mean" not in errors[0]


def test_a_key_another_node_type_reads_is_only_a_warning():
    errors, warnings = adw.schema_findings(node("code", command=["true"], session="fresh"))
    assert errors == []
    assert len(warnings) == 1 and "no effect on a `code` node" in warnings[0]
    errors, warnings = adw.schema_findings(node("agent", prompt="p", sandbox=True))
    assert errors == [] and len(warnings) == 1


def test_a_reserved_key_is_accepted_with_a_warning():
    errors, warnings = adw.schema_findings(node("agent", prompt="p", max_turns=3))
    assert errors == [] and len(warnings) == 1 and "not honoured yet" in warnings[0]


def test_policy_keeps_its_refusal_message():
    errors, _ = adw.schema_findings(node("parallel", policy="quorum:2"))
    assert errors == ["parallel `n`: `policy` is not a field — the pass condition is "
                      "`on_branch_fail` (all|any|ignore|best_effort|quorum:N)"]


def test_dedicated_keys_get_no_second_message_from_the_table():
    """persona/must_read/tools/... have their own lint for placement and shape; the table
    must not report the same defect twice."""
    n = node("code", command=["true"], persona={"role": "x"}, must_read=["a"], tools="none",
             findings_contract=True, triage_of="z", output_schema={}, mcp="none")
    assert adw.schema_findings(n) == ([], [])


# ── the schema: kinds ──────────────────────────────────────────────────────────


@pytest.mark.parametrize("ntype, key, value", [
    ("code", "retries", "3"),
    ("code", "retries", True),          # a boolean is not an integer
    ("code", "sandbox", "false"),       # a truthy string would switch the sandbox ON
    ("code", "idempotent", 1),
    ("code", "timeout_ms", "100"),
    ("code", "timeout_ms", 1.5),
    ("code", "command", "echo hi"),     # iterated as characters at run time
    ("code", "command", ["echo", 3]),
    ("agent", "allowed_tools", "Read"),
    ("agent", "budget_usd", "1"),
    ("agent", "session", ["fresh"]),
    ("agent", "prompt", 7),
    ("loop", "dry_rounds", "2"),
    ("human", "zte", "yes"),
    ("human", "message", ["a"]),
    ("gate", "stagnation_rounds", 1.5),
    ("code", "readonly", "yes"),        # node_readonly ignores a non-boolean silently
    ("code", "max_retries", "3"),
    ("code", "budget_tokens", 1.5),
])
def test_a_wrong_toml_kind_is_an_error(ntype, key, value):
    errors, _ = adw.schema_findings(node(ntype, **{key: value}))
    assert len([e for e in errors if f"`{key}` must be" in e]) == 1, errors


@pytest.mark.parametrize("ntype, key, value", [
    ("code", "retries", 2),
    ("code", "timeout_ms", 120000),
    ("code", "sandbox", True),
    ("code", "command", ["bash", "-c", "true"]),
    ("agent", "budget_usd", 1),         # an integer is a valid number
    ("agent", "budget_usd", 0.5),
    ("agent", "skill", "taco-planning"),
    ("agent", "skill", ["a", "b"]),
    ("agent", "allowed_tools", ["Read", "Bash(git *)"]),
    ("gate", "control_waived", True),
    ("gate", "control_waived", "quorum de críticos independentes é o controle deste gate"),
    ("gate", "verdict_contract", True),
    ("human", "message", "ok?"),
])
def test_the_kinds_the_shipped_flows_use_are_accepted(ntype, key, value):
    errors, warnings = adw.schema_findings(node(ntype, **{key: value}))
    assert errors == [] and warnings == []


def test_session_has_a_closed_vocabulary():
    assert adw.schema_findings(node("agent", prompt="p", session="resume_on_fail")) == ([], [])
    assert adw.schema_findings(node("agent", prompt="p", session="fresh")) == ([], [])
    errors, _ = adw.schema_findings(node("agent", prompt="p", session="resume-on-fail"))
    assert len(errors) == 1 and "resume_on_fail" in errors[0]


def test_lint_spec_runs_the_schema_before_the_other_node_lints(root):
    errors, _ = lint(root, HEAD.format(entry="run") + '''
[node.run]
type = "code"
command = ["true"]
idempotent = "false"
timeuot_ms = 1000
''')
    assert any("`idempotent` must be a boolean" in e for e in errors)
    assert any("unknown key `timeuot_ms`" in e for e in errors)


# ── the table is tied to the code, in both directions ──────────────────────────


def _runtime_keys() -> set[str]:
    """Constant keys read from a node's raw table anywhere in adw.py."""
    tree = ast.parse(SCRIPT.read_text(encoding="utf-8"))

    def is_raw(expr) -> bool:
        return (isinstance(expr, ast.Attribute) and expr.attr == "raw") or (
            isinstance(expr, ast.Name) and expr.id == "raw")

    def const(expr):
        return expr.value if isinstance(expr, ast.Constant) and isinstance(expr.value, str) else None

    keys: set[str] = set()
    for n in ast.walk(tree):
        if isinstance(n, ast.Subscript) and is_raw(n.value) and const(n.slice):
            keys.add(const(n.slice))
        elif (isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and n.args
              and n.func.attr in {"get", "pop", "setdefault"} and is_raw(n.func.value) and const(n.args[0])):
            keys.add(const(n.args[0]))
        elif isinstance(n, ast.Compare) and const(n.left) and any(is_raw(c) for c in n.comparators):
            keys.add(const(n.left))
    return keys


def test_node_schema_covers_every_key_the_runtime_reads():
    """A key read at run time and absent from the table would be refused by the lint as
    unknown — the build fails here first, not on someone's flow."""
    missing = _runtime_keys() - set(adw.NODE_FIELDS)
    assert not missing, f"read from Node.raw but not in NODE_FIELDS: {sorted(missing)}"


def test_node_schema_covers_the_named_key_families():
    families = (set(adw.EDGE_FIELDS), set(adw.RAW_EDGE_FIELDS), set(adw.AGENT_CONTRACT_FIELDS),
                {"good_input", "bad_input", "type"})
    for family in families:
        assert family <= set(adw.NODE_FIELDS), sorted(family - set(adw.NODE_FIELDS))


def test_every_row_of_the_table_is_referenced_by_the_code():
    """The reverse direction: a row nobody reads documents a field that does not exist.
    (`env` would have been caught here.) Reserved and refused rows are exempt by design."""
    tree = ast.parse(SCRIPT.read_text(encoding="utf-8"))
    def assigns_table(n) -> bool:
        targets = n.targets if isinstance(n, ast.Assign) else [n.target]
        return any(isinstance(t, ast.Name) and t.id == "NODE_FIELDS" for t in targets)

    table = next(n for n in ast.walk(tree)
                 if isinstance(n, (ast.Assign, ast.AnnAssign)) and assigns_table(n))
    inside = {id(c) for c in ast.walk(table)}
    literals = {n.value for n in ast.walk(tree)
                if isinstance(n, ast.Constant) and isinstance(n.value, str) and id(n) not in inside}
    unreferenced = sorted(k for k, f in adw.NODE_FIELDS.items()
                          if not f.reserved and not f.refused and k not in literals)
    assert not unreferenced, f"rows nothing in adw.py reads: {unreferenced}"


def test_table_rows_are_well_formed():
    for key, f in adw.NODE_FIELDS.items():
        assert f.kinds and all(isinstance(k, type) for k in f.kinds), key
        assert f.types <= adw.NODE_TYPES, (key, f.types - adw.NODE_TYPES)
        if f.choices:
            assert f.kinds == (str,), key
        if f.of:
            assert list in f.kinds, key
        if f.refused:
            assert f.dedicated, key  # the refusal is the message; no second one
        assert not (f.refused and f.reserved), key


# ── the shipped corpora pass the schema ────────────────────────────────────────

LIB = Path.home() / ".claude" / "skills" / "Touring" / "adw-library"
CORPUS_FILES = sorted([*LIB.glob("*.toml"), *(LIB / "fragments").glob("*.toml")])

#: Known finding (02/11/2026): `skilling-pack` sets RECURRENCE through `env`, which
#: `run_code_node` never passes on. strict xfail: fixing the fragment (or implementing
#: `env`) turns this into an XPASS and forces the marker out.
KNOWN_FINDINGS = {"skilling-pack.toml": "env is declared on node `should_skill` and never read"}


def _corpus_param(path: Path):
    marks = []
    if path.name in KNOWN_FINDINGS:
        marks.append(pytest.mark.xfail(strict=True, reason=KNOWN_FINDINGS[path.name]))
    return pytest.param(path, id=f"{path.parent.name}/{path.name}", marks=marks)


@pytest.mark.skipif(not CORPUS_FILES, reason="no real adw-library present")
@pytest.mark.parametrize("path", [_corpus_param(p) for p in CORPUS_FILES])
def test_shipped_library_files_have_no_schema_findings(path):
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    findings: list[str] = []
    for name, body in (data.get("node") or {}).items():
        errors, warnings = adw.schema_findings(adw.Node(name=name, type=body.get("type", ""), raw=body))
        findings += errors + warnings
    assert findings == []


PROJECT_ADW = Path.home() / "projects" / "touring" / ".touring" / "adw"


@pytest.mark.skipif(not PROJECT_ADW.is_dir(), reason="no project .touring/adw here")
@pytest.mark.parametrize("path", sorted(PROJECT_ADW.glob("*.toml")), ids=lambda p: p.name)
def test_project_specs_have_no_schema_findings(path):
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    findings: list[str] = []
    for name, body in (data.get("node") or {}).items():
        errors, warnings = adw.schema_findings(adw.Node(name=name, type=body.get("type", ""), raw=body))
        findings += errors + warnings
    assert findings == []
