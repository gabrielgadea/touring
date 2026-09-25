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
Total budget: <5s (settings.json timeout=5). Per-touring-call timeout: 1.5-2s.

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
PATH_CMD_TIMEOUT = 2.0
TOTAL_BUDGET_S = 4.5


def slim_enabled() -> bool:
    """S-04: is SLIM (stable-prefix-only) mode active? Default ON (Gabriel,
    2026-07-04) — the slim prefix is the real STR win (−83.6% startup bytes), so
    it is the default. Kill-switch: `TOURING_SESSIONSTART_SLIM=0` (or false/no/off)
    restores the verbose intelligence dashboard."""
    val = os.environ.get("TOURING_SESSIONSTART_SLIM", "").strip().lower()
    return val not in ("0", "false", "no", "off")


def run_touring(args: list[str], timeout: float = PER_CMD_TIMEOUT) -> str | None:
    """Run touring CLI subcommand, return stdout or None on any failure."""
    try:
        r = subprocess.run(
            ["touring", *args],
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        if r.returncode != 0:
            return None
        return r.stdout.strip() or None
    except Exception:
        return None


def parse_json_safe(s: str | None) -> Any:
    if not s:
        return None
    try:
        return json.loads(s)
    except Exception:
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
    with ThreadPoolExecutor(max_workers=7) as ex:
        futures = {ex.submit(run_touring, args): name for name, args in tasks.items()}
        for f in as_completed(futures, timeout=TOTAL_BUDGET_S):
            name = futures[f]
            try:
                results[name] = parse_json_safe(f.result())
            except Exception:
                results[name] = None
    results["fs_mtimes"] = _collect_fs_mtimes()
    return results


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
    except Exception:
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
            except Exception:
                results[name] = None
    return results


def _health_headline(g: dict[str, Any]) -> list[str]:
    """The one stable, high-SNR line: composite + doctor N/N + degraded parts."""
    out: list[str] = []
    status = g.get("status") or {}
    composite = status.get("composite_health_score", 0.0)
    doctor = g.get("doctor")
    ok = total = 0
    if isinstance(doctor, list) and doctor:
        ok = sum(1 for d in doctor if isinstance(d, dict) and d.get("status") == "ok")
        total = len(doctor)
    comp_str = f"{composite:.2f}" if isinstance(composite, (int, float)) else "?"
    out.append(f"## Saúde: composite **{comp_str}** · Doctor **{ok}/{total} OK**")
    if isinstance(doctor, list):
        for d in doctor:
            if isinstance(d, dict) and d.get("status") not in ("ok", None):
                out.append(f"  - ⚠ `{d.get('name', '?')}`: {(d.get('detail') or '')[:80]}")
    return out


def _health_recs(g: dict[str, Any]) -> list[str]:
    """Data-driven recommendations, emitted ONLY when there is a real problem."""
    status = g.get("status") or {}
    composite = status.get("composite_health_score", 0.0)
    learning = g.get("learning") or {}
    arl = learning.get("agentic_rl_state") or {} if isinstance(learning, dict) else {}
    recs: list[str] = []
    if isinstance(composite, (int, float)) and composite < 0.7:
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
    elif project_type == "python":
        out.append("- `touring ast meta <file>` antes de edits · `touring index find` (VGP)")
    elif project_type == "node":
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

    status = g.get("status") or {}
    composite = status.get("composite_health_score", 0.0)
    if isinstance(composite, (int, float)):
        out.append(f"## Saúde do Sistema (composite_health_score = **{composite:.2f}**)")
    else:
        out.append("## Saúde do Sistema (composite indisponível)")

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
                out.append(f"- Top arquivos editados (últimos 30d):")
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


def main() -> int:
    t0 = time.monotonic()
    try:
        data = json.load(sys.stdin)
    except Exception:
        data = {}
    cwd = data.get("cwd") or os.getcwd()
    session_id = data.get("session_id", "unknown")
    project_type = detect_project_type(cwd)
    slim = slim_enabled()
    try:
        global_intel = collect_global(slim)
    except Exception:
        global_intel = {}
    try:
        path_intel = collect_path(cwd)
    except Exception:
        path_intel = {}
    try:
        section_global = fmt_global(global_intel, slim)
    except Exception as e:
        section_global = f"# 🧠 TOURING — degraded\n(collection error: {e})"
    try:
        section_path = fmt_path(path_intel, cwd, project_type, slim)
    except Exception as e:
        section_path = f"# 📁 PATH — degraded\n(collection error: {e})"
    elapsed_ms = int((time.monotonic() - t0) * 1000)
    additional = (
        section_global
        + "\n"
        + section_path
        + "\n\n---\n_Session intelligence"
        + f" | session_id={str(session_id)[:8]} | path-aware"
        + " | hook=session_startup_intelligence.py_"
    )
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
    except Exception:
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
