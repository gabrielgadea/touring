#!/usr/bin/env python3
"""adw.py — Touring ADW: durable declarative agent workflows (F0, plan 2026-07-19).

Layer-3 master command (forwarded as `touring adw <sub>`). Code orchestrates at the
boundary BETWEEN agent sessions (Law L1); the runner owns loop termination and
convergence verdicts (Law L2); node success is decided by gates + Class-D detection,
never by agent self-report (Law L3); inter-node context is {summary, omitted_bytes,
full_ref} — dense summary inline first (Law L4).

Subcommands:
  list                          — specs available in .touring/adw/
  lint <name>                   — validate spec (edges, orphans, cycles, budget-verify)
  run <name> [--resume-run ID] [--approve NODE] [--record] [--var k=v ...]
  test <name>                   — run with agent nodes mocked from recordings (edge test)
  from-template <name>          — scaffold a spec skeleton (G1-seed)
  explain <name>                — print the FLAT resolved spec ([[use]] inlined)
  fragments                     — list the composable pieces a flow can be built from
  new <name>                    — build a flow from fragments, guided (prior-art first)

Spec: .touring/adw/<name>.toml — [adw] name/entry/budget_tokens, [node.X] typed
  code  : command[] + timeout_ms + retries + idempotent + sandbox
  agent : driver(claude|mock) + prompt + tier + allowed_tools + skill + budget_usd +
          max_turns(reserved) + session(fresh|resume_on_fail)
  gate  : same as code; a FAIL right after a success-narrating agent → Class-D.
          With `verdict_contract = true` the gate speaks PASS|REJECT|ESCALATE and
          MUST declare `on_escalate`; an unparseable verdict reads as REJECT.
  loop  : body + max_iters + dry_rounds. The body MUST print `NEW_FINDINGS=<n>`
          (stdout or stderr, own line, last is safest against `tail -c`); the
          RUNNER counts and owns termination (Law L2). An ABSENT marker is
          *unknown*, never zero: it can never start a dry streak, so a body that
          stays silent exhausts max_iters instead of faking convergence.
  human : message; passes only with --approve <node> on (re)run
  parallel: branches[] + merge(collect|tally|concat) + on_branch_fail(all|any|ignore)
          + max_branches. Read-only fan-out with a real barrier: every branch
          journals as an ordinary node, so resume skips the ones that finished.
Edges: on_pass / on_fail / on_dry → node name | "__end__" | "__fail__".

Durability: append-only journal (.touring/adw-runs/<run_id>/journal.jsonl, fsync'd)
+ per-exec results store; --resume-run replays the journal and skips completed
execs (Temporal-style). Each transition mirrors into `touring activity append`
(best-effort). Run dir is flock-guarded; a dead lock is reclaimed (fail-open on
dead state, per the 02/07 hardening laws).
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import re
import shlex
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
import time
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

# ── constants ─────────────────────────────────────────────────────────────────

END = "__end__"
FAIL = "__fail__"
TERMINALS = {END, FAIL}
NODE_TYPES = {"code", "agent", "gate", "loop", "human", "parallel", "probe", "control"}
# B4 fan-out. `merge` has NO default on purpose: without a declared join, N
# branches racing into one result slot is last-write-wins — the silent default
# LangGraph documents for a state key with no reducer. Here it fails the lint.
MERGE_STRATEGIES = {"collect", "tally", "concat"}
# Fail-CLOSED default: the branch-failure policy must be declared, and `all`
# (every branch must pass) is what an undeclared one resolves to.
#: `quorum:N` completes the set: "the block passes if at least N branches did".
#: `all`/`any` cannot express it, and the downstream verdict gate answers a
#: different question (what the critics CONCLUDED, not how many ran clean).
#: `best_effort` is the name the sources use for `ignore`; both are accepted so a
#: spec written from either vocabulary runs.
BRANCH_FAIL_POLICIES = {"all", "any", "ignore", "best_effort"}
QUORUM_POLICY_RE = re.compile(r"^quorum:([1-9][0-9]*)$")


def branch_fail_policy_error(policy: str) -> str | None:
    """None when well-formed, else why not. `quorum:N` is parameterised, so a
    plain set-membership test cannot validate the vocabulary on its own."""
    if policy in BRANCH_FAIL_POLICIES or QUORUM_POLICY_RE.match(policy):
        return None
    return (f"must be one of {sorted(BRANCH_FAIL_POLICIES)} or `quorum:N` "
            f"(got `{policy}`)")
# Fan-out is for READING. A branch that can mutate the tree turns a parallel
# block into a write race the journal cannot linearise.
WRITE_TOOLS = {"Edit", "Write", "MultiEdit", "NotebookEdit"}
SUMMARY_LIMIT = 2000  # bytes of dense inline summary (Law L4)
DEFAULT_TIMEOUT_MS = 120_000
#: Teto de wall-clock do `touring run` (o sandbox recusa acima disso). Um nó
#: com `sandbox = true` e timeout maior é limitado a este valor, com aviso.
#: 28/08/2026 (QW-3): o executor real subiu o clamp de 120s para 600s
#: (`ctx_execute_impl`: `.min(600_000)`) — manter 120 aqui clampava com uma
#: nota que afirmava um teto FALSO (anti-padrão D8: texto ≠ executor).
SANDBOX_MAX_TIMEOUT_MS = 600_000
ACTIVITY_TIMEOUT_S = 5
# F3: tier → model mapping. The library's tiers.toml (central, swappable) wins;
# these are the fallback when no library mapping exists.
TIER_MODELS = {"sota": "opus", "mid": "sonnet", "fast": "haiku",
               "workhorse": "sonnet", "light": "haiku"}
# Narrated-success heuristic for Class-D detection (Law L3): the agent CLAIMS
# success; only a gate may confirm it.
SUCCESS_NARRATIVE = re.compile(
    r"\b(success|succeeded|complete[d]?|done|pronto|conclu[íi]d[oa]|all tests pass)\b|✅",
    re.IGNORECASE,
)
NEW_FINDINGS_RE = re.compile(r"^NEW_FINDINGS=(\d+)\s*$", re.MULTILINE)
# E4 campaign contract: the --until predicate may emit `METRIC=<float>` — the
# campaign's progress signal. Absent means UNKNOWN (fail-closed, the same
# reading the loop node applies to NEW_FINDINGS), never zero and never
# stagnation.
METRIC_RE = re.compile(r"^METRIC=([0-9]+(?:\.[0-9]+)?)\s*$", re.MULTILINE)
# W4 S-4.1 (plano code-mode-total) — o loop `predicate = "fixpoint"` lê o MESMO
# marcador METRIC=; converge quando o valor REPETE por `stable_rounds`. Marcador
# ausente é UNKNOWN: nunca inicia nem estende a sequência estável (Lei L2).
# W4 S-4.2 — `predicate = "covered"`: o corpo emite `COVERAGE=n/m`. Termina
# APENAS com n==m e m >= min_discovered; exaurir com n<m é __fail__, jamais
# sucesso (a classe familia_parcial, 19% do corpus de memórias). Um `m` que
# ENCOLHE entre rodadas é erro: o universo não diminui — se diminuiu, a
# descoberta está instável e o número não é confiável.
COVERAGE_RE = re.compile(r"^COVERAGE=(\d+)/(\d+)\s*$", re.MULTILINE)
# O fixpoint compara o TOKEN CRU (igualdade de string): cobre número E hash —
# `METRIC=$(sha256sum alvo | cut -d' ' -f1)` é exatamente o caso freshness-audit
# (rótulo não prova build; hash repetido em duas leituras prova). O METRIC_RE
# numérico acima segue sendo o contrato dos campaigns (--until lê progresso).
METRIC_ANY_RE = re.compile(r"^METRIC=(\S+)\s*$", re.MULTILINE)
# W4 S-4.4 — contrato do nó `probe`: DEVE emitir >= 1 `FACT=<chave>=<valor>`;
# os fatos ganham endereço ({{nodes.X.facts.chave}} + journal com run_id).
FACT_RE = re.compile(r"^FACT=([A-Za-z_][A-Za-z0-9_]*)=(.+?)\s*$", re.MULTILINE)
# W4 S-4.3 — marcador emitido pelo RUNNER (run_control_node), nunca pelo corpo:
# `CONTROL=pass` só existe quando o verificador aprovou o bom E reprovou o ruim.
CONTROL_PASS_RE = re.compile(r"^CONTROL=pass\s*$", re.MULTILINE)
#: Predicados de terminação aceitos por um nó loop. Cada um ataca uma classe
#: medida do corpus de memórias (staleness 42% → fixpoint; familia_parcial 19%
#: → covered; instrumento_errado 38% → calibrated/control).
LOOP_PREDICATES = {"dry", "fixpoint", "covered", "calibrated"}
# B6 verification contract. Three verdicts, because two cannot express the case
# that costs the most: a check that COULD NOT RUN. Under a boolean gate a broken
# environment is indistinguishable from a real rejection, so the runner spends
# its whole retry budget re-invoking an agent against something no agent can fix.
VERDICT_RE = re.compile(r"^VERDICT=(PASS|REJECT|ESCALATE)\b", re.MULTILINE)
PASS, REJECT, ESCALATE = "PASS", "REJECT", "ESCALATE"
# Default stance is REJECT: an unparseable verdict is not a pass. Silence never
# clears a gate — the same fail-closed reading the loop applies to NEW_FINDINGS.
DEFAULT_VERDICT = REJECT
#: A run aborts when this file appears in its run dir — the per-run kill switch.
KILL_SWITCH = "STOP"


# ── persona (B3): posture declared in the flow, compiled to `--agents` ────────
# A node says WHAT to do; a persona says WHO does it and under which refusals.
# Declared INLINE in the spec — never a reference to a global agent catalogue —
# so a flow stays portable: copying the .toml carries its personas with it.
# Each entry below is one prompt technique; the section order is fixed so the
# same persona always compiles to the same bytes (stable for caching + tests).
PERSONA_SECTIONS: tuple[tuple[str, str, str], ...] = (
    ("stance", "STANCE",
     "Your default posture is: {v}. You hold it until evidence moves you — not until you are asked to."),
    ("lens", "LENS",
     "You judge through exactly one lens: {v}. Findings outside that lens are not yours to raise."),
    ("scope", "SCOPE",
     "Your scope is closed: {v}. Anything outside it you decline; you do not 'briefly also' do it."),
    ("bar", "BAR",
     "The bar to clear is concrete: {v}. Not 'looks good' — that, and nothing looser."),
    ("burden", "BURDEN", "{v}"),
    ("refuses", "CANNOT",
     "You do not have the capability to: {v}. Asked for it, you say so plainly instead of improvising."),
    ("forbids", "FORBIDDEN",
     "These shortcuts are forbidden even when they would be faster: {v}."),
    ("blind_to", "BLIND",
     "You have deliberately NOT been shown: {v}. Do not ask for it and do not assume its content."),
    ("escalate_when", "ESCALATE",
     "Escalate instead of deciding when: {v}. Escalation is a valid outcome, never a failure."),
    ("emits", "OUTPUT",
     "Your final line MUST match this contract exactly, with nothing after it: {v}"),
)
PERSONA_FIELDS = {name for name, _, _ in PERSONA_SECTIONS} | {"role", "description", "prompt"}
# A persona holding this stance IS a critic, and the lint holds it to a critic's
# structure: a parseable verdict, and a context it did not help build.
CRITIC_STANCE = "reject_by_default"


def compile_persona(persona: dict) -> str:
    """Render a persona into its system prompt — deterministic, section-ordered."""
    parts: list[str] = []
    for key, label, template in PERSONA_SECTIONS:
        value = persona.get(key)
        if not value:
            continue
        rendered = ", ".join(str(v) for v in value) if isinstance(value, (list, tuple)) else str(value)
        parts.append(f"{label}: {template.format(v=rendered)}")
    extra = str(persona.get("prompt", "")).strip()
    if extra:
        parts.append(extra)
    return "\n\n".join(parts)


def compile_agent_definition(persona: dict, results: dict,
                             variables: dict) -> tuple[str, dict[str, str]]:
    """(role, definition) for `--agents` — persona fields resolve {{vars}}/{{nodes}} too."""
    resolved = {
        k: ([render_template(str(x), results, variables) for x in v]
            if isinstance(v, (list, tuple)) else render_template(str(v), results, variables))
        for k, v in persona.items()
    }
    role = str(resolved["role"]).strip()
    description = str(resolved.get("description", "")).strip() or (
        f"{role} — {resolved.get('stance', 'declared in-flow')}")
    return role, {"description": description, "prompt": compile_persona(resolved)}


# ── spec model ────────────────────────────────────────────────────────────────


@dataclass
class Node:
    """One typed workflow node parsed from [node.<name>]."""

    name: str
    type: str
    raw: dict
    on_pass: str = END
    on_fail: str = FAIL
    on_dry: str = END


@dataclass
class Spec:
    """A parsed and structurally valid ADW spec."""

    name: str
    entry: str
    path: Path
    nodes: dict[str, Node]
    budget_tokens: int = 0
    budget_usd: float = 0.0
    description: str = ""
    #: The [purpose] block: what this flow is FOR, so the portfolio can retrieve
    #: it by intent instead of by filename.
    purpose: dict = field(default_factory=dict)

    def node(self, name: str) -> Node:
        return self.nodes[name]


class SpecError(ValueError):
    """Raised when a spec fails structural validation."""


def adw_dir(root: Path) -> Path:
    return root / ".touring" / "adw"


def runs_dir(root: Path) -> Path:
    return root / ".touring" / "adw-runs"


def library_dir() -> Path:
    """Central ADW library (F3): reusable spec templates + tiers.toml."""
    override = os.environ.get("TOURING_ADW_LIBRARY")
    if override:
        return Path(override)
    return Path.home() / ".claude" / "skills" / "Touring" / "adw-library"


def tier_models() -> dict[str, str]:
    """Tier → model map: library tiers.toml `[tiers]` overrides the built-ins."""
    mapping = dict(TIER_MODELS)
    tiers_path = library_dir() / "tiers.toml"
    if tiers_path.is_file():
        try:
            data = tomllib.loads(tiers_path.read_text(encoding="utf-8"))
            mapping.update({k: str(v) for k, v in (data.get("tiers") or {}).items()})
        except (tomllib.TOMLDecodeError, OSError):
            pass  # a broken central config must not brick every runner
    return mapping


# ── fragments (B1): composable pieces, resolved by inlining ───────────────────
# A fragment is a reusable sub-graph. `[[use]]` inlines one under a namespace, so
# after resolution the spec is FLAT: the runner, journal, resume, Class-D and lint
# all keep operating on the same shape they always did. Composition is a loader
# concern and never reaches the engine — which is why it costs no durability risk.
#
#   [[use]]
#   module  = "recall-pack"                  # fragments/<module>.toml
#   as      = "recall"                       # namespace → nodes become recall.<n>
#   with    = { topic = "{{vars.symptom}}" } # binds the fragment's declared inputs
#   on_pass = "diagnose"                     # where the fragment's __exit__ lands
#   on_fail = "diagnose"                     # where its __exit_fail__ lands
#
# Inside a fragment, `__exit__`/`__exit_fail__` are the seams: the fragment never
# names a host node, which is exactly what makes it reusable across flows.
EXIT = "__exit__"
EXIT_FAIL = "__exit_fail__"
FRAGMENT_SEAMS = {EXIT, EXIT_FAIL}
NAMESPACE_SEP = "."
#: Edge fields that live only in `node.raw` (not as Node attributes). Kept as a
#: named family because FOUR walkers must agree on what an edge is (inliner,
#: control_successors, _lint_node, mermaid) — per-site drift here is the same
#: class as the five hook-count sites (2026-08-23).
RAW_EDGE_FIELDS = ("on_escalate", "on_stable", "on_covered", "on_calibrated")
#: Every field that carries an edge. Kept in one place because the inliner and the
#: namespace deref must agree: a field missing here is one a fragment cannot use.
EDGE_FIELDS = ("on_pass", "on_fail", "on_dry", *RAW_EDGE_FIELDS)


def fragment_dirs(root: Path) -> list[Path]:
    """Project fragments shadow library ones — a flow may override a shared piece."""
    return [adw_dir(root) / "fragments", library_dir() / "fragments"]


def find_fragment(root: Path, module: str) -> Path:
    for directory in fragment_dirs(root):
        candidate = directory / f"{module}.toml"
        if candidate.is_file():
            return candidate
    searched = ", ".join(str(d) for d in fragment_dirs(root))
    raise SpecError(f"fragment `{module}` not found (searched: {searched})")


def _substitute(value, mapping: dict[str, str]):
    """Deep {{inputs.k}} substitution over a node body (str | list | dict)."""
    if isinstance(value, str):
        def sub(match: re.Match) -> str:
            key = match.group(1)
            if key not in mapping:
                raise SpecError(f"fragment references undeclared input `{key}`")
            return mapping[key]
        return re.sub(r"\{\{inputs\.([\w.-]+?)\}\}", sub, value)
    if isinstance(value, list):
        return [_substitute(v, mapping) for v in value]
    if isinstance(value, dict):
        return {k: _substitute(v, mapping) for k, v in value.items()}
    return value


def _rewrite_edge(target: str, namespace: str, local_names: set[str],
                  on_pass: str, on_fail: str) -> str:
    """Fragment-local → namespaced; seams → the host's wiring; terminals unchanged."""
    if target == EXIT:
        return on_pass
    if target == EXIT_FAIL:
        return on_fail
    if target in TERMINALS:
        return target
    if target in local_names:
        return f"{namespace}{NAMESPACE_SEP}{target}"
    raise SpecError(
        f"fragment node edge → `{target}`, which is neither a fragment node nor a seam "
        f"({sorted(FRAGMENT_SEAMS)}) nor a terminal — a fragment may not name a host node")


def _namespace_node_refs(value, namespace: str, local_names: set[str]):
    """`{{nodes.<local>.summary}}` → `{{nodes.<ns>.<local>.summary}}` inside a fragment."""
    if isinstance(value, str):
        def sub(match: re.Match) -> str:
            key = match.group(1)
            tail = match.group(2) or ""
            head = key.split(NAMESPACE_SEP)[0]
            if head in local_names:
                return f"{{{{nodes.{namespace}{NAMESPACE_SEP}{key}{tail}}}}}"
            return match.group(0)
        return re.sub(r"\{\{nodes\.([\w.-]+?)(\.summary)?\}\}", sub, value)
    if isinstance(value, list):
        return [_namespace_node_refs(v, namespace, local_names) for v in value]
    if isinstance(value, dict):
        return {k: _namespace_node_refs(v, namespace, local_names) for k, v in value.items()}
    return value


def _inline_one(use: dict, root: Path, host_nodes: dict, index: int) -> tuple[str, dict]:
    """Resolve one [[use]] into (namespace, {namespaced node name: body})."""
    module = str(use.get("module", "")).strip()
    if not module:
        raise SpecError(f"[[use]] #{index}: missing `module`")
    namespace = str(use.get("as", module)).strip()
    if NAMESPACE_SEP in namespace:
        raise SpecError(f"[[use]] `{module}`: namespace `{namespace}` may not contain `{NAMESPACE_SEP}`")

    data = tomllib.loads(find_fragment(root, module).read_text(encoding="utf-8"))
    meta = data.get("fragment") or {}
    frag_nodes = data.get("node") or {}
    if not frag_nodes:
        raise SpecError(f"fragment `{module}` has no [node.*] tables")

    declared = list(meta.get("inputs") or [])
    bound = dict(use.get("with") or {})
    missing = [k for k in declared if k not in bound]
    if missing:
        raise SpecError(f"[[use]] `{module}` as `{namespace}`: unbound input(s) {missing}")
    extra = [k for k in bound if k not in declared]
    if extra:
        raise SpecError(f"[[use]] `{module}` as `{namespace}`: input(s) {extra} not declared "
                        f"by the fragment (declares {declared})")

    entry = str(meta.get("entry", "")).strip()
    if entry not in frag_nodes:
        raise SpecError(f"fragment `{module}`: entry `{entry}` is not one of its nodes")

    local_names = set(frag_nodes)
    on_pass = str(use.get("on_pass", END))
    on_fail = str(use.get("on_fail", FAIL))
    mapping = {k: str(v) for k, v in bound.items()}

    resolved: dict[str, dict] = {}
    for local, body in frag_nodes.items():
        qualified = f"{namespace}{NAMESPACE_SEP}{local}"
        if qualified in host_nodes:
            raise SpecError(f"[[use]] `{module}` as `{namespace}`: node `{qualified}` "
                            f"collides with an existing node")
        new_body = _namespace_node_refs(_substitute(dict(body), mapping), namespace, local_names)
        for edge in EDGE_FIELDS:
            if edge in new_body:
                new_body[edge] = _rewrite_edge(str(new_body[edge]), namespace, local_names,
                                               on_pass, on_fail)
        if new_body.get("type") == "loop" and new_body.get("body") in local_names:
            new_body["body"] = f"{namespace}{NAMESPACE_SEP}{new_body['body']}"
        # A fan-out inside a fragment names its branches locally; without this they
        # resolve against the HOST's node table and the whole panel reads as orphans.
        if new_body.get("type") == "parallel":
            raw = new_body.get("branches")
            if isinstance(raw, list):
                new_body["branches"] = [
                    f"{namespace}{NAMESPACE_SEP}{b}" if b in local_names else b
                    for b in raw
                ]
            # A DYNAMIC fan-out carries a template string in `branches`, which must
            # be left alone — iterating it namespaces one CHARACTER per branch. The
            # node it names is `template`, and that one does need qualifying.
            if new_body.get("template") in local_names:
                new_body["template"] = f"{namespace}{NAMESPACE_SEP}{new_body['template']}"
        resolved[qualified] = new_body
    return namespace, resolved


def resolve_uses(data: dict, root: Path) -> dict:
    """Inline every [[use]] into `data['node']`; returns the flat spec data.

    Edges that name a bare namespace (`on_pass = "critic"`) are rewired to that
    fragment's entry, so a composed piece is referenced exactly like one node —
    the compound-node shape LangGraph gets from compiling a subgraph."""
    uses = data.get("use") or []
    if not uses:
        return data
    nodes = dict(data.get("node") or {})
    entries: dict[str, str] = {}
    for index, use in enumerate(uses):
        namespace, resolved = _inline_one(use, root, nodes, index)
        if namespace in entries:
            raise SpecError(f"[[use]]: namespace `{namespace}` used twice")
        if namespace in nodes:
            raise SpecError(f"[[use]]: namespace `{namespace}` collides with a node of that name")
        entries[namespace] = f"{namespace}{NAMESPACE_SEP}" + str(
            (tomllib.loads(find_fragment(root, str(use['module'])).read_text(encoding='utf-8'))
             .get('fragment') or {}).get('entry'))
        nodes.update(resolved)

    def deref(target: str) -> str:
        return entries.get(target, target)

    for body in nodes.values():
        for edge in EDGE_FIELDS:
            if edge in body:
                body[edge] = deref(str(body[edge]))
        if body.get("type") == "loop" and "body" in body:
            body["body"] = deref(str(body["body"]))
        if body.get("type") == "parallel":
            raw = body.get("branches")
            if isinstance(raw, list):
                body["branches"] = [deref(str(b)) for b in raw]
            if body.get("template"):
                body["template"] = deref(str(body["template"]))
    flat = dict(data)
    flat["node"] = nodes
    meta = dict(flat.get("adw") or {})
    if meta.get("entry"):
        meta["entry"] = deref(str(meta["entry"]))
    flat["adw"] = meta
    flat.pop("use", None)
    return flat


