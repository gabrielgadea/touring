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

Spec: .touring/adw/<name>.toml — [adw] name/entry/budget_tokens, [node.X] typed
  code  : command[] + timeout_ms + retries + idempotent + sandbox
  agent : driver(claude|mock) + prompt + tier + allowed_tools + budget_usd +
          max_turns(reserved) + session(fresh|resume_on_fail)
  gate  : same as code; a FAIL right after a success-narrating agent → Class-D
  loop  : body + max_iters + dry_rounds (body prints NEW_FINDINGS=<n>; runner counts)
  human : message; passes only with --approve <node> on (re)run
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
import time
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

# ── constants ─────────────────────────────────────────────────────────────────

END = "__end__"
FAIL = "__fail__"
TERMINALS = {END, FAIL}
NODE_TYPES = {"code", "agent", "gate", "loop", "human"}
SUMMARY_LIMIT = 2000  # bytes of dense inline summary (Law L4)
DEFAULT_TIMEOUT_MS = 120_000
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
    description: str = ""

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


def load_spec(root: Path, name: str) -> Spec:
    path = adw_dir(root) / f"{name}.toml"
    if not path.is_file():
        raise SpecError(f"spec not found: {path}")
    data = tomllib.loads(path.read_text(encoding="utf-8"))
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
        description=meta.get("description", ""),
    )


# ── lint (G19 + budget-verify) ────────────────────────────────────────────────


def _lint_node(node: Node, names: set[str], errors: list[str], warnings: list[str]) -> None:
    for edge_name, target in (
        ("on_pass", node.on_pass),
        ("on_fail", node.on_fail),
        ("on_dry", node.on_dry),
    ):
        if target not in names and target not in TERMINALS:
            errors.append(f"node `{node.name}`: {edge_name} → unknown node `{target}`")
    if node.type == "loop":
        body = node.raw.get("body", "")
        if body not in names:
            errors.append(f"loop `{node.name}`: body → unknown node `{body}`")
        if int(node.raw.get("max_iters", 0)) <= 0:
            errors.append(f"loop `{node.name}`: max_iters must be >= 1 (Law L2: runner owns termination)")
    if node.type in {"code", "gate"}:
        if not node.raw.get("command"):
            errors.append(f"node `{node.name}`: missing command[]")
        if not node.raw.get("idempotent", False):
            warnings.append(f"node `{node.name}`: not marked idempotent — resume replays at-least-once")
    if node.type == "agent":
        driver = node.raw.get("driver", "claude")
        if driver not in {"claude", "mock"}:
            errors.append(f"agent `{node.name}`: unknown driver `{driver}`")
        if driver == "claude" and not node.raw.get("prompt"):
            errors.append(f"agent `{node.name}`: missing prompt")
        if not node.raw.get("allowed_tools"):
            warnings.append(f"agent `{node.name}`: no allowed_tools — driver runs fail-closed default")


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
        stack.extend(t for t in (node.on_pass, node.on_fail, node.on_dry) if t not in TERMINALS)
        if node.type == "loop" and node.raw.get("body") in spec.nodes:
            stack.append(node.raw["body"])
    for orphan in sorted(set(spec.nodes) - reachable):
        warnings.append(f"node `{orphan}`: unreachable from entry `{spec.entry}` (orphan)")


def _lint_cycles(spec: Spec, errors: list[str]) -> None:
    """Pass-edge cycle where no member exits and none is a loop node → error."""
    for node in spec.nodes.values():
        seen: list[str] = []
        cur = node.name
        while cur not in TERMINALS and cur not in seen and cur in spec.nodes:
            seen.append(cur)
            cur = spec.nodes[cur].on_pass
        if cur in seen:
            cycle = seen[seen.index(cur):]
            has_exit = any(
                spec.nodes[m].on_fail not in cycle or spec.nodes[m].type == "loop"
                for m in cycle
            )
            if not has_exit:
                errors.append(f"cycle without exit: {' → '.join(cycle)}")
            return


def _lint_budget(spec: Spec, errors: list[str]) -> None:
    """budget-verify (I6/C11): sum of node budgets must fit the run budget."""
    if spec.budget_tokens <= 0:
        return
    total = sum(int(n.raw.get("budget_tokens", 0)) for n in spec.nodes.values())
    if total > spec.budget_tokens:
        errors.append(f"budget-verify: Σ node budgets ({total}) > run budget ({spec.budget_tokens})")


def lint_spec(spec: Spec) -> tuple[list[str], list[str]]:
    """Return (errors, warnings). Errors make `adw lint` exit non-zero."""
    errors: list[str] = []
    warnings: list[str] = []
    names = set(spec.nodes)
    for node in spec.nodes.values():
        _lint_node(node, names, errors, warnings)
    _lint_reachability(spec, warnings)
    _lint_cycles(spec, errors)
    _lint_budget(spec, errors)
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


