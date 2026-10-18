"""Gotcha database operations — Tier 1 (SQLite direct, <10ms)."""
from __future__ import annotations

import sqlite3

from .db import KNOWLEDGE_DB, connect

#: No writer increments gotchas.prevented_errors (the Rust FileKnowledgeDB::PREVENTED_MEASURED is false, 17/10/2026):
#: hit_count counts warnings shown and nothing records an error that did not happen.  The column's default 0 is not a
#: count, so the harness serves null plus this reason.
PREVENTED_MEASURED = False
PREVENTED_NOTE = "não medido: nenhum produtor grava prevented_errors (hit_count conta avisos exibidos)"


def add(pattern: str, gotcha_text: str, severity: str = "warning", symbol_name: str | None = None) -> dict:
    """Add a gotcha entry."""
    with connect(KNOWLEDGE_DB, readonly=False) as conn:
        # Ensure table exists
        conn.execute("""
            CREATE TABLE IF NOT EXISTS gotchas (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                gotcha TEXT NOT NULL,
                severity TEXT NOT NULL DEFAULT 'warning',
                symbol_name TEXT,
                hit_count INTEGER NOT NULL DEFAULT 0,
                prevented_errors INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        """)
        cursor = conn.execute(
            "INSERT INTO gotchas (pattern, gotcha, severity, symbol_name) VALUES (?, ?, ?, ?)",
            (pattern, gotcha_text, severity, symbol_name),
        )
        conn.commit()
        return {"id": cursor.lastrowid, "pattern": pattern, "gotcha": gotcha_text, "severity": severity}


def list_all(limit: int = 50) -> list[dict]:
    """List all gotchas."""
    with connect(KNOWLEDGE_DB) as conn:
        try:
            rows = conn.execute(
                "SELECT id, pattern, gotcha, severity, symbol_name, hit_count, prevented_errors, created_at "
                "FROM gotchas ORDER BY hit_count DESC, severity DESC LIMIT ?",
                (limit,),
            ).fetchall()
            out = [dict(r) for r in rows]
            if not PREVENTED_MEASURED:
                for r in out:
                    r["prevented_errors"] = None
            return out
        except sqlite3.Error:  # no gotchas table yet: an empty answer, not a crash
            return []


def match_file(file_path: str) -> list[dict]:
    """Find gotchas matching a file path."""
    with connect(KNOWLEDGE_DB) as conn:
        try:
            rows = conn.execute(
                "SELECT id, pattern, gotcha, severity, symbol_name, hit_count "
                "FROM gotchas WHERE ? LIKE '%' || pattern || '%' "
                "ORDER BY severity DESC, hit_count DESC",
                (file_path,),
            ).fetchall()
            return [dict(r) for r in rows]
        except sqlite3.Error:  # no gotchas table yet: an empty answer, not a crash
            return []


def stats() -> dict:
    """Get gotcha statistics."""
    with connect(KNOWLEDGE_DB) as conn:
        try:
            total = conn.execute("SELECT COUNT(*) FROM gotchas").fetchone()[0]
            total_hits = conn.execute("SELECT COALESCE(SUM(hit_count), 0) FROM gotchas").fetchone()[0]
            total_prevented = (conn.execute("SELECT COALESCE(SUM(prevented_errors), 0) FROM gotchas").fetchone()[0]
                               if PREVENTED_MEASURED else None)
            by_severity: dict[str, int] = {}
            for row in conn.execute("SELECT severity, COUNT(*) FROM gotchas GROUP BY severity").fetchall():
                by_severity[row[0]] = row[1]
            return {
                "total": total,
                "total_hits": total_hits,
                "total_prevented": total_prevented,
                "prevented_measured": PREVENTED_MEASURED,
                "prevented_note": None if PREVENTED_MEASURED else PREVENTED_NOTE,
                "by_severity": by_severity,
            }
        except sqlite3.Error:  # no gotchas table yet: an empty answer, not a crash
            return {"total": 0, "total_hits": 0, "total_prevented": None, "prevented_measured": PREVENTED_MEASURED,
                    "prevented_note": None if PREVENTED_MEASURED else PREVENTED_NOTE, "by_severity": {}}
