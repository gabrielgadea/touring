"""Memory operations against rlm_memory.db.

73k+ entries across 5 tiers: ephemeral (49k), reference (24k),
project (15), working (13), global (1).

For recall, defaults to non-ephemeral tiers to reduce noise.
"""

from __future__ import annotations

import json
import time
from typing import Any

from .db import MEMORY_DB, connect


VALID_TIERS = ("ephemeral", "working", "reference", "project", "global")


def recall(
    query: str,
    *,
    limit: int = 10,
    tier: str = "all",
    entry_type: str | None = None,
) -> list[dict[str, Any]]:
    """Search memory entries by key or value substring match.

    Args:
        query: Substring to search for (case-insensitive).
        limit: Maximum results. Default 10.
        tier: Filter by tier. "all" = all tiers, "durable" = non-ephemeral.
        entry_type: Optional filter by entry_type (skill, insight, etc.).

    Returns:
        List of memory entry dicts.
    """
    conditions = ["(key LIKE ? COLLATE NOCASE OR value LIKE ? COLLATE NOCASE)"]
    params: list[Any] = [f"%{query}%", f"%{query}%"]

    if tier == "durable":
        conditions.append("tier != 'ephemeral'")
    elif tier != "all" and tier in VALID_TIERS:
        conditions.append("tier = ?")
        params.append(tier)

    if entry_type:
        conditions.append("entry_type = ?")
        params.append(entry_type)

    where = " AND ".join(conditions)
    sql = f"""
        SELECT key, tier, value, entry_type, access_count,
               created_at, accessed_at
        FROM memory_entries
        WHERE {where}
        ORDER BY access_count DESC, accessed_at DESC
        LIMIT ?
    """
    params.append(limit)

    with connect(MEMORY_DB) as conn:
        rows = conn.execute(sql, params).fetchall()

    return [dict(r) for r in rows]


def store(
    key: str, value: str, *, tier: str = "working", entry_type: str = "insight"
) -> dict[str, Any]:
    """Store a new memory entry or update existing.

    Args:
        key: Memory key (unique within tier).
        value: Memory value text.
        tier: Target tier. Default "working".
        entry_type: Entry type. Default "insight".

    Returns:
        Dict with stored entry details.

    Raises:
        ValueError: If tier is not valid.
    """
    if tier not in VALID_TIERS:
        raise ValueError(f"Invalid tier '{tier}'. Valid: {', '.join(VALID_TIERS)}")

    now = int(time.time())

    with connect(MEMORY_DB, readonly=False) as conn:
        conn.execute(
            """
            INSERT INTO memory_entries (key, tier, value, entry_type, created_at, accessed_at, access_count)
            VALUES (?, ?, ?, ?, ?, ?, 1)
            ON CONFLICT(key, tier) DO UPDATE SET
                value = excluded.value,
                accessed_at = excluded.accessed_at,
                access_count = access_count + 1
            """,
            (key, tier, value, entry_type, now, now),
        )
        conn.commit()

    return {"key": key, "tier": tier, "entry_type": entry_type, "stored_at": now}


def list_entries(
    *,
    tier: str = "all",
    limit: int = 20,
    sort_by: str = "accessed_at",
) -> list[dict[str, Any]]:
    """List memory entries with optional tier filter.

    Args:
        tier: Filter by tier, or "all" for all tiers.
        limit: Maximum results.
        sort_by: Sort field: "accessed_at", "access_count", "created_at".

    Returns:
        List of memory entry dicts.
    """
    valid_sorts = {"accessed_at", "access_count", "created_at"}
    if sort_by not in valid_sorts:
        sort_by = "accessed_at"

    conditions = []
    params: list[Any] = []

    if tier != "all" and tier in VALID_TIERS:
        conditions.append("tier = ?")
        params.append(tier)

    where = f"WHERE {' AND '.join(conditions)}" if conditions else ""

    sql = f"""
        SELECT key, tier, entry_type, access_count,
               length(value) as value_length, created_at, accessed_at
        FROM memory_entries
        {where}
        ORDER BY {sort_by} DESC
        LIMIT ?
    """
    params.append(limit)

    with connect(MEMORY_DB) as conn:
        rows = conn.execute(sql, params).fetchall()

    return [dict(r) for r in rows]


def stats() -> dict[str, Any]:
    """Get memory statistics: counts by tier and entry_type.

    Returns:
        Dict with total, by_tier, by_type, oldest/newest timestamps.
    """
    with connect(MEMORY_DB) as conn:
        total = conn.execute("SELECT COUNT(*) FROM memory_entries").fetchone()[0]

        tier_rows = conn.execute(
            "SELECT tier, COUNT(*) as cnt FROM memory_entries GROUP BY tier ORDER BY cnt DESC"
        ).fetchall()

        type_rows = conn.execute(
            "SELECT entry_type, COUNT(*) as cnt FROM memory_entries GROUP BY entry_type ORDER BY cnt DESC LIMIT 15"
        ).fetchall()

        time_row = conn.execute(
            "SELECT MIN(created_at), MAX(created_at), MIN(accessed_at), MAX(accessed_at) FROM memory_entries"
        ).fetchone()

    return {
        "total_entries": total,
        "by_tier": {r["tier"]: r["cnt"] for r in tier_rows},
        "by_type": {r["entry_type"]: r["cnt"] for r in type_rows},
        "oldest_created": time_row[0],
        "newest_created": time_row[1],
        "oldest_accessed": time_row[2],
        "newest_accessed": time_row[3],
    }


def format_results(results: list[dict[str, Any]], *, as_json: bool = False) -> str:
    """Format memory results for CLI output.

    Args:
        results: List of memory dicts.
        as_json: If True, return JSON.

    Returns:
        Formatted string.
    """
    if as_json:
        return json.dumps(results, indent=2, sort_keys=True, ensure_ascii=False)

    if not results:
        return "No results found."

    lines = []
    for r in results:
        key = r.get("key", "?")
        tier = r.get("tier", "?")
        etype = r.get("entry_type", "?")
        count = r.get("access_count", 0)
        value = r.get("value", "")

        # Truncate value for display
        preview = (
            value[:120].replace("\n", " ") + ("..." if len(value) > 120 else "")
            if value
            else ""
        )
        lines.append(f"  [{tier:10s}] {key[:50]:50s} ({etype}, x{count})")
        if preview:
            lines.append(f"             {preview}")

    return "\n".join(lines)