def render_template(text: str, results: dict[str, dict], variables: dict[str, str]) -> str:
    """Resolve {{nodes.X.summary}} / {{vars.k}} references (inter-node context)."""

    def sub(match: re.Match) -> str:
        kind, key = match.group(1), match.group(2)
        if kind == "nodes":
            return results.get(key, {}).get("summary", "")
        return variables.get(key, "")

    return re.sub(r"\{\{(nodes|vars)\.([\w.-]+?)(?:\.summary)?\}\}", sub, text)


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


def run_code_node(node: Node, results: dict, variables: dict,
                  cwd: Path | None = None) -> ExecResult:
    command = [render_template(str(part), results, variables) for part in node.raw["command"]]
    if node.raw.get("sandbox", False):
        command = ["touring", "run", "--lang", "bash", "--code", " ".join(shlex.quote(c) for c in command)]
    timeout_s = int(node.raw.get("timeout_ms", DEFAULT_TIMEOUT_MS)) / 1000
    retries = int(node.raw.get("retries", 0))
    attempt = 0
    while True:
        try:
            proc = subprocess.run(command, capture_output=True, text=True,
                                  timeout=timeout_s, cwd=cwd)
            output = proc.stdout + (("\n" + proc.stderr) if proc.stderr.strip() else "")
            result = ExecResult(exit_code=proc.returncode, output=output)
        except subprocess.TimeoutExpired:
            result = ExecResult(exit_code=124, output=f"timeout after {timeout_s:.0f}s")
        if result.exit_code == 0 or attempt >= retries:
            return result
        attempt += 1
        time.sleep(min(2 ** attempt, 10))


def mock_recording_path(spec: Spec, node: str) -> Path:
    return spec.path.parent / f"{spec.name}.recordings" / f"{node}.json"


def _agent_mock(spec: Spec, node: Node) -> ExecResult:
    rec_path = mock_recording_path(spec, node.name)
    if not rec_path.is_file():
        return ExecResult(exit_code=1, output=f"no recording for agent `{node.name}` at {rec_path}")
    rec = json.loads(rec_path.read_text(encoding="utf-8"))
    output = rec.get("result", "")
    return ExecResult(
        exit_code=int(rec.get("exit_code", 0)),
        output=output,
        session_id=rec.get("session_id"),
        narrated_success=bool(SUCCESS_NARRATIVE.search(output)),
    )


def _claude_cmd(node: Node, journal: Journal, prompt: str) -> list[str]:
    """Assemble the fail-closed headless invocation from the node spec."""
    cmd = ["claude", "-p", prompt, "--output-format", "json"]
    tier = node.raw.get("tier", "")
    if tier:
        cmd += ["--model", tier_models().get(tier, tier)]
    tools = node.raw.get("allowed_tools") or []
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
                  cwd: Path | None = None) -> ExecResult:
    cmd = _claude_cmd(node, journal, prompt)
    timeout_s = int(node.raw.get("timeout_ms", 600_000)) / 1000
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout_s, cwd=cwd)
    except subprocess.TimeoutExpired:
        return ExecResult(exit_code=124, output=f"agent timeout after {timeout_s:.0f}s")
    session_id = None
    output = proc.stdout
    try:
        parsed = json.loads(proc.stdout)
        session_id = parsed.get("session_id")
        output = parsed.get("result", proc.stdout)
    except (json.JSONDecodeError, AttributeError):
        pass
    result = ExecResult(
        exit_code=proc.returncode,
        output=output,
        session_id=session_id,
        narrated_success=bool(SUCCESS_NARRATIVE.search(output or "")),
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
) -> ExecResult:
    driver = "mock" if force_mock else node.raw.get("driver", "claude")
    prompt = render_template(node.raw.get("prompt", ""), results, variables)
    if driver == "mock":
        return _agent_mock(spec, node)
    return _agent_claude(spec, node, journal, prompt, record, cwd=cwd)


# ── runner ────────────────────────────────────────────────────────────────────


@dataclass
class RunOutcome:
    status: str  # completed | failed | waiting_human
    run_id: str
    steps: list[dict] = field(default_factory=list)
    class_d: bool = False


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


def execute(
    spec: Spec, root: Path, resume_run: str | None = None,
    approve: set[str] | None = None, force_mock: bool = False,
    record: bool = False, variables: dict[str, str] | None = None,
) -> RunOutcome:
    approve = approve or set()
    variables = variables or {}
    run_id = resume_run or f"{spec.name}-{int(time.time())}"
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
        journal.append("run_finished", status=outcome.status, class_d=outcome.class_d)
        activity_append("task_completed", {"adw": spec.name, "run": run_id, "status": outcome.status})
    finally:
        lock.close()
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


