"""Symbol index operations against touring_symbols.db or touring/symbols.db.

Supports both legacy schema (name, kind, file, line, language) and
v5.0+ schema (name, file_path, line). Auto-detects which schema is active.
"""

from __future__ import annotations

import json
from typing import Any

from .db import SYMBOLS_DB, connect


def _detect_schema(conn: Any) -> str:
    """Detect which schema the symbols table uses.

    Returns 'legacy' (has kind, file, language) or 'v5' (has file_path only).
    """
    cursor = conn.execute("PRAGMA table_info(symbols)")
    columns = {row[1] for row in cursor.fetchall()}
    if "kind" in columns and "file" in columns:
        return "legacy"
    return "v5"


def search(query: str, *, limit: int = 10, kind: str | None = None) -> list[dict[str, Any]]:
    """Search symbols by substring match on name.

    Auto-detects DB schema (legacy vs v5) and adapts queries.

    Args:
        query: Substring to search for (case-insensitive).
        limit: Maximum results to return. Default 10.
        kind: Optional filter by symbol kind (legacy schema only).

    Returns:
        List of symbol dicts with keys: name, file, line (+ kind, language if legacy).
    """
    with connect(SYMBOLS_DB) as conn:
        schema = _detect_schema(conn)
        if schema == "legacy":
            if kind:
                rows = conn.execute(
                    "SELECT name, kind, file, line, language FROM symbols "
                    "WHERE name LIKE ? COLLATE NOCASE AND kind = ? "
                    "ORDER BY length(name) ASC, name ASC LIMIT ?",
                    (f"%{query}%", kind, limit),
                ).fetchall()
            else:
                rows = conn.execute(
                    "SELECT name, kind, file, line, language FROM symbols "
                    "WHERE name LIKE ? COLLATE NOCASE "
                    "ORDER BY length(name) ASC, name ASC LIMIT ?",
                    (f"%{query}%", limit),
                ).fetchall()
        else:
            # v5 schema: no kind/language columns, file → file_path
            rows = conn.execute(
                "SELECT name, file_path as file, line FROM symbols "
                "WHERE name LIKE ? COLLATE NOCASE "
                "ORDER BY length(name) ASC, name ASC LIMIT ?",
                (f"%{query}%", limit),
            ).fetchall()

    return [dict(r) for r in rows]


def find(name: str, *, exact: bool = False, limit: int = 10) -> list[dict[str, Any]]:
    """Find symbols by exact or prefix match.

    Auto-detects DB schema (legacy vs v5) and adapts queries.

    Args:
        name: Symbol name to find.
        exact: If True, match exactly. If False, prefix match.
        limit: Maximum results.

    Returns:
        List of symbol dicts with keys: name, file, line (+ kind, language if legacy).
    """
    with connect(SYMBOLS_DB) as conn:
        schema = _detect_schema(conn)
        if schema == "legacy":
            cols = "name, kind, file, line, language"
        else:
            cols = "name, file_path as file, line"

        if exact:
            rows = conn.execute(
                f"SELECT {cols} FROM symbols WHERE name = ? LIMIT ?",
                (name, limit),
            ).fetchall()
        else:
            rows = conn.execute(
                f"""
                SELECT {cols} FROM symbols
                WHERE name LIKE ? COLLATE NOCASE
                ORDER BY CASE WHEN name = ? THEN 0 ELSE 1 END, length(name) ASC
                LIMIT ?
                """,
                (f"{name}%", name, limit),
            ).fetchall()

    return [dict(r) for r in rows]


