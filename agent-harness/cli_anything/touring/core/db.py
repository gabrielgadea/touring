"""Database discovery and connection management for Touring SQLite databases.

Three databases:
- touring_symbols.db: 60k+ symbols (name, kind, file, line, language)
- rlm_memory.db: 73k+ memory entries (key, tier, value, entry_type)
- touring_knowledge.db: file_knowledge, file_relations, bash_outcomes
"""

from __future__ import annotations

import os
import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

# Database filenames
SYMBOLS_DB = "touring_symbols.db"
MEMORY_DB = "rlm_memory.db"
KNOWLEDGE_DB = "touring_knowledge.db"


def _find_project_root() -> Path | None:
    """Walk up from CWD to find project root (has CLAUDE.md)."""
    cur = Path.cwd().resolve()
    for _ in range(10):
        if (cur / "CLAUDE.md").exists():
            return cur
        if cur == cur.parent:
            break
        cur = cur.parent
    return None


def get_db_path(db_name: str) -> Path:
    """Resolve full path for a Touring database file.

    Priority: project root > env var > CWD > home.

    Args:
        db_name: One of SYMBOLS_DB, MEMORY_DB, KNOWLEDGE_DB.

    Returns:
        Absolute path to the database file.

    Raises:
        FileNotFoundError: If not found in any location.
    """
    candidates: list[Path] = []

    # 1. Project root (highest priority — has CLAUDE.md)
    root = _find_project_root()
    if root:
        # 1a. New touring DB location (.claude/touring/) — v5.0+ bootstrap
        touring_dir = root / ".claude" / "touring"
        if touring_dir.is_dir():
            # Map old DB names to new locations
            alt_name = {"touring_symbols.db": "symbols.db"}.get(db_name, db_name)
            candidates.append(touring_dir / alt_name)
        # 1b. Legacy data location (.claude/data/)
        data_dir = root / ".claude" / "data"
        if data_dir.is_dir():
            candidates.append(data_dir / db_name)

    # 2. Env var override
    env_dir = os.environ.get("TOURING_DATA_DIR")
    if env_dir:
        candidates.append(Path(env_dir) / db_name)

    # 3. CWD direct
    cwd_data = Path.cwd() / ".claude" / "data" / db_name
    if cwd_data not in candidates:
        candidates.append(cwd_data)

    # 4. Global fallback
    candidates.append(
        Path.home() / ".claude" / "data" / db_name,
    )

    for p in candidates:
        if p.exists():
            return p

    raise FileNotFoundError(
        f"Database '{db_name}' not found. Searched:\n"
        + "\n".join(f"  {c}" for c in candidates)
    )


@contextmanager
def connect(
    db_name: str,
    *,
    readonly: bool = True,
) -> Iterator[sqlite3.Connection]:
    """Context manager for SQLite connection with WAL mode."""
    db_path = get_db_path(db_name)

    if readonly:
        uri = f"file:{db_path}?mode=ro"
        conn = sqlite3.connect(uri, uri=True, timeout=5.0)
    else:
        conn = sqlite3.connect(str(db_path), timeout=5.0)

    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA busy_timeout=3000")

    try:
        yield conn
    finally:
        conn.close()