def _run_single_node(ctx: _RunCtx, node: Node, exec_key: str) -> str | None:
    """Execute one non-loop node; returns the next node, or None on human pause."""
    ctx.journal.append("node_started", node=node.name, exec_key=exec_key, type=node.type)
    activity_append("task_started", {"adw": ctx.spec.name, "run": ctx.run_id, "node": node.name})

    if node.type == "human":
        result = _human_node(ctx, node, exec_key)
        if result is None:
            return None
    elif node.type == "agent":
        result = run_agent_node(node, ctx.spec, ctx.journal, ctx.results, ctx.variables,
                                ctx.force_mock, ctx.record, cwd=ctx.root)
    else:  # code | gate
        result = run_code_node(node, ctx.results, ctx.variables, cwd=ctx.root)

    ctx.results[node.name] = store_result(ctx.run_path, exec_key, result.output)
    passed = result.exit_code == 0
    class_d = _track_class_d(ctx, node, exec_key, result, passed)
    nxt = _next_edge(ctx, node, exec_key, passed)
    ctx.journal.append(
        "node_completed", node=node.name, exec_key=exec_key,
        exit_code=result.exit_code, verdict="pass" if passed else "fail",
        next=nxt, session_id=result.session_id, class_d=class_d,
    )
    activity_append(
        "task_completed" if passed else "error_occurred",
        {"adw": ctx.spec.name, "run": ctx.run_id, "node": node.name, "exit": result.exit_code},
    )
    ctx.outcome.steps.append({"node": node.name, "exec_key": exec_key,
                              "exit_code": result.exit_code,
                              "verdict": "pass" if passed else "fail"})
    return nxt


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
    elif node.type == "gate":
        ctx.last_agent = None
    return class_d


def _next_edge(ctx: _RunCtx, node: Node, exec_key: str, passed: bool) -> str:
    """Law L2 — the runner, not the graph, terminates feedback loops: a node that
    keeps failing exhausts its retry budget and the run fails loud instead of
    re-invoking (and re-billing) agents forever."""
    if passed:
        return node.on_pass
    nxt = node.on_fail
    ctx.fail_counts[node.name] = ctx.fail_counts.get(node.name, 0) + 1
    max_retries = int(node.raw.get("max_retries", 3))
    if nxt not in TERMINALS and ctx.fail_counts[node.name] > max_retries:
        ctx.journal.append("retry_limit_exceeded", node=node.name,
                           exec_key=exec_key, failures=ctx.fail_counts[node.name],
                           max_retries=max_retries)
        nxt = FAIL
    return nxt


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
    """Execute a loop node: the RUNNER counts findings and owns termination (L2)."""
    body = spec.node(node.raw["body"])
    max_iters = int(node.raw.get("max_iters", 1))
    dry_rounds = int(node.raw.get("dry_rounds", 2))
    dry = 0
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
            journal.append("node_started", node=body.name, exec_key=exec_key, type=body.type)
            if body.type == "agent":
                result = run_agent_node(body, spec, journal, results, variables,
                                        force_mock, record, cwd=run_path.parent.parent.parent)
            else:
                result = run_code_node(body, results, variables, cwd=run_path.parent.parent.parent)
            store_result(run_path, exec_key, result.output)
            results[body.name] = {"summary": result.output[:SUMMARY_LIMIT],
                                  "omitted_bytes": 0, "full_ref": ""}
            journal.append("node_completed", node=body.name, exec_key=exec_key,
                           exit_code=result.exit_code,
                           verdict="pass" if result.exit_code == 0 else "fail",
                           next=node.name, session_id=result.session_id, class_d=False)
            outcome.steps.append({"node": body.name, "exec_key": exec_key,
                                  "exit_code": result.exit_code})
            if result.exit_code != 0:
                return node.on_fail
            output = result.output
        found = NEW_FINDINGS_RE.search(output or "")
        new_findings = int(found.group(1)) if found else 0
        dry = dry + 1 if new_findings == 0 else 0
        journal.append("loop_round", loop=node.name, body_exec=exec_key,
                       new_findings=new_findings, dry_streak=dry)
        if dry >= dry_rounds:
            return node.on_dry
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
        "class_d_divergence": outcome.class_d, "steps": outcome.steps,
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


# ── F5b: racing — N parallel lanes, first-to-pass wins, losers canceled ───────

RACE_IGNORE = ("*.pyc", "__pycache__", ".touring", ".touring-explore", ".touring-plan")


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
    if args.sub == "race":
        return cmd_race(root, args.name, args.lanes,
                        dict(kv.split("=", 1) for kv in (args.var or [])))
    return 2


if __name__ == "__main__":
    sys.exit(main())