def status() -> dict[str, Any]:
    """Get index status: total symbols, files, distribution. Auto-detects schema."""
    with connect(SYMBOLS_DB) as conn:
        schema = _detect_schema(conn)
        total = conn.execute("SELECT COUNT(*) FROM symbols").fetchone()[0]
        fcol = "file" if schema == "legacy" else "file_path"
        total_files = conn.execute(f"SELECT COUNT(DISTINCT {fcol}) FROM symbols").fetchone()[0]
        by_language: dict[str, int] = {}
        by_kind: dict[str, int] = {}
        if schema == "legacy":
            for r in conn.execute("SELECT language, COUNT(*) c FROM symbols GROUP BY language ORDER BY c DESC"):
                by_language[r[0]] = r[1]
            for r in conn.execute("SELECT kind, COUNT(*) c FROM symbols GROUP BY kind ORDER BY c DESC"):
                by_kind[r[0]] = r[1]
        else:
            for r in conn.execute(
                "SELECT CASE WHEN file_path LIKE '%.py' THEN 'python' WHEN file_path LIKE '%.rs' THEN 'rust'"
                " WHEN file_path LIKE '%.ts' THEN 'typescript' ELSE 'other' END l, COUNT(*) c"
                " FROM symbols GROUP BY l ORDER BY c DESC"
            ):
                by_language[r[0]] = r[1]
    return {"total_symbols": total, "total_files": total_files, "by_language": by_language, "by_kind": by_kind, "schema": schema}


def files(pattern: str | None = None, *, limit: int = 20) -> list[dict[str, Any]]:
    """List indexed files, optionally filtered by pattern.

    Auto-detects DB schema (legacy vs v5) and adapts queries.

    Args:
        pattern: Optional glob-like pattern to filter file paths.
        limit: Maximum results.

    Returns:
        List of dicts with file, symbol_count (+ languages if legacy).
    """
    with connect(SYMBOLS_DB) as conn:
        schema = _detect_schema(conn)
        fcol = "file" if schema == "legacy" else "file_path"
        lang_agg = ", GROUP_CONCAT(DISTINCT language) as languages" if schema == "legacy" else ""

        if pattern:
            rows = conn.execute(
                f"""
                SELECT {fcol} as file, COUNT(*) as symbol_count{lang_agg}
                FROM symbols
                WHERE {fcol} LIKE ?
                GROUP BY {fcol}
                ORDER BY symbol_count DESC
                LIMIT ?
                """,
                (f"%{pattern}%", limit),
            ).fetchall()
        else:
            rows = conn.execute(
                f"""
                SELECT {fcol} as file, COUNT(*) as symbol_count{lang_agg}
                FROM symbols
                GROUP BY {fcol}
                ORDER BY symbol_count DESC
                LIMIT ?
                """,
                (limit,),
            ).fetchall()

    return [dict(r) for r in rows]


def file_overview(file_path: str) -> dict[str, Any]:
    """Get all symbols defined in a specific file.

    Auto-detects DB schema (legacy vs v5) and adapts queries.

    Args:
        file_path: File path (substring match via LIKE).

    Returns:
        Dict with file, total_symbols, and list of symbols.
    """
    with connect(SYMBOLS_DB) as conn:
        schema = _detect_schema(conn)
        if schema == "legacy":
            cols = "name, kind, file, line, language"
            fcol = "file"
        else:
            cols = "name, file_path as file, line"
            fcol = "file_path"

        rows = conn.execute(
            f"""
            SELECT {cols}
            FROM symbols
            WHERE {fcol} LIKE ?
            ORDER BY line ASC
            """,
            (f"%{file_path}%",),
        ).fetchall()

    symbols = [dict(r) for r in rows]
    matched_file = symbols[0]["file"] if symbols else file_path

    return {
        "file": matched_file,
        "total_symbols": len(symbols),
        "symbols": symbols,
    }


def format_results(results: list[dict[str, Any]], *, as_json: bool = False) -> str:
    """Format symbol results for CLI output.

    Args:
        results: List of symbol dicts.
        as_json: If True, return JSON. If False, return table.

    Returns:
        Formatted string.
    """
    if as_json:
        return json.dumps(results, indent=2, sort_keys=True, ensure_ascii=False)

    if not results:
        return "No results found."

    lines = []
    for r in results:
        kind = r.get("kind", "?")
        name = r.get("name", "?")
        fpath = r.get("file", "?")
        line = r.get("line", 0)
        lang = r.get("language", "?")
        lines.append(f"  {kind:12s} {name:40s} {fpath}:{line} [{lang}]")

    return "\n".join(lines)
