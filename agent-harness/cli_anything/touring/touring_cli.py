"""cli-anything-touring: CLI harness for Touring intelligence system v0.4.0.

Tier 1 (SQLite, <10ms): index, ast, memory, evolution, gotcha, flywheel, incremental
Tier 2 (subprocess, <50ms): cortex (pii, classify), context (compile)
Tier 3 (MCP bridge, ~200ms): suggest, session, decompose, shadow, mcts, learning, mask

v9.0 commands:
  shadow validate              — Speculative file validation (shadow workspace v2)
  mcts search                  — Monte Carlo Tree Search for action planning
  learning status/reward       — Online RL engine status and reward injection
  incremental status           — Incremental parser cache status
  mask test                    — Observation masking for context token reduction

Flywheel commands (v6.0.0):
  gotcha add/list/match/stats  — Gotcha database CRUD
  context compile              — Coalesced context for subagents
  flywheel status              — Component health check
"""

from __future__ import annotations

import json
import sys

import typer

from .core import context, cortex, evolution, flywheel, gotcha, index, memory

# Global JSON mode flag — set by root -j/--json callback
_json_mode: bool = False


def _jflag(local: bool = False) -> bool:
    """Return True if JSON output requested (global OR local flag)."""
    return _json_mode or local


def _output(data: object) -> None:
    """Print data as deterministic JSON (sort_keys, default=str)."""
    typer.echo(
        json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False, default=str),
    )


def _check_database_health(db_name: str, label: str) -> dict[str, object]:
    """Check a single database health and return status dict."""
    from .core.db import connect, get_db_path

    try:
        path = get_db_path(db_name)
        with connect(db_name) as conn:
            tables = conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table'",
            ).fetchall()
            table_names = [t["name"] for t in tables if t["name"] != "sqlite_sequence"]
            counts = {}
            for tn in table_names:
                row = conn.execute(f"SELECT COUNT(*) FROM [{tn}]").fetchone()
                counts[tn] = row[0]
        return {"status": "ok", "path": str(path), "tables": counts}
    except Exception as e:
        return {"status": "error", "error": str(e)}


def _check_binary_health() -> dict[str, object]:
    """Check touring binary presence."""
    from .core import cortex

    try:
        bp = cortex._find_touring_binary()
        return {"status": "ok", "path": bp}
    except FileNotFoundError as e:
        return {"status": "missing", "error": str(e)}


def _format_health_text(checks: dict[str, object], all_ok: bool) -> None:
    """Format and print health results as human-readable text."""
    st = "HEALTHY" if all_ok else "DEGRADED"
    typer.echo(f"cli-anything-touring {st}")
    for label, info in checks.items():
        if not isinstance(info, dict):
            continue
        s = info.get("status", "?")
        icon = "+" if s == "ok" else "x"
        typer.echo(f"  {icon} {label}: {s}")
        tbl = info.get("tables")
        if isinstance(tbl, dict):
            for tn, cnt in tbl.items():
                typer.echo(f"      {tn}: {cnt:,} rows")


def _global_callback(
    json_out: bool = typer.Option(
        False,
        "-j",
        "--json",
        help="JSON output for all commands.",
        is_eager=True,
    ),
) -> None:
    """Global callback to capture -j flag before subcommands."""
    global _json_mode
    _json_mode = json_out


app = typer.Typer(
    name="cli-anything-touring",
    help="CLI harness for Touring intelligence system.",
    no_args_is_help=True,
    callback=_global_callback,
)

