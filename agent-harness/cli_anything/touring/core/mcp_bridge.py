"""MCP bridge for Tier 3 operations requiring the Touring MCP server.

Connects to the touring binary via stdio transport, calling MCP tools
that have no SQLite equivalent (suggest, decompose, evolve, session).

The server is started on-demand and kept alive for the duration of
the CLI process to amortize the ~200ms startup cost.
"""

from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path
from typing import Any

from mcp import ClientSession, StdioServerParameters, stdio_client


def _get_server_params() -> StdioServerParameters:
    """Build server parameters for the touring binary."""
    from .cortex import _find_touring_binary

    binary = _find_touring_binary()

    project_dir = Path.cwd()
    data_dir = project_dir / ".claude" / "data"
    symbols_db = project_dir / ".claude" / "rust" / "symbols.db"

    if not symbols_db.exists():
        alt = data_dir / "touring_symbols.db"
        if alt.exists():
            symbols_db = alt

    memory_path = (
        str(data_dir)
        if data_dir.exists()
        else str(
            Path.home() / ".claude" / "data",
        )
    )
    env = {
        **os.environ,
        "RUST_LOG": "warn",
        "TOURING_MEMORY_PATH": memory_path,
    }
    if symbols_db.exists():
        env["TOURING_DB_PATH"] = str(symbols_db)

    return StdioServerParameters(
        command=binary,
        args=["serve"],
        env=env,
    )


async def _call_tool(
    tool_name: str,
    arguments: dict[str, Any],
) -> Any:
    """Call a single MCP tool on the touring server."""
    params = _get_server_params()

    async with stdio_client(params) as streams:
        read_stream, write_stream = streams
        async with ClientSession(read_stream, write_stream) as sess:
            await sess.initialize()
            result = await sess.call_tool(tool_name, arguments)

            if result.content:
                texts = [block.text for block in result.content if hasattr(block, "text")]
                combined = "\n".join(texts)
                try:
                    return json.loads(combined)
                except json.JSONDecodeError:
                    return {"raw_output": combined}

            return {"empty": True}


def call_tool_sync(
    tool_name: str,
    arguments: dict[str, Any],
) -> Any:
    """Synchronous wrapper for _call_tool."""
    return asyncio.run(_call_tool(tool_name, arguments))


# ─── High-level wrappers for Tier 3 tools ─────────────────


def suggest(
    action: str = "next_action",
    state: int | None = None,
    query: str | None = None,
) -> dict[str, Any]:
    """Get RL-based suggestion.

    Args:
        action: "next_action", "similar_patterns",
                "skill_recommendation", "code_pattern".
        state: State ID for QTable (next_action).
        query: Query text (similar_patterns).
    """
    args: dict[str, Any] = {"action": action}
    if state is not None:
        args["state"] = state
    if query is not None:
        args["query"] = query
    return call_tool_sync("touring_suggest", args)


def decompose(
    action: str,
    description: str = "",
    task_id: str | None = None,
    task_type: str | None = None,
) -> dict[str, Any]:
    """Create or manage task decomposition DAGs.

    Args:
        action: "create", "add_subtask", "get_plan",
                "update_status", "validate_order".
        description: Task description (create/add_subtask).
        task_id: Task ID (add_subtask/get_plan/update_status).
        task_type: "refactor", "debug", "feature" (create).
    """
    args: dict[str, Any] = {"action": action}
    if description:
        args["description"] = description
    if task_id:
        args["task_id"] = task_id
    if task_type:
        args["task_type"] = task_type
    return call_tool_sync("touring_decompose", args)


def session(
    action: str,
    task_type: str | None = None,
    objective: str | None = None,
    session_id: str | None = None,
) -> dict[str, Any]:
    """Manage touring sessions.

    Args:
        action: "start", "checkpoint", "assess",
                "end", "list", "get".
        task_type: Session type for "start".
        objective: Objective for "start".
        session_id: ID for checkpoint/assess/end/get.
    """
    args: dict[str, Any] = {"action": action}
    if task_type:
        args["task_type"] = task_type
    if objective:
        args["objective"] = objective
    if session_id:
        args["session_id"] = session_id
    return call_tool_sync("touring_session", args)


def evolve(
    action: str = "auto_learn",
    metric: str | None = None,
) -> dict[str, Any]:
    """Trigger evolution/learning operations.

    Args:
        action: "extract_patterns", "auto_learn",
                "drift_report", "recommend".
        metric: Metric name for "drift_report".
    """
    args: dict[str, Any] = {"action": action}
    if metric:
        args["metric"] = metric
    return call_tool_sync("touring_evolve", args)


def insights(
    category: str | None = None,
    axis: str | None = None,
) -> dict[str, Any]:
    """Get evolution insights via MCP.

    Args:
        category: "tool_effectiveness", "cila_progression",
                  "cost_efficiency", "drift_detection".
        axis: "self_improvement" or "project_evolution".
    """
    args: dict[str, Any] = {}
    if category:
        args["category"] = category
    if axis:
        args["axis"] = axis
    return call_tool_sync("touring_insights", args)


# ─── v9.0 wrappers ───────────────────────────────────────


def speculate(
    file_path: str,
    content: str | None = None,
) -> dict[str, Any]:
    """Speculatively validate a file edit via shadow workspace v2.

    Args:
        file_path: Path to the file being validated.
        content: New content to validate (reads file if omitted).
    """
    args: dict[str, Any] = {"file_path": file_path}
    if content is not None:
        args["content"] = content
    return call_tool_sync("touring_speculate", args)


def mcts_search(
    state: int = 0,
    actions: list[int] | None = None,
    rollouts: int = 50,
    depth: int = 5,
) -> dict[str, Any]:
    """Run MCTS search for optimal action from given state.

    Args:
        state: Root state for search.
        actions: Candidate action IDs.
        rollouts: Number of rollouts per action.
        depth: Maximum search depth.
    """
    args: dict[str, Any] = {
        "state": state,
        "actions": actions or [1, 2, 3, 4, 5],
        "rollouts": rollouts,
        "depth": depth,
    }
    return call_tool_sync("touring_mcts_search", args)


def online_learn_status() -> dict[str, Any]:
    """Get online RL engine status — EMA reward, update count, LinUCB stats."""
    return call_tool_sync("touring_online_learn", {"action": "status"})


def online_learn_reward(
    tool: str,
    accepted: bool = True,
    latency_ms: int = 100,
    errors: int = 0,
) -> dict[str, Any]:
    """Inject an immediate reward signal for the online RL engine.

    Args:
        tool: Tool name that generated the outcome.
        accepted: Whether the tool result was accepted.
        latency_ms: Latency of the tool call in milliseconds.
        errors: Number of errors in the tool output.
    """
    return call_tool_sync(
        "touring_online_learn",
        {
            "action": "reward",
            "tool": tool,
            "accepted": accepted,
            "latency_ms": latency_ms,
            "errors": errors,
        },
    )


def incremental_status() -> dict[str, Any]:
    """Get incremental parser cache status."""
    return call_tool_sync("touring_incremental_status", {})


def mask_context(
    text: str,
    threshold: int = 4000,
) -> dict[str, Any]:
    """Test observation masking on text.

    Args:
        text: Text to mask.
        threshold: Token threshold for masking activation.
    """
    return call_tool_sync(
        "touring_mask_context",
        {"text": text, "threshold": threshold},
    )
