#!/usr/bin/env python3
"""Session Startup Intelligence Hook for Claude Code + Touring.

Emits a structured intelligence dashboard at SessionStart, in TWO sections:

    1. TOURING — Estado Global de Inteligencia
       - Health (doctor + composite_health_score)
       - Learning (EMA reward, TD error, agentic_rl state)
       - Synergy & Wiring (wired_pairs count)
       - Telemetry (CEG counters, health_delta records)
       - Recommendations (data-driven from current state)

    2. PATH/PROJETO ATUAL (cwd-aware)
       - Recent memories filtered by path prefix
       - Relevant plans in ~/.claude/plans/
       - Pending decompose tasks (DAGs)
       - Technical diagnostic (orphans scoped)
       - Next actions per project type (rust|python|node|touring-workspace|generic)

Output contract (stdout, single JSON line):
    {
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": "<composed markdown report>"
        }
    }

Fail-open: any internal error emits minimal valid JSON with status note.
Total budget: <5s (settings.json timeout=8). Per-touring-call timeout: 1.5-3s.
An absent source is shown as "indisponível (<reason>)", never as 0.00 or 0/0, and derives no recommendation
(D1, analise-92, 06/10/2026: `status -j` took 4.6 s against a 1.5 s timeout and every panel read 0.00).

S-04 (SNR slice) — SLIM mode + cache-prefix discipline
------------------------------------------------------
Anthropic prompt-caching + context-editing best practice: keep the model's
prompt prefix STABLE so it stays cache-warm, and confine the churn to a short,
recomputed tail. This SessionStart context is a prompt prefix — so the more
volatile telemetry it injects eagerly, the more it invalidates the cache each
session and dilutes attention (the SNR argument: utility is not monotonic in
tokens).

The intelligence this hook can emit splits into two classes:

  * STABLE / cacheable-worthy (high SNR at startup) — the health headline, the
    active plans / ready DAG subtasks (what I am building), the per-project next
    actions, cwd + type. These change slowly and are directly actionable.
  * VOLATILE / pull-on-demand (low SNR eagerly) — RL EMA/TD, CEG counters, DB
    mtimes, cumulative memory/index/bash stats, evolution insights. These churn
    every session and are one command away when actually needed:
    `touring status -j` · `touring gate-metrics -j` · `touring learning status`
    · `touring evolution insights -j`.

SLIM is the DEFAULT (Gabriel, 2026-07-04): unset emits ONLY the stable class
(and fewer touring calls, so startup is faster and the prefix is more stable) —
the A/B (S-05) showed this −83.6% prefix is the real STR win. Kill-switch:
`TOURING_SESSIONSTART_SLIM=0` (or false/no/off) restores the verbose dashboard.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

HOOK_NAME = "SessionStart"
PER_CMD_TIMEOUT = 1.5
STATUS_TIMEOUT = 3.0  # status walks more than doctor; still inside TOTAL_BUDGET_S
PATH_CMD_TIMEOUT = 2.0
TOTAL_BUDGET_S = 4.5
JEV_TIMEOUT_S = 2.0  # opt-in rerank; two calls must fit the hook's 8 s budget (was 8.0 each)


def slim_enabled() -> bool:
    """S-04: is SLIM (stable-prefix-only) mode active? Default ON (Gabriel,
    2026-07-04) — the slim prefix is the real STR win (−83.6% startup bytes), so
    it is the default. Kill-switch: `TOURING_SESSIONSTART_SLIM=0` (or false/no/off)
    restores the verbose intelligence dashboard."""
    val = os.environ.get("TOURING_SESSIONSTART_SLIM", "").strip().lower()
    return val not in ("0", "false", "no", "off")


def run_touring_detail(args: list[str], timeout: float = PER_CMD_TIMEOUT) -> tuple[str | None, str | None]:
    """(stdout, None) on success; (None, reason) otherwise — the reason travels to the panel."""
    try:
        r = subprocess.run(
            ["touring", *args],
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return None, f"timeout {timeout:g}s"
    except OSError as e:
        return None, f"não executou ({e.__class__.__name__})"
    if r.returncode != 0:
        return None, f"rc={r.returncode}"
    out = r.stdout.strip()
    return (out, None) if out else (None, "saída vazia")


def run_touring(args: list[str], timeout: float = PER_CMD_TIMEOUT) -> str | None:
    """Run touring CLI subcommand, return stdout or None on any failure."""
    return run_touring_detail(args, timeout)[0]


def parse_json_safe(s: str | None) -> Any:
    if not s:
        return None
    try:
        return json.loads(s)
    except (ValueError, TypeError, RecursionError):
        return None


def detect_project_type(cwd: str | None) -> str:
    if not cwd:
        return "unknown"
    p = Path(cwd)
    if not p.exists():
        return "unknown"
    if (p / "Cargo.toml").exists():
        return "rust"
    if (p / "package.json").exists():
        return "node"
    if (p / "pyproject.toml").exists() or (p / "setup.py").exists():
        return "python"
    touring_root = Path.home() / ".claude" / "rust"
    try:
        p.relative_to(touring_root)
        return "touring-workspace"
    except ValueError:
        pass
    if str(p).startswith(str(Path.home() / ".claude")):
        return "claude-config"
    return "generic"


def collect_global(slim: bool = False) -> dict[str, Any]:
    tasks: dict[str, list[str]] = {
        "doctor": ["doctor", "-j"],
        "status": ["status", "-j"],
        "learning": ["learning", "status", "-j"],
        "synergy": ["synergy", "report", "-j"],
        "gate": ["gate-metrics", "-j"],
        "evolution": ["evolution", "insights", "-j"],
        "mem_stats": ["memory", "stats"],
    }
    if slim:
        # S-04: only the stable, high-SNR sources feed the prefix. The volatile
        # telemetry (learning/synergy/gate/evolution/mem_stats) is pulled on demand.
        tasks = {"doctor": tasks["doctor"], "status": tasks["status"]}
    results: dict[str, Any] = {}
    why: dict[str, str] = {}
    with ThreadPoolExecutor(max_workers=7) as ex:
        futures = {ex.submit(run_touring_detail, args, STATUS_TIMEOUT if name == "status" else PER_CMD_TIMEOUT): name
                   for name, args in tasks.items()}
        for f in as_completed(futures, timeout=TOTAL_BUDGET_S):
            name = futures[f]
            try:
                out, reason = f.result()
            except Exception as e:  # noqa: BLE001 — a collector failure is a reason, not a crash
                out, reason = None, f"erro ({e.__class__.__name__})"
            results[name] = parse_json_safe(out)
            if results[name] is None:
                why[name] = reason or "saída não é JSON"
    results["_why"] = why
    results["fs_mtimes"] = _collect_fs_mtimes()
    return results


def _absent(g: dict[str, Any], name: str) -> str:
    """'indisponível (<reason>)' for a source that did not answer."""
    return f"indisponível ({(g.get('_why') or {}).get(name, 'sem resposta')})"


def _composite(g: dict[str, Any]) -> float | None:
    """The measured composite, or None when status did not answer — never a default 0.0."""
    v = (g.get("status") or {}).get("composite_health_score")
    return v if isinstance(v, (int, float)) and not isinstance(v, bool) else None


def _collect_fs_mtimes() -> dict[str, float]:
    """Filesystem mtimes for canonical Touring artifacts (timestamp source)."""
    paths = {
        "symbols_db": Path.home() / ".claude" / "touring" / "symbols.db",
        "update_log": Path.home() / ".claude" / "touring" / "update.log",
        "knowledge_db": Path.home() / ".claude" / "touring" / "knowledge.db",
    }
    out: dict[str, float] = {}
    for name, p in paths.items():
        try:
            if p.exists():
                out[name] = p.stat().st_mtime
        except OSError:
            pass
    return out


def _fmt_age(ts: float | None) -> str:
    """Format unix timestamp as 'há Xmin/Xh/Xd' relative to now."""
    if not ts:
        return "?"
    try:
        delta = max(0.0, time.time() - ts)
        # W6 D9 (KV-cache hygiene, 2026-08-23): coarse buckets — a per-second/
        # per-minute label churns the injected prefix on every session start
        # and invalidates the provider prompt cache for zero information gain.
        if delta < 3600:
            return "recente (<1h)"
        if delta < 86400:
            return "hoje"
        return f"há {int(delta // 86400)}d"
    except (TypeError, ValueError, OverflowError):
        return "?"


def collect_path(cwd: str) -> dict[str, Any]:
    """Collect path-specific intelligence via Touring native sources ONLY.

    Plans and tasks are read from Touring decompose DAGs and memory tier,
    NOT from filesystem .md files. The path is used as the query filter
    for memory recall.
    """
    project_name = Path(cwd).name if cwd else "unknown"
    # Touring memory recall is semantic (ANN+TF-IDF), not key-prefix. Use multiple
    # discriminating queries — short, distinctive — to surface relevant entries.
    path_basename = project_name
    parent_segment = Path(cwd).parent.name if cwd else ""
    tasks: dict[str, list[str]] = {
        "memory_path": ["memory", "recall", path_basename],
        "memory_parent": ["memory", "recall", parent_segment or path_basename],
        "memory_decomp": ["memory", "recall", "decomp"],
        "memory_recent_edits": ["memory", "recall", f"edited {path_basename}"],
        "orphans": ["wiring", "orphans", "-j"],
        "decompose_status": ["decompose", "status", "-j"],
        "decompose_ready": ["decompose", "ready", "-j"],
    }
    results: dict[str, Any] = {}
    with ThreadPoolExecutor(max_workers=6) as ex:
        futures = {
            ex.submit(run_touring, args, PATH_CMD_TIMEOUT): name
            for name, args in tasks.items()
        }
        for f in as_completed(futures, timeout=3.0):
            name = futures[f]
            try:
                results[name] = parse_json_safe(f.result())
            except Exception:  # noqa: BLE001 — a collector failure is a missing source, not a crash
                results[name] = None
    return results


def _health_headline(g: dict[str, Any]) -> list[str]:
    """The one stable, high-SNR line: composite + doctor N/N + degraded parts."""
    out: list[str] = []
    composite = _composite(g)
    doctor = g.get("doctor")
    comp_str = f"**{composite:.2f}**" if composite is not None else _absent(g, "status")
    if isinstance(doctor, list) and doctor:
        ok = sum(1 for d in doctor if isinstance(d, dict) and d.get("status") == "ok")
        doc_str = f"**{ok}/{len(doctor)} OK**"
    else:
        doc_str = _absent(g, "doctor")
    out.append(f"## Saúde: composite {comp_str} · Doctor {doc_str}")
    if isinstance(doctor, list):
        for d in doctor:
            if isinstance(d, dict) and d.get("status") not in ("ok", None):
                out.append(f"  - ⚠ `{d.get('name', '?')}`: {(d.get('detail') or '')[:80]}")
    return out


def _health_recs(g: dict[str, Any]) -> list[str]:
    """Data-driven recommendations, emitted ONLY when there is a real problem."""
    composite = _composite(g)  # None when status did not answer: absence derives no recommendation
    learning = g.get("learning") or {}
    arl = learning.get("agentic_rl_state") or {} if isinstance(learning, dict) else {}
    recs: list[str] = []
    if composite is not None and composite < 0.7:
        recs.append(f"- composite {composite:.2f} < 0.7 → `touring e2e -j` + investigar degradados")
    td = learning.get("mean_td_error") if isinstance(learning, dict) else None
    if isinstance(td, (int, float)) and td > 5.0:
        recs.append("- TD error alto → process rewards step-level (insight NOVO-1)")
    if isinstance(arl, dict) and arl.get("active") and arl.get("update_count", 0) == 0:
        recs.append("- agentic_rl ocioso → wire feeder pós-fase TACO (insight NOVO-2)")
    # W7 S-7.3 (plano code-mode-total): a régua de adoção visível a cada sessão
    # — 1 linha, só quando abaixo do alvo (a mesma disciplina problem-only).
    gate = g.get("gate") or {}
    runs = gate.get("code_mode_runs_count")
    bash = gate.get("bash_calls_total_count")
    if isinstance(runs, (int, float)) and isinstance(bash, (int, float)) and bash >= 20:
        ratio = runs / bash
        if ratio < 0.10:
            recs.append(
                f"- code-mode adoption {ratio:.0%} < alvo 10% → ≥3 arquivos = "
                f"`touring run --file` primeiro (R1-R8: `touring memory query "
                f"\"#kind:snippet #process:code-mode\"`)")
    return recs


def _fmt_global_slim(g: dict[str, Any]) -> str:
    """S-04 SLIM: stable prefix only — health headline + problem-only recs. The
    volatile tail (RL/CEG/mtimes/evolution) is pulled on demand, not injected."""
    out = ["# 🧠 TOURING (slim)", ""]
    out.extend(_health_headline(g))
    recs = _health_recs(g)
    if recs:
        out.append("")
        out.append("## Atenção")
        out.extend(recs)
    out.append("")
    out.append("_telemetria on-demand: `touring status -j` · `gate-metrics -j` · `learning status` · `evolution insights -j`_")
    return "\n".join(out)


def _dag_ready_lines(dr: Any) -> list[str]:
    """Ready/blocked DAG headline + up to 5 ready subtasks (stable, high-SNR)."""
    if not isinstance(dr, dict):
        return []
    ready = dr.get("ready_subtasks") or []
    blocked = dr.get("blocked_subtasks") or []
    if not (ready or blocked):
        return []
    out = [f"## DAG: ready **{len(ready)}** · blocked **{len(blocked)}**"]
    for t in ready[:5]:
        if isinstance(t, dict):
            tid = t.get("id") or t.get("subtask_id") or "?"
            desc = (t.get("description") or t.get("title") or "?")[:80]
            out.append(f"  - `{tid}`: {desc}")
    return out


def _fmt_path_slim(p: dict[str, Any], cwd: str, project_type: str) -> str:
    """S-04 SLIM: the actionable path prefix — active plans + orphans + next
    actions. Verbose memory-recall dumps are dropped (one `memory recall` away)."""
    out = ["", "---", "", f"# 📁 `{cwd}` · `{project_type}`", ""]
    out.extend(_dag_ready_lines(p.get("decompose_ready")))
    orphans = p.get("orphans") or {}
    if isinstance(orphans, dict) and orphans.get("orphan_count"):
        out.append(f"- Orphan pub symbols: **{orphans.get('orphan_count', 0)}** (REGRA #0)")
    out.append("")
    out.extend(_next_actions(project_type))
    return "\n".join(out)


def _next_actions(project_type: str) -> list[str]:
    """Per-project-type next actions (shared by verbose + slim formatters)."""
    out = ["## Próximas Ações Recomendadas"]
    if project_type in ("rust", "touring-workspace"):
        out.append("- `touring ast meta <file> --depth summary -j` antes de edits (file-metadata-first)")
        out.append("- `cargo check --workspace` + `touring e2e -j` para validar baseline")
    elif project_type in ("python", "node"):
        out.append("- `touring ast meta <file>` antes de edits · `touring index find` (VGP)")
    else:
        out.append("- `touring index find <symbol>` antes de criar/usar (VGP)")
        out.append('- `touring memory recall "<topic>"` antes de pesquisar do zero')
    return out


def fmt_global(g: dict[str, Any], slim: bool = False) -> str:
    if slim:
        return _fmt_global_slim(g)
    out: list[str] = []
    out.append("# 🧠 TOURING — Estado Global de Inteligência")
    out.append("")

    composite = _composite(g)
    if composite is not None:
        out.append(f"## Saúde do Sistema (composite_health_score = **{composite:.2f}**)")
    else:
        out.append(f"## Saúde do Sistema (composite {_absent(g, 'status')})")

    doctor = g.get("doctor")
    if isinstance(doctor, list) and doctor:
        ok = sum(1 for d in doctor if isinstance(d, dict) and d.get("status") == "ok")
        total = len(doctor)
        out.append(f"- Doctor: **{ok}/{total} components OK**")
        for d in doctor:
            if isinstance(d, dict) and d.get("status") not in ("ok", None):
                name = d.get("name", "?")
                detail = (d.get("detail") or "")[:80]
                out.append(f"  - ⚠ `{name}`: {detail}")

    learning = g.get("learning") or {}
    arl = learning.get("agentic_rl_state") or {} if isinstance(learning, dict) else {}
    if learning:
        ema = learning.get("ema_reward", 0.0)
        td = learning.get("mean_td_error", 0.0)
        upd = learning.get("update_count", 0)
        arl_active = arl.get("active", False) if isinstance(arl, dict) else False
        arl_upd = arl.get("update_count", 0) if isinstance(arl, dict) else 0
        out.append("")
        out.append("## Aprendizado (RL bandit + agentic_rl)")
        out.append(
            f"- EMA reward: **{ema:.2f}** | TD error: **{td:.1f}** | updates: **{upd}**"
        )
        out.append(f"- agentic_rl: active=**{arl_active}**, updates=**{arl_upd}**")
        if arl_active and arl_upd == 0:
            out.append(
                "  - ⚠ **GAP**: agentic_rl ligado mas sem feeder de rewards "
                "(insight NOVO-2 do roadmap Code-as-Agent-Harness)"
            )
        if isinstance(td, (int, float)) and td > 5.0:
            out.append(
                f"  - ⚠ TD error alto ({td:.2f}) — bandit não convergiu (alvo < 5.0)"
            )

    synergy = g.get("synergy") or {}
    pair_count = 0
    if isinstance(synergy, dict):
        pair_count = (
            synergy.get("wired_pairs_count") or synergy.get("count") or 0
        )
    if pair_count:
        out.append("")
        out.append("## Sinergia & Wiring")
        out.append(f"- Wired pairs ativos: **{pair_count}**")

    gate = g.get("gate") or {}
    if isinstance(gate, dict) and gate:
        ceg_cap = gate.get("ceg_captured_count", 0)
        ceg_block = gate.get("ceg_blocked_count", 0)
        ceg_sandbox = gate.get("ceg_sandboxed_count", 0)
        health_rec = gate.get("health_delta_record_count", 0)
        out.append("")
        out.append("## Telemetria Recente (esta sessão)")
        out.append(
            f"- CEG: capturados={ceg_cap}, sandboxed={ceg_sandbox}, blocked={ceg_block}"
        )
        out.append(f"- Health delta records: {health_rec}")
        if ceg_cap == 0 and ceg_sandbox == 0 and ceg_block == 0:
            out.append(
                "  - ℹ Counters zerados — daemon recém-iniciado "
                "(sem persistência cross-session; insight NOVO-3)"
            )

    # Última atividade de inteligência (timestamps + cumulative counts)
    mem_stats = g.get("mem_stats") or {}
    fs_mt = g.get("fs_mtimes") or {}
    evolution = g.get("evolution") or {}
    if mem_stats or fs_mt or evolution:
        out.append("")
        out.append("## Última Atividade de Inteligência (quando + cumulativo)")
        if isinstance(fs_mt, dict):
            if fs_mt.get("symbols_db"):
                out.append(
                    f"- Symbols DB atualizado: **{_fmt_age(fs_mt['symbols_db'])}**"
                )
            if fs_mt.get("update_log"):
                out.append(
                    f"- Touring binary update.log: **{_fmt_age(fs_mt['update_log'])}**"
                )
            if fs_mt.get("knowledge_db"):
                out.append(
                    f"- Knowledge DB: **{_fmt_age(fs_mt['knowledge_db'])}**"
                )
        if isinstance(mem_stats, dict):
            mc = mem_stats.get("memory_entry_count", 0)
            bc = mem_stats.get("bash_outcome_count", 0)
            ec = mem_stats.get("edit_event_count", 0)
            fc = mem_stats.get("file_count", 0)
            rc = mem_stats.get("relation_count", 0)
            gc = (mem_stats.get("gotcha_stats") or {}).get("total_count", 0)
            out.append(
                f"- Memory entries: **{mc}** | Edits tracked: **{ec}** | "
                f"Bash outcomes: **{bc}**"
            )
            out.append(
                f"- Index: **{fc}** files | **{rc}** relations | "
                f"Gotchas DB: **{gc}**"
            )
        if isinstance(evolution, dict):
            insights = evolution.get("insights") or {}
            bo = insights.get("bash_outcomes") or {}
            if bo:
                sr = bo.get("success_rate", 0)
                total = bo.get("total", 0)
                out.append(
                    f"- Bash success rate (cumulativo): **{sr * 100:.2f}%** "
                    f"({bo.get('success', 0)}/{total})"
                )
            top_edited = insights.get("top_edited_files") or []
            if top_edited:
                out.append("- Top arquivos editados (últimos 30d):")
                for entry in top_edited[:3]:
                    if isinstance(entry, list) and len(entry) >= 2:
                        path = entry[0][:80]
                        count = entry[1]
                        out.append(f"  - `{path}` ({count}×)")
            gotcha = evolution.get("gotcha_coverage") or {}
            if gotcha:
                out.append(
                    f"- Gotcha coverage: resolved=**{gotcha.get('resolved', 0)}**, "
                    f"prevented_errors=**{gotcha.get('prevented_errors', 0)}**, "
                    f"total=**{gotcha.get('total', 0)}**"
                )

    out.append("")
    out.append("## Recomendações (data-driven)")
    recs: list[str] = []
    if isinstance(composite, (int, float)) and composite < 0.7:
        recs.append(
            f"- composite_health = {composite:.2f} < 0.7 → executar `touring e2e -j` "
            "+ investigar componentes degradados"
        )
    if isinstance(learning, dict) and isinstance(learning.get("mean_td_error"), (int, float)) and learning["mean_td_error"] > 5.0:
        recs.append(
            "- TD error alto → considerar process rewards step-level (insight NOVO-1)"
        )
    if isinstance(arl, dict) and arl.get("active") and arl.get("update_count", 0) == 0:
        recs.append(
            "- agentic_rl ocioso → wire feeder em pós-fase TACO (insight NOVO-2)"
        )
    if not recs:
        recs.append("- Sistema saudável; manter VGP em pre-edit operations")
    out.extend(recs)
    return "\n".join(out)


def fmt_path(p: dict[str, Any], cwd: str, project_type: str, slim: bool = False) -> str:
    if slim:
        return _fmt_path_slim(p, cwd, project_type)
    out: list[str] = []
    out.append("")
    out.append("---")
    out.append("")
    out.append("# 📁 PATH/PROJETO ATUAL")
    out.append("")
    out.append(f"**CWD**: `{cwd}` | **Tipo detectado**: `{project_type}`")
    out.append("")

    # Recent actions on this path (touring memory recall <cwd>)
    mp = p.get("memory_path") or {}
    entries = mp.get("entries", []) if isinstance(mp, dict) else []
    if entries:
        out.append("## Últimas Ações no Path (touring memory)")
        for e in entries[:5]:
            if not isinstance(e, dict):
                continue
            key = (e.get("key") or "?")[:80]
            value = (e.get("value") or "")[:120].replace("\n", " ")
            score = e.get("score", 0)
            out.append(f"- `{key}` (score={score:.2f}): {value}")

    # Recent edits on this project (touring memory recall "edited:<project>")
    me = p.get("memory_recent_edits") or {}
    edits = me.get("entries", []) if isinstance(me, dict) else []
    if edits:
        out.append("")
        out.append("## Edits Recentes Registrados (touring memory)")
        for e in edits[:5]:
            if not isinstance(e, dict):
                continue
            key = (e.get("key") or "?")[:80]
            value = (e.get("value") or "")[:100].replace("\n", " ")
            out.append(f"- `{key}`: {value}")

    # Planos/decomposições registrados no Touring (memory key prefix "decomp:")
    md = p.get("memory_decomp") or {}
    decomps = md.get("entries", []) if isinstance(md, dict) else []
    if decomps:
        out.append("")
        out.append("## Planos / Decomposições Registrados (touring decompose + memory)")
        for d in decomps[:5]:
            if not isinstance(d, dict):
                continue
            key = (d.get("key") or "?")[:80]
            value = (d.get("value") or "")[:100].replace("\n", " ")
            out.append(f"- `{key}`: {value}")

    # Status agregado de decomposições (touring decompose status -j)
    ds = p.get("decompose_status") or {}
    if isinstance(ds, dict) and ds:
        total_tasks = ds.get("total_tasks", 0)
        total_sub = ds.get("total_subtasks", 0)
        out.append("")
        out.append("## DAGs Registrados no Touring (decompose)")
        out.append(
            f"- Total de tasks: **{total_tasks}** | Total de subtasks: **{total_sub}**"
        )
        if total_tasks == 0:
            out.append(
                "  - ℹ Nenhum DAG registrado — use `touring decompose create` "
                "para novas tarefas multi-step"
            )

    # Tasks prontos para execução (touring decompose ready -j)
    dr = p.get("decompose_ready") or {}
    if isinstance(dr, dict):
        ready = dr.get("ready_subtasks") or []
        blocked = dr.get("blocked_subtasks") or []
        if ready or blocked:
            out.append("")
            out.append("## Tasks Pendentes no DAG (touring decompose ready)")
            if ready:
                out.append(f"- Ready (deps satisfeitas): **{len(ready)}**")
                for t in ready[:5]:
                    if isinstance(t, dict):
                        tid = t.get("id") or t.get("subtask_id") or "?"
                        desc = (t.get("description") or t.get("title") or "?")[:80]
                        out.append(f"  - `{tid}`: {desc}")
            if blocked:
                out.append(f"- Blocked: **{len(blocked)}**")

    orphans = p.get("orphans") or {}
    if isinstance(orphans, dict):
        ocount = orphans.get("orphan_count", 0)
        out.append("")
        out.append("## Diagnóstico Técnico do Path")
        out.append(
            f"- Orphan pub symbols (workspace): **{ocount}** "
            "(filtrar ruído `.cargo/registry/`)"
        )

    out.append("")
    out.append("## Próximas Ações Recomendadas")
    if project_type in ("rust", "touring-workspace"):
        out.append(
            "- `touring ast meta <file> --depth summary -j` antes de edits "
            "(REGRA file-metadata-first)"
        )
        out.append(
            "- `cargo check --workspace` + `touring e2e -j` para validar baseline"
        )
        out.append(
            "- Task complexa: `taco-forge plan --quality high --auto-populate`"
        )
    elif project_type == "python":
        out.append("- Para scripts: `taco-forge perfect-create-script` (REGRA #1.6)")
        out.append("- Para edits: `taco-forge perfect-edit` (REGRA #14)")
    elif project_type == "node":
        out.append("- Para .ts: `taco-forge perfect-create-ts` (REGRA #1.7)")
        out.append("- Para .tsx: `taco-forge perfect-create-tsx` (REGRA #1.8)")
    else:
        out.append("- `touring index find <symbol>` antes de criar/usar (VGP)")
        out.append('- `touring memory recall "<topic>"` antes de pesquisar do zero')
    return "\n".join(out)


def _count_injected(entries: list, context: str) -> int:
    """Memórias cuja chave ou prefixo de valor aparecem no contexto final."""
    n = 0
    for e in entries:
        key = str(e.get("key", ""))[:60]
        val = str(e.get("value", ""))[:60]
        if (key and key in context) or (val and val in context):
            n += 1
    return n


def _rerank_with_jev(query: str, entries: list) -> list:
    """S-5.4 (replay aprovado: Jev top-1 5×4, MRR +25%): reordena os candidatos do
    recall por P(sim) do Noul — UMA chamada por query, candidata dentro das
    instructions (anti-context-rot, padrão da doc). Ordenação, nunca corte por τ
    (ECE 0,709 medido no G2 — Noul não é τ sem recalibração).
    Flag TOURING_JEV_RECALL_RERANK (default-OFF). Fail-open: qualquer falha
    devolve a ordem original (o rerank nunca piora o caminho atual).
    """
    try:
        sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))  # repo/scripts via the symlink
        import jev_client  # import tardio por desenho (hook quente)

        questions = {}
        for i, e in enumerate(entries):
            questions[f"c{i}"] = {
                "type": "noul",
                "instructions": {
                    "candidate": str(e.get("value", ""))[:2500],
                    "question": "Does `candidate` contain information that directly helps with the task in `query`?",
                },
            }
        answers = jev_client.system_one({"query": query[:1500]}, questions, timeout=JEV_TIMEOUT_S)
        if answers is None:
            return entries
        scored = [
            (answers.get(f"c{i}", {}).get("noul", 0.0), i, e)
            for i, e in enumerate(entries)
        ]
        scored.sort(key=lambda t: t[0], reverse=True)
        return [e for _, _, e in scored]
    except Exception:  # noqa: BLE001 — the rerank calls out to Jev; any failure keeps the original order
        return entries  # fail-open: ordem original


def _score_gate_with_jev(query: str, entries: list) -> list:
    """S-7 (plan r2, replay medido 01/10): FILTRA por Score de utilidade — injeta só
    score ≥ τ. Medido no replay congelado: τ=1,0 injeta 31,1% dos candidatos com a
    MESMA eficácia do contexto cheio (top1 4, mrr 0,1212 — zero perda com 69% menos
    injeção; padrão "Classifying RAG passages" da doc: o modelo pontua, o código decide).
    Ativado com TOURING_JEV_RECALL_MODE=score_gate (a flag-mestra segue TOURING_JEV_RECALL_RERANK).
    Fail-open: qualquer falha devolve TODAS as entradas (nunca injeta menos por erro).
    """
    try:
        sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))  # repo/scripts via the symlink
        import jev_client  # import tardio por desenho (hook quente)

        questions = {}
        for i, e in enumerate(entries):
            questions[f"c{i}"] = {
                "type": "score",
                "instructions": {
                    "candidate": str(e.get("value", ""))[:2500],
                    "question": "Rate how directly this candidate helps with the task in `query`: 0 = irrelevant, 1 = tangential, 2 = directly useful.",
                },
                "criteria": ["irrelevant to the query", "tangential or background", "directly useful for the query"],
            }
        answers = jev_client.system_one({"query": query[:1500]}, questions, timeout=JEV_TIMEOUT_S)
        if answers is None:
            return entries
        return [e for i, e in enumerate(entries) if float(answers.get(f"c{i}", {}).get("score", 0.0)) >= 1.0]
    except Exception:  # noqa: BLE001 — the score gate calls out to Jev; any failure keeps every entry
        return entries  # fail-open: todas (nunca injeta menos por erro)


def _rerank_recall_if_enabled(results: dict) -> dict:
    """Aplica rerank (ordena) ou score_gate (filtra ≥ τ medido) conforme o modo."""
    if os.environ.get("TOURING_JEV_RECALL_RERANK", "") not in ("1", "true", "on"):
        return results
    mode = os.environ.get("TOURING_JEV_RECALL_MODE", "rerank")
    out = dict(results)
    for name, payload in list(out.items()):
        if not name.startswith("memory_") or not isinstance(payload, dict):
            continue
        cases = payload.get("cases") or {}
        for group_name, group in cases.items():
            if not isinstance(group, list) or not group:
                continue
            query = name.removeprefix("memory_")
            cases[group_name] = _score_gate_with_jev(query, group) if mode == "score_gate" else _rerank_with_jev(query, group)
    return out


def _journal_recall_injection(results: dict, context: str, cwd: str) -> None:
    """S-3.4 (plano Jev×Touring, 01/10): conta o que o recall trouxe vs o que o
    contexto injetou — a régua `recall_curated_share` da família decision.

    Uma linha JSONL por session-start em recall_journal.jsonl, append-only
    (O_APPEND, um write por linha — a lição da intercalação do intent_journal).
    Fail-open por contrato: telemetria nunca derruba o hook.
    """
    try:
        per_query: dict[str, dict[str, int]] = {}
        for name, payload in (results or {}).items():
            if not name.startswith("memory_"):
                continue
            cases = (payload or {}).get("cases") or {}
            # cases mistura listas de entradas com metadados (cap_per_class:int,
            # guidance:str) — só listas carregam entradas; qualquer outra coisa
            # é metadado. Bug real 01/10: o flatten cego quebrava no int e o
            # fail-open engolia calado (o journal nunca nascia).
            entries = [e for g in cases.values() if isinstance(g, list) for e in g if isinstance(e, dict)]
            per_query[name] = {"candidates": len(entries), "injected": _count_injected(entries, context)}
        if not per_query:
            return
        rec = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "project": Path(cwd).name if cwd else "unknown",
            "per_query": per_query,
            "candidates": sum(q["candidates"] for q in per_query.values()),
            "injected": sum(q["injected"] for q in per_query.values()),
        }
        path = os.environ.get(
            "TOURING_RECALL_JOURNAL",
            str(Path.home() / ".claude" / "touring" / "logs" / "recall_journal.jsonl"),
        )
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
    except Exception as exc:  # noqa: BLE001 — telemetry never takes the hook down; stderr keeps the miss visible
        sys.stderr.write(f"session_startup_intelligence: recall_journal não gravado ({type(exc).__name__}: {exc})\n")


def main() -> int:
    try:
        data = json.load(sys.stdin)
    except (ValueError, OSError):
        data = {}
    if not isinstance(data, dict):  # a payload we cannot read is absent, not a crash
        data = {}
    cwd = data.get("cwd") or os.getcwd()
    session_id = data.get("session_id", "unknown")
    project_type = detect_project_type(cwd)
    slim = slim_enabled()
    try:
        global_intel = collect_global(slim)
    except Exception:  # noqa: BLE001 — a failed collection leaves every source absent, which the panel prints
        global_intel = {}
    try:
        path_intel = _rerank_recall_if_enabled(collect_path(cwd))
    except Exception:  # noqa: BLE001 — a failed collection leaves every source absent, which the panel prints
        path_intel = {}
    try:
        section_global = fmt_global(global_intel, slim)
    except Exception as e:  # noqa: BLE001 — the error is printed in the degraded section
        section_global = f"# 🧠 TOURING — degraded\n(collection error: {e})"
    try:
        section_path = fmt_path(path_intel, cwd, project_type, slim)
    except Exception as e:  # noqa: BLE001 — the error is printed in the degraded section
        section_path = f"# 📁 PATH — degraded\n(collection error: {e})"
    additional = (
        section_global
        + "\n"
        + section_path
        + "\n\n---\n_Session intelligence"
        + f" | session_id={str(session_id)[:8]} | path-aware"
        + " | hook=session_startup_intelligence.py_"
    )
    _journal_recall_injection(path_intel, additional, cwd)  # fail-open inside, stderr on a miss (S-3.4)
    out = {
        "hookSpecificOutput": {
            "hookEventName": HOOK_NAME,
            "additionalContext": additional,
        }
    }
    print(json.dumps(out))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:  # noqa: BLE001 — last line: the hook never takes the session down; the note says it degraded
        print(
            json.dumps(
                {
                    "hookSpecificOutput": {
                        "hookEventName": HOOK_NAME,
                        "additionalContext": "[session_startup_intelligence: degraded fail-open]",
                    }
                }
            )
        )
        sys.exit(0)