index_app = typer.Typer(help="Symbol index ops (60k+ symbols).", no_args_is_help=True)
ast_app = typer.Typer(help="AST and blast radius ops.", no_args_is_help=True)
memory_app = typer.Typer(help="Memory store/recall (73k+ entries).", no_args_is_help=True)
evolution_app = typer.Typer(help="Evolution insights, drift, tools.", no_args_is_help=True)
cortex_app = typer.Typer(help="Cortex ops (PII scan, CILA classify).", no_args_is_help=True)
suggest_app = typer.Typer(help="RL suggestions (Tier 3, MCP).", no_args_is_help=True)
session_app = typer.Typer(help="Session management (Tier 3).", no_args_is_help=True)
decompose_app = typer.Typer(help="Task decomposition DAGs (Tier 3).", no_args_is_help=True)
gotcha_app = typer.Typer(help="Gotcha DB ops (24+ entries, Tier 1).", no_args_is_help=True)
context_app = typer.Typer(help="Context compiler (Tier 2, subprocess).", no_args_is_help=True)
flywheel_app = typer.Typer(help="Flywheel status and audit.", no_args_is_help=True)
shadow_app = typer.Typer(help="Shadow workspace v2 — speculative multi-branch execution.", no_args_is_help=True)
mcts_app = typer.Typer(help="Monte Carlo Tree Search for action planning.", no_args_is_help=True)
learning_app = typer.Typer(help="Online reinforcement learning engine status.", no_args_is_help=True)
incremental_app = typer.Typer(help="Incremental parser pipeline status.", no_args_is_help=True)
mask_app = typer.Typer(help="Observation masking for context token reduction.", no_args_is_help=True)

app.add_typer(index_app, name="index")
app.add_typer(ast_app, name="ast")
app.add_typer(memory_app, name="memory")
app.add_typer(evolution_app, name="evolution")
app.add_typer(cortex_app, name="cortex")
app.add_typer(suggest_app, name="suggest")
app.add_typer(session_app, name="session")
app.add_typer(decompose_app, name="decompose")
app.add_typer(gotcha_app, name="gotcha")
app.add_typer(context_app, name="context")
app.add_typer(flywheel_app, name="flywheel")
app.add_typer(shadow_app, name="shadow")
app.add_typer(mcts_app, name="mcts")
app.add_typer(learning_app, name="learning")
app.add_typer(incremental_app, name="incremental")
app.add_typer(mask_app, name="mask")


# ---------------------------------------------------------------------------
# INDEX
# ---------------------------------------------------------------------------


