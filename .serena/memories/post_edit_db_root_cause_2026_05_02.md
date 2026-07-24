---
name: post_edit_db_root_cause_2026_05_02
description: post_edit hook working but populating wrong knowledge.db
type: project
---

# Root Cause: post_edit populating wrong database

**Daemon**: touring-hook --start-daemon (PID 3242188)
**PROJECT_ROOT**: `/home/gabrielgadea/projects/konverter` (NOT `~/.claude/rust/`)
**TOURING_DATA_DIR**: `/home/gabrielgadea/projects/konverter/.claude/data`

**Problem**: knowledge.db at `~/.claude/rust/.claude/touring/knowledge.db` has EMPTY tables (file_knowledge=0, edit_history=0, file_coedits=0).

**Actual behavior**: post_edit IS working — it populates the database at `~/.claude/projects/konverter/.claude/touring/knowledge.db`:
- file_knowledge: 118 files
- edit_history: 312 entries
- file_coedits: 758 pairs

**Why**: Daemon was started via Claude Code session with `CLAUDE_PROJECT_DIR=/home/gabrielgadea/projects/konverter`. HookRuntime uses `CLAUDE_PROJECT_DIR` to set `project_root`, not `TOURING_PROJECT_ROOT`.

**Fix**: Restart daemon with correct PROJECT_ROOT before touring edits.

**Verification**:
```bash
sqlite3 ~/.claude/projects/konverter/.claude/touring/knowledge.db "SELECT COUNT(*) FROM file_knowledge"  # 118
sqlite3 ~/.claude/rust/.claude/touring/knowledge.db "SELECT COUNT(*) FROM file_knowledge"  # 0
```
