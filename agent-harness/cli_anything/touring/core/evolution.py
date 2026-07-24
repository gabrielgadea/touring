"""Evolution analytics from rlm_memory.db and touring_knowledge.db.

Provides insights, drift detection, and tool effectiveness metrics
using the learning_wilson, learning_drift, and bash_outcomes tables.
"""

from __future__ import annotations

import json
import math
from typing import Any

from .db import KNOWLEDGE_DB, MEMORY_DB, connect


def insights() -> dict[str, Any]:
    """Get evolution insights: Wilson-ranked items, drift metrics, top patterns.

    Returns:
        Dict with wilson_top, drift_metrics, knowledge_stats.
    """
    result: dict[str, Any] = {}

    # Wilson-ranked items (confidence-adjusted success rate)
    with connect(MEMORY_DB) as conn:
        wilson_rows = conn.execute(
            """
            SELECT item_id, successes, trials,
                   CASE WHEN trials > 0
                        THEN (successes + 1.0) / (trials + 2.0)
                        ELSE 0.0
                   END as wilson_score
            FROM learning_wilson
            WHERE trials >= 3
            ORDER BY wilson_score DESC
            LIMIT 15
            """
        ).fetchall()
        result["wilson_top"] = [dict(r) for r in wilson_rows]

        # Drift metrics
        drift_rows = conn.execute(
            "SELECT metric, values_json FROM learning_drift ORDER BY metric"
        ).fetchall()
        result["drift_metrics"] = {}
        for r in drift_rows:
            try:
                values = json.loads(r["values_json"])
                if isinstance(values, list) and len(values) >= 2:
                    recent = values[-5:] if len(values) >= 5 else values
                    avg_recent = sum(recent) / len(recent)
                    avg_overall = sum(values) / len(values)
                    trend = (
                        "improving"
                        if avg_recent > avg_overall
                        else (
                            "degrading" if avg_recent < avg_overall * 0.95 else "stable"
                        )
                    )
                    result["drift_metrics"][r["metric"]] = {
                        "total_samples": len(values),
                        "recent_avg": round(avg_recent, 4),
                        "overall_avg": round(avg_overall, 4),
                        "trend": trend,
                    }
            except (json.JSONDecodeError, TypeError):
                result["drift_metrics"][r["metric"]] = {"error": "invalid values_json"}

    # Knowledge stats
    with connect(KNOWLEDGE_DB) as conn:
        fk_count = conn.execute("SELECT COUNT(*) FROM file_knowledge").fetchone()[0]
        fr_count = conn.execute("SELECT COUNT(*) FROM file_relations").fetchone()[0]
        bo_count = conn.execute("SELECT COUNT(*) FROM bash_outcomes").fetchone()[0]

        result["knowledge_stats"] = {
            "files_known": fk_count,
            "relations": fr_count,
            "bash_outcomes": bo_count,
        }

    return result


def drift() -> dict[str, Any]:
    """Get drift report: trend analysis for all tracked metrics.

    Returns:
        Dict with metrics and their trend analysis.
    """
    with connect(MEMORY_DB) as conn:
        rows = conn.execute(
            "SELECT metric, values_json FROM learning_drift ORDER BY metric"
        ).fetchall()

    metrics: dict[str, Any] = {}
    for r in rows:
        try:
            values = json.loads(r["values_json"])
            if isinstance(values, list) and values:
                n = len(values)
                mean = sum(values) / n
                variance = sum((v - mean) ** 2 for v in values) / n if n > 1 else 0.0
                stddev = math.sqrt(variance)

                recent = values[-10:] if n >= 10 else values
                recent_mean = sum(recent) / len(recent)

                metrics[r["metric"]] = {
                    "samples": n,
                    "mean": round(mean, 4),
                    "stddev": round(stddev, 4),
                    "recent_mean": round(recent_mean, 4),
                    "trend": (
                        "improving"
                        if recent_mean > mean * 1.02
                        else "degrading"
                        if recent_mean < mean * 0.98
                        else "stable"
                    ),
                    "last_value": round(values[-1], 4) if values else None,
                }
        except (json.JSONDecodeError, TypeError):
            metrics[r["metric"]] = {"error": "parse_failed"}

    return {"metrics": metrics, "total_tracked": len(metrics)}


def tools() -> dict[str, Any]:
    """Get tool effectiveness from bash_outcomes and Wilson scoring.

    Returns:
        Dict with tool success rates and Wilson scores.
    """
    with connect(KNOWLEDGE_DB) as conn:
        # Top commands by frequency
        cmd_rows = conn.execute(
            """
            SELECT command_short, COUNT(*) as total,
                   SUM(success) as successes,
                   ROUND(AVG(success) * 100, 1) as success_rate
            FROM bash_outcomes
            GROUP BY command_short
            HAVING total >= 5
            ORDER BY total DESC
            LIMIT 20
            """
        ).fetchall()

        # Recent failure patterns
        fail_rows = conn.execute(
            """
            SELECT command_short, error_pattern, COUNT(*) as cnt
            FROM bash_outcomes
            WHERE success = 0 AND error_pattern IS NOT NULL
            GROUP BY command_short, error_pattern
            ORDER BY cnt DESC
            LIMIT 10
            """
        ).fetchall()

    with connect(MEMORY_DB) as conn:
        # Wilson-scored tools (from hook events)
        wilson_tools = conn.execute(
            """
            SELECT item_id, successes, trials,
                   ROUND((successes + 1.0) / (trials + 2.0), 4) as wilson_score
            FROM learning_wilson
            WHERE item_id LIKE 'tool:%'
            ORDER BY wilson_score DESC
            LIMIT 15
            """
        ).fetchall()

    return {
        "top_commands": [dict(r) for r in cmd_rows],
        "failure_patterns": [dict(r) for r in fail_rows],
        "wilson_scored_tools": [dict(r) for r in wilson_tools],
    }


def blast(file_path: str) -> dict[str, Any]:
    """Get blast radius for a file: what other files depend on it.

    Args:
        file_path: Relative file path to analyze.

    Returns:
        Dict with direct_dependents, reverse_dependents, depth.
    """
    with connect(KNOWLEDGE_DB) as conn:
        # Files that depend on this file (forward: this file is source)
        forward = conn.execute(
            """
            SELECT target_path, relation_type, COUNT(*) as strength
            FROM file_relations
            WHERE source_path LIKE ?
            GROUP BY target_path, relation_type
            ORDER BY strength DESC
            """,
            (f"%{file_path}%",),
        ).fetchall()

        # Files this file depends on (reverse: this file is target)
        reverse = conn.execute(
            """
            SELECT source_path, relation_type, COUNT(*) as strength
            FROM file_relations
            WHERE target_path LIKE ?
            GROUP BY source_path, relation_type
            ORDER BY strength DESC
            """,
            (f"%{file_path}%",),
        ).fetchall()

        # File metadata
        meta = conn.execute(
            """
            SELECT file_path, language, line_count, symbol_count, read_count
            FROM file_knowledge
            WHERE file_path LIKE ?
            LIMIT 1
            """,
            (f"%{file_path}%",),
        ).fetchone()

    return {
        "file": file_path,
        "metadata": dict(meta) if meta else None,
        "affects": [dict(r) for r in forward],
        "affected_by": [dict(r) for r in reverse],
        "blast_radius": len(forward),
        "dependency_count": len(reverse),
    }