def load_spec(root: Path, name: str) -> Spec:
    path = adw_dir(root) / f"{name}.toml"
    if not path.is_file():
        raise SpecError(f"spec not found: {path}")
    data = resolve_uses(tomllib.loads(path.read_text(encoding="utf-8")), root)
    meta = data.get("adw") or {}
    nodes_raw = data.get("node") or {}
    if not nodes_raw:
        raise SpecError("spec has no [node.*] tables")
    nodes: dict[str, Node] = {}
    for nname, body in nodes_raw.items():
        ntype = body.get("type", "")
        if ntype not in NODE_TYPES:
            raise SpecError(f"node `{nname}`: unknown type `{ntype}` (want {sorted(NODE_TYPES)})")
        nodes[nname] = Node(
            name=nname,
            type=ntype,
            raw=body,
            on_pass=body.get("on_pass", END),
            on_fail=body.get("on_fail", FAIL),
            on_dry=body.get("on_dry", END),
        )
    entry = meta.get("entry", "")
    if entry not in nodes:
        raise SpecError(f"entry `{entry}` is not a defined node")
    return Spec(
        name=meta.get("name", name),
        entry=entry,
        path=path,
        nodes=nodes,
        budget_tokens=int(meta.get("budget_tokens", 0)),
        budget_usd=float(meta.get("budget_usd", 0.0)),
        description=meta.get("description", ""),
        purpose=data.get("purpose") or {},
    )


# ── lint (G19 + budget-verify) ────────────────────────────────────────────────


# ── the two topologies (T1.0) ─────────────────────────────────────────────────
#
# A spec carries two overlapping graphs, and until 2026-08-19 only one of them
# existed as an object:
#
#   · the CONTROL graph, declared — `on_pass`/`on_fail`/`branches`: who runs after whom.
#   · the DATA graph, real — who actually INTERPOLATES `{{nodes.X.summary}}` from whom.
#
# Their divergence is the finding. A control edge with no data edge is Isenberg's
# "fake waiting" (*Why Graph Engineering will 10x your Claude/Codex*): B waits on
# A for nothing. A node with no outgoing data edge is dead weight — the
# constructive half of "the goal is the SMALLEST graph that improves the quality
# of the work", which beats capping node counts by decree.

#: Mirrors the runtime resolver in `render_template`, minus `vars`. Kept adjacent
#: on purpose: if one accepts a form the other does not, the lint reasons about a
#: graph the runner never executes.
NODE_REF_RE = re.compile(r"\{\{nodes\.([\w.-]+?)(?:\.summary)?\}\}")

#: Tools that cannot mutate anything. `Bash` is deliberately absent — it is a
#: general-purpose writer wearing a read-only-looking name.
READ_ONLY_TOOLS = frozenset({"Read", "Grep", "Glob", "WebFetch", "WebSearch", "NotebookRead", "LS"})

# ── skill binding (B4, 2026-08-20): the "O" of TACO ──────────────────────────
#
# An `agent` node compiles to a headless `claude -p` with a persona. The persona
# says WHO acts and under which refusals; nothing said which CRAFT to apply, so
# every planning agent re-derived what `taco-planning` already encodes and every
# auditing agent re-derived `TACO-cross-audit`. Measured 20/08/2026: `Skill`
# appeared ZERO times in this runner and zero times across the ten library specs,
# while `claude -p --allowedTools Skill` was proven to load a skill successfully.
# The mechanism existed and no flow reached for it.
#
# `skill = "taco-planning"` (or a list) binds the craft. Both ways it can fail
# SILENTLY — a skill that does not exist, and a skill switched `off` in
# `skillOverrides` (28 are) — so both are lint verdicts, never runtime surprises,
# and `Skill` is injected into `allowed_tools` because naming a tool the agent
# cannot call is the same silent no-op wearing a different hat.
SKILLS_ROOT = Path.home() / ".claude" / "skills"
SKILL_TOOL = "Skill"
SKILL_PREAMBLE = (
    "SKILL: before anything else, invoke the Skill tool with skill='{name}' and follow "
    "the procedure it returns. It is the canonical procedure for this step — do not "
    "re-derive it, and do not substitute your own method for it.")


def skill_binding(node: "Node") -> list[str]:
    """Skills this node binds, normalised to a list (empty when none)."""
    raw = node.raw.get("skill")
    if not raw:
        return []
    if isinstance(raw, str):
        return [raw.strip()] if raw.strip() else []
    return [str(x).strip() for x in raw if str(x).strip()]