@index_app.command("search")
def index_search(
    query: str = typer.Argument(..., help="Substring to search."),
    n: int = typer.Option(10, "-n", "--limit", help="Max results."),
    kind: str | None = typer.Option(None, "-k", "--kind", help="Filter kind."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Search symbols by name substring."""
    results = index.search(query, limit=n, kind=kind)
    if _jflag(json_out):
        _output(results)
    else:
        typer.echo(f"Found {len(results)} symbols for '{query}':")
        typer.echo(index.format_results(results))


@index_app.command("status")
def index_status(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show index stats (symbols, files, freshness)."""
    data = index.status()
    if _jflag(json_out):
        _output(data)
    else:
        ts = data["total_symbols"]
        tf = data["total_files"]
        typer.echo(f"Symbols: {ts:,}  |  Files: {tf:,}")
        typer.echo("\nBy language:")
        for lang, cnt in data["by_language"].items():
            typer.echo(f"  {lang:12s} {cnt:>6d}")
        typer.echo("\nBy kind:")
        for knd, cnt in data["by_kind"].items():
            typer.echo(f"  {knd:12s} {cnt:>6d}")


@index_app.command("find")
def index_find(
    name: str = typer.Argument(..., help="Symbol name."),
    exact: bool = typer.Option(False, "-e", "--exact", help="Exact match."),
    n: int = typer.Option(10, "-n", "--limit", help="Max results."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Find symbols by exact or prefix match."""
    results = index.find(name, exact=exact, limit=n)
    if _jflag(json_out):
        _output(results)
    else:
        typer.echo(f"Found {len(results)} for '{name}':")
        typer.echo(index.format_results(results))


@index_app.command("files")
def index_files(
    pattern: str | None = typer.Argument(None, help="File path pattern."),
    n: int = typer.Option(20, "-n", "--limit", help="Max results."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """List indexed files with symbol counts."""
    results = index.files(pattern, limit=n)
    if _jflag(json_out):
        _output(results)
    else:
        typer.echo(f"Top {len(results)} indexed files:")
        for r in results:
            typer.echo(f"  {r['symbol_count']:>4d} sym  {r['file']}  [{r['languages']}]")


# ---------------------------------------------------------------------------
# AST
# ---------------------------------------------------------------------------


@ast_app.command("find")
def ast_find(
    symbol: str = typer.Argument(..., help="Symbol name."),
    n: int = typer.Option(10, "-n", "--limit", help="Max results."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Find symbol definitions in the index."""
    results = index.find(symbol, limit=n)
    if _jflag(json_out):
        _output(results)
    else:
        typer.echo(f"Found {len(results)} for '{symbol}':")
        typer.echo(index.format_results(results))


@ast_app.command("overview")
def ast_overview(
    file_path: str = typer.Argument(..., help="File to get overview of."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show symbols defined in a file (structure overview)."""
    data = index.file_overview(file_path)
    if _jflag(json_out):
        _output(data)
    else:
        typer.echo(f"File: {data['file']}  ({data['total_symbols']} symbols)")
        if data["symbols"]:
            typer.echo("")
            for s in data["symbols"]:
                typer.echo(f"  {s['kind']:12s} {s['name']:40s} L{s['line']}")
        else:
            typer.echo("  No symbols found in index for this file.")


@ast_app.command("blast")
def ast_blast(
    file_path: str = typer.Argument(..., help="File to analyze."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show blast radius for a file."""
    data = evolution.blast(file_path)
    if _jflag(json_out):
        _output(data)
    else:
        meta = data.get("metadata")
        if meta:
            typer.echo(f"File: {meta.get('file_path', file_path)}")
            typer.echo(
                f"  Lang: {meta.get('language', '?')}, "
                f"Lines: {meta.get('line_count', '?')}, "
                f"Symbols: {meta.get('symbol_count', '?')}"
            )
        br = data["blast_radius"]
        typer.echo(f"\nBlast radius: {br} files")
        if data["affects"]:
            typer.echo("\nAffects:")
            for r in data["affects"][:20]:
                typer.echo(f"  -> {r['target_path']} ({r['relation_type']})")
        dc = data["dependency_count"]
        typer.echo(f"\nDependencies: {dc} files")
        if data["affected_by"]:
            typer.echo("\nDepends on:")
            for r in data["affected_by"][:20]:
                typer.echo(f"  <- {r['source_path']} ({r['relation_type']})")


# ---------------------------------------------------------------------------
# MEMORY
# ---------------------------------------------------------------------------


@memory_app.command("recall")
def memory_recall(
    query: str = typer.Argument(..., help="Search query."),
    n: int = typer.Option(10, "-n", "--limit", help="Max results."),
    tier: str = typer.Option("all", "-t", "--tier", help="Tier: all, durable, ephemeral, working, etc."),
    entry_type: str | None = typer.Option(None, "--type", help="Filter entry_type."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Search memory entries by key/value substring."""
    results = memory.recall(query, limit=n, tier=tier, entry_type=entry_type)
    if _jflag(json_out):
        _output(results)
    else:
        typer.echo(f"Found {len(results)} entries for '{query}' (tier={tier}):")
        typer.echo(memory.format_results(results))


@memory_app.command("store")
def memory_store(
    key: str = typer.Argument(..., help="Memory key."),
    value: str = typer.Argument(..., help="Memory value."),
    tier: str = typer.Option("working", "-t", "--tier", help="Target tier."),
    entry_type: str = typer.Option("cli_store", "--type", help="Entry type."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Store a memory entry (upsert)."""
    result = memory.store(key, value, tier=tier, entry_type=entry_type)
    if _jflag(json_out):
        _output(result)
    else:
        typer.echo(f"Stored: [{result['tier']}] {result['key']} ({result['entry_type']})")


@memory_app.command("list")
def memory_list(
    tier: str = typer.Option("all", "-t", "--tier", help="Filter tier."),
    n: int = typer.Option(20, "-n", "--limit", help="Max results."),
    sort: str = typer.Option("accessed_at", "-s", "--sort", help="Sort: accessed_at, access_count."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """List memory entries."""
    results = memory.list_entries(tier=tier, limit=n, sort_by=sort)
    if _jflag(json_out):
        _output(results)
    else:
        typer.echo(f"Listing {len(results)} entries:")
        for r in results:
            typer.echo(
                f"  [{r['tier']:10s}] {r['key'][:50]:50s} "
                f"({r['entry_type']}, x{r['access_count']}, {r['value_length']}B)"
            )


@memory_app.command("stats")
def memory_stats(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show memory statistics."""
    data = memory.stats()
    if _jflag(json_out):
        _output(data)
    else:
        typer.echo(f"Total: {data['total_entries']:,}")
        typer.echo("\nBy tier:")
        for t, cnt in data["by_tier"].items():
            typer.echo(f"  {t:12s} {cnt:>6d}")
        typer.echo("\nTop entry types:")
        for et, cnt in data["by_type"].items():
            typer.echo(f"  {et:20s} {cnt:>6d}")


# ---------------------------------------------------------------------------
# EVOLUTION
# ---------------------------------------------------------------------------


@evolution_app.command("insights")
def evolution_insights(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show evolution insights (Wilson + drift + knowledge)."""
    data = evolution.insights()
    if _jflag(json_out):
        _output(data)
    else:
        typer.echo("=== Wilson Top Items ===")
        for w in data.get("wilson_top", []):
            typer.echo(f"  {w['item_id']:40s} score={w['wilson_score']:.4f} ({w['successes']}/{w['trials']})")
        typer.echo("\n=== Drift Metrics ===")
        for metric, info in data.get("drift_metrics", {}).items():
            if "error" in info:
                typer.echo(f"  {metric}: {info['error']}")
            else:
                typer.echo(
                    f"  {metric}: trend={info['trend']} "
                    f"recent={info['recent_avg']} overall={info['overall_avg']} "
                    f"(n={info['total_samples']})"
                )
        ks = data.get("knowledge_stats", {})
        typer.echo("\n=== Knowledge ===")
        typer.echo(
            f"  Files: {ks.get('files_known', 0)} | Rels: {ks.get('relations', 0)} | Bash: {ks.get('bash_outcomes', 0)}"
        )


@evolution_app.command("drift")
def evolution_drift(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show drift report with trend analysis."""
    data = evolution.drift()
    if _jflag(json_out):
        _output(data)
    else:
        typer.echo(f"Tracking {data['total_tracked']} metrics:")
        arrows = {"improving": "^", "degrading": "v", "stable": "="}
        for metric, info in data.get("metrics", {}).items():
            if "error" in info:
                typer.echo(f"  {metric}: {info['error']}")
            else:
                a = arrows.get(info["trend"], "?")
                typer.echo(
                    f"  {a} {metric:30s} mean={info['mean']:.4f} "
                    f"sd={info['stddev']:.4f} recent={info['recent_mean']:.4f} "
                    f"last={info['last_value']}"
                )


@evolution_app.command("tools")
def evolution_tools(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show tool effectiveness metrics."""
    data = evolution.tools()
    if _jflag(json_out):
        _output(data)
    else:
        typer.echo("=== Top Commands ===")
        for r in data.get("top_commands", []):
            typer.echo(f"  {r['command_short']:30s} {r['total']:>5d} runs {r['success_rate']:>5.1f}%")
        if data.get("failure_patterns"):
            typer.echo("\n=== Failure Patterns ===")
            for r in data["failure_patterns"]:
                ep = (r["error_pattern"] or "")[:50]
                typer.echo(f"  {r['command_short']:30s} {ep} (x{r['cnt']})")
        if data.get("wilson_scored_tools"):
            typer.echo("\n=== Wilson Tools ===")
            for r in data["wilson_scored_tools"]:
                typer.echo(f"  {r['item_id']:40s} score={r['wilson_score']:.4f} ({r['successes']}/{r['trials']})")


# ---------------------------------------------------------------------------
# CORTEX (Tier 2 — subprocess)
# ---------------------------------------------------------------------------


@cortex_app.command("pii")
def cortex_pii(
    text: str = typer.Argument(..., help="Text to scan for PII."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Scan text for PII."""
    data = cortex.scan_pii(text)
    if _jflag(json_out) or "error" not in data:
        _output(data)
    else:
        typer.echo(f"Error: {data['error']}", err=True)
        raise typer.Exit(1)


@cortex_app.command("classify")
def cortex_classify(
    text: str = typer.Argument(..., help="Prompt text to classify."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Classify intent/CILA level."""
    data = cortex.classify_intent(text)
    if _jflag(json_out) or "error" not in data:
        _output(data)
    else:
        typer.echo(f"Error: {data['error']}", err=True)
        raise typer.Exit(1)


# ---------------------------------------------------------------------------
# SUGGEST / SESSION / DECOMPOSE (Tier 3 — MCP bridge)
# ---------------------------------------------------------------------------


def _mcp_call(fn: object, *args: object, **kwargs: object) -> object:
    """Wrap MCP bridge calls with error handling."""
    try:
        from .core import mcp_bridge

        func = getattr(mcp_bridge, fn) if isinstance(fn, str) else fn
        return func(*args, **kwargs)
    except ImportError:
        return {"error": "MCP not installed: pip install mcp"}
    except FileNotFoundError as e:
        return {"error": f"Binary not found: {e}"}
    except Exception as e:
        return {"error": f"MCP bridge error: {e}"}


@suggest_app.command("next")
def suggest_next(
    state: int = typer.Option(0, "-s", "--state", help="QTable state ID."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Get RL suggestion for next action."""
    _output(_mcp_call("suggest", action="next_action", state=state))


@suggest_app.command("skill")
def suggest_skill(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Get skill recommendation."""
    _output(_mcp_call("suggest", action="skill_recommendation"))


@session_app.command("start")
def session_start(
    task_type: str = typer.Argument("analysis", help="Session/task type."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Start a new touring session."""
    _output(_mcp_call("session", action="start", task_type=task_type))


@session_app.command("checkpoint")
def session_checkpoint(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Create a session checkpoint."""
    _output(_mcp_call("session", action="checkpoint"))


@session_app.command("list")
def session_list(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """List active sessions."""
    _output(_mcp_call("session", action="list"))


@decompose_app.command("create")
def decompose_create(
    description: str = typer.Argument(..., help="Task description."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Create a task decomposition DAG."""
    _output(_mcp_call("decompose", action="create", description=description))


@decompose_app.command("status")
def decompose_status(
    task_id: str = typer.Argument(..., help="Task ID to check."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Get status of a decomposition plan."""
    _output(_mcp_call("decompose", action="get_plan", task_id=task_id))


# ---------------------------------------------------------------------------
# GOTCHA (Tier 1 — SQLite direct)
# ---------------------------------------------------------------------------


@gotcha_app.command("add")
def gotcha_add(
    pattern: str = typer.Argument(..., help="File pattern to match."),
    text: str = typer.Argument(..., help="Gotcha description."),
    severity: str = typer.Option("warning", "-s", "--severity", help="critical|warning|info."),
    symbol: str | None = typer.Option(None, "--symbol", help="Related symbol name."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Add a gotcha to the database."""
    result = gotcha.add(pattern, text, severity=severity, symbol_name=symbol)
    if _jflag(json_out):
        _output(result)
    else:
        typer.echo(f"Added gotcha #{result['id']}: [{severity}] {pattern} — {text[:60]}")


@gotcha_app.command("list")
def gotcha_list(
    n: int = typer.Option(20, "-n", "--limit", help="Max results."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """List all gotchas (sorted by hit count)."""
    results = gotcha.list_all(limit=n)
    if _jflag(json_out):
        _output(results)
    else:
        typer.echo(f"Gotchas ({len(results)}):")
        for g in results:
            sym = f" ({g['symbol_name']})" if g.get("symbol_name") else ""
            typer.echo(
                f"  #{g['id']:3d} [{g['severity']:8s}] {g['pattern']:20s}{sym}"
                f"  hits={g['hit_count']} prevented={g['prevented_errors']}"
            )
            typer.echo(f"       {g['gotcha'][:80]}")


@gotcha_app.command("match")
def gotcha_match(
    file_path: str = typer.Argument(..., help="File path to match against."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Find gotchas matching a file path."""
    results = gotcha.match_file(file_path)
    if _jflag(json_out):
        _output(results)
    else:
        if not results:
            typer.echo(f"No gotchas match '{file_path}'")
        else:
            typer.echo(f"{len(results)} gotchas match '{file_path}':")
            for g in results:
                typer.echo(f"  [{g['severity']}] {g['gotcha']}")


@gotcha_app.command("stats")
def gotcha_stats(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show gotcha statistics."""
    data = gotcha.stats()
    if _jflag(json_out):
        _output(data)
    else:
        typer.echo(f"Total: {data['total']} gotchas")
        typer.echo(f"Total hits: {data['total_hits']}")
        typer.echo(f"Errors prevented: {data['total_prevented']}")
        typer.echo("\nBy severity:")
        for sev, cnt in data.get("by_severity", {}).items():
            typer.echo(f"  {sev}: {cnt}")


# ---------------------------------------------------------------------------
# CONTEXT (Tier 2 — subprocess)
# ---------------------------------------------------------------------------


@context_app.command("compile")
def context_compile(
    intent: str = typer.Argument(..., help="Intent/purpose of the context."),
    files: str = typer.Option("", "-f", "--files", help="Comma-separated file paths."),
    max_tokens: int = typer.Option(2000, "-m", "--max-tokens", help="Max token budget."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Compile coalesced context for subagent prompts."""
    data = context.compile_context(intent, files, max_tokens=max_tokens)
    if _jflag(json_out):
        _output(data)
    else:
        if "error" in data:
            typer.echo(f"Error: {data['error']}", err=True)
            raise typer.Exit(1)
        typer.echo(f"Intent: {intent}")
        typer.echo(f"Tokens: ~{data.get('estimated_tokens', '?')}")
        typer.echo(f"Cache key: {data.get('cache_key', '?')}")
        typer.echo(f"\n{data.get('context', '')}")


# ---------------------------------------------------------------------------
# FLYWHEEL (Tier 1 — status)
# ---------------------------------------------------------------------------


@flywheel_app.command("status")
def flywheel_status(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show flywheel component status."""
    data = flywheel.status()
    if _jflag(json_out):
        _output(data)
    else:
        typer.echo(f"Flywheel v{data['version']}")
        typer.echo(f"Components: {data['components_active']}/{data['components_total']}")
        typer.echo("")
        for name, active in data.get("components", {}).items():
            icon = "+" if active else "x"
            typer.echo(f"  [{icon}] {name}")
        typer.echo("")
        gdb = data.get("gotcha_db", {})
        typer.echo(f"Gotcha DB: {gdb.get('entries', 0)} entries, {gdb.get('total_hits', 0)} hits")
        kb = data.get("knowledge", {})
        typer.echo(f"Knowledge: {kb.get('files', 0)} files, {kb.get('relations', 0)} rels, {kb.get('edits', 0)} edits")
        mem = data.get("memory", {})
        typer.echo(f"Memory: {mem.get('total_entries', 0)} entries")
        sym = data.get("symbols", {})
        typer.echo(f"Symbols: {sym.get('total', 0)}")


# ---------------------------------------------------------------------------
# SHADOW (Tier 3 — MCP bridge, shadow workspace v2)
# ---------------------------------------------------------------------------


@shadow_app.command("validate")
def shadow_validate(
    file: str = typer.Argument(..., help="File path to validate."),
    content: str | None = typer.Option(None, "--content", "-c", help="Content to validate (reads file if omitted)."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Speculatively validate a file edit via ruff without modifying the filesystem."""
    if content is None:
        from pathlib import Path

        p = Path(file)
        if not p.exists():
            typer.echo(f"Error: file not found: {file}", err=True)
            raise typer.Exit(1)
        content = p.read_text(encoding="utf-8")

    data = _mcp_call("speculate", file_path=file, content=content)
    if _jflag(json_out):
        _output(data)
    else:
        if isinstance(data, dict) and "error" in data:
            typer.echo(f"Error: {data['error']}", err=True)
            raise typer.Exit(1)
        score = data.get("score", "?") if isinstance(data, dict) else "?"
        diag_count = len(data.get("diagnostics", [])) if isinstance(data, dict) else 0
        typer.echo(f"Shadow validation: {file}")
        typer.echo(f"  Score: {score}  |  Diagnostics: {diag_count}")
        for d in (data.get("diagnostics", []) if isinstance(data, dict) else []):
            sev = d.get("severity", "?")
            msg = d.get("message", "?")
            line = d.get("line", "?")
            typer.echo(f"  L{line} [{sev}] {msg}")


# ---------------------------------------------------------------------------
# MCTS (Tier 3 — MCP bridge, cognitive engine)
# ---------------------------------------------------------------------------


@mcts_app.command("search")
def mcts_search(
    state: int = typer.Argument(0, help="Root state for search."),
    actions: str = typer.Argument("1,2,3,4,5", help="Comma-separated candidate actions."),
    rollouts: int = typer.Option(50, "--rollouts", "-r", help="Number of rollouts per action."),
    depth: int = typer.Option(5, "--depth", "-d", help="Maximum search depth."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Run MCTS search for optimal action from given state."""
    action_list = [int(a.strip()) for a in actions.split(",") if a.strip()]
    data = _mcp_call(
        "mcts_search",
        state=state,
        actions=action_list,
        rollouts=rollouts,
        depth=depth,
    )
    if _jflag(json_out):
        _output(data)
    else:
        if isinstance(data, dict) and "error" in data:
            typer.echo(f"Error: {data['error']}", err=True)
            raise typer.Exit(1)
        best = data.get("best_action", "?") if isinstance(data, dict) else "?"
        conf = data.get("confidence", 0) if isinstance(data, dict) else 0
        value = data.get("value", 0) if isinstance(data, dict) else 0
        typer.echo(f"MCTS search from state {state} (actions: {actions})")
        typer.echo(f"  Best action: {best}  |  Confidence: {conf:.4f}  |  Value: {value:.4f}")
        typer.echo(f"  Rollouts: {rollouts}  |  Depth: {depth}")


# ---------------------------------------------------------------------------
# LEARNING (Tier 3 — MCP bridge, online RL)
# ---------------------------------------------------------------------------


@learning_app.command("status")
def learning_status(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show online RL engine status — EMA reward, update count, LinUCB stats."""
    data = _mcp_call("online_learn_status")
    if _jflag(json_out):
        _output(data)
    else:
        if isinstance(data, dict) and "error" in data:
            typer.echo(f"Error: {data['error']}", err=True)
            raise typer.Exit(1)
        ol = data.get("online_rl", {}) if isinstance(data, dict) else {}
        lb = data.get("linucb", {}) if isinstance(data, dict) else {}
        typer.echo("=== Online RL Engine ===")
        typer.echo(f"  EMA reward:   {ol.get('ema_reward', '?')}")
        typer.echo(f"  Update count: {ol.get('update_count', '?')}")
        typer.echo("\n=== LinUCB ===")
        typer.echo(f"  Arms:        {lb.get('arms', '?')}")
        typer.echo(f"  Total pulls: {lb.get('total_pulls', '?')}")


@learning_app.command("reward")
def learning_reward(
    tool: str = typer.Argument(..., help="Tool name."),
    accepted: bool = typer.Option(True, "--accepted/--rejected", help="Whether result was accepted."),
    latency: int = typer.Option(100, "--latency-ms", help="Latency in milliseconds."),
    errors: int = typer.Option(0, "--errors", help="Number of errors."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Manually inject an immediate reward signal for testing."""
    data = _mcp_call(
        "online_learn_reward",
        tool=tool,
        accepted=accepted,
        latency_ms=latency,
        errors=errors,
    )
    if _jflag(json_out):
        _output(data)
    else:
        if isinstance(data, dict) and "error" in data:
            typer.echo(f"Error: {data['error']}", err=True)
            raise typer.Exit(1)
        typer.echo(f"Reward injected for tool '{tool}':")
        typer.echo(f"  Accepted: {accepted}  |  Latency: {latency}ms  |  Errors: {errors}")
        reward = data.get("reward", "?") if isinstance(data, dict) else "?"
        typer.echo(f"  Computed reward: {reward}")


# ---------------------------------------------------------------------------
# INCREMENTAL (Tier 3 — MCP bridge, incremental parser)
# ---------------------------------------------------------------------------


@incremental_app.command("status")
def incremental_status(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Show incremental parser cache status."""
    data = _mcp_call("incremental_status")
    if _jflag(json_out):
        _output(data)
    else:
        if isinstance(data, dict) and "error" in data:
            typer.echo(f"Error: {data['error']}", err=True)
            raise typer.Exit(1)
        typer.echo("=== Incremental Parser ===")
        if isinstance(data, dict):
            for key, val in data.items():
                typer.echo(f"  {key}: {val}")
        else:
            typer.echo(f"  {data}")


# ---------------------------------------------------------------------------
# MASK (Tier 3 — MCP bridge, observation masking)
# ---------------------------------------------------------------------------


@mask_app.command("test")
def mask_test(
    text: str = typer.Argument(None, help="Text to mask (reads stdin if omitted)."),
    threshold: int = typer.Option(4000, "--threshold", "-t", help="Token threshold for masking."),
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Test observation masking on text — shows token reduction stats."""
    if text is None:
        if sys.stdin.isatty():
            typer.echo("Error: provide text argument or pipe to stdin.", err=True)
            raise typer.Exit(1)
        text = sys.stdin.read()

    data = _mcp_call("mask_context", text=text, threshold=threshold)
    if _jflag(json_out):
        _output(data)
    else:
        if isinstance(data, dict) and "error" in data:
            typer.echo(f"Error: {data['error']}", err=True)
            raise typer.Exit(1)
        orig = data.get("original_tokens", "?") if isinstance(data, dict) else "?"
        masked = data.get("masked_tokens", "?") if isinstance(data, dict) else "?"
        pct = data.get("reduction_pct", "?") if isinstance(data, dict) else "?"
        typer.echo(f"Observation masking (threshold={threshold}):")
        typer.echo(f"  Original tokens: {orig}")
        typer.echo(f"  Masked tokens:   {masked}")
        typer.echo(f"  Reduction:       {pct}%")


# ---------------------------------------------------------------------------
# ROOT commands
# ---------------------------------------------------------------------------


@app.command("version")
def version() -> None:
    """Show version."""
    from . import __version__

    typer.echo(f"cli-anything-touring v{__version__}")


@app.command("health")
def health(
    json_out: bool = typer.Option(False, "-j", "--json", help="JSON output."),
) -> None:
    """Quick health check — databases + binary."""
    from .core.db import KNOWLEDGE_DB, MEMORY_DB, SYMBOLS_DB

    checks: dict[str, object] = {}
    all_ok = True

    for db_name, label in [
        (SYMBOLS_DB, "symbols"),
        (MEMORY_DB, "memory"),
        (KNOWLEDGE_DB, "knowledge"),
    ]:
        result = _check_database_health(db_name, label)
        checks[label] = result
        if result.get("status") != "ok":
            all_ok = False

    binary_result = _check_binary_health()
    checks["binary"] = binary_result

    result = {"healthy": all_ok, "databases": checks}

    if _jflag(json_out):
        _output(result)
    else:
        _format_health_text(checks, all_ok)


def main() -> None:
    """Entry point for the CLI."""
    app()


if __name__ == "__main__":
    main()
