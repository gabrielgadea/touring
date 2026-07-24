"""Flywheel status operations — Tier 1 (SQLite direct, <10ms)."""
from __future__ import annotations

from pathlib import Path

from .db import KNOWLEDGE_DB, MEMORY_DB, SYMBOLS_DB, connect


def status() -> dict:
    """Get comprehensive flywheel status."""
    result: dict = {"version": "6.0.0"}

    # Gotcha DB
    try:
        with connect(KNOWLEDGE_DB) as conn:
            gotcha_count = conn.execute("SELECT COUNT(*) FROM gotchas").fetchone()[0]
            gotcha_hits = conn.execute("SELECT COALESCE(SUM(hit_count), 0) FROM gotchas").fetchone()[0]
            result["gotcha_db"] = {"entries": gotcha_count, "total_hits": gotcha_hits}
    except Exception:
        result["gotcha_db"] = {"entries": 0, "total_hits": 0}

    # Knowledge DB
    try:
        with connect(KNOWLEDGE_DB) as conn:
            files = conn.execute("SELECT COUNT(*) FROM file_knowledge").fetchone()[0]
            rels = conn.execute("SELECT COUNT(*) FROM file_relations").fetchone()[0]
            edits = conn.execute("SELECT COUNT(*) FROM edit_history").fetchone()[0]
            bash = conn.execute("SELECT COUNT(*) FROM bash_outcomes").fetchone()[0]
            result["knowledge"] = {"files": files, "relations": rels, "edits": edits, "bash_outcomes": bash}
    except Exception:
        result["knowledge"] = {"files": 0, "relations": 0, "edits": 0, "bash_outcomes": 0}

    # Memory
    try:
        with connect(MEMORY_DB) as conn:
            total = conn.execute("SELECT COUNT(*) FROM memory_entries").fetchone()[0]
            result["memory"] = {"total_entries": total}
    except Exception:
        result["memory"] = {"total_entries": 0}

    # Symbols
    try:
        with connect(SYMBOLS_DB) as conn:
            syms = conn.execute("SELECT COUNT(*) FROM symbols").fetchone()[0]
            result["symbols"] = {"total": syms}
    except Exception:
        result["symbols"] = {"total": 0}

    # Flywheel components
    rust_dir = Path.cwd() / ".claude" / "rust"
    components = {
        "focus_cache_lru": (rust_dir / "crates/touring-cognitive/src/focus_cache.rs").exists(),
        "gotcha_db": result.get("gotcha_db", {}).get("entries", 0) > 0,
        "bandit_linucb": (rust_dir / "crates/touring-learning/src/bandit/linucb.rs").exists(),
        "templates_evolving": (rust_dir / "crates/touring-learning/src/templates/evolving.rs").exists(),
        "session_insights": (rust_dir / "crates/touring-hooks/src/session_insights.rs").exists(),
        "context_compiler": (rust_dir / "crates/touring-server/src/context_compiler.rs").exists(),
        "enrichment_pipeline": (rust_dir / "crates/touring-server/src/cortex/enrichment.rs").exists(),
        "rules_engine": (rust_dir / "crates/touring-rules/rules").is_dir(),
    }
    result["components"] = components
    result["components_active"] = sum(1 for v in components.values() if v)
    result["components_total"] = len(components)

    return result