def disabled_skills() -> frozenset[str]:
    """Skills `skillOverrides` switched off — a bound one would be a silent no-op.

    Unreadable settings mean UNKNOWN, so an empty set is returned and nothing is
    accused: this gate must never invent a disabled skill."""
    try:
        data = json.loads((Path.home() / ".claude" / "settings.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return frozenset()
    overrides = data.get("skillOverrides")
    if not isinstance(overrides, dict):
        return frozenset()
    return frozenset(k for k, v in overrides.items() if str(v).lower() in {"off", "false", "disabled"})


def skill_status(name: str) -> str:
    """`ok` · `missing` · `disabled` — and `ok` whenever it cannot be told."""
    if not SKILLS_ROOT.is_dir():
        return "ok"  # cannot inspect the corpus → never accuse
    if not (SKILLS_ROOT / name / "SKILL.md").is_file():
        return "missing"
    return "disabled" if name in disabled_skills() else "ok"


def skill_tools(node: "Node") -> list[str]:
    """`allowed_tools` with `Skill` present whenever the node binds one."""
    tools = [str(t) for t in (node.raw.get("allowed_tools") or [])]
    if skill_binding(node) and SKILL_TOOL not in tools:
        tools.append(SKILL_TOOL)
    return tools


def skill_prompt_prefix(node: "Node") -> str:
    """Deterministic preamble — same node, same bytes (stable for tests/caching)."""
    names = skill_binding(node)
    if not names:
        return ""
    return "\n\n".join(SKILL_PREAMBLE.format(name=n) for n in names) + "\n\n"


@dataclass
class NodeIO:
    """One node's place in the DATA graph.

    ``readonly`` is deliberately tri-state. ``None`` means *unprovable*, not
    *false*: a `code` node runs an arbitrary command and nothing in the spec says
    whether it writes. Every consumer treats ``None`` as "stay silent", because
    the expensive mistake here is telling someone to parallelise two nodes that
    are coupled through a file the interpolator cannot see.
    """

    reads: set[str] = field(default_factory=set)
    readonly: bool | None = None


def _interpolable_strings(node: Node) -> list[str]:
    """Every string in a node that the runner passes through `render_template`.

    Derived from the call sites of `render_template`, not guessed: `prompt`
    (agent), `command` parts (code/gate), the dynamic `branches` expression, and
    every persona field.
    """
    out: list[str] = []
    raw = node.raw
    out.append(str(raw.get("prompt", "")))
    out.append(str(raw.get("branches", "")))
    command = raw.get("command")
    if isinstance(command, (list, tuple)):
        out.extend(str(part) for part in command)
    persona = raw.get("persona")
    if isinstance(persona, dict):
        for value in persona.values():
            if isinstance(value, (list, tuple)):
                out.extend(str(v) for v in value)
            else:
                out.append(str(value))
    return out


def node_data_reads(node: Node) -> set[str]:
    """The node names whose output this node interpolates."""
    found: set[str] = set()
    for text in _interpolable_strings(node):
        found.update(NODE_REF_RE.findall(text))
    return found


def node_readonly(node: Node) -> bool | None:
    """Can this node be proven not to mutate anything? None = cannot tell.

    An `agent` proves it through `allowed_tools`: every tool in the read-only
    allowlist. Absent `allowed_tools` means the agent may reach for anything, so
    the answer is ``None`` rather than ``True`` — an omission is not a promise.
    Any node may state `readonly = true` explicitly, which is how a `code` node
    says what its command does; the declaration is visible in the spec and
    reviewable, which guessing from a shell string is not.
    """
    declared = node.raw.get("readonly")
    if isinstance(declared, bool):
        return declared
    if node.type == "agent":
        if skill_binding(node):
            return None  # a skill may do anything the session can — unknowable here
        tools = node.raw.get("allowed_tools")
        if isinstance(tools, (list, tuple)) and tools:
            return all(str(t) in READ_ONLY_TOOLS for t in tools)
        return None
    if node.type == "human":
        return True  # it asks a person; it changes nothing itself
    return None


# ── write detection (24/08/2026) ─────────────────────────────────────────────
#
# Um detector que lê SINTAXE DE SHELL (`>`, `rm`, `sed -i`) não enxerga a
# escrita feita DENTRO de um programa. Foi assim que a auditoria do code mode
# aprovou `arm_marker` como leitura: o comando é
# `python3 .../loop_marker.py write --task OUTER ...` — nada em `python3` nem no
# caminho do script diz que ele grava; quem diz é o VERBO `write`, na posição de
# subcomando. O detector abaixo lê as duas metades, e sobretudo nunca conclui
# "leitura" a partir da AUSÊNCIA de sintaxe de escrita: o que ele não consegue
# provar vira `None`, jamais `False`.

#: Palavras cujo propósito É mutar a árvore — visíveis na própria linha.
WRITING_COMMANDS = frozenset({
    "rm", "rmdir", "mv", "cp", "mkdir", "touch", "tee", "dd", "truncate",
    "install", "chmod", "chown", "chgrp", "ln", "shred", "unlink", "patch",
})

#: Palavras que, SOZINHAS, não mutam nada. Pertencer a este conjunto não basta
#: para chamar um comando de leitura: `sed` é inofensivo até `-i`, e `echo` até
#: `>`. Os dois casos são checados à parte.
READING_COMMANDS = frozenset({
    "cat", "head", "tail", "grep", "rg", "echo", "printf", "ls", "wc", "sort",
    "uniq", "cut", "sed", "awk", "jq", "true", "false", "test", "date", "diff",
    "basename", "dirname", "realpath", "readlink", "stat", "comm", "tr", "seq",
    "pwd", "which", "command", "env", "sleep", "id", "hostname", "df", "du",
    "set", "cd", "export", "local", "read", "wait",
    # Palavras-chave do shell: estrutura, não efeito.
    "if", "then", "else", "elif", "fi", "case", "esac", "for", "while", "do",
    "done", "exit", "return", "shift", "eval", "source", ".",
})

#: O subconjunto de READING_COMMANDS que de fato TOCA dados (FS, streams) — o
#: que separa `grep`/`cat` de `echo`/`true`. É a régua do lint
#: `_lint_readonly_without_sandbox`: só um leitor substantivo ganha o convite
#: ao sandbox; um puro-stdout não tem o que conter.
_FS_READING_COMMANDS = frozenset({
    "cat", "head", "tail", "grep", "rg", "ls", "wc", "sort", "uniq", "cut",
    "sed", "awk", "jq", "diff", "stat", "comm", "find", "du", "df",
    "readlink", "realpath", "tr",
})

#: Multiplexadores cujo PRIMEIRO argumento é um subcomando — é ali que o verbo
#: de escrita aparece quando a escrita mora dentro do programa.
PROGRAM_MULTIPLEXERS = frozenset({
    "touring", "git", "cargo", "npm", "pip", "pip3", "docker", "systemctl",
    "gh", "snapper", "rustup", "uv", "poetry",
})

#: Sufixos que denunciam "isto é um programa, e o que ele faz não está aqui".
PROGRAM_SUFFIXES = (".py", ".sh", ".pl", ".rb", ".js", ".ts", ".bash")

#: Verbos que, em posição de subcomando, dizem que o programa GRAVA.
WRITING_VERBS = frozenset({
    "write", "store", "save", "commit", "add", "create", "update", "delete",
    "remove", "set", "put", "push", "init", "install", "apply", "arm", "emit",
    "rebuild", "sync", "finalize", "claim", "release", "reset", "restore",
    "checkout", "merge", "rebase", "tag", "publish", "deploy",
})

#: Flags inequívocas de "e então grave". `-o`/`--output` ficam DE FORA de
#: propósito: `set -o pipefail` abre três nós desta biblioteca, e um detector
#: que os acusasse de escrita seria ruído com cara de rigor.
WRITING_FLAGS = frozenset({
    "--in-place", "--write", "--apply", "--fix", "--emit", "--save",
    "--overwrite", "--force-write",
})

#: `>`/`>>` para um ARQUIVO. `2>&1` e `>&2` redirecionam descritor, não gravam,
#: e aparecem em quase todo nó da biblioteca — contá-los tornaria o detector
#: inútil justamente onde ele precisa discriminar.
_REDIRECT_TO_FILE_RE = re.compile(r"(?<![0-9<>&])>>?(?!\s*&)")


def _command_tokens(command) -> list[str]:
    """Todo token executável do comando, inclusive os de dentro de `bash -c`.

    Um `bash -c "<corpo do programa>"` esconde o comando real dentro de UM token. Sem
    reabrir esse token o detector examinaria a casca (`bash`, `-c`) e declararia
    desconhecido tudo que importa.
    """
    import shlex
    tokens: list[str] = []
    for part in (command or []):
        texto = str(part)
        tokens.append(texto)
        if any(ch in texto for ch in ";|&\n") or " " in texto:
            try:
                tokens.extend(shlex.split(texto))
            except ValueError:
                tokens.extend(texto.split())
    return tokens


def command_writes(command) -> bool | None:
    """O comando muta alguma coisa? ``None`` = não dá para provar.

    Três valores, e o terceiro é o ponto: ``False`` é reservado para o caso em
    que TODA palavra executada é conhecidamente de leitura. Qualquer programa
    cujo efeito não esteja na linha devolve ``None`` — nunca ``False``, que foi
    exatamente o erro que deixou `arm_marker` passar por leitura.

    Args:
        command: A lista ``command`` do nó, como está no TOML.

    Returns:
        ``True`` se grava comprovadamente, ``False`` se é comprovadamente
        leitura, ``None`` se não é possível decidir pela linha de comando.
    """
    if not command:
        return None
    tokens = _command_tokens(command)
    bruto = " ".join(str(c) for c in command)

    if _REDIRECT_TO_FILE_RE.search(bruto):
        return True

    palavras = [t for t in tokens if t]
    for i, tok in enumerate(palavras):
        base = tok.rsplit("/", 1)[-1]
        if base in WRITING_COMMANDS:
            return True
        if tok in WRITING_FLAGS:
            return True
        # `-i` só é escrita para quem edita no lugar; em `grep -i` é
        # case-insensitive, e acusá-lo seria o falso-positivo clássico.
        if tok == "-i" and any(
            p.rsplit("/", 1)[-1] in {"sed", "perl"} for p in palavras[:i]
        ):
            return True
        # A metade que a sintaxe não mostra: o verbo em posição de subcomando.
        if base in PROGRAM_MULTIPLEXERS or base.endswith(PROGRAM_SUFFIXES):
            seguintes = [t for t in palavras[i + 1 : i + 6] if not t.startswith("-")]
            if any(s in WRITING_VERBS for s in seguintes[:3]):
                return True

    # Só agora, e só se NADA no comando for um programa opaco, a resposta pode
    # ser "leitura". Um único token desconhecido rebaixa para `None`.
    invoca_programa = any(
        t.rsplit("/", 1)[-1].endswith(PROGRAM_SUFFIXES)
        or t.rsplit("/", 1)[-1] in PROGRAM_MULTIPLEXERS
        or t.rsplit("/", 1)[-1] in {"python", "python3", "node", "ruby", "perl"}
        for t in palavras
    )
    if invoca_programa:
        return None
    executaveis = [
        t.rsplit("/", 1)[-1]
        for t in palavras
        if t and not t.startswith("-") and not t.startswith("{{")
    ]
    conhecidos = [e for e in executaveis if e in READING_COMMANDS]
    if conhecidos and all(
        e in READING_COMMANDS or e in {"bash", "sh", "--"} or not e.isalpha()
        for e in executaveis
    ):
        return False
    return None


def _invokes_opaque_program(tokens: list[str]) -> bool:
    """O comando chama um script ou interpretador — código que a linha não mostra.

    É a fronteira exata da cegueira do detector antigo: `touring wiring impact` é
    uma chamada documentada, `python3 loop_marker.py` é um programa arbitrário.
    """
    return any(
        t.rsplit("/", 1)[-1].endswith(PROGRAM_SUFFIXES)
        or t.rsplit("/", 1)[-1] in {"python", "python3", "node", "ruby", "perl"}
        for t in tokens
    )


def _lint_readonly_claim(node: Node, errors: list[str], warnings: list[str]) -> None:
    """Uma declaração `readonly` que o comando contradiz é recusada AQUI.

    O princípio é o D8: quem aplica a regra é o executor, não o comentário. O
    `arm_marker` documentava a própria limitação num comentário — que nada
    verifica — e foi essa a lacuna que deixou a auditoria aprová-lo.
    """
    if node.type not in {"code", "gate"}:
        return
    declarado = node.raw.get("readonly")
    grava = command_writes(node.raw.get("command"))
    if declarado is True and grava is True:
        errors.append(
            f"node `{node.name}`: declara readonly=true mas o comando GRAVA — "
            f"a declaração é lida por lints de fan-out e de espera falsa, que "
            f"passariam a raciocinar sobre um grafo que o runner não executa"
        )
    elif declarado is True and grava is None and _invokes_opaque_program(
        _command_tokens(node.raw.get("command"))
    ):
        # O aviso vale onde mora a cegueira do detector antigo: um script ou
        # interpretador cujo código não está na linha. Um comando feito só de
        # chamadas documentadas do CLI (`touring wiring impact`) ou de shell puro
        # não ganha aviso — punir o spec correto é como um lint perde a atenção
        # que ele precisa ter quando o achado é real.
        warnings.append(
            f"node `{node.name}`: declara readonly=true e chama um programa cujo "
            f"efeito não está na linha de comando — a ausência de `>`/`rm` não "
            f"prova leitura; confirme o que o programa faz"
        )


def flow_dataflow(spec: Spec) -> dict[str, NodeIO]:
    """The spec's DATA graph, keyed by node name."""
    return {
        name: NodeIO(reads=node_data_reads(node), readonly=node_readonly(node))
        for name, node in spec.nodes.items()
    }


def control_successors(spec: Spec, node: Node) -> list[str]:
    """Every node the CONTROL graph can reach from ``node`` in one step.

    Single source of truth for graph walks: reachability, fake-waiting and the
    cost estimate must agree about what an edge is, or one of them reasons about
    a graph the others do not have.
    """
    out = [t for t in (node.on_pass, node.on_fail, node.on_dry) if t not in TERMINALS]
    if node.type == "loop" and node.raw.get("body") in spec.nodes:
        out.append(str(node.raw["body"]))
    if node.type == "parallel":
        raw = node.raw.get("branches")
        if isinstance(raw, str):
            # Dynamic fan-out: the clones do not exist yet, but the node they are
            # cloned FROM is reached by this block — without this it reads as an
            # orphan on every dynamic spec.
            template = str(node.raw.get("template", ""))
            if template in spec.nodes:
                out.append(template)
        else:
            out.extend(b for b in (raw or []) if b in spec.nodes)
    # RAW_EDGE_FIELDS é a família inteira (escalate/stable/covered/calibrated):
    # um walker que enumera só parte dela raciocina sobre um grafo que os outros
    # não têm — a deriva por sítio que a tupla nomeada existe para impedir.
    for campo in RAW_EDGE_FIELDS:
        alvo = node.raw.get(campo)
        if alvo in spec.nodes:
            out.append(str(alvo))
    return out


def _lint_fake_waiting(spec: Spec, warnings: list[str]) -> None:
    """A control edge with no data edge, between two provably read-only nodes.

    Isenberg's "delete the fake waiting": B is scheduled after A, never reads A,
    and neither can touch the world — so the sequence buys nothing but latency.

    The `readonly` requirement is the whole safety of this lint, not a nicety.
    Coupling through a side effect (A writes a file B reads) is invisible to the
    interpolator, so flagging it would send someone to parallelise two nodes that
    genuinely depend on each other. Where the proof is missing the lint says
    NOTHING — the same fail-quiet posture `_lint_purpose` takes, for the same
    reason: a lint that cries wolf is a lint people learn to skip.
    """
    flow = flow_dataflow(spec)
    seen: set[tuple[str, str]] = set()
    for node in spec.nodes.values():
        src = flow[node.name]
        if src.readonly is not True:
            continue
        for succ in control_successors(spec, node):
            target = spec.nodes.get(succ)
            if target is None or (node.name, succ) in seen:
                continue
            seen.add((node.name, succ))
            # A gate re-reads the preceding agent through `ctx.last_agent` (the
            # Class-D check), so its dependency is real even with no `{{nodes.}}`
            # reference anywhere in its command.
            if target.type == "gate":
                continue
            dst = flow[succ]
            if dst.readonly is not True or node.name in dst.reads:
                continue
            warnings.append(
                f"fake waiting: `{succ}` runs after `{node.name}` but never reads "
                f"`{{{{nodes.{node.name}.summary}}}}`, and both are read-only — the wait "
                f"buys latency, not order. Consider one `parallel` block."
            )


def _lint_dead_node(spec: Spec, warnings: list[str]) -> None:
    """A node whose output nobody reads.

    The constructive half of "the smallest graph that improves the quality of the
    work": rather than capping how many nodes a flow may have, name the ones
    carrying no weight. Terminals, gates and `human` nodes are exempt — a gate's
    product is the routing decision and a human node's is the approval, neither of
    which travels as an interpolated summary.
    """
    flow = flow_dataflow(spec)
    read_by_someone: set[str] = set()
    for io in flow.values():
        read_by_someone |= io.reads
    # A branch's product is consumed by its block's `merge`, never by an
    # interpolation — the critics in `critic-panel` are read by the tally, and no
    # prompt anywhere quotes `{{nodes.judge.correctness.summary}}`. They escaped
    # this lint only because their `on_pass` happened to be terminal; a branch
    # that routed somewhere would have been called dead weight while doing the
    # block's entire work.
    branch_members: set[str] = set()
    for node in spec.nodes.values():
        if node.type != "parallel":
            continue
        raw = node.raw.get("branches")
        if isinstance(raw, str):
            branch_members.add(str(node.raw.get("template", "")))
        elif isinstance(raw, (list, tuple)):
            branch_members.update(str(b) for b in raw)
    for name, node in spec.nodes.items():
        if name in read_by_someone or name in branch_members or node.type in {"gate", "human"}:
            continue
        # Only a provably read-only node can be judged by whether its summary is
        # read, because for such a node the summary is the ENTIRE product. A node
        # that may write delivers through the filesystem: an `edit` step whose
        # text nobody quotes still did the work, and calling it dead weight would
        # be the same class of error as flagging side-effect coupling as fake
        # waiting. Caught by the shipped library on the first run — `fix` and
        # `recall.memory` both reported, both wrong.
        if flow[name].readonly is not True:
            continue
        # A node that ends the flow IS the output; nothing downstream exists to
        # read it.
        if node.on_pass in TERMINALS:
            continue
        warnings.append(
            f"node `{name}`: nothing downstream reads `{{{{nodes.{name}.summary}}}}` and it does "
            f"not end the flow — either wire its result in, or drop the node"
        )


def _node_invocations(spec: Spec, name: str) -> int:
    """How many times one node can run in a single pass of the flow.

    A loop body runs up to `max_iters`; a dynamic fan-out template is cloned up
    to `max_branches`. Both ceilings are declared, so the number is the spec's
    own promise rather than an estimate.
    """
    for node in spec.nodes.values():
        if node.type == "loop" and str(node.raw.get("body", "")) == name:
            return max(1, int(node.raw.get("max_iters", 1)))
        if node.type == "parallel" and isinstance(node.raw.get("branches"), str) \
                and str(node.raw.get("template", "")) == name:
            return max(1, int(node.raw.get("max_branches", 1)))
    return 1


def flow_cost(spec: Spec) -> dict:
    """What this graph costs at its ceiling (T1.3).

    Isenberg's warning — "more agents don't automatically mean better output …
    sometimes it means five AI workers confidently repeating the same wrong idea"
    — only becomes a decision when the author can see the number. Everything here
    is a declared maximum, never a guess: agent calls at full fan-out, and the
    wall-clock the timeouts already permit.
    """
    tiers: dict[str, int] = {}
    agent_calls = 0
    timeout_s = 0.0
    widest = {"node": None, "branches": 0}
    for name, node in spec.nodes.items():
        runs = _node_invocations(spec, name)
        if node.type == "agent":
            agent_calls += runs
            tier = str(node.raw.get("tier", "unspecified"))
            tiers[tier] = tiers.get(tier, 0) + runs
        timeout_s += runs * int(node.raw.get("timeout_ms", 0)) / 1000.0
        if node.type == "parallel":
            raw = node.raw.get("branches")
            count = (int(node.raw.get("max_branches", 0)) if isinstance(raw, str)
                     else len(raw or []))
            if count > widest["branches"]:
                widest = {"node": name, "branches": count}
    return {
        "nodes": len(spec.nodes),
        "agent_nodes": sum(1 for n in spec.nodes.values() if n.type == "agent"),
        "max_agent_calls": agent_calls,
        "serial_timeout_budget_s": round(timeout_s, 1),
        "calls_by_tier": dict(sorted(tiers.items())),
        "widest_fanout": widest,
    }


def _is_critic(node: Node) -> bool:
    """A node that judges: its persona promises a VERDICT.

    That contract — not a name, not a stance — is what makes the runner treat the
    output as a verdict, so it is the honest signature to match on.
    """
    persona = node.raw.get("persona")
    return isinstance(persona, dict) and "VERDICT=" in str(persona.get("emits", ""))


def _lint_critique_without_brief(spec: Spec, warnings: list[str]) -> None:
    """A critic reachable from entry without anything having produced or approved
    what it judges.

    Jay E's caveat about the gauntlet loop, the part that is easiest to lose:
    *"if you don't start with a really good minimum viable design or product, then
    what the gauntlet loop will do is just optimize towards probably the wrong
    thing … the way I would use them is probably not to start with them as your
    initial prompt"*. A panel is a polishing instrument. Pointed at nothing, it
    spends hours and tokens converging confidently on a direction nobody chose.

    "Produced or approved" is read structurally: some upstream node may write (it
    made the artifact) or is a `human` node (someone signed off). Neither present
    means the flow opens by grading a thing it never established.
    """
    flow = flow_dataflow(spec)
    critics = [n for n in spec.nodes.values() if _is_critic(n)]
    if not critics:
        return
    # Reverse the control graph once; each critic then asks who can reach it.
    parents: dict[str, set[str]] = {name: set() for name in spec.nodes}
    for node in spec.nodes.values():
        for succ in control_successors(spec, node):
            if succ in parents:
                parents[succ].add(node.name)
    for critic in critics:
        seen: set[str] = set()
        stack = list(parents[critic.name])
        grounded = False
        while stack and not grounded:
            cur = stack.pop()
            if cur in seen:
                continue
            seen.add(cur)
            node = spec.nodes.get(cur)
            if node is None:
                continue
            # `parallel`, `loop` and `gate` route; they never produce. Letting a
            # fan-out block ground a critic made this lint silent on exactly the
            # shape it exists to catch — a panel wired straight to the entry —
            # because `node_readonly` cannot prove a control construct read-only
            # and "unprovable" was being read as "may have produced something".
            if node.type == "human":
                grounded = True
                break
            if node.type in {"agent", "code"} and flow[cur].readonly is not True:
                grounded = True
                break
            stack.extend(parents[cur])
        if not grounded:
            warnings.append(
                f"critique without brief: `{critic.name}` judges before anything in this flow "
                f"produced or approved what it judges — a panel is a polishing instrument, and "
                f"pointed at nothing it optimises towards a direction nobody chose"
            )


def _lint_agent_needs_permission(spec: Spec, warnings: list[str]) -> None:
    """An agent that can write, run headless with no permission mode, asks a human
    who is not there — and reports `pass` for having asked.

    Observed, not theorised. `chore-1784540624` invoked a real Claude agent four
    times over three minutes; each invocation returned exit 0 with a verdict of
    `pass` and a summary that reads "Permissão necessária para editar o arquivo.
    Você aprova …?". Nothing was ever written, the `verify` gate rejected each
    attempt, and the run died on `retry_limit_exceeded`. FIVE of the shipped
    library's write-capable agent nodes were in that state — which is to say every
    flow that changes anything.

    `--permission-mode` already existed in `_claude_cmd`; it was simply optional,
    and optional is indistinguishable from absent when nobody is at the terminal.
    """
    for node in spec.nodes.values():
        if node.type != "agent" or str(node.raw.get("driver", "claude")) != "claude":
            continue
        if node_readonly(node) is not False:
            continue  # read-only, or unprovable — nothing to approve
        if node.raw.get("permission_mode"):
            continue
        warnings.append(
            f"agent `{node.name}`: has write tools but no `permission_mode`, so a headless "
            f"run will stop and ask a human who is not there — and report `pass` for asking. "
            f"Set `permission_mode = \"acceptEdits\"` (or another mode) on the node."
        )


def _lint_agent_codegen_tier(spec: Spec, warnings: list[str]) -> None:
    """W8 T8 (2026-08-23) — a codegen agent node without an explicit `tier`
    silently inherits the session's (expensive) default model.

    Measured externally: TanStack runs its entire code mode on Haiku because
    *generating code is an easier task than orchestrating tools* — the model
    writes TS "extremely fast" and the eval (models-eval/results.json) shows
    haiku-4-5 at accuracy 10/10 on the multi-table join. The same economics
    apply to ADW nodes whose whole job is mechanical code generation: the
    warning induces an explicit cost decision, never a silent default.
    """
    CODEGEN_MARKERS = (
        "gere o código", "gerar o código", "escreva o código", "generate code",
        "write the code", "scaffold", "boilerplate", "codegen",
    )
    for node in spec.nodes.values():
        if node.type != "agent" or str(node.raw.get("driver", "claude")) != "claude":
            continue
        if node.raw.get("tier"):
            continue
        prompt = str(node.raw.get("prompt", "")).lower()
        if any(m in prompt for m in CODEGEN_MARKERS):
            warnings.append(
                f"agent `{node.name}`: the prompt reads as mechanical code generation but the "
                f"node declares no `tier`, so it inherits the session's expensive default. "
                f"Codegen runs well on a cheap tier (measured: TanStack code mode on haiku) — "
                f"set `tier = \"haiku\"` (see tiers.toml) or declare the expensive tier on purpose."
            )


def _lint_node(node: Node, names: set[str], errors: list[str], warnings: list[str]) -> None:
    for edge_name, target in (
        ("on_pass", node.on_pass),
        ("on_fail", node.on_fail),
        ("on_dry", node.on_dry),
    ):
        if target not in names and target not in TERMINALS:
            errors.append(f"node `{node.name}`: {edge_name} → unknown node `{target}`")
    # W4 — arestas que vivem só em raw (on_stable/on_covered/on_calibrated);
    # on_escalate mantém sua checagem dedicada no bloco verdict_contract abaixo.
    for edge_name in ("on_stable", "on_covered", "on_calibrated"):
        target = node.raw.get(edge_name)
        if target is not None and target not in names and target not in TERMINALS:
            errors.append(f"node `{node.name}`: {edge_name} → unknown node `{target}`")
    if node.type == "loop":
        body = node.raw.get("body", "")
        if body not in names:
            errors.append(f"loop `{node.name}`: body → unknown node `{body}`")
        if int(node.raw.get("max_iters", 0)) <= 0:
            errors.append(f"loop `{node.name}`: max_iters must be >= 1 (Law L2: runner owns termination)")
        predicate = str(node.raw.get("predicate", "dry"))
        if predicate not in LOOP_PREDICATES:
            errors.append(f"loop `{node.name}`: predicate `{predicate}` desconhecido — "
                          f"use um de {sorted(LOOP_PREDICATES)}")
    if node.type == "control":
        if not node.raw.get("command"):
            errors.append(f"control `{node.name}`: missing command[]")
        for chave in ("good_input", "bad_input"):
            if chave not in node.raw:
                errors.append(f"control `{node.name}`: falta `{chave}` — sem os dois lados "
                              f"o verificador não distingue constante (R4)")
    if node.type == "probe" and not node.raw.get("command"):
        errors.append(f"probe `{node.name}`: missing command[]")
    # F4 28/08/2026 — A14: 3 tentativas de transporte é o teto; a 4ª tem de
    # mudar de ESTRATÉGIA (outra rota, outra decomposição), nunca repetir o
    # mesmo corpo. Erro (não warning): cada retry de agente é custo LLM real.
    if node.type == "agent" and int(node.raw.get("retries", 0)) > 3:
        errors.append(
            f"agent `{node.name}`: retries={node.raw.get('retries')} > 3 — A14: "
            f"após 3 retries de transporte muda-se de estratégia; o retry de "
            f"VEREDITO já existe e mora no gate (feedback verbatim)")
    if node.type in {"code", "gate"}:
        if not node.raw.get("command"):
            errors.append(f"node `{node.name}`: missing command[]")
        if not node.raw.get("idempotent", False):
            warnings.append(f"node `{node.name}`: not marked idempotent — resume replays at-least-once")
    if node.type == "gate" and node.raw.get("verdict_contract", False):
        target = node.raw.get("on_escalate")
        if target is None:
            errors.append(f"gate `{node.name}`: verdict_contract needs `on_escalate` — "
                          f"an escalation with nowhere to go is a rejection wearing a "
                          f"different name, and it will burn the retry budget it exists "
                          f"to protect")
        elif target not in names and target not in TERMINALS:
            errors.append(f"gate `{node.name}`: on_escalate → unknown node `{target}`")
    if node.type == "agent":
        driver = node.raw.get("driver", "claude")
        if driver not in {"claude", "mock"}:
            errors.append(f"agent `{node.name}`: unknown driver `{driver}`")
        if driver == "claude" and not node.raw.get("prompt"):
            errors.append(f"agent `{node.name}`: missing prompt")
        if not node.raw.get("allowed_tools"):
            warnings.append(f"agent `{node.name}`: no allowed_tools — driver runs fail-closed default")
        for skill in skill_binding(node):
            status = skill_status(skill)
            if status == "missing":
                errors.append(f"agent `{node.name}`: skill `{skill}` does not exist "
                              f"(~/.claude/skills/{skill}/SKILL.md) — a bound skill that is "
                              f"not there is a silent no-op at run time, so it fails HERE")
            elif status == "disabled":
                errors.append(f"agent `{node.name}`: skill `{skill}` is `off` in "
                              f"skillOverrides — the agent would call Skill and receive "
                              f"nothing, which reads exactly like success")
        if skill_binding(node) and node.raw.get("readonly") is True:
            warnings.append(f"agent `{node.name}`: declares readonly=true while binding a "
                            f"skill — a skill can do anything the session can, so the "
                            f"declaration is a promise the node cannot keep")


def _lint_reachability(spec: Spec, warnings: list[str]) -> None:
    reachable: set[str] = set()
    stack = [spec.entry]
    while stack:
        cur = stack.pop()
        if cur in reachable or cur in TERMINALS:
            continue
        reachable.add(cur)
        node = spec.nodes.get(cur)
        if node is None:  # dangling edge — already reported as an error
            continue
        stack.extend(control_successors(spec, node))
    for orphan in sorted(set(spec.nodes) - reachable):
        warnings.append(f"node `{orphan}`: unreachable from entry `{spec.entry}` (orphan)")


def _cycle_key(cycle: list[str]) -> tuple[str, ...]:
    """Rotation-invariant identity of a cycle: the same ring discovered from any
    of its members yields one key, so an N-node cycle is reported once, not N times."""
    pivot = min(range(len(cycle)), key=lambda i: cycle[i])
    return tuple(cycle[pivot:] + cycle[:pivot])


def _lint_cycles(spec: Spec, errors: list[str]) -> None:
    """Pass-edge cycle where no member exits and none is a loop node → error.

    Every node is tried as a start point. Until 2026-08-18 this loop ended in a
    `return`, so the scan stopped at the FIRST node that reached any cycle: a
    spec whose first cycle had a legitimate exit passed lint with a second,
    exitless cycle intact — `valid: true`, exit 0, on a graph that cannot
    terminate. Reproduced with `start → {a↔b with exit, x↔y without}`; the only
    thing between that spec and 10.000 node executions was the runner's step guard.
    """
    reported: set[tuple[str, ...]] = set()
    for node in spec.nodes.values():
        seen: list[str] = []
        cur = node.name
        while cur not in TERMINALS and cur not in seen and cur in spec.nodes:
            seen.append(cur)
            cur = spec.nodes[cur].on_pass
        if cur not in seen:
            continue
        cycle = seen[seen.index(cur):]
        key = _cycle_key(cycle)
        if key in reported:
            continue
        reported.add(key)
        has_exit = any(
            spec.nodes[m].on_fail not in cycle or spec.nodes[m].type == "loop"
            for m in cycle
        )
        if not has_exit:
            errors.append(f"cycle without exit: {' → '.join(cycle)}")


BRANCH_BINDINGS = ("value", "index")
BRANCH_REF_RE = re.compile(r"\{\{branch\.([\w.-]+?)\}\}")


def _lint_dynamic_parallel(spec: Spec, node: Node, errors: list[str],
                           warnings: list[str]) -> None:
    """A fan-out whose branch COUNT is decided at runtime (LangGraph's `Send`).

    Static fan-out names its branches, so the lint can check each one. Here the
    list arrives from a var or an upstream node's summary, and only one node —
    the `template` — is cloned per value. That is the shape the gauntlet actually
    has (N critics from runtime input) and the only way to express the canonical
    orchestrator-workers pattern, where the subtasks are not known in advance.
    """
    source = str(node.raw.get("branches", ""))
    if "{{" not in source:
        errors.append(f"parallel `{node.name}`: `branches` is a string with no "
                      f"`{{{{...}}}}` reference — a literal branch list must be a TOML array")
    template_name = str(node.raw.get("template", "")).strip()
    if not template_name:
        errors.append(f"parallel `{node.name}`: dynamic branches need `template` — "
                      f"the node cloned once per runtime value")
        return
    template = spec.nodes.get(template_name)
    if template is None:
        errors.append(f"parallel `{node.name}`: template → unknown node `{template_name}`")
        return
    if template.type in {"parallel", "loop", "human"}:
        errors.append(f"parallel `{node.name}`: template `{template_name}` is a "
                      f"`{template.type}` node — only code/gate/agent may run in a fan-out")
    writes = WRITE_TOOLS.intersection(template.raw.get("allowed_tools") or [])
    if writes:
        errors.append(f"parallel `{node.name}`: template `{template_name}` may use "
                      f"{sorted(writes)} — fan-out is read-only; parallel writers race")
    unknown = sorted({k for k in BRANCH_REF_RE.findall(json.dumps(template.raw))
                      if k not in BRANCH_BINDINGS})
    if unknown:
        errors.append(f"parallel `{node.name}`: template `{template_name}` references "
                      f"{{{{branch.{unknown[0]}}}}} — only {list(BRANCH_BINDINGS)} are bound")
    # The distinctness rule, in its dynamic form. Every branch is a clone of ONE
    # node, so unless the lens is parameterised by the branch value they are N
    # identical critics — the "more agents, more noise" the sources warn about,
    # bought at N times the price.
    persona = template.raw.get("persona") or {}
    if template.type == "agent" and persona:
        lens = str(persona.get("lens", ""))
        if lens and not BRANCH_REF_RE.search(lens):
            errors.append(f"parallel `{node.name}`: template `{template_name}` has a fixed "
                          f"lens `{lens}` — every clone would judge identically; bind it to "
                          f"{{{{branch.value}}}}")


def _lint_parallel(spec: Spec, node: Node, errors: list[str], warnings: list[str]) -> None:
    """Fan-out has four ways to fail silently; each is an error here, not a default.

    (1) No `merge`: branches collapse into one slot and all but one result is
        lost — the last-write-wins a reducer exists to prevent.
    (2) No `on_branch_fail`: a failed branch is indistinguishable from a passing
        one, so the graph advances on partial evidence (Law L2 forbids it).
    (3) A branch that can write: parallel writers race, and no journal ordering
        can reconstruct who clobbered whom.
    (4) No `max_branches`: with a runtime-sized fan-out the cost is a number
        nobody declared, and the budget lint would under-count it by a factor N.
    """
    raw_branches = node.raw.get("branches")
    dynamic = isinstance(raw_branches, str)
    branches = [] if dynamic else list(raw_branches or [])
    branch_count = len(branches)

    if dynamic:
        _lint_dynamic_parallel(spec, node, errors, warnings)
    else:
        if not branches:
            errors.append(f"parallel `{node.name}`: no branches[]")
            return
        duplicates = sorted({b for b in branches if branches.count(b) > 1})
        if duplicates:
            errors.append(f"parallel `{node.name}`: branch(es) {duplicates} listed twice — "
                          f"one node cannot be two branches; its result slot would collide")

    merge = str(node.raw.get("merge", "")).strip()
    if merge not in MERGE_STRATEGIES:
        errors.append(f"parallel `{node.name}`: `merge` must be declared as one of "
                      f"{sorted(MERGE_STRATEGIES)} — without it the branches overwrite "
                      f"one another in a single result slot")
    policy = str(node.raw.get("on_branch_fail", "all")).strip()
    policy_error = branch_fail_policy_error(policy)
    if policy_error:
        errors.append(f"parallel `{node.name}`: `on_branch_fail` {policy_error}")
    # A quorum nobody can reach is a gate that always fails — declared, not silent.
    quorum = QUORUM_POLICY_RE.match(policy)
    if quorum and branch_count and int(quorum.group(1)) > branch_count:
        errors.append(f"parallel `{node.name}`: `on_branch_fail = {policy}` needs more "
                      f"passing branches than the {branch_count} declared — unsatisfiable")
    # `policy` is the cartografia's third field for the same decision. Two fields
    # meaning one thing is how a spec ends up contradicting itself (its own example
    # pairs policy = "quorum:2" with on_branch_fail = "all"), so it is refused with
    # the name that owns the semantics rather than silently ignored.
    if node.raw.get("policy") is not None:
        errors.append(f"parallel `{node.name}`: `policy` is not a field — the pass "
                      f"condition is `on_branch_fail` (all|any|ignore|best_effort|quorum:N)")
    max_branches = int(node.raw.get("max_branches", 0))
    if max_branches <= 0:
        errors.append(f"parallel `{node.name}`: `max_branches` must be >= 1 — an unbounded "
                      f"fan-out multiplies the per-branch cost by a number nobody declared")
    elif branch_count > max_branches:
        errors.append(f"parallel `{node.name}`: {branch_count} branches exceed "
                      f"max_branches={max_branches}")

    for branch in dict.fromkeys(branches):
        target = spec.nodes.get(branch)
        if target is None:
            errors.append(f"parallel `{node.name}`: branch → unknown node `{branch}`")
            continue
        if target.type in {"parallel", "loop", "human"}:
            errors.append(f"parallel `{node.name}`: branch `{branch}` is a `{target.type}` node — "
                          f"only code/gate/agent may run inside a fan-out")
            continue
        writes = WRITE_TOOLS.intersection(target.raw.get("allowed_tools") or [])
        if writes:
            errors.append(f"parallel `{node.name}`: branch `{branch}` may use {sorted(writes)} — "
                          f"fan-out is read-only; parallel writers race")
        if target.type == "agent" and "Bash" in (target.raw.get("allowed_tools") or []):
            warnings.append(f"parallel `{node.name}`: branch `{branch}` has Bash, which can write — "
                            f"the read-only guarantee is yours to keep")
        for edge in ("on_pass", "on_fail"):
            declared = target.raw.get(edge)
            if declared is not None:
                warnings.append(f"parallel `{node.name}`: branch `{branch}` declares {edge}="
                                f"`{declared}`, which is ignored — the join owns the next edge")
    # More agents is not more signal. N reviewers sharing one lens re-derive one
    # opinion N times and charge N times for it; the value of a panel comes from the
    # lenses being DIFFERENT, so a panel that lost that property fails here.
    lenses = [str((spec.nodes[b].raw.get("persona") or {}).get("lens", "")).strip()
              for b in dict.fromkeys(branches)
              if b in spec.nodes and spec.nodes[b].type == "agent"
              and spec.nodes[b].raw.get("persona")]
    if len(lenses) > 1 and len(set(lenses)) == 1:
        errors.append(f"parallel `{node.name}`: all {len(lenses)} critic branches share the "
                      f"lens `{lenses[0]}` — identical critics multiply cost, not coverage")


def _lint_persona(node: Node, errors: list[str], warnings: list[str]) -> None:
    """A persona must be structurally enforceable, not decorative prose.

    The critic rules exist because the two ways a critic silently stops being a
    critic are both invisible in the output: a verdict no code can parse (so the
    graph cannot branch on it), and a resumed session (so it inherits — and
    trusts — the very context it was hired to doubt)."""
    persona = node.raw.get("persona")
    if persona is None:
        return
    if node.type != "agent":
        errors.append(f"node `{node.name}`: [persona] only applies to an agent node (type is `{node.type}`)")
        return
    if not isinstance(persona, dict) or not str(persona.get("role", "")).strip():
        errors.append(f"agent `{node.name}`: [persona] needs a `role` — it becomes `--agent <role>`")
        return
    unknown = set(persona) - PERSONA_FIELDS
    if unknown:
        errors.append(f"agent `{node.name}`: unknown persona field(s) {sorted(unknown)} "
                      f"— want {sorted(PERSONA_FIELDS)}")
    if str(persona.get("stance", "")).strip() != CRITIC_STANCE:
        return
    if not str(persona.get("emits", "")).strip():
        errors.append(f"critic `{node.name}`: stance `{CRITIC_STANCE}` requires `emits` — "
                      f"a verdict no code can parse is prose, not a gate")
    if node.raw.get("session", "fresh") == "resume_on_fail":
        errors.append(f"critic `{node.name}`: `session = \"resume_on_fail\"` contradicts stance "
                      f"`{CRITIC_STANCE}` — a critic that resumes the session it judges "
                      f"inherits the context it exists to distrust")
    if not str(persona.get("bar", "")).strip():
        warnings.append(f"critic `{node.name}`: no `bar` — 'looks good' is not a stopping criterion")
    prompt = str(node.raw.get("prompt", ""))
    for hidden in persona.get("blind_to") or []:
        if f"nodes.{hidden}" in prompt:
            errors.append(f"critic `{node.name}`: declares blindness to `{hidden}` yet its prompt "
                          f"reads `{{{{nodes.{hidden}.summary}}}}` — the blindness is fiction")


def _lint_gate_feedback(spec: Spec, warnings: list[str]) -> None:
    """A gate that hands control back to an agent must tell it WHY it rejected.

    Without the reference the retry re-invokes the agent with a byte-identical
    prompt: same context, same instructions, no verdict — the second attempt is
    blind to what rejected the first, and the only thing that reliably changes
    is the bill. `session = "resume_on_fail"` does NOT mitigate it: the gate is a
    separate process that runs AFTER the agent session ended, so its verdict was
    never in that transcript to begin with.
    """
    for node in spec.nodes.values():
        if node.type != "gate":
            continue
        target = spec.nodes.get(node.on_fail)
        if target is None or target.type != "agent":
            continue
        if f"nodes.{node.name}" in str(target.raw.get("prompt", "")):
            continue
        warnings.append(
            f"gate `{node.name}`: on_fail → agent `{target.name}`, whose prompt never reads "
            f"`{{{{nodes.{node.name}.summary}}}}` — the retry re-runs the agent blind to the "
            f"verdict that rejected it"
        )


#: N5 (2026-09-23): the flow KINDS the lint accepts in `[purpose] class`. The
#: class is what lets System-1 screening enter by flow kind — the measured Jev
#: evidence is per-class (helps fix/review diffs, HURTS create: +61% input
#: tokens, 83% slower on Opus 5 feature work), so routing needs the field.
VALID_PURPOSE_CLASSES = ("fix", "create", "review")


def _lint_purpose(spec: Spec, errors: list[str], warnings: list[str]) -> None:
    """A flow nobody can find by intent is a flow nobody reuses.

    Warning, not error: a private one-off flow may legitimately not care. But
    `when_not_to_use` is called out separately because it is the field that keeps
    a purpose block from becoming advertising — without it the portfolio can only
    pitch the closest candidate, never show that the gap is real. `class` follows
    the same posture: missing warns (an unclassed flow just cannot be routed);
    an INVALID value errors, because it routes to the WRONG screening policy.
    """
    purpose = spec.purpose
    if not purpose:
        # Deliberately silent. A private one-off flow has nothing to be findable
        # FOR, and warning on every such spec would train exactly the reflex
        # `test_a_created_flow_wires_its_own_feedback_...` names: an author who
        # learns to ignore lint output. Presence is enforced where it means
        # something — the shipped library, by test — and demanded by `adw new`.
        return
    if not purpose.get("intent"):
        warnings.append("[purpose]: no `intent` — the one sentence the portfolio indexes")
    if not purpose.get("when_not_to_use"):
        warnings.append(
            "[purpose]: no `when_not_to_use` — without a stated boundary the portfolio "
            "can only recommend this flow, never rule it out")
    cls = purpose.get("class")
    if cls is None:
        warnings.append(
            "[purpose]: no `class` — the flow kind (fix|create|review) is what lets "
            "System-1 screening enter by class: the measured evidence helps "
            "fix/review and HURTS create, so an unclassed flow cannot be routed (N5)")
    elif cls not in VALID_PURPOSE_CLASSES:
        errors.append(
            f"[purpose]: `class = \"{cls}\"` is not one of fix|create|review — a "
            "typo'd class routes the flow to the wrong screening policy")


def _lint_budget(spec: Spec, errors: list[str]) -> None:
    """budget-verify (I6/C11): sum of node budgets must fit the run budget."""
    if spec.budget_tokens <= 0:
        return
    total = sum(int(n.raw.get("budget_tokens", 0)) for n in spec.nodes.values())
    if total > spec.budget_tokens:
        errors.append(f"budget-verify: Σ node budgets ({total}) > run budget ({spec.budget_tokens})")


def _node_marker_text(node: Node) -> str:
    """Prompt + command de um nó, achatado para busca de marcadores de contrato."""
    parts = [str(node.raw.get("prompt", ""))]
    parts.extend(str(c) for c in (node.raw.get("command") or []))
    return " ".join(parts)


def _lint_loop_marker_matches_type(spec: Spec, errors: list[str], warnings: list[str]) -> None:
    """W4 S-4.5(2) — predicado sem o marcador correspondente no corpo = loop cego.

    O runner só lê o que o corpo emite (Lei L2); um predicado cujo marcador não
    aparece no texto do corpo nunca recebe sinal e exaure `max_iters` em
    silêncio. Para `dry` (specs pré-W4) é warning — grandfathered; para os
    predicados novos é erro, porque nasceram com o contrato.
    """
    markers = {"dry": "NEW_FINDINGS=", "fixpoint": "METRIC=", "covered": "COVERAGE="}
    for node in spec.nodes.values():
        if node.type != "loop":
            continue
        predicate = str(node.raw.get("predicate", "dry"))
        body = spec.nodes.get(str(node.raw.get("body", "")))
        if body is None or predicate not in LOOP_PREDICATES:
            continue  # _lint_node já reporta corpo/predicado inválido
        if predicate == "calibrated":
            if body.type != "control":
                errors.append(f"loop `{node.name}`: predicate `calibrated` exige corpo "
                              f"do tipo control (o corpo `{body.name}` é `{body.type}`)")
            continue
        marker = markers[predicate]
        if marker not in _node_marker_text(body):
            msg = (f"loop `{node.name}`: predicate `{predicate}` exige `{marker}<...>` no "
                   f"corpo `{body.name}` — sem o marcador o runner nunca lê o sinal (Lei L2)")
            if predicate == "dry":
                warnings.append(msg)
            else:
                errors.append(msg)


def _lint_verdict_needs_evidence(spec: Spec, warnings: list[str]) -> None:
    """W4 S-4.5(1) — agente que emite VERDICT=/METRIC= deve ler >= 1 nó medidor.

    Um veredito sem leitura de code/probe/control/gate é opinião com marcador:
    o texto certo, nenhum instrumento por trás (instrumento_errado, 38% do
    corpus). code/probe/control/gate SÃO instrumentos — só agentes são cobrados.
    """
    medidores = {"code", "probe", "control", "gate"}
    for node in spec.nodes.values():
        if node.type != "agent":
            continue
        text = _node_marker_text(node)
        if "VERDICT=" not in text and "METRIC=" not in text:
            continue
        reads = node_data_reads(node)
        grounded = any(
            (lido := spec.nodes.get(r)) is not None and lido.type in medidores
            for r in reads
        )
        if not grounded:
            warnings.append(
                f"agente `{node.name}`: emite VERDICT=/METRIC= sem ler nenhum nó "
                f"code/probe/control/gate — veredito sem evidência medida (S-4.5)")


def _lint_gate_has_control(spec: Spec, warnings: list[str]) -> None:
    """W4 S-4.5(3) — verdict_contract sem control no fluxo e sem waiver declarado.

    Um juiz nunca calibrado pode aprovar tudo (`uma-execucao-nao-distingue-
    constante`). A checagem é flow-wide (não upstream estrito) — v1 documentada:
    a presença de UM control no fluxo já prova que o autor calibrou algo; o
    refino por caminho fica para quando a telemetria mostrar que vale.
    """
    has_control = any(n.type == "control" for n in spec.nodes.values())
    for node in spec.nodes.values():
        if node.type != "gate" or not node.raw.get("verdict_contract", False):
            continue
        if has_control or node.raw.get("control_waived"):
            continue
        warnings.append(
            f"gate `{node.name}`: verdict_contract sem nó control no fluxo e sem "
            f"`control_waived = \"razão\"` — calibre o juiz ou declare por que não (S-4.5)")


def _lint_readonly_without_sandbox(spec: Spec, warnings: list[str]) -> None:
    """F3 ADW 28/08/2026 — o dual do S-8.4: um nó `code`/`gate` PROVADAMENTE
    leitor (`command_writes()==False` ou `readonly = true` declarado) rodando
    FORA do sandbox paga zero para ser contido — `sandbox = true` roteia pelo
    CEG (Landlock FS + rlimit + seccomp-UDP) sem mudar a semântica de quem só
    lê. Medido 28/08: 16/66 nós usavam sandbox; a afordância existia e nada a
    apontava (adoption does not emerge from availability — 4 Pilares). Warning,
    nunca erro: quem escreve fica de fora (o custo lá é real) e a decisão
    permanece visível no spec."""
    for node in spec.nodes.values():
        if node.type not in {"code", "gate"}:
            continue
        if node.raw.get("sandbox", False):
            continue
        command = node.raw.get("command")
        provado_leitor = (
            node.raw.get("readonly") is True or command_writes(command) is False
        )
        if not provado_leitor:
            continue
        # Precedência (28/08/2026): um leitor que consome TEXTO DE AGENTE nos
        # posicionais NÃO recebe o convite — lá o sandbox é o defeito (o X6
        # classifica o programa inteiro e prosa vira deny não-determinístico;
        # ver _lint_sandboxed_gate_reads_agent_text). Dois lints em guerra
        # ensinariam a oscilar entre dois estados errados.
        if _command_agent_text_refs(node, _agent_text_sources(spec)):
            continue
        # Calibração: um nó puro-stdout (`echo done`, `true`) não toca nada —
        # contê-lo não compra contenção, e avisar ali é ruído com cara de
        # rigor (a mesma régua do `-o` em WRITING_FLAGS). O aviso exige um
        # leitor SUBSTANTIVO: toca FS/dados ou invoca um programa opaco.
        tokens = _command_tokens(command)
        substantivo = any(
            t in _FS_READING_COMMANDS
            or t in PROGRAM_MULTIPLEXERS
            or t.endswith(PROGRAM_SUFFIXES)
            for t in tokens
        )
        if substantivo:
            warnings.append(
                f"nó `{node.name}`: leitura provada rodando fora do sandbox — "
                f"ligue `sandbox = true` no [node.{node.name}]: contenção de "
                f"kernel (Landlock/rlimit/seccomp) de graça para um leitor.")


def _lint_unsandboxed_writer_wants_human(spec: Spec, warnings: list[str]) -> None:
    """W8 S-8.4 — a lacuna que a fonte TanStack nomeia, no lado ADW: um nó que
    ESCREVE (command_writes()==True) sem sandbox, num fluxo sem nenhum gate
    humano, executa mutação arbitrária sem aprovação. Warning (nunca erro):
    fluxos maduros declaram a intenção; o lint induz a decisão explícita."""
    tem_humano = any(n.type == "human" for n in spec.nodes.values())
    if tem_humano:
        return
    for node in spec.nodes.values():
        if node.type not in {"code", "gate"}:
            continue
        if node.raw.get("sandbox", False):
            continue
        if command_writes(node.raw.get("command")) is True:
            warnings.append(
                f"nó `{node.name}`: escreve (command_writes) sem sandbox e o fluxo não "
                f"tem gate humano — a tool de mutação arbitrária era a única sem "
                f"aprovação (lacuna TanStack, S-8.4). Adicione um nó `human` upstream, "
                f"ligue `sandbox = true`, ou declare a intenção no header.")


_AGENT_TEXT_REF_RE = re.compile(r"\{\{nodes\.([\w.-]+)\.(?:summary|full_ref)\}\}")


def _agent_text_sources(spec: Spec) -> set[str]:
    """Nós cujo output é TEXTO DE AGENTE: os `agent` diretos e todo `parallel`
    cujo template ou branches são agents (o merge entrega a mesma prosa)."""
    sources = {n.name for n in spec.nodes.values() if n.type == "agent"}
    for n in spec.nodes.values():
        if n.type != "parallel":
            continue
        template = n.raw.get("template")
        template_is_agent = (
            (isinstance(template, dict) and template.get("type") == "agent")
            or (isinstance(template, str) and template in sources)
        )
        branches = n.raw.get("branches")
        branch_names = ([b for b in branches if isinstance(b, str)]
                        if isinstance(branches, list) else [])
        if template_is_agent or any(b in sources for b in branch_names):
            sources.add(n.name)
    return sources


def _command_agent_text_refs(node: Node, sources: set[str]) -> set[str]:
    refs: set[str] = set()
    for part in node.raw.get("command") or []:
        refs.update(_AGENT_TEXT_REF_RE.findall(str(part)))
    return refs & sources


def _lint_sandboxed_gate_reads_agent_text(spec: Spec, warnings: list[str]) -> None:
    """Estreia do cross-audit 28/08/2026 — um nó `sandbox = true` cujo command
    interpola texto de AGENTE (`{{nodes.X.summary}}`/`full_ref` de um nó agent)
    manda esse texto ao classificador do CEG como parte do programa: prosa com
    cara de comando vira deny não-determinístico ("X6 denied the subprocess
    capability 'bash'", 3 gates seguidos — e o retry recebia feedback VAZIO,
    então o agente degradava sem saber por quê). Predicado de string é leitor
    puro e os posicionais já bloqueiam injection — o remédio é sair do sandbox."""
    sources = _agent_text_sources(spec)
    if not sources:
        return
    for node in spec.nodes.values():
        if node.type not in {"code", "gate"} or not node.raw.get("sandbox", False):
            continue
        toca_agente = _command_agent_text_refs(node, sources)
        if toca_agente:
            warnings.append(
                f"nó `{node.name}`: sandbox = true com texto de agente no command "
                f"({', '.join(sorted(toca_agente))}) — o X6 classifica o programa "
                f"INTEIRO (args inclusos) e prosa com cara de comando vira deny "
                f"não-determinístico (estreia cross-audit 28/08). Remova o sandbox: "
                f"predicado de string é leitor puro e posicionais já bloqueiam "
                f"injection.")


def _lint_sweep_declares_floor(spec: Spec, errors: list[str]) -> None:
    """W4 S-4.5(4) — `covered` exige `min_discovered`: sem piso, a descoberta
    vazia (COVERAGE=0/0) leria como família completa — o 0/0 do auditor."""
    for node in spec.nodes.values():
        if (node.type == "loop"
                and str(node.raw.get("predicate", "dry")) == "covered"
                and "min_discovered" not in node.raw):
            errors.append(
                f"loop `{node.name}`: predicate `covered` exige `min_discovered` — "
                f"sem piso declarado COVERAGE=0/0 contaria como cobertura completa")


def lint_spec(spec: Spec) -> tuple[list[str], list[str]]:
    """Return (errors, warnings). Errors make `adw lint` exit non-zero."""
    errors: list[str] = []
    warnings: list[str] = []
    names = set(spec.nodes)
    for node in spec.nodes.values():
        _lint_node(node, names, errors, warnings)
        _lint_persona(node, errors, warnings)
        _lint_readonly_claim(node, errors, warnings)
        if node.type == "parallel":
            _lint_parallel(spec, node, errors, warnings)
    _lint_reachability(spec, warnings)
    _lint_cycles(spec, errors)
    _lint_gate_feedback(spec, warnings)
    _lint_purpose(spec, errors, warnings)
    _lint_budget(spec, errors)
    _lint_fake_waiting(spec, warnings)
    _lint_dead_node(spec, warnings)
    _lint_critique_without_brief(spec, warnings)
    _lint_agent_needs_permission(spec, warnings)
    _lint_agent_codegen_tier(spec, warnings)
    # W4 S-4.5 — os 4 lints da honestidade (plano code-mode-total)
    _lint_loop_marker_matches_type(spec, errors, warnings)
    _lint_verdict_needs_evidence(spec, warnings)
    _lint_gate_has_control(spec, warnings)
    _lint_sweep_declares_floor(spec, errors)
    _lint_unsandboxed_writer_wants_human(spec, warnings)
    # F3 ADW 28/08 — o dual: leitor provado fora do sandbox ganha o remédio.
    _lint_readonly_without_sandbox(spec, warnings)
    # Potencialização skills 28/08 — texto de agente nunca entra no sandbox.
    _lint_sandboxed_gate_reads_agent_text(spec, warnings)
    return errors, warnings


# ── journal (durable, append-only, fsync'd) ───────────────────────────────────


class Journal:
    """Append-only run journal; the single source of truth for resume replay."""

    def __init__(self, run_path: Path):
        self.path = run_path / "journal.jsonl"
        self._seq = 0
        self.events: list[dict] = []
        if self.path.is_file():
            for line in self.path.read_text(encoding="utf-8").splitlines():
                if line.strip():
                    self.events.append(json.loads(line))
            self._seq = len(self.events)

    def append(self, event: str, **fields) -> dict:
        rec = {"seq": self._seq, "ts": time.time(), "event": event, **fields}
        self._seq += 1
        self.events.append(rec)
        with self.path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
            fh.flush()
            os.fsync(fh.fileno())
        return rec

    def completed_execs(self) -> dict[str, dict]:
        """exec_key → node_completed event (for replay-skip on resume)."""
        return {e["exec_key"]: e for e in self.events if e["event"] == "node_completed"}

    def session_id_for(self, node: str) -> str | None:
        for e in reversed(self.events):
            if e.get("node") == node and e.get("session_id"):
                return e["session_id"]
        return None


# ── results store (Law L4) ────────────────────────────────────────────────────


def store_result(run_path: Path, exec_key: str, output: str) -> dict:
    """Persist {summary, omitted_bytes, full_ref} for one node execution."""
    raw = output.encode("utf-8", errors="replace")
    full_ref = ""
    omitted = 0
    if len(raw) > SUMMARY_LIMIT:
        full = run_path / f"{exec_key}.out"
        full.write_bytes(raw)
        full_ref = str(full)
        omitted = len(raw) - SUMMARY_LIMIT
    artifact = {
        "summary": raw[:SUMMARY_LIMIT].decode("utf-8", errors="replace"),
        "omitted_bytes": omitted,
        "full_ref": full_ref,
    }
    (run_path / f"{exec_key}.json").write_text(
        json.dumps(artifact, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    return artifact


#: The three fields Law L4 promises as inter-node context. Until 2026-08-20 the resolver
#: silently DROPPED the suffix and always returned `summary`, so `{{nodes.X.full_ref}}`
#: rendered the truncated summary instead of the path to the whole output — two thirds of
#: the contract were unreachable from any template.
#:
#: Measured cost: `critic-panel`'s quorum counts `^VERDICT=` over `{{nodes.panel.summary}}`,
#: and a critic emits its verdict at the END of its report. With SUMMARY_LIMIT at 2000
#: bytes and 14 718 omitted, the quorum saw ZERO verdicts and escalated — while all three
#: critics had returned REJECT. A panel that can never count is not a panel.
RESULT_FIELDS = ("summary", "full_ref", "omitted_bytes")


def render_template(text: str, results: dict[str, dict], variables: dict[str, str]) -> str:
    """Resolve {{nodes.X.<field>}} / {{vars.k}} references (inter-node context).

    `<field>` is one of :data:`RESULT_FIELDS`; omitted, it is `summary` — the dense
    inline default of Law L4. An unknown suffix is part of the node NAME, not a field,
    so a node called `a.b` keeps resolving as before.
    """

    def sub(match: re.Match) -> str:
        kind, key = match.group(1), match.group(2)
        if kind != "nodes":
            return variables.get(key, "")
        if ".facts." in key:
            # W4 S-4.4 — {{nodes.X.facts.chave}}: o fato de um probe, endereçável
            # a jusante (E3 ganha o endereço que faltava).
            node_key, fact_key = key.split(".facts.", 1)
            return str(results.get(node_key, {}).get("facts", {}).get(fact_key, ""))
        campo = "summary"
        for f in RESULT_FIELDS:
            if key.endswith(f".{f}"):
                key, campo = key[: -len(f) - 1], f
                break
        return str(results.get(key, {}).get(campo, ""))

    return re.sub(r"\{\{(nodes|vars)\.([\w.-]+?)\}\}", sub, text)


# ── activity mirror (best-effort) ─────────────────────────────────────────────


def activity_append(action: str, payload: dict) -> None:
    try:
        subprocess.run(
            ["touring", "activity", "append", action, "--actor", "adw-runner",
             "--payload", json.dumps(payload, ensure_ascii=False)],
            capture_output=True, timeout=ACTIVITY_TIMEOUT_S, check=False,
        )
    except Exception:
        pass  # mirror only; the journal is the source of truth


# ── node executors ────────────────────────────────────────────────────────────


@dataclass
class ExecResult:
    exit_code: int
    output: str
    session_id: str | None = None
    narrated_success: bool = False
    cost_usd: float = 0.0
    #: `adw test` walked this node with no recording to replay. Reported by name,
    #: never silent: a synthesized walk is not evidence that the agent behaves.
    synthesized: bool = False


def parse_verdict(output: str, exit_code: int) -> str:
    """Read the gate's declared verdict; fall back to the exit code, then to REJECT.

    A gate under contract speaks the three-verdict vocabulary. One that does not
    still works: a clean exit is PASS, anything else REJECT. What never happens is
    a missing signal reading as success."""
    found = VERDICT_RE.findall(output or "")
    if found:
        return found[-1]
    return PASS if exit_code == 0 else DEFAULT_VERDICT


def _unwrap_sandbox_output(raw_stdout: str, raw_stderr: str) -> str:
    """Devolve o stdout/stderr do PROGRAMA, não o envelope do `touring run`.

    Sob `sandbox = true` o comando do nó vira `touring run --code ...`, cuja
    saída é um JSON. Isso quebrava, em silêncio, os três contratos de marcador
    do runner: `NEW_FINDINGS_RE`, `METRIC_RE` e `VERDICT_RE` casam
    `^MARCADOR=` com MULTILINE, e dentro do JSON a linha vira
    `  "stdout": "NEW_FINDINGS=5\n",` — nenhum casa. O efeito seria pior que um
    erro: um loop nunca contaria achados (marcador ausente é *unknown*, então
    ele exauriria `max_iters` em vez de convergir) e todo gate sob contrato
    leria "unparseable" = REJECT. Nenhuma spec usava sandbox, então nada disso
    tinha sido exercido (medido 24/08/2026: 0 de 24 nós `code`).

    O `retrieval_hint` da W1 é anexado quando houve spill: o nó passa a saber
    onde está a saída completa, em vez de recebê-la cortada — que é justamente
    o ganho que o `head -c` das specs jogava fora.

    Fail-open: envelope ilegível devolve o texto cru, porque perder a saída de
    um nó é pior do que exibi-la com ruído.
    """
    try:
        payload = json.loads(raw_stdout)
    except (json.JSONDecodeError, TypeError):
        return raw_stdout + (("\n" + raw_stderr) if raw_stderr.strip() else "")
    if not isinstance(payload, dict):
        return raw_stdout
    stdout = payload.get("stdout", "") or ""
    stderr = payload.get("stderr", "") or ""
    partes = [stdout]
    if stderr.strip():
        partes.append(stderr)
    hint = payload.get("retrieval_hint")
    if hint:
        partes.append(f"[adw] saída completa: {hint}")
    return "\n".join(p for p in partes if p)


def run_code_node(node: Node, results: dict, variables: dict,
                  cwd: Path | None = None) -> ExecResult:
    command = [render_template(str(part), results, variables) for part in node.raw["command"]]
    node_timeout_ms = int(node.raw.get("timeout_ms", DEFAULT_TIMEOUT_MS))
    sandbox_note = ""
    if node.raw.get("sandbox", False):
        # O `touring run` tem orçamento PRÓPRIO: 30s de default e 600s de teto
        # (QW-3, 28/08/2026). Sem propagar o timeout do nó, um nó que declara
        # 300_000 abortava em 30s ao ligar `sandbox = true` — a afordância
        # existia mas quebrava quem a usasse (0/24 nós adotavam, medido 24/08).
        sandbox_ms = min(node_timeout_ms, SANDBOX_MAX_TIMEOUT_MS)
        if node_timeout_ms > SANDBOX_MAX_TIMEOUT_MS:
            # Nunca silenciosamente: o nó pediu mais do que o sandbox concede, e
            # quem lê o resultado precisa saber por que ele pode expirar antes.
            sandbox_note = (
                f"[adw] sandbox: timeout do nó {node_timeout_ms}ms excede o teto do "
                f"`touring run` ({SANDBOX_MAX_TIMEOUT_MS}ms) — limitado ao teto\n"
            )
        command = [
            "touring", "run", "--lang", "bash",
            "--timeout-ms", str(sandbox_ms),
            "--code", " ".join(shlex.quote(c) for c in command),
        ]
    timeout_s = node_timeout_ms / 1000
    retries = int(node.raw.get("retries", 0))
    attempt = 0
    while True:
        try:
            proc = subprocess.run(command, capture_output=True, text=True,
                                  timeout=timeout_s, cwd=cwd)
            if node.raw.get("sandbox", False):
                output = sandbox_note + _unwrap_sandbox_output(proc.stdout, proc.stderr)
            else:
                output = proc.stdout + (("\n" + proc.stderr) if proc.stderr.strip() else "")
            result = ExecResult(exit_code=proc.returncode, output=output)
        except subprocess.TimeoutExpired:
            result = ExecResult(exit_code=124, output=f"timeout after {timeout_s:.0f}s")
        if result.exit_code == 0 or attempt >= retries:
            return result
        attempt += 1
        time.sleep(min(2 ** attempt, 10))


def run_probe_node(node: Node, results: dict, variables: dict,
                   cwd: Path | None = None) -> ExecResult:
    """W4 S-4.4 — probe: um nó code cujo contrato é >= 1 linha FACT=<k>=<v>.

    O fato ganha endereço: `attach_probe_facts` grava {fact, run_id, exec_key}
    no journal e expõe `{{nodes.X.facts.chave}}` à interpolação. Sem FACT o nó
    FALHA — contrato, não cortesia: um probe silencioso é o mesmo buraco que o
    loop fail-closed fecha para NEW_FINDINGS.
    """
    result = run_code_node(node, results, variables, cwd=cwd)
    if result.exit_code == 0 and not FACT_RE.search(result.output or ""):
        return ExecResult(
            exit_code=1,
            output=(result.output or "")
            + "\n[adw] probe sem FACT=<chave>=<valor> — o contrato exige ao menos um fato",
        )
    return result


def attach_probe_facts(results: dict, node_name: str, output: str, journal: Journal,
                       run_id: str, exec_key: str) -> None:
    """W4 S-4.4 — FACT= vira dict endereçável + registro no journal com run_id."""
    facts = {m.group(1): m.group(2) for m in FACT_RE.finditer(output or "")}
    entry = results.get(node_name)
    if isinstance(entry, dict):
        entry["facts"] = facts
    journal.append("probe_facts", node=node_name, exec_key=exec_key,
                   run_id=run_id, facts=facts)


def run_control_node(node: Node, results: dict, variables: dict,
                     cwd: Path | None = None) -> ExecResult:
    """W4 S-4.3 — R4 como nó: o verificador aprova o bom E reprova o ruim.

    Um verificador que aprova tudo não sabe reprovar — a constante 0.990 do
    `predict-action` (memória `uma-execucao-nao-distingue-constante`). O runner
    roda `command` duas vezes, substituindo `{input}` por `good_input` e por
    `bad_input`; o nó passa SÓ quando o bom passa E o ruim falha, e o RUNNER
    (nunca o corpo) emite `CONTROL=pass|fail` — o marcador que o predicado
    `calibrated` lê. Sem sandbox na v1 (documentado): o verificador é o comando
    do próprio fluxo, já sujeito ao lint `_lint_readonly_claim`.
    """
    timeout_s = int(node.raw.get("timeout_ms", DEFAULT_TIMEOUT_MS)) / 1000
    lines: list[str] = []
    ok_all = True
    for label, chave, want_pass in (("good", "good_input", True),
                                    ("bad", "bad_input", False)):
        # good/bad_input também passam pelo template: num fluxo composto eles
        # chegam como "{{vars.good}}" — sem render, o verificador testava o
        # literal "{{vars.good}}" e reprovava os DOIS lados (medido 24/08 no
        # exercício real do instrument-first; os testes com literais não viam).
        valor = render_template(str(node.raw.get(chave, "")), results, variables)
        cmd = [render_template(str(p), results, variables).replace("{input}", valor)
               for p in node.raw["command"]]
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True,
                                  timeout=timeout_s, cwd=cwd)
            code = proc.returncode
            out = (proc.stdout + proc.stderr)[-400:]
        except subprocess.TimeoutExpired:
            code, out = 124, f"timeout after {timeout_s:.0f}s"
        ok = (code == 0) == want_pass
        ok_all = ok_all and ok
        lines.append(f"[control:{label}] exit={code} esperava_passar={want_pass} -> "
                     f"{'ok' if ok else 'VIOLACAO'}")
        if not ok:
            lines.append(f"[control:{label}] saida: {out.strip()[:200]}")
    lines.append(f"CONTROL={'pass' if ok_all else 'fail'}")
    return ExecResult(exit_code=0 if ok_all else 1, output="\n".join(lines))


def mock_recording_path(spec: Spec, node: str) -> Path:
    return spec.path.parent / f"{spec.name}.recordings" / f"{node}.json"


def _synthesized_mock(node: Node) -> ExecResult:
    """Stand in for an agent with no recording so `adw test` can walk the graph.

    `adw test` exists to prove the GRAPH holds: every edge resolves, every gate
    parses what the step before it emits. A flow that never ran has no
    recordings, so demanding one turned that graph check into an agent check and
    failed every freshly created flow — `adw new` promises a flow born
    lint-clean and testable, and that promise could not be kept.

    The stub is reported by name (`synthesized`), because a synthesized walk must
    never read as a replayed one — the same fail-closed reading the verdict
    contract gives to silence. The text avoids every success word deliberately: a
    stub that narrated success would forge the Class-D divergence Law L3 exists
    to detect.
    """
    emits = str((node.raw.get("persona") or {}).get("emits", ""))
    lines = [f"SYNTHESIZED MOCK for agent `{node.name}` — no recording to replay; "
             f"the graph was walked, this agent's behaviour was NOT exercised."]
    if "VERDICT=" in emits:
        # The node's own contract says a downstream gate parses a verdict here.
        # Emitting nothing would stall every panel behind an ESCALATE and hide
        # the rest of the graph — the part `adw test` is here to check.
        lines += ["VERDICT=PASS", "REASON=synthesized stub, nothing was replayed"]
    return ExecResult(exit_code=0, output="\n".join(lines), synthesized=True)


def _agent_mock(spec: Spec, node: Node, synthesize: bool = False) -> ExecResult:
    rec_path = mock_recording_path(spec, node.name)
    if not rec_path.is_file():
        if not synthesize:
            return ExecResult(exit_code=1, output=f"no recording for agent `{node.name}` at {rec_path}")
        return _synthesized_mock(node)
    rec = json.loads(rec_path.read_text(encoding="utf-8"))
    output = rec.get("result", "")
    return ExecResult(
        exit_code=int(rec.get("exit_code", 0)),
        output=output,
        session_id=rec.get("session_id"),
        narrated_success=bool(SUCCESS_NARRATIVE.search(output)),
    )


def _claude_cmd(node: Node, journal: Journal, prompt: str,
                results: dict | None = None, variables: dict | None = None) -> list[str]:
    """Assemble the fail-closed headless invocation from the node spec."""
    cmd = ["claude", "-p", skill_prompt_prefix(node) + prompt, "--output-format", "json"]
    persona = node.raw.get("persona")
    if persona:
        role, definition = compile_agent_definition(persona, results or {}, variables or {})
        cmd += ["--agents", json.dumps({role: definition}, ensure_ascii=False), "--agent", role]
    tier = node.raw.get("tier", "")
    if tier:
        cmd += ["--model", tier_models().get(tier, tier)]
    tools = skill_tools(node)
    if tools:
        cmd += ["--allowedTools", *tools]
    budget_usd = node.raw.get("budget_usd")
    if budget_usd:
        cmd += ["--max-budget-usd", str(budget_usd)]
    permission_mode = node.raw.get("permission_mode")
    if permission_mode:
        cmd += ["--permission-mode", str(permission_mode)]
    for extra_dir in node.raw.get("add_dirs") or []:
        cmd += ["--add-dir", str(extra_dir)]
    if node.raw.get("session", "fresh") == "resume_on_fail":
        prev = journal.session_id_for(node.name)
        if prev:
            cmd += ["--resume", prev]
    return cmd


def _agent_claude(spec: Spec, node: Node, journal: Journal, prompt: str, record: bool,
                  cwd: Path | None = None, results: dict | None = None,
                  variables: dict | None = None) -> ExecResult:
    cmd = _claude_cmd(node, journal, prompt, results, variables)
    timeout_s = int(node.raw.get("timeout_ms", 600_000)) / 1000
    # The headless subagent inherits the session's hooks, so loop_outer_arm
    # (UserPromptSubmit) read the node's imperative prompt as substantive work
    # and armed the `work-outer` flow INSIDE the agent: its Stop hook then
    # demanded the OUTER artifacts and the final answer became guard narrative,
    # never the FACT=/VERDICT= contract (exercise-idle-infra debut 28/08/2026:
    # 4 real attempts answered the guard; run exercise-idle-infra-1787920381).
    # An ADW node is already gated by its OWN flow's gate (Law L2) — the
    # documented kill switch disarms the redundant guard at the executor (D8).
    env = {**os.environ, "TOURING_WORK_OUTER_DISABLED": "1"}
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout_s,
                              cwd=cwd, env=env)
    except subprocess.TimeoutExpired:
        return ExecResult(exit_code=124, output=f"agent timeout after {timeout_s:.0f}s")
    session_id = None
    output = proc.stdout
    cost = 0.0
    try:
        parsed = json.loads(proc.stdout)
        session_id = parsed.get("session_id")
        output = parsed.get("result", proc.stdout)
        cost = float(parsed.get("total_cost_usd") or 0.0)
    except (json.JSONDecodeError, AttributeError, TypeError, ValueError):
        pass
    result = ExecResult(
        exit_code=proc.returncode,
        output=output,
        session_id=session_id,
        narrated_success=bool(SUCCESS_NARRATIVE.search(output or "")),
        cost_usd=cost,
    )
    if record and result.exit_code == 0:
        rec_path = mock_recording_path(spec, node.name)
        rec_path.parent.mkdir(parents=True, exist_ok=True)
        rec_path.write_text(json.dumps(
            {"result": output, "exit_code": result.exit_code, "session_id": session_id},
            ensure_ascii=False, indent=1), encoding="utf-8")
    return result


def run_agent_node(
    node: Node, spec: Spec, journal: Journal, results: dict, variables: dict,
    force_mock: bool, record: bool, cwd: Path | None = None,
    feedback: str | None = None,
) -> ExecResult:
    driver = "mock" if force_mock else node.raw.get("driver", "claude")
    prompt = render_template(node.raw.get("prompt", ""), results, variables)
    if feedback:
        # W4 S-4.6 — retry sem o porquê é a definição de retry cego (memória
        # `adw-retry-sem-feedback-do-gate`): o veredito/saída do gate REPROVADOR
        # entra verbatim no prompt do retry.
        prompt = f"{prompt}\n\n[gate feedback]\n{feedback}"
    if driver == "mock":
        # Synthesis belongs to `adw test` alone. A spec that DECLARES
        # driver = "mock" is asking for one specific recording, and inventing it
        # would turn an explicit contract into a guess.
        return _agent_mock(spec, node, synthesize=force_mock)
    # F4 28/08/2026 — retry de TRANSPORTE (paridade com o nó `code`): exit != 0
    # do `claude -p` é infra (timeout 124, rede, CLI morto), nunca veredito —
    # veredito ruim chega com exit 0 e quem o julga é o GATE (com feedback).
    # Cap 3 imposto pelo lint (A14): na 4ª muda-se de estratégia, jamais
    # re-roda o mesmo corpo. Cada tentativa fica no journal — custo visível.
    retries = int(node.raw.get("retries", 0))
    attempt = 0
    while True:
        result = _agent_claude(spec, node, journal, prompt, record, cwd=cwd,
                               results=results, variables=variables)
        if result.exit_code == 0 or attempt >= retries:
            return result
        attempt += 1
        journal.append("agent_transport_retry", node=node.name, attempt=attempt,
                       exit_code=result.exit_code)
        time.sleep(min(2 ** attempt, 10))


# ── runner ────────────────────────────────────────────────────────────────────


@dataclass
class RunOutcome:
    status: str  # completed | failed | waiting_human
    run_id: str
    steps: list[dict] = field(default_factory=list)
    class_d: bool = False
    #: Nodes walked without a recording. A synthesized walk proves the graph
    #: connects — never that the agent behaves — so it is always named.
    synthesized: list[str] = field(default_factory=list)


def new_run_id(name: str) -> str:
    """Collision-proof run id: ``<adw>-<epoch>-<session8>.<pid>``.

    The epoch alone has 1-second granularity, so two Claude Code sessions
    starting the same ADW in the same project within the same second derived the
    SAME run dir: the loser died on the flock, and had it taken the lock later it
    would have replayed the OTHER session's journal as its own resume state.
    Negligible while ADWs were hand-invoked; material now that the deterministic
    OUTER is the default and every session fires `strategy-loop`. The session
    prefix keeps the dir attributable; the pid guarantees uniqueness even for two
    runs of one session inside the same second.
    """
    session = (os.environ.get("CLAUDE_CODE_SESSION_ID")
               or os.environ.get("TOURING_SESSION_ID") or "local")
    return f"{name}-{int(time.time())}-{str(session)[:8]}.{os.getpid()}"


def acquire_lock(run_path: Path):
    """flock the run dir; a dead holder is reclaimed (fail-open on dead state)."""
    lock = (run_path / "run.lock").open("w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        raise SystemExit(f"run dir is locked by a live runner: {run_path}")
    lock.write(str(os.getpid()))
    lock.flush()
    return lock  # keep the handle alive for the run's duration


def _resume_state(
    journal: Journal, run_path: Path, resuming: bool,
) -> tuple[dict[str, dict], dict[str, dict], dict[str, int]]:
    """Rebuild (completed, results, visits) from the journal for durable replay."""
    completed = journal.completed_execs() if resuming else {}
    results: dict[str, dict] = {}
    for exec_key in completed:
        artifact_path = run_path / f"{exec_key}.json"
        if artifact_path.is_file():
            results[exec_key.split("#")[0]] = json.loads(artifact_path.read_text(encoding="utf-8"))
    # Visits start at zero even on resume: replaying the graph from the entry
    # re-derives the SAME exec_keys deterministically (Temporal-style replay),
    # so completed execs skip and the interrupted one re-runs.
    visits: dict[str, int] = {}
    return completed, results, visits


#: Verdict of a finished run, as the arm's reward. `failed` is 0.0, not the
#: 0.2 the FACTORY router uses for the same word: routing to an ADW that failed
#: is a partial loss for the router's choice, while the run itself simply did
#: not do what it set out to do.
RUN_OUTCOME_REWARD = {"completed": 1.0, "failed": 0.0, "aborted": 0.0}


def deposit_run_outcome(spec_name: str, outcome: "RunOutcome") -> None:
    """Feed a finished run back into the learning substrate. Fail-open, always.

    Until 2026-08-25 this did not exist: `adw.py` had exactly ONE reward call
    site in 3.823 lines, on the `campaign` path, so 35 recorded runs produced a
    fsync'd journal and taught the system nothing. Two deposits, because a run
    generates two distinct signals:

    * the ARM — was running this flow worth it (`adw:run:<spec>`);
    * the CASES — the memories its agents recalled along the way are now known
      to have informed work with a measured verdict (Memento Eq. 9). The run
      knows its verdict but not its questions, so it drains rather than naming
      them.

    Never raises and never blocks the run: a learning deposit that could fail a
    run would make the system worse at the exact moment it tries to improve.
    """
    value = RUN_OUTCOME_REWARD.get(outcome.status)
    if value is None:  # waiting_human and any future status: the run is not over
        return
    for cmd in (
        ["touring", "learning", "reward", f"adw:run:{spec_name}", f"{value:.3f}"],
        ["touring", "memory", "credit", "--all-pending", "--reward", f"{value:.3f}"],
    ):
        try:
            subprocess.run(cmd, capture_output=True, timeout=30, check=False)
        except (subprocess.TimeoutExpired, OSError):
            pass


def execute(
    spec: Spec, root: Path, resume_run: str | None = None,
    approve: set[str] | None = None, force_mock: bool = False,
    record: bool = False, variables: dict[str, str] | None = None,
) -> RunOutcome:
    approve = approve or set()
    variables = variables or {}
    run_id = resume_run or new_run_id(spec.name)
    run_path = runs_dir(root) / run_id
    run_path.mkdir(parents=True, exist_ok=True)
    lock = acquire_lock(run_path)
    journal = Journal(run_path)
    outcome = RunOutcome(status="failed", run_id=run_id)
    completed, results, visits = _resume_state(journal, run_path, bool(resume_run))

    if not resume_run:
        journal.append("run_started", adw=spec.name, run_id=run_id)
    activity_append("task_started", {"adw": spec.name, "run": run_id, "resume": bool(resume_run)})

    ctx = _RunCtx(spec=spec, journal=journal, run_path=run_path, results=results,
                  variables=variables, approve=approve, force_mock=force_mock,
                  record=record, run_id=run_id, outcome=outcome, visits=visits,
                  completed=completed, root=root)
    current = spec.entry
    guard = 0
    try:
        while current not in TERMINALS:
            guard += 1
            if guard > 10_000:
                journal.append("run_aborted", reason="step guard exceeded")
                break
            # Kill switch: a human (or a supervising script) drops a STOP file into
            # the run dir and the runner stands down at the next node boundary —
            # never mid-node, so the journal stays replayable.
            if (run_path / KILL_SWITCH).exists():
                journal.append("run_aborted", reason="kill switch", switch=str(run_path / KILL_SWITCH))
                outcome.status = "aborted"
                break
            # Cost stop, measured rather than estimated: the driver reports what it
            # actually spent, and the ceiling is checked BEFORE committing to another
            # node — a budget verified only after the fact is an epitaph, not a budget.
            if spec.budget_usd > 0 and ctx.spent_usd >= spec.budget_usd:
                journal.append("run_aborted", reason="budget exhausted",
                               spent_usd=round(ctx.spent_usd, 4), budget_usd=spec.budget_usd)
                outcome.status = "aborted"
                break
            node = spec.node(current)

            if node.type == "loop":
                current = run_loop(node, spec, journal, run_path, results, variables,
                                   visits, completed, force_mock, record, outcome)
                continue

            exec_key = f"{node.name}#{visits.get(node.name, 0)}"
            visits[node.name] = visits.get(node.name, 0) + 1

            if exec_key in completed:  # durable replay: skip, reuse recorded edge
                current = completed[exec_key].get("next", node.on_pass)
                outcome.steps.append({"node": node.name, "exec_key": exec_key, "replayed": True})
                continue

            nxt = _run_single_node(ctx, node, exec_key)
            if nxt is None:  # waiting_human — the run pauses durably
                return outcome
            current = nxt

        outcome.status = "completed" if current == END else outcome.status
        journal.append("run_finished", status=outcome.status, class_d=outcome.class_d,
                       synthesized=outcome.synthesized)
        activity_append("task_completed", {"adw": spec.name, "run": run_id, "status": outcome.status})
    finally:
        lock.close()
    # Outside the lock: a deposit must never hold the run dir, and it must fire
    # for EVERY path that finishes a run (cmd_run, each campaign round, the
    # factory) rather than only the one the caller happened to take.
    deposit_run_outcome(spec.name, outcome)
    return outcome


@dataclass
class _RunCtx:
    """Everything one node execution needs; threads runner state (Law L2)."""

    spec: Spec
    journal: Journal
    run_path: Path
    results: dict
    variables: dict
    approve: set[str]
    force_mock: bool
    record: bool
    run_id: str
    outcome: RunOutcome
    visits: dict
    completed: dict
    root: Path | None = None  # code/agent nodes run anchored here, not in the caller cwd
    last_agent: dict | None = None  # {exec_key, narrated_success} for Class-D
    fail_counts: dict[str, int] = field(default_factory=dict)  # Law L2 retry shutoff
    #: node → the outputs its gate produced, newest last. Identical consecutive
    #: outputs mean the retry changed nothing the gate can see: stagnation.
    gate_history: dict[str, list[str]] = field(default_factory=dict)
    #: W4 S-4.6 — nó de retry → saída do gate que o reprovou; consumido (pop)
    #: pelo agent com `session = "resume_on_fail"` no próximo run.
    pending_feedback: dict[str, str] = field(default_factory=dict)
    spent_usd: float = 0.0  # measured, not estimated — from the driver's own accounting


def _agent_journal_fields(node: Node) -> dict:
    """M0 (29/08/2026) — agent nodes journal their tier/skill so EXECUTED (not
    estimated) tier adoption is measurable (`touring.adw.tiered_agent_share`
    reads these lines; `calls_by_tier` in explain --cost remains the static
    estimate). Non-agent nodes contribute nothing: absence of the key means
    "not an agent", never "agent without tier"."""
    if node.type != "agent":
        return {}
    campos: dict = {"tier": node.raw.get("tier")}
    if node.raw.get("skill"):
        campos["skill"] = node.raw.get("skill")
    return campos


def _run_single_node(ctx: _RunCtx, node: Node, exec_key: str) -> str | None:
    """Execute one non-loop node; returns the next node, or None on human pause."""
    ctx.journal.append("node_started", node=node.name, exec_key=exec_key, type=node.type,
                       **_agent_journal_fields(node))
    activity_append("task_started", {"adw": ctx.spec.name, "run": ctx.run_id, "node": node.name})

    if node.type == "human":
        result = _human_node(ctx, node, exec_key)
        if result is None:
            return None
    elif node.type == "agent":
        feedback = None
        if node.raw.get("session", "fresh") == "resume_on_fail":
            # W4 S-4.6 — o veredito do gate reprovador viaja para o retry.
            feedback = ctx.pending_feedback.pop(node.name, None)
        result = run_agent_node(node, ctx.spec, ctx.journal, ctx.results, ctx.variables,
                                ctx.force_mock, ctx.record, cwd=ctx.root,
                                feedback=feedback)
    elif node.type == "parallel":
        result = run_parallel_node(ctx, node, exec_key)
    elif node.type == "control":
        result = run_control_node(node, ctx.results, ctx.variables, cwd=ctx.root)
    elif node.type == "probe":
        result = run_probe_node(node, ctx.results, ctx.variables, cwd=ctx.root)
    else:  # code | gate
        result = run_code_node(node, ctx.results, ctx.variables, cwd=ctx.root)

    ctx.results[node.name] = store_result(ctx.run_path, exec_key, result.output)
    if node.type == "probe":
        attach_probe_facts(ctx.results, node.name, result.output, ctx.journal,
                           ctx.run_id, exec_key)
    if result.synthesized:
        ctx.outcome.synthesized.append(node.name)
    ctx.spent_usd += result.cost_usd
    verdict = None
    if node.type == "gate" and node.raw.get("verdict_contract", False):
        verdict = parse_verdict(result.output, result.exit_code)
    passed = verdict == PASS if verdict is not None else result.exit_code == 0
    class_d = _track_class_d(ctx, node, exec_key, result, passed)
    nxt = _next_edge(ctx, node, exec_key, passed, verdict)
    ctx.journal.append(
        "node_completed", node=node.name, exec_key=exec_key,
        exit_code=result.exit_code, verdict="pass" if passed else "fail",
        contract_verdict=verdict, cost_usd=round(result.cost_usd, 6),
        next=nxt, session_id=result.session_id, class_d=class_d,
    )
    activity_append(
        "task_completed" if passed else "error_occurred",
        {"adw": ctx.spec.name, "run": ctx.run_id, "node": node.name, "exit": result.exit_code},
    )
    step = {"node": node.name, "exec_key": exec_key, "exit_code": result.exit_code,
            "verdict": "pass" if passed else "fail"}
    if verdict is not None:
        step["contract_verdict"] = verdict
    ctx.outcome.steps.append(step)
    return nxt


def merge_branches(strategy: str, outcomes: list[tuple[str, ExecResult]]) -> str:
    """Join N branch results into one artifact — the reducer the fan-out declared.

    `tally` re-emits an aggregate `NEW_FINDINGS=` so a loop node can wrap a whole
    fan-out and still own its own termination (Law L2) — without it, a parallel
    body would leave the loop's dry-streak signal permanently absent."""
    if strategy == "concat":
        return "\n\n".join(f"── {name} (exit {r.exit_code}) ──\n{r.output}" for name, r in outcomes)
    if strategy == "tally":
        passed = [n for n, r in outcomes if r.exit_code == 0]
        failed = [n for n, r in outcomes if r.exit_code != 0]
        total = 0
        for _, r in outcomes:
            found = NEW_FINDINGS_RE.search(r.output or "")
            if found:
                total += int(found.group(1))
        lines = [f"branches={len(outcomes)} passed={len(passed)} failed={len(failed)}",
                 f"passed: {', '.join(passed) or '—'}",
                 f"failed: {', '.join(failed) or '—'}"]
        return "\n".join(lines) + f"\nNEW_FINDINGS={total}"
    digest = []
    for name, r in outcomes:  # collect: dense per-branch verdict first (Law L4)
        head = (r.output or "").strip().splitlines()
        digest.append(f"{name}: {'pass' if r.exit_code == 0 else 'FAIL'} :: "
                      f"{head[0][:200] if head else '(no output)'}")
    body = "\n\n".join(f"── {name} ──\n{r.output}" for name, r in outcomes)
    return "\n".join(digest) + "\n\n" + body


def _branch_passed(policy: str, outcomes: list[tuple[str, ExecResult]]) -> bool:
    if policy in {"ignore", "best_effort"}:
        return True
    if policy == "any":
        return any(r.exit_code == 0 for _, r in outcomes)
    quorum = QUORUM_POLICY_RE.match(policy)
    if quorum:
        return sum(1 for _, r in outcomes if r.exit_code == 0) >= int(quorum.group(1))
    return all(r.exit_code == 0 for _, r in outcomes)  # `all` — fail-closed default


def _substitute_branch(value, mapping: dict[str, str]):
    """Deep {{branch.k}} substitution — the per-clone binding of a dynamic fan-out."""
    if isinstance(value, str):
        return BRANCH_REF_RE.sub(lambda m: mapping.get(m.group(1), ""), value)
    if isinstance(value, list):
        return [_substitute_branch(v, mapping) for v in value]
    if isinstance(value, dict):
        return {k: _substitute_branch(v, mapping) for k, v in value.items()}
    return value


def dynamic_branch_values(rendered: str) -> list[str]:
    """Parse a rendered branch source into values: a JSON array, else newline/comma
    separated. Both shapes occur in practice — a `--var` is typed by a human as a
    comma list, while an upstream node's summary is usually JSON."""
    text = rendered.strip()
    if text.startswith("["):
        try:
            parsed = json.loads(text)
        except json.JSONDecodeError:
            parsed = None
        if isinstance(parsed, list):
            return [str(v).strip() for v in parsed if str(v).strip()]
    return [part.strip() for part in re.split(r"[\n,]", text) if part.strip()]


def _branch_slug(value: str) -> str:
    slug = re.sub(r"[^\w.-]+", "-", value.strip()).strip("-")
    return slug[:40] or "branch"


def dynamic_branch_nodes(ctx: _RunCtx, node: Node) -> list[Node]:
    """Clone `template` once per runtime value (LangGraph's `Send`, declaratively).

    Raises SpecError when the resolved count exceeds `max_branches`: the cap is the
    only thing standing between a runtime-sized fan-out and an unbounded bill, so
    exceeding it fails the block rather than silently truncating the work."""
    template_name = str(node.raw.get("template", ""))
    template = ctx.spec.node(template_name)
    rendered = render_template(str(node.raw.get("branches", "")), ctx.results, ctx.variables)
    values = dynamic_branch_values(rendered)
    cap = int(node.raw.get("max_branches", 0))
    if len(values) > cap:
        raise SpecError(f"dynamic fan-out resolved {len(values)} branches, "
                        f"max_branches={cap}")
    out: list[Node] = []
    seen: dict[str, int] = {}
    for index, value in enumerate(values):
        slug = _branch_slug(value)
        seen[slug] = seen.get(slug, 0) + 1
        if seen[slug] > 1:  # two values slugging alike must not share a result slot
            slug = f"{slug}-{seen[slug]}"
        body = _substitute_branch(dict(template.raw),
                                  {"value": value, "index": str(index)})
        out.append(Node(name=f"{template_name}:{slug}", type=template.type, raw=body,
                        on_pass=END, on_fail=FAIL, on_dry=END))
    return out


def _run_branch(ctx: _RunCtx, node: Node) -> ExecResult:
    """One branch, in its own thread. Reads shared state; writes nothing."""
    if node.type == "agent":
        return run_agent_node(node, ctx.spec, ctx.journal, ctx.results, ctx.variables,
                              ctx.force_mock, record=False, cwd=ctx.root)
    return run_code_node(node, ctx.results, ctx.variables, cwd=ctx.root)


def run_parallel_node(ctx: _RunCtx, node: Node, exec_key: str) -> ExecResult:
    """Fan out to read-only branches, then join at a real barrier.

    Durability: every branch journals as an ordinary node (`node_started` /
    `node_completed`) under its own exec_key, so the existing replay machinery
    skips branches that already finished — a `kill -9` mid-fan-out resumes
    without re-running (or re-billing) the ones that completed. Journal writes
    stay on the main thread in declared branch order, so the event stream is
    deterministic no matter which branch finishes first."""
    merge = str(node.raw.get("merge", "collect"))
    policy = str(node.raw.get("on_branch_fail", "all"))
    raw_branches = node.raw.get("branches")
    if isinstance(raw_branches, str):
        try:
            resolved = dynamic_branch_nodes(ctx, node)
        except SpecError as err:
            ctx.journal.append("parallel_unresolved", node=node.name,
                               exec_key=exec_key, reason=str(err))
            return ExecResult(exit_code=1, output=f"dynamic fan-out refused: {err}")
    else:
        resolved = [ctx.spec.node(b) for b in (raw_branches or [])]
    branches = [b.name for b in resolved]
    ctx.journal.append("parallel_started", node=node.name, exec_key=exec_key,
                       branches=branches, merge=merge, on_branch_fail=policy,
                       dynamic=isinstance(raw_branches, str))

    plan: list[tuple[Node, str]] = []
    for b_node in resolved:
        branch = b_node.name
        b_key = f"{branch}#{ctx.visits.get(branch, 0)}"
        ctx.visits[branch] = ctx.visits.get(branch, 0) + 1
        plan.append((b_node, b_key))

    pending = [(n, k) for n, k in plan if k not in ctx.completed]
    for b_node, b_key in pending:
        ctx.journal.append("node_started", node=b_node.name, exec_key=b_key,
                           type=b_node.type, parallel=node.name,
                           **_agent_journal_fields(b_node))

    computed: dict[str, ExecResult] = {}
    if pending:
        with ThreadPoolExecutor(max_workers=len(pending)) as pool:
            futures = {pool.submit(_run_branch, ctx, b_node): b_key for b_node, b_key in pending}
            for future, b_key in futures.items():
                try:
                    computed[b_key] = future.result()
                except Exception as err:  # a branch that dies is a failed branch, not a dead run
                    computed[b_key] = ExecResult(exit_code=1, output=f"branch raised: {err!r}")

    outcomes: list[tuple[str, ExecResult]] = []
    narrated: list[str] = []
    for b_node, b_key in plan:  # declared order — deterministic journal + merge
        if b_key in computed:
            result = computed[b_key]
            ctx.results[b_node.name] = store_result(ctx.run_path, b_key, result.output)
            if result.synthesized:
                ctx.outcome.synthesized.append(b_node.name)
            ctx.journal.append("node_completed", node=b_node.name, exec_key=b_key,
                               exit_code=result.exit_code,
                               verdict="pass" if result.exit_code == 0 else "fail",
                               next=node.name, session_id=result.session_id,
                               class_d=False, parallel=node.name)
            ctx.outcome.steps.append({"node": b_node.name, "exec_key": b_key,
                                      "exit_code": result.exit_code, "parallel": node.name,
                                      "verdict": "pass" if result.exit_code == 0 else "fail"})
        else:  # durable replay: this branch already completed in an earlier attempt
            recorded = ctx.completed[b_key]
            artifact = ctx.results.get(b_node.name, {})
            result = ExecResult(exit_code=int(recorded.get("exit_code", 0)),
                                output=artifact.get("summary", ""))
            ctx.outcome.steps.append({"node": b_node.name, "exec_key": b_key,
                                      "parallel": node.name, "replayed": True})
        if b_node.type == "agent" and result.narrated_success:
            narrated.append(b_node.name)
        outcomes.append((b_node.name, result))

    passed = _branch_passed(policy, outcomes)
    ctx.journal.append("parallel_joined", node=node.name, exec_key=exec_key, merge=merge,
                       passed=passed, narrated_success=narrated,
                       verdicts={n: r.exit_code for n, r in outcomes})
    return ExecResult(
        exit_code=0 if passed else 1,
        output=merge_branches(merge, outcomes),
        # Law L3 under fan-out: the single `last_agent` slot held whichever branch
        # happened to finish last, so a downstream Class-D pointed at an arbitrary
        # agent. The parallel node reports narration for the whole block and the
        # journal names every branch that claimed success.
        narrated_success=bool(narrated),
    )


def _track_class_d(ctx: _RunCtx, node: Node, exec_key: str, result: ExecResult,
                   passed: bool) -> bool:
    """Law L3 — Class-D: a gate FAIL immediately after a success-narrating agent
    means the narrative diverged from the verified verdict."""
    class_d = False
    if node.type == "gate" and not passed and ctx.last_agent and ctx.last_agent["narrated_success"]:
        class_d = True
        ctx.outcome.class_d = True
        ctx.journal.append("class_d_divergence", agent_exec=ctx.last_agent["exec_key"],
                           gate_exec=exec_key)
    if node.type == "agent":
        ctx.last_agent = {"exec_key": exec_key, "narrated_success": result.narrated_success}
    elif node.type == "parallel":
        # A parallel block replaces the standing claim only if it MADE one. A panel
        # of critics narrates nothing about the work it is judging, so treating it
        # like an agent silently erased the worker's "all done ✅" — and the gate
        # failure that followed stopped counting as a divergence. Observed with
        # critic-panel: build narrated success, two critics rejected, class_d=false.
        if result.narrated_success:
            ctx.last_agent = {"exec_key": exec_key, "narrated_success": True}
    elif node.type == "gate":
        ctx.last_agent = None
    return class_d


def _is_stagnant(ctx: _RunCtx, node: Node, output: str) -> bool:
    """Has this gate produced the same rejection N times running?

    A retry budget alone counts attempts; it cannot tell an agent that is closing
    in from one that is resubmitting the same work. Identical consecutive gate
    output means the last attempt changed nothing the gate can observe — spending
    the rest of the budget on it buys nothing but tokens.

    OPT-IN (`stagnation_rounds = N`), and deliberately so. Plenty of gates fail
    silently — `exit 1` with no output — and for those every rejection is trivially
    identical. Defaulting this on would cut such a spec off at 2 attempts while its
    author had written `max_retries = 5`: a silent default overriding an explicit
    declaration, which is the exact failure class this whole contract exists to
    remove. An author who wants the stronger stop asks for it."""
    rounds = int(node.raw.get("stagnation_rounds", 0))
    if rounds <= 0:
        return False
    history = ctx.gate_history.setdefault(node.name, [])
    history.append((output or "").strip())
    recent = history[-rounds:]
    return len(recent) == rounds and len(set(recent)) == 1


def _next_edge(ctx: _RunCtx, node: Node, exec_key: str, passed: bool,
               verdict: str | None = None) -> str:
    """Law L2 — the runner, not the graph, terminates feedback loops: a node that
    keeps failing exhausts its retry budget and the run fails loud instead of
    re-invoking (and re-billing) agents forever.

    Under the B6 contract there are three outcomes, not two. ESCALATE routes to
    `on_escalate` and deliberately does NOT consume a retry: the check could not
    run, so no number of further agent attempts is the answer."""
    if verdict == ESCALATE:
        target = str(node.raw.get("on_escalate", FAIL))
        ctx.journal.append("escalated", node=node.name, exec_key=exec_key, next=target,
                           reason="gate reported ESCALATE — the check could not decide")
        return target
    if passed:
        return node.on_pass
    nxt = node.on_fail
    ctx.fail_counts[node.name] = ctx.fail_counts.get(node.name, 0) + 1
    max_retries = int(node.raw.get("max_retries", 3))
    if nxt not in TERMINALS and ctx.fail_counts[node.name] > max_retries:
        ctx.journal.append("retry_limit_exceeded", node=node.name,
                           exec_key=exec_key, failures=ctx.fail_counts[node.name],
                           max_retries=max_retries)
        return FAIL
    if nxt not in TERMINALS and node.type == "gate" and _is_stagnant(
            ctx, node, ctx.results.get(node.name, {}).get("summary", "")):
        ctx.journal.append("stagnation_detected", node=node.name, exec_key=exec_key,
                           rounds=int(node.raw.get("stagnation_rounds", 0)),
                           reason="the gate reported an identical rejection — the retry "
                                  "changed nothing it can see")
        return FAIL
    if nxt not in TERMINALS:
        # W4 S-4.6 — registra a saída do nó reprovador para o retry consumir
        # (`resume_on_fail` a injeta como [gate feedback] no prompt).
        veredito = ctx.results.get(node.name, {}).get("summary", "")
        ctx.pending_feedback[nxt] = veredito
        _store_gate_rejection(ctx, node.name, veredito)
    return nxt


def _store_gate_rejection(ctx: "_RunCtx", node_name: str, veredito: str) -> None:
    """Persiste a reprovação como CASO ROTULADO NEGATIVO. Fail-open.

    O corpus de casos é quase constante: medido 26/08/2026, 209 de 225 memórias
    com `outcome_reward` valem ≥ 0,5 e a média é 0,919 — 93% positivas. Um
    trainset sem fracasso não dá gradiente: o piso de 30-300 exemplos do DSPy
    está atingido e a DISCRIMINAÇÃO não, então otimizar contra ele otimizaria
    contra uma constante.

    A causa é estrutural: o veredito vem de `status == "done"`, e quem fecha uma
    fase é quem acabou de fazê-la funcionar — sinal auto-referencial, o que o
    survey MSR diz que degrada com a iteração. Uma reprovação de gate é o
    oposto: falha real, medida por CÓDIGO, independente de quem a produziu. É o
    rótulo negativo determinístico que faltava.

    Nunca levanta e nunca bloqueia o run: um caso não gravado é um exemplo a
    menos, não um workflow quebrado.
    """
    if not veredito:
        return
    chave = f"gate-reject:{ctx.spec.name}:{node_name}"
    corpo = veredito.strip()[:600]
    try:
        subprocess.run(
            ["touring", "memory", "store", chave, corpo,
             "--tier", "semantic", "--type", "gotcha",
             "--reward", "0.0",
             "--outcome-context", f"adw_gate_reject:{node_name}",
             "--tag", "#kind:gotcha", "--tag", "#status:negative"],
            capture_output=True, timeout=30, check=False,
        )
    except (subprocess.TimeoutExpired, OSError):
        pass


def _human_node(ctx: _RunCtx, node: Node, exec_key: str) -> ExecResult | None:
    """Approved → pass result; otherwise journal a durable pause and return None.

    F5a ZTE: a spec may opt a human node into a conformal bypass (`zte = true`).
    Review is dropped ONLY with a statistical guarantee (calibrate-confidence IN)
    plus warm-up history, and the bypass is journaled for a-posteriori audit.
    Any doubt — thin history, low run confidence, OUT verdict, probe error —
    falls closed back to the durable human pause.
    """
    if node.name in ctx.approve:
        return ExecResult(exit_code=0, output=f"approved via --approve {node.name}")
    if ctx.force_mock:
        # `adw test` is a mocked edge test: it walks the graph, it does not run it.
        # Pausing here made every flow with a human gate untestable — and the kit
        # ships `human-approve` as a standard piece, so that was most of them.
        ctx.journal.append("human_auto_approved", node=node.name, exec_key=exec_key,
                           reason="mocked edge test")
        return ExecResult(exit_code=0, synthesized=True, output=(
            f"auto-approved under `adw test`, which walks the graph. "
            f"A real run of `{node.name}` still pauses for a human."))
    if node.raw.get("zte", False):
        bypass = _zte_bypass(ctx, node, exec_key)
        if bypass is not None:
            return bypass
    ctx.journal.append("waiting_human", node=node.name, exec_key=exec_key,
                       message=node.raw.get("message", ""))
    ctx.outcome.status = "waiting_human"
    print(json.dumps({
        "status": "waiting_human", "run_id": ctx.run_id, "node": node.name,
        "resume_with": f"touring adw run {ctx.spec.name} --resume-run {ctx.run_id} --approve {node.name}",
    }, ensure_ascii=False))
    return None


def run_confidence(ctx: _RunCtx) -> float:
    """Deterministic, auditable confidence of the CURRENT run so far: clean runs
    score high; every failed node execution costs 0.1; a Class-D divergence is
    disqualifying by construction (narrative ≠ verdict → no bypass)."""
    if ctx.outcome.class_d:
        return 0.0
    total_failures = sum(ctx.fail_counts.values())
    return max(0.0, 1.0 - 0.1 * total_failures)


def completed_run_count(root_runs: Path, spec_name: str, current_run: str) -> int:
    """Warm-up evidence: prior runs of THIS spec that finished `completed`."""
    count = 0
    if not root_runs.is_dir():
        return 0
    for run_dir in root_runs.glob(f"{spec_name}-*"):
        if run_dir.name == current_run:
            continue
        journal_file = run_dir / "journal.jsonl"
        if not journal_file.is_file():
            continue
        for line in journal_file.read_text(encoding="utf-8").splitlines():
            if '"run_finished"' in line and '"completed"' in line:
                count += 1
                break
    return count


def conformal_in(confidence: float) -> bool:
    """Ask the live conformal calibrator (KnowNo); anything but a clear IN is OUT."""
    try:
        proc = subprocess.run(["touring", "calibrate-confidence", f"{confidence:.3f}"],
                              capture_output=True, text=True, timeout=30)
        return "IN prediction set" in proc.stdout
    except Exception:
        return False  # fail-closed: no calibrator, no bypass


def _zte_bypass(ctx: _RunCtx, node: Node, exec_key: str) -> ExecResult | None:
    warmup = int(node.raw.get("zte_warmup", 3))
    prior = completed_run_count(ctx.run_path.parent, ctx.spec.name, ctx.run_id)
    if prior < warmup:
        return None
    confidence = run_confidence(ctx)
    if not conformal_in(confidence):
        return None
    ctx.journal.append("zte_bypass", node=node.name, exec_key=exec_key,
                       confidence=confidence, prior_completed_runs=prior,
                       conformal="IN", audit="a-posteriori review required")
    return ExecResult(
        exit_code=0,
        output=(f"ZTE bypass: conformal IN at confidence {confidence:.2f} "
                f"with {prior} prior completed runs (audited in journal)"),
    )


def run_loop(
    node: Node, spec: Spec, journal: Journal, run_path: Path, results: dict,
    variables: dict, visits: dict, completed: dict, force_mock: bool, record: bool,
    outcome: RunOutcome,
) -> str:
    """Execute a loop node: the RUNNER counts findings and owns termination (L2).

    W4 (plano code-mode-total, 2026-08-24): quatro predicados de terminação,
    cada um o remédio de uma classe medida do corpus de 703 memórias —
    `dry` (NEW_FINDINGS=, o original), `fixpoint` (METRIC= repete →
    staleness 42%), `covered` (COVERAGE=n/m família completa →
    familia_parcial 19%), `calibrated` (corpo control passa →
    instrumento_errado 38%).
    """
    body = spec.node(node.raw["body"])
    max_iters = int(node.raw.get("max_iters", 1))
    dry_rounds = int(node.raw.get("dry_rounds", 2))
    predicate = str(node.raw.get("predicate", "dry"))
    stable_rounds = int(node.raw.get("stable_rounds", 2))
    min_discovered = int(node.raw.get("min_discovered", 1))
    dry = 0
    last_metric: float | None = None
    stable = 0
    prev_m: int | None = None
    for _ in range(max_iters):
        exec_key = f"{body.name}#{visits.get(body.name, 0)}"
        visits[body.name] = visits.get(body.name, 0) + 1
        if exec_key in completed:
            output = ""
            artifact_path = run_path / f"{exec_key}.json"
            if artifact_path.is_file():
                output = json.loads(artifact_path.read_text(encoding="utf-8"))["summary"]
            outcome.steps.append({"node": body.name, "exec_key": exec_key, "replayed": True})
        else:
            journal.append("node_started", node=body.name, exec_key=exec_key, type=body.type,
                           **_agent_journal_fields(body))
            if body.type == "agent":
                result = run_agent_node(body, spec, journal, results, variables,
                                        force_mock, record, cwd=run_path.parent.parent.parent)
            elif body.type == "control":
                result = run_control_node(body, results, variables,
                                          cwd=run_path.parent.parent.parent)
            elif body.type == "probe":
                result = run_probe_node(body, results, variables,
                                        cwd=run_path.parent.parent.parent)
            else:
                result = run_code_node(body, results, variables, cwd=run_path.parent.parent.parent)
            store_result(run_path, exec_key, result.output)
            results[body.name] = {"summary": result.output[:SUMMARY_LIMIT],
                                  "omitted_bytes": 0, "full_ref": ""}
            if body.type == "probe":
                attach_probe_facts(results, body.name, result.output, journal,
                                   run_path.name, exec_key)
            journal.append("node_completed", node=body.name, exec_key=exec_key,
                           exit_code=result.exit_code,
                           verdict="pass" if result.exit_code == 0 else "fail",
                           next=node.name, session_id=result.session_id, class_d=False)
            outcome.steps.append({"node": body.name, "exec_key": exec_key,
                                  "exit_code": result.exit_code})
            if result.exit_code != 0 and predicate != "calibrated":
                # `calibrated` ESPERA rodadas reprovadas — um control que falha é
                # exatamente o que o loop re-tenta; os demais predicados leem um
                # corpo falho como falha do loop.
                return node.on_fail
            output = result.output

        if predicate == "fixpoint":
            found = METRIC_ANY_RE.search(output or "")
            metric = found.group(1) if found else None
            if metric is None:
                # UNKNOWN nunca inicia nem estende a sequência estável (Lei L2):
                # silêncio não é estabilidade — é ausência de instrumento.
                last_metric, stable = None, 0
            elif last_metric is not None and metric == last_metric:
                stable += 1
            else:
                last_metric, stable = metric, 1
            journal.append("loop_round", loop=node.name, body_exec=exec_key,
                           predicate=predicate, metric=metric, stable_streak=stable,
                           dry_signal="present" if found else "absent")
            outcome.steps.append({"node": node.name, "loop_round": exec_key,
                                  "metric": metric, "stable_streak": stable})
            if metric is not None and stable >= stable_rounds:
                return str(node.raw.get("on_stable") or node.on_pass)
        elif predicate == "covered":
            found = COVERAGE_RE.search(output or "")
            n = int(found.group(1)) if found else None
            m = int(found.group(2)) if found else None
            journal.append("loop_round", loop=node.name, body_exec=exec_key,
                           predicate=predicate, covered_n=n, covered_m=m,
                           dry_signal="present" if found else "absent")
            outcome.steps.append({"node": node.name, "loop_round": exec_key,
                                  "covered_n": n, "covered_m": m})
            if m is not None:
                if prev_m is not None and m < prev_m:
                    journal.append("loop_universe_shrank", loop=node.name,
                                   previous=prev_m, current=m,
                                   reason="o universo não encolhe — descoberta instável")
                    return FAIL
                prev_m = m
                if n == m and m >= min_discovered:
                    return str(node.raw.get("on_covered") or node.on_pass)
        elif predicate == "calibrated":
            calibrated = bool(CONTROL_PASS_RE.search(output or ""))
            journal.append("loop_round", loop=node.name, body_exec=exec_key,
                           predicate=predicate, calibrated=calibrated)
            outcome.steps.append({"node": node.name, "loop_round": exec_key,
                                  "calibrated": calibrated})
            if calibrated:
                return str(node.raw.get("on_calibrated") or node.on_pass)
        else:
            found = NEW_FINDINGS_RE.search(output or "")
            # FAIL-CLOSED (2026-08-02): an absent marker means *unknown*, never zero.
            # Reading it as 0 made silence indistinguishable from a dry round, so a
            # body that never emits the signal — or whose output was truncated by a
            # `tail -c` in the spec — terminated the loop after exactly `dry_rounds`
            # iterations while still finding new material every round. Observed on
            # strategy-loop: rounds of 30 and 4 new findings both counted as dry.
            # Same defect class as loop_converged.py's dag_done (fail-open → closed).
            new_findings = int(found.group(1)) if found else None
            dry = dry + 1 if new_findings == 0 else 0
            journal.append("loop_round", loop=node.name, body_exec=exec_key,
                           new_findings=new_findings, dry_streak=dry,
                           dry_signal="present" if found else "absent")
            outcome.steps.append({"node": node.name, "loop_round": exec_key,
                                  "new_findings": new_findings,
                                  "dry_signal": "present" if found else "absent"})
            if dry >= dry_rounds:
                return node.on_dry
    if predicate == "covered":
        # Exaurir com n<m é __fail__, jamais sucesso: a cobertura parcial que
        # termina confiante é a classe familia_parcial (Lei L2 sobre conjunto).
        journal.append("loop_exhausted_incomplete", loop=node.name,
                       predicate=predicate,
                       reason="max_iters com cobertura incompleta (n<m)")
        return FAIL
    if predicate == "calibrated":
        journal.append("loop_exhausted_incomplete", loop=node.name,
                       predicate=predicate,
                       reason="max_iters sem calibrar o verificador")
        return FAIL
    return node.on_pass


# ── templates (G1-seed) ───────────────────────────────────────────────────────

TEMPLATE = """\
# ADW spec scaffold — generated by `touring adw from-template {name}`
# Do it by hand first: sketch this flow in mermaid, then refine the spec.

[adw]
name = "{name}"
description = "TODO: one-line purpose"
entry = "scout"
budget_tokens = 0            # 0 = unlimited; if set, Σ node budgets must fit (budget-verify)

[node.scout]
type = "agent"
driver = "claude"
tier = "sota"                 # sota | mid | fast (or a literal model name)
prompt = "Explore the target and report findings. End with NEW_FINDINGS=<n>."
allowed_tools = ["Read", "Grep", "Glob", "Bash"]
session = "fresh"
on_pass = "implement"
on_fail = "__fail__"

[node.implement]
type = "agent"
driver = "claude"
tier = "mid"
prompt = "Implement based on: {{{{nodes.scout.summary}}}}"
allowed_tools = ["Read", "Edit", "Write", "Bash"]
session = "resume_on_fail"    # gate failure feeds back into the SAME session
on_pass = "verify"
on_fail = "__fail__"

[node.verify]
type = "gate"
command = ["bash", "-lc", "echo TODO: real verification; exit 1"]
timeout_ms = 300000
idempotent = true
on_pass = "__end__"
on_fail = "implement"         # feedback loop: verified failure returns to the agent
"""


# ── CLI ───────────────────────────────────────────────────────────────────────


def cmd_list(root: Path) -> int:
    specs = sorted(p.stem for p in adw_dir(root).glob("*.toml")) if adw_dir(root).is_dir() else []
    lib = library_dir()
    templates = sorted(p.stem for p in lib.glob("*.toml") if p.stem != "tiers") if lib.is_dir() else []
    print(json.dumps({"specs": specs, "dir": str(adw_dir(root)),
                      "library": templates, "library_dir": str(lib)}, ensure_ascii=False))
    return 0


# ── promotion: the library carries evidence, not intent (T5.1) ───────────────
#
# Isenberg's warning about automating too early — "if the manual version doesn't
# produce way better work, automating it will just produce mediocre work way
# faster" — is not enforceable as a ceremony, and demanding one would just be
# process. What IS enforceable: a flow may not join the shipped library on the
# strength of having been written. It joins on the strength of having RUN, with
# the outcome attached.
#
# A synthesized walk does not count. `adw test` proves the graph connects; it
# says nothing about whether the agents behave, so a record whose `synthesized`
# list is non-empty is evidence of wiring and is stored as such.
#
# The record is also the bridge to the variant archive: a promotion IS a scored
# observation of a flow, which is exactly what a stepping-stone archive needs.

PROMOTIONS_FILENAME = "promotions.json"


def promotions_path() -> Path:
    return library_dir() / PROMOTIONS_FILENAME


def load_promotions() -> dict:
    """The promotion ledger, or an empty one. Never raises: a corrupt ledger must
    not stop a run, only fail the guard that reads it."""
    try:
        return json.loads(promotions_path().read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def read_run_outcome(root: Path, run_id: str) -> dict | None:
    """The recorded outcome of one run, read from its journal.

    The journal is the source of truth for resume, so it is the source of truth
    here too — a promotion derived from anything else could disagree with what
    the runner actually did.
    """
    journal = runs_dir(root) / run_id / "journal.jsonl"
    if not journal.is_file():
        return None
    finished = None
    started = None
    # Agent sessions are counted from the journal rather than trusted from a
    # field, because "the run completed" and "an agent was actually invoked" are
    # different claims and a ledger that conflates them answers the wrong
    # question. A mock session id is prefixed `mock`; a real one is the driver's
    # own UUID.
    real_agents = mock_agents = 0
    try:
        for line in journal.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            rec = json.loads(line)
            if rec.get("event") == "run_started":
                started = rec
            elif rec.get("event") == "run_finished":
                finished = rec
            elif rec.get("event") == "node_completed" and rec.get("session_id"):
                if str(rec["session_id"]).startswith("mock"):
                    mock_agents += 1
                else:
                    real_agents += 1
    except (OSError, ValueError):
        return None
    if finished is None:
        return None
    # `synthesized` was only journaled from 2026-08-19. A journal written before
    # that carries no such key, and `... or []` read that silence as "nothing was
    # mocked" — which promoted a fully MOCKED run of `feature` as evidence of
    # behaviour on the ledger's first day. Absence of a reading is not a reading
    # of absence; `None` here means unknown, and the caller must say so.
    synthesized = finished.get("synthesized")
    return {
        "run_id": run_id,
        "status": finished.get("status", "unknown"),
        "class_d": bool(finished.get("class_d", False)),
        "synthesized": list(synthesized) if synthesized is not None else None,
        "agent_sessions": {"real": real_agents, "mock": mock_agents},
        "started_at": (started or {}).get("ts"),
        "finished_at": finished.get("ts"),
    }


def cmd_promote(root: Path, name: str, run_id: str | None, exempt: str | None) -> int:
    """Record the evidence that lets a flow ship, or declare why there is none."""
    ledger = load_promotions()
    if exempt:
        ledger[name] = {"exempt": exempt, "recorded_at": time.time()}
    else:
        if not run_id:
            print(json.dumps({"error": "either --run <run_id> or --exempt <reason> is required"}))
            return 2
        outcome = read_run_outcome(root, run_id)
        if outcome is None:
            print(json.dumps({"error": f"no finished run `{run_id}` under {runs_dir(root)}"}))
            return 1
        outcome["recorded_at"] = time.time()
        sessions = outcome["agent_sessions"]
        if outcome["synthesized"] is None:
            outcome["evidence"] = (
                "unknown — this journal predates the synthesized field; re-run to claim behaviour"
            )
        elif outcome["synthesized"] or sessions["mock"]:
            outcome["evidence"] = "wiring only — the walk synthesized agents"
        elif sessions["real"]:
            outcome["evidence"] = (
                f"behaviour — {sessions['real']} real agent session(s), every node ran for real"
            )
        else:
            # Completing a flow that HAS no agent node says nothing about agent
            # invocation. Reporting it as behaviour would answer a question
            # nobody asked with evidence nobody has.
            outcome["evidence"] = "behaviour — every node ran for real (this flow has no agent node)"
        ledger[name] = outcome
    try:
        promotions_path().write_text(
            json.dumps(dict(sorted(ledger.items())), indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8")
    except OSError as err:
        print(json.dumps({"error": f"cannot write the ledger: {err}"}))
        return 1
    print(json.dumps({"promoted": name, **ledger[name]}, ensure_ascii=False, indent=1))
    return 0


def cmd_lint(root: Path, name: str) -> int:
    try:
        spec = load_spec(root, name)
    except SpecError as err:
        print(json.dumps({"valid": False, "errors": [str(err)], "warnings": []}, ensure_ascii=False))
        return 1
    errors, warnings = lint_spec(spec)
    print(json.dumps({"valid": not errors, "errors": errors, "warnings": warnings},
                     ensure_ascii=False, indent=1))
    return 1 if errors else 0


def flat_graph(spec: Spec) -> dict:
    """The resolved graph, normalised — what the runner actually walks.

    Composition must never be the only representation of a flow: a spec built
    from fragments has to be readable as the flat thing it becomes, or the
    indirection is a cost with no counterweight. Two specs that resolve to the
    same graph produce byte-identical output here, which is how a composed spec
    is proven equivalent to the monolith it replaces.

    A skill-bound node reports its EFFECTIVE `allowed_tools` — the ones the runner
    will pass, `Skill` included. Rendering the declared list while the invocation
    adds a tool would make this graph lie by omission about the one thing it
    promises: what actually runs. Unbound nodes are byte-identical to before."""
    def body(node: Node) -> dict:
        out = {
            "type": node.type,
            "on_pass": node.on_pass,
            "on_fail": node.on_fail,
            "on_dry": node.on_dry,
            **{k: v for k, v in sorted(node.raw.items())
               if k not in {"on_pass", "on_fail", "on_dry", "type"}},
        }
        if node.type == "agent" and skill_binding(node):
            out["allowed_tools"] = skill_tools(node)
        return out

    return {
        "name": spec.name,
        "entry": spec.entry,
        "budget_tokens": spec.budget_tokens,
        "nodes": {name: body(node) for name, node in sorted(spec.nodes.items())},
    }


def flow_mermaid(spec: Spec) -> str:
    """The ONE flow drawn — Isenberg's level 1 ("draw the graph before you
    automate it") applied to the individual spec, not just the portfolio.

    Measured 2026-08-23: the system had a mermaid for the process registry and
    one for the portfolio (`portfolio_graph.py mermaid`), and NONE for a single
    ADW flow — the level the sources start from. Shapes carry the node type
    (code `[ ]` · gate `{ }` · agent `[/ /]` · human `([ ])` · loop `[[ ]]`);
    solid edges are pass, dashed are fail, `dry` is labelled.
    """
    def mid(nome: str) -> str:
        return re.sub(r"[^A-Za-z0-9]", "_", nome)

    shape = {"code": ("[", "]"), "gate": ("{", "}"), "agent": ("[/", "/]"),
             "human": ("([", "])"), "loop": ("[[", "]]"),
             "parallel": ("{{", "}}")}
    L = ["graph TD",
         "  classDef code fill:#eef6ff,stroke:#4a7fb5,stroke-width:1px;",
         "  classDef gate fill:#fff8e1,stroke:#b58a00,stroke-width:1px;",
         "  classDef agent fill:#f3e8ff,stroke:#7c4dbe,stroke-width:1px;",
         "  classDef human fill:#ffe8ee,stroke:#c0506e,stroke-width:1px;",
         "  classDef loop fill:#e8fff1,stroke:#3d9960,stroke-width:1px;",
         "  classDef parallel fill:#fff0f8,stroke:#b0508e,stroke-width:1px;",
         "  classDef fim fill:#eafaea,stroke:#2f7d3b,stroke-width:2px;",
         "  classDef falha fill:#fdeaea,stroke:#c0504d,stroke-width:2px;",
         f"  START(((entry))) --> N_{mid(spec.entry)}"]
    terminais: set[str] = set()

    def alvo(dest: str) -> str:
        if dest == END:
            terminais.add("END"); return "FIM"
        if dest == FAIL:
            terminais.add("FAIL"); return "FALHA"
        return f"N_{mid(dest)}"

    for nome, node in sorted(spec.nodes.items()):
        a, b = shape.get(node.type, ("[", "]"))
        L.append(f'  N_{mid(nome)}{a}"{nome}<br/><small>{node.type}</small>"{b}'
                 f":::{node.type}")
    for nome, node in sorted(spec.nodes.items()):
        de = f"N_{mid(nome)}"
        if node.type == "loop":
            corpo = node.raw.get("body")
            if corpo:
                L.append(f"  {de} -->|body| N_{mid(str(corpo))}")
            if node.on_dry:
                L.append(f"  {de} -->|dry| {alvo(node.on_dry)}")
        if node.type == "parallel":
            # The fan-out IS the point of a parallel node — a drawing that omits
            # it shows the critics floating free of the block that dispatches
            # them (cross-audit probe, 2026-08-23). `branches` is either a
            # literal node list (static) or a template string (dynamic, resolved
            # at runtime against the `template` node).
            ramos = node.raw.get("branches")
            if isinstance(ramos, list):
                for ramo in ramos:
                    L.append(f"  {de} -->|branch| N_{mid(str(ramo))}")
            elif ramos:
                molde = node.raw.get("template")
                if molde:
                    L.append(f'  {de} -.->|"branches: {ramos}"| N_{mid(str(molde))}')
        if node.on_pass:
            L.append(f"  {de} --> {alvo(node.on_pass)}")
        if node.on_fail and node.on_fail != node.on_pass:
            L.append(f"  {de} -.-> {alvo(node.on_fail)}")
    if "END" in terminais:
        L.append("  FIM(((__end__))):::fim")
    if "FAIL" in terminais:
        L.append("  FALHA(((__fail__))):::falha")
    return "\n".join(L)


def cmd_explain(root: Path, name: str, cost: bool = False,
                mermaid: bool = False) -> int:
    """Print the flat resolved spec — every [[use]] inlined; `--cost` adds the
    ceiling; `--mermaid` draws the resolved graph instead of printing JSON."""
    try:
        spec = load_spec(root, name)
    except SpecError as err:
        print(json.dumps({"error": str(err)}, ensure_ascii=False))
        return 1
    if mermaid:
        print(flow_mermaid(spec))
        return 0
    errors, warnings = lint_spec(spec)
    out = {**flat_graph(spec), "lint": {"errors": errors, "warnings": warnings}}
    if cost:
        out["cost"] = flow_cost(spec)
    print(json.dumps(out, ensure_ascii=False, indent=1, sort_keys=False))
    return 0


def cmd_fragments(root: Path) -> int:
    """List the composable pieces available to build a flow from."""
    out = []
    seen: set[str] = set()
    for directory in fragment_dirs(root):
        if not directory.is_dir():
            continue
        for path in sorted(directory.glob("*.toml")):
            if path.stem in seen:
                continue  # a project fragment shadows the library one
            seen.add(path.stem)
            try:
                data = tomllib.loads(path.read_text(encoding="utf-8"))
            except tomllib.TOMLDecodeError as err:
                out.append({"module": path.stem, "error": str(err)})
                continue
            meta = data.get("fragment") or {}
            out.append({
                "module": path.stem,
                "description": meta.get("description", ""),
                "inputs": list(meta.get("inputs") or []),
                "entry": meta.get("entry", ""),
                "nodes": sorted(data.get("node") or {}),
                "source": "project" if directory == fragment_dirs(root)[0] else "library",
            })
    print(json.dumps({"fragments": out}, ensure_ascii=False, indent=1))
    return 0


def cmd_run(root: Path, args: argparse.Namespace) -> int:
    try:
        spec = load_spec(root, args.name)
    except SpecError as err:
        print(json.dumps({"status": "failed", "error": str(err)}, ensure_ascii=False))
        return 1
    errors, _ = lint_spec(spec)
    if errors:
        print(json.dumps({"status": "failed", "error": "lint failed", "errors": errors},
                         ensure_ascii=False))
        return 1
    variables = dict(kv.split("=", 1) for kv in (args.var or []))
    outcome = execute(
        spec, root, resume_run=args.resume_run, approve=set(args.approve or []),
        force_mock=args.mock, record=args.record, variables=variables,
    )
    if outcome.status != "waiting_human":
        print(json.dumps({
            "status": outcome.status, "run_id": outcome.run_id,
            "class_d_divergence": outcome.class_d, "steps": outcome.steps,
        }, ensure_ascii=False, indent=1))
    return 0 if outcome.status == "completed" else (3 if outcome.status == "waiting_human" else 1)


def cmd_campaign(root: Path, args: argparse.Namespace) -> int:
    """E4 (2026-08-24) — repeat a WHOLE flow until a CODE predicate converges.

    The loop node (E2) iterates one node until NEW_FINDINGS runs dry; a campaign
    iterates one FLOW until `--until CMD` exits 0. Both obey L2 — the runner owns
    termination, never the model — and both read their signal fail-closed:
    `METRIC=<float>` on the predicate's stdout feeds the curve and the stagnation
    check; an ABSENT metric is unknown, never zero and never stagnation.

    Born from a measured anti-pattern (2026-08-24): the error-teach campaign ran
    as a hand-written `for i in 2 3 4; do touring adw run …` — no predicate, no
    curve, no fail-closed reading, invisible to the journal. This subcommand is
    the affordance that replaces it (enforcement-by-affordance, D8).

    Stops on: converged | flow_failed | stagnation | max_rounds. Persists the
    curve as a memory (`campaign:<flow>:<runid>`) and deposits a delta-based
    reward per round (the ACO pheromone), both fail-open.
    """
    try:
        spec = load_spec(root, args.name)
    except SpecError as err:
        print(json.dumps({"status": "failed", "error": str(err)}, ensure_ascii=False))
        return 1
    errors, _ = lint_spec(spec)
    if errors:
        print(json.dumps({"status": "failed", "error": "lint failed", "errors": errors},
                         ensure_ascii=False))
        return 1
    variables = dict(kv.split("=", 1) for kv in (args.var or []))
    max_rounds = max(1, int(args.max_rounds))
    stagnation_rounds = max(0, int(args.stagnation_rounds))
    curve: list[dict] = []
    status = "max_rounds"
    prev_metric: float | None = None
    stagnant = 0
    for round_no in range(1, max_rounds + 1):
        outcome = execute(spec, root, force_mock=args.mock, record=False,
                          variables=variables)
        if outcome.status != "completed":
            curve.append({"round": round_no, "run_id": outcome.run_id,
                          "flow_status": outcome.status, "metric": None,
                          "predicate_exit": None})
            status = "flow_failed"
            break
        try:
            pred = subprocess.run(
                ["bash", "-c", args.until], capture_output=True, text=True,
                timeout=600, cwd=str(root), check=False,
            )
            pred_exit: int | None = pred.returncode
            found = METRIC_RE.search(pred.stdout or "")
            metric = float(found.group(1)) if found else None
        except (subprocess.TimeoutExpired, OSError):
            pred_exit, metric = None, None
        curve.append({"round": round_no, "run_id": outcome.run_id,
                      "flow_status": "completed", "metric": metric,
                      "predicate_exit": pred_exit,
                      "metric_signal": "present" if metric is not None else "absent"})
        # Pheromone: reward the ROUND by its measured delta (fail-open).
        if metric is not None and prev_metric is not None:
            delta = metric - prev_metric
            reward = max(-1.0, min(1.0, delta * 10))
            subprocess.run(
                ["touring", "learning", "reward", f"adw:campaign:{spec.name}",
                 f"{reward:.3f}"],
                capture_output=True, timeout=30, check=False,
            )
        if pred_exit == 0:
            status = "converged"
            break
        # Stagnation: only a PRESENT, repeated metric counts (fail-closed).
        if stagnation_rounds > 0 and metric is not None:
            stagnant = stagnant + 1 if metric == prev_metric else 0
            if stagnant >= stagnation_rounds:
                status = "stagnation"
                break
        prev_metric = metric if metric is not None else prev_metric
    result = {
        "status": status, "flow": spec.name, "rounds": len(curve),
        "max_rounds": max_rounds, "until": args.until, "curve": curve,
    }
    # Persist the curve (fail-open): the next campaign recalls this one.
    last_run = curve[-1]["run_id"] if curve else "none"
    subprocess.run(
        ["touring", "memory", "store", f"campaign:{spec.name}:{last_run}",
         json.dumps(result, ensure_ascii=False), "--tier", "semantic",
         "--tag", "#kind:campaign", "--tag", "#domain:adw"],
        capture_output=True, timeout=30, check=False,
    )
    print(json.dumps(result, ensure_ascii=False, indent=1))
    return 0 if status == "converged" else 1


def cmd_test(root: Path, name: str, variables: dict[str, str] | None = None) -> int:
    """Edge test: run the full workflow with agent nodes replayed from recordings."""
    try:
        spec = load_spec(root, name)
    except SpecError as err:
        print(json.dumps({"status": "failed", "error": str(err)}, ensure_ascii=False))
        return 1
    outcome = execute(spec, root, force_mock=True, variables=variables or {})
    print(json.dumps({
        "status": outcome.status, "run_id": outcome.run_id, "mocked": True,
        "class_d_divergence": outcome.class_d,
        # Named, not counted: "3 synthesized" tells a reader nothing about WHICH
        # part of the flow is still unproven.
        "synthesized": outcome.synthesized, "steps": outcome.steps,
    }, ensure_ascii=False, indent=1))
    return 0 if outcome.status == "completed" else 1


def cmd_from_template(root: Path, name: str) -> int:
    """Instantiate a spec: from the central library when a template exists there
    (F3 — `touring adw from-template bugfix`), else the generic scaffold."""
    target = adw_dir(root) / f"{name}.toml"
    if target.exists():
        print(json.dumps({"created": False, "error": f"already exists: {target}"}, ensure_ascii=False))
        return 1
    target.parent.mkdir(parents=True, exist_ok=True)
    lib_template = library_dir() / f"{name}.toml"
    if lib_template.is_file():
        target.write_text(lib_template.read_text(encoding="utf-8"), encoding="utf-8")
        source = "library"
    else:
        target.write_text(TEMPLATE.format(name=name), encoding="utf-8")
        source = "scaffold"
    print(json.dumps({"created": True, "path": str(target), "source": source}, ensure_ascii=False))
    return 0


# ── B2: `adw new` — guided creation from fragments ───────────────────────────
# Creating a flow is where a portfolio is won or lost. Left to `cp`, every new flow
# is a fork of the nearest one and the shared pieces drift apart. This path makes
# the disciplined route the cheap one: prior art with an explicit verdict, a stated
# destination, composition from the kit, and a flow that lints and runs before it is
# ever handed over — the author never edits TOML to reach a working baseline.
VERDICTS = ("reuse", "extend", "supersede", "create_new")


class OrderedStep(argparse.Action):
    """Record --use/--job in the order they were typed.

    argparse hands each option its own list, which loses the interleaving — and a
    flow's step order IS the flow. `--use recall --job fix --use gate` silently
    became recall → gate → fix, so the verification ran before the work it verifies."""

    def __call__(self, parser, namespace, values, option_string=None):
        steps = getattr(namespace, "steps", None) or []
        steps.append(("use" if option_string == "--use" else "job", values))
        setattr(namespace, "steps", steps)
        setattr(namespace, self.dest, [v for k, v in steps if k == self.dest[:3]])


def _toml_value(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(_toml_value(v) for v in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{k} = {_toml_value(v)}" for k, v in value.items()) + " }"
    return json.dumps(str(value), ensure_ascii=False)


def _toml_table(header: str, body: dict) -> str:
    lines = [f"[{header}]"]
    lines += [f"{k} = {_toml_value(v)}" for k, v in body.items() if v is not None]
    return "\n".join(lines)


def _parse_step(raw: str, kind: str) -> tuple[str, str]:
    """`module:as` or `name:type`; the second half defaults sensibly when omitted."""
    head, _, tail = raw.partition(":")
    head = head.strip()
    if not head:
        raise SpecError(f"--{kind} `{raw}`: empty name")
    return head, (tail.strip() or ("agent" if kind == "job" else head))


def prior_art(intent: str) -> str:
    """Consult the capability portfolio by PURPOSE. Evidence, never a constraint."""
    try:
        probe = subprocess.run(["touring", "portfolio", intent, "--top", "5"],
                               capture_output=True, text=True, timeout=90)
        return (probe.stdout or probe.stderr or "").strip()[:2000]
    except Exception as err:
        return f"(portfolio unavailable: {err!r})"


def _fragment_exit_node(module: str, root: Path) -> str | None:
    """The node a fragment leaves through — the one whose verdict the host can read."""
    data = tomllib.loads(find_fragment(root, module).read_text(encoding="utf-8"))
    for local, body in (data.get("node") or {}).items():
        if str(body.get("on_pass", "")) == EXIT:
            return local
    return None


def _verdict_reference(rest: list[str], by_name: dict, root: Path) -> str:
    """Point an agent at the verdict of whatever verifies it next.

    Generated flows must satisfy the blind-retry rule by construction — a creation
    path that emits specs the lint then complains about has taught the author
    nothing except to ignore warnings."""
    for later in rest:
        kind, a, b = by_name[later]
        target = None
        if kind == "job" and b == "gate":
            target = a
        elif kind == "use":
            exit_node = _fragment_exit_node(a, root)
            target = f"{b}{NAMESPACE_SEP}{exit_node}" if exit_node else None
        if target:
            return (f" Verification verdict of the previous attempt (empty on the first "
                    f"pass): {{{{nodes.{target}.summary}}}}")
    return ""


def _new_flow_source(name: str, args: argparse.Namespace, root: Path) -> str:
    """Assemble the spec text: [adw] + [purpose] + an ordered chain of steps."""
    steps: list[tuple[str, str, str]] = []  # (kind, a, b) in the order the user typed
    for kind, raw in getattr(args, "steps", None) or []:
        a, b = _parse_step(raw, kind)
        steps.append((kind, a, b))
    if not steps:
        raise SpecError("a flow needs at least one --use or --job")
    order = list(args.order or []) or [b if k == "job" else c for k, b, c in steps]
    by_name = {(c if k == "use" else b): (k, b, c) for k, b, c in steps}
    unknown = [n for n in order if n not in by_name]
    if unknown:
        raise SpecError(f"--order names unknown step(s) {unknown}; known: {sorted(by_name)}")
    if len(order) != len(by_name):
        raise SpecError(f"--order must list every step exactly once "
                        f"(got {len(order)} for {len(by_name)} steps)")

    binds: dict[str, str] = {}
    for raw in args.bind or []:
        key, _, value = raw.partition("=")
        binds[key.strip()] = value.strip()

    # Feedback wiring: a verifying step hands control back to the nearest preceding
    # agent, and that agent's prompt is generated already reading the verdict — so a
    # flow born here can never be one of the blind retries the lint warns about.
    def previous_agent(index: int) -> str | None:
        for earlier in reversed(order[:index]):
            kind, a, b = by_name[earlier]
            if kind == "job" and b == "agent":
                return a
        return None

    chunks: list[str] = []
    for index, step_name in enumerate(order):
        kind, a, b = by_name[step_name]
        nxt = order[index + 1] if index + 1 < len(order) else END
        back = previous_agent(index) or FAIL
        if kind == "use":
            module, alias = a, b
            frag = tomllib.loads(find_fragment(root, module).read_text(encoding="utf-8"))
            declared = list((frag.get("fragment") or {}).get("inputs") or [])
            with_block = {inp: binds.get(f"{alias}.{inp}", f"{{{{vars.{inp}}}}}")
                          for inp in declared}
            body = {"module": module, "as": alias}
            if with_block:
                body["with"] = with_block
            body["on_pass"] = nxt
            body["on_fail"] = back
            chunks.append(_toml_table("[use]", body))
        elif b == "agent":
            gate_ref = _verdict_reference(order[index + 1:], by_name, root)
            chunks.append(_toml_table(f"node.{a}", {
                "type": "agent", "driver": "claude", "tier": args.tier,
                # A generated agent that can write must be able to write: without
                # this a headless run asks a human who is not there.
                "permission_mode": "acceptEdits",
                "prompt": f"{args.intent}. Step `{a}`.{gate_ref}",
                "allowed_tools": ["Read", "Grep", "Glob", "Edit", "Write", "Bash"],
                "session": "resume_on_fail", "timeout_ms": 900000,
                "on_pass": nxt, "on_fail": FAIL,
            }))
        elif b == "gate":
            chunks.append(_toml_table(f"node.{a}", {
                "type": "gate",
                "command": ["bash", "-c", "$1", "--", f"{{{{vars.{a}_cmd}}}}"],
                "timeout_ms": 900000, "idempotent": True,
                "on_pass": nxt, "on_fail": back,
            }))
        else:
            chunks.append(_toml_table(f"node.{a}", {
                "type": "code",
                "command": ["bash", "-c", "echo \"$1\"", "--", f"{{{{vars.{a}_input}}}}"],
                "timeout_ms": 120000, "idempotent": True,
                "on_pass": nxt, "on_fail": nxt,
            }))

    header = _toml_table("adw", {
        "name": name,
        "description": args.intent,
        # `order[0]` is the step's own handle: a namespace for a [[use]] (which the
        # loader derefs to that fragment's entry) or the node name for a job.
        "entry": order[0],
        "budget_tokens": 0,
    })
    # [purpose] is what makes a flow findable by INTENT rather than by filename.
    # `when_not_to_use` is required by construction: a portfolio entry that only
    # advertises is a sales pitch, and the next author cannot rule it out.
    purpose = _toml_table("purpose", {
        "intent": args.intent,
        "when_to_use": args.when_to_use or [args.intent],
        "when_not_to_use": args.when_not_to_use,
        "class": args.purpose_class,
        "produces": args.produces or ["a verified change"],
        "tags": args.tag or [],
        "prior_art_verdict": args.verdict,
    })
    banner = (f"# ADW `{name}` — created by `touring adw new` on prior-art verdict "
              f"`{args.verdict}`.\n# Run: touring adw run {name} --var <k>=<v> ...\n")
    return banner + "\n" + header + "\n\n" + purpose + "\n\n" + "\n\n".join(chunks) + "\n"


def cmd_new(root: Path, args: argparse.Namespace) -> int:
    target = adw_dir(root) / f"{args.name}.toml"
    if target.exists():
        print(json.dumps({"created": False, "error": f"already exists: {target}"},
                         ensure_ascii=False))
        return 1
    if not (args.intent or "").strip():
        print(json.dumps({"created": False,
                          "error": "--intent is required: state the destination in one sentence "
                                   "before building a route to it"}, ensure_ascii=False))
        return 1
    if not args.when_not_to_use:
        print(json.dumps({"created": False,
                          "error": "--when-not-to-use is required: a flow that never says when it "
                                   "is the wrong tool cannot be ruled out by the next author"},
                         ensure_ascii=False))
        return 1

    evidence = prior_art(args.intent)
    if args.verdict not in VERDICTS:
        print(json.dumps({
            "created": False,
            "error": f"--verdict must be one of {list(VERDICTS)}",
            "prior_art": evidence,
            "next_action": "read the prior art above, then re-run with an explicit --verdict",
        }, ensure_ascii=False, indent=1))
        return 1
    if args.verdict == "reuse":
        print(json.dumps({
            "created": False, "verdict": "reuse", "prior_art": evidence,
            "reason": "the verdict says something already serves this purpose — "
                      "reuse it instead of adding a near-duplicate to the portfolio",
        }, ensure_ascii=False, indent=1))
        return 1

    try:
        source = _new_flow_source(args.name, args, root)
    except SpecError as err:
        print(json.dumps({"created": False, "error": str(err)}, ensure_ascii=False))
        return 1
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(source, encoding="utf-8")

    # Born verified: a flow that does not lint was never created, it was drafted.
    try:
        spec = load_spec(root, args.name)
        errors, warnings = lint_spec(spec)
    except SpecError as err:
        target.unlink()
        print(json.dumps({"created": False, "error": f"generated spec does not load: {err}"},
                         ensure_ascii=False))
        return 1
    if errors:
        target.unlink()
        print(json.dumps({"created": False, "error": "generated spec failed lint",
                          "errors": errors}, ensure_ascii=False, indent=1))
        return 1
    print(json.dumps({
        "created": True, "path": str(target), "verdict": args.verdict,
        "entry": spec.entry, "nodes": sorted(spec.nodes), "lint_warnings": warnings,
        "prior_art": evidence,
        "next_action": f"touring adw explain {args.name}  # then: touring adw run {args.name}",
    }, ensure_ascii=False, indent=1))
    return 0


# ── F5b: racing — N parallel lanes, first-to-pass wins, losers canceled ───────

# Caches, VCS e estado de runtime NUNCA entram numa lane: `target/` custa
# dezenas de GB (REGRA #12), `.git` é história inteira, `.claude/` carrega
# symbols.db (~350MB) e `*.db` são bancos vivos. O merge do vencedor é por
# bytes (não usa git), então excluí-los não muda a semântica do race — só o
# torna executável num workspace real (medido 28/08/2026: sem isto, N lanes
# × o touring copiavam N × ~30GB antes do primeiro nó rodar).
RACE_IGNORE = ("*.pyc", "__pycache__", ".touring", ".touring-explore", ".touring-plan",
               "target", ".git", ".claude", "node_modules", "*.db")


def _copy_lane(root: Path, lane_dir: Path) -> None:
    import shutil
    shutil.copytree(root, lane_dir, ignore=shutil.ignore_patterns(*RACE_IGNORE),
                    dirs_exist_ok=False)


def _merge_winner(root: Path, lane_dir: Path) -> list[str]:
    """Serialize the merge: only the winner writes back — changed files only."""
    merged: list[str] = []
    for src in lane_dir.rglob("*"):
        if not src.is_file():
            continue
        rel = src.relative_to(lane_dir)
        if any(part.startswith(".touring") or part == "__pycache__" for part in rel.parts):
            continue
        dst = root / rel
        if not dst.is_file() or dst.read_bytes() != src.read_bytes():
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(src.read_bytes())
            merged.append(str(rel))
    return merged


def cmd_race(root: Path, name: str, n_lanes: int, variables: dict[str, str]) -> int:
    """Race N independent copies of the project through the same ADW; the first
    lane whose run passes wins, the losers are canceled [video 16:30]. Lanes are
    plain directory copies (no VCS involved); the merge is winner-only and the
    write-set goes through `touring conflict-check` for the audit trail."""
    try:
        spec = load_spec(root, name)
    except SpecError as err:
        print(json.dumps({"status": "failed", "error": str(err)}, ensure_ascii=False))
        return 1
    race_id = f"race-{spec.name}-{int(time.time())}"
    race_dir = root / ".touring" / "adw-races" / race_id
    race_dir.mkdir(parents=True)

    procs = _spawn_lanes(root, race_dir, spec, name, n_lanes, variables)
    winner, lanes_report = _await_first_pass(procs)

    merged: list[str] = []
    conflict_probe = ""
    if winner is not None:
        merged = _merge_winner(root, race_dir / f"lane{winner}")
        conflict_probe = _conflict_probe(merged)

    report = {"race_id": race_id, "winner": winner, "lanes": lanes_report,
              "merged_files": merged, "conflict_check": conflict_probe}
    (race_dir / "race.json").write_text(json.dumps(report, ensure_ascii=False, indent=1),
                                        encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, indent=1))
    return 0 if winner is not None else 1


def _spawn_lanes(root: Path, race_dir: Path, spec: Spec, name: str, n_lanes: int,
                 variables: dict[str, str]) -> list[tuple[int, subprocess.Popen, Path, float]]:
    procs: list[tuple[int, subprocess.Popen, Path, float]] = []
    for lane in range(n_lanes):
        lane_dir = race_dir / f"lane{lane}"
        _copy_lane(root, lane_dir)
        (lane_dir / ".touring" / "adw").mkdir(parents=True, exist_ok=True)
        (lane_dir / ".touring" / "adw" / f"{name}.toml").write_text(
            spec.path.read_text(encoding="utf-8"), encoding="utf-8")
        cmd = [sys.executable, str(Path(__file__).resolve()), "--root", str(lane_dir),
               "run", name, "--var", f"lane={lane}"]
        for key, value in variables.items():
            cmd += ["--var", f"{key}={value}"]
        procs.append((lane, subprocess.Popen(cmd, stdout=subprocess.DEVNULL,
                                             stderr=subprocess.DEVNULL, cwd=lane_dir),
                      lane_dir, time.time()))
    return procs


def _await_first_pass(procs) -> tuple[int | None, list[dict]]:
    """Poll the lanes; first exit 0 wins, still-running losers are terminated."""
    winner: int | None = None
    lanes_report: list[dict] = []
    finished: set[int] = set()
    while len(finished) < len(procs) and winner is None:
        for lane, proc, _, started in procs:
            if lane in finished or proc.poll() is None:
                continue
            finished.add(lane)
            lanes_report.append({"lane": lane, "exit": proc.returncode,
                                 "duration_s": round(time.time() - started, 2)})
            if proc.returncode == 0 and winner is None:
                winner = lane
        time.sleep(0.05)
    for lane, proc, _, started in procs:  # losers are canceled, not awaited
        if proc.poll() is None:
            proc.terminate()
            lanes_report.append({"lane": lane, "exit": "canceled",
                                 "duration_s": round(time.time() - started, 2)})
    return winner, lanes_report


def _conflict_probe(merged: list[str]) -> str:
    if not merged:
        return ""
    try:
        probe = subprocess.run(
            ["touring", "conflict-check", *(f"writes:{m}" for m in merged)],
            capture_output=True, text=True, timeout=30)
        return probe.stdout.strip().splitlines()[0] if probe.stdout else ""
    except Exception:
        return "conflict-check unavailable"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="touring adw", description=__doc__)
    parser.add_argument("--root", default=".", help="project root (default: cwd)")
    sub = parser.add_subparsers(dest="sub", required=True)
    sub.add_parser("list")
    p_lint = sub.add_parser("lint")
    p_lint.add_argument("name")
    p_run = sub.add_parser("run")
    p_run.add_argument("name")
    p_run.add_argument("--resume-run", default=None)
    p_run.add_argument("--approve", action="append", default=[])
    p_run.add_argument("--record", action="store_true")
    p_run.add_argument("--mock", action="store_true")
    p_run.add_argument("--var", action="append", default=[])
    p_test = sub.add_parser("test")
    p_test.add_argument("name")
    p_test.add_argument("--var", action="append", default=[])
    p_tpl = sub.add_parser("from-template")
    p_tpl.add_argument("name")
    p_exp = sub.add_parser("explain")
    p_exp.add_argument("--mermaid", action="store_true",
                       help="draw the resolved flow graph (Isenberg level 1) "
                            "instead of printing the flat JSON")
    p_exp.add_argument("name")
    p_exp.add_argument("--cost", action="store_true",
                       help="add the declared ceiling: agent calls, fan-out width, timeout budget")
    p_prom = sub.add_parser("promote")
    p_prom.add_argument("name")
    p_prom.add_argument("--run", default=None, help="run_id whose outcome is the evidence")
    p_prom.add_argument("--exempt", default=None, help="declare why this flow ships without a run")
    sub.add_parser("fragments")
    p_new = sub.add_parser("new")
    p_new.add_argument("name")
    p_new.add_argument("--intent", default="", help="the destination, in one sentence")
    p_new.add_argument("--verdict", default="", help=f"prior-art verdict: {'|'.join(VERDICTS)}")
    p_new.add_argument("--use", action=OrderedStep, default=[], metavar="MODULE[:AS]",
                       help="compose a fragment into the flow (order is significant)")
    p_new.add_argument("--job", action=OrderedStep, default=[], metavar="NAME[:TYPE]",
                       help="a step of your own (agent|gate|code); default agent")
    p_new.add_argument("--bind", action="append", default=[], metavar="AS.INPUT=VALUE",
                       help="bind a fragment input (default: {{vars.<input>}})")
    p_new.add_argument("--order", action="append", default=[],
                       help="explicit step order; default is the order given")
    p_new.add_argument("--tier", default="workhorse")
    p_new.add_argument("--when-to-use", action="append", default=[])
    p_new.add_argument("--when-not-to-use", action="append", default=[])
    p_new.add_argument("--produces", action="append", default=[])
    p_new.add_argument("--tag", action="append", default=[])
    p_new.add_argument("--purpose-class", default=None, dest="purpose_class",
                       choices=VALID_PURPOSE_CLASSES,
                       help="the flow kind (fix|create|review) — what lets System-1 "
                            "screening enter by class (N5)")
    p_camp = sub.add_parser("campaign")
    p_camp.add_argument("name")
    p_camp.add_argument("--until", required=True,
                        help="CODE predicate run after each round; exit 0 = converged; "
                             "stdout may carry METRIC=<float> (curve + stagnation signal)")
    p_camp.add_argument("--max-rounds", default=10)
    p_camp.add_argument("--stagnation-rounds", default=0,
                        help="stop after N consecutive rounds with the SAME present metric (0 = off)")
    p_camp.add_argument("--mock", action="store_true")
    p_camp.add_argument("--var", action="append", default=[])
    p_race = sub.add_parser("race")
    p_race.add_argument("name")
    p_race.add_argument("--lanes", type=int, default=2)
    p_race.add_argument("--var", action="append", default=[])
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()

    if args.sub == "list":
        return cmd_list(root)
    if args.sub == "lint":
        return cmd_lint(root, args.name)
    if args.sub == "run":
        return cmd_run(root, args)
    if args.sub == "test":
        return cmd_test(root, args.name, dict(kv.split("=", 1) for kv in (args.var or [])))
    if args.sub == "from-template":
        return cmd_from_template(root, args.name)
    if args.sub == "explain":
        return cmd_explain(root, args.name, cost=args.cost, mermaid=args.mermaid)
    if args.sub == "promote":
        return cmd_promote(root, args.name, args.run, args.exempt)
    if args.sub == "fragments":
        return cmd_fragments(root)
    if args.sub == "new":
        return cmd_new(root, args)
    if args.sub == "campaign":
        return cmd_campaign(root, args)
    if args.sub == "race":
        return cmd_race(root, args.name, args.lanes,
                        dict(kv.split("=", 1) for kv in (args.var or [])))
    return 2


if __name__ == "__main__":
    sys.exit(main())
