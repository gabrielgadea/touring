## OPERATIONAL NOTE (2026-03-29): settings.json Must Stay in Sync with main.rs Handlers

### Problem
The `settings.json` hook registry can silently fall out of sync with handlers implemented in `main.rs`.
The `Setup` lifecycle event was handled in `main.rs` dispatch but was missing from `settings.json` for an unknown period — Claude Code never sent the event to the daemon, so the handler was dead code.

### Fix Applied
Added `"Setup"` entry to `settings.json`:
```json
{
  "event": "Setup",
  "command": "~/.claude/hooks/touring-hook setup",
  "timeout": 5000
}
```

### Rule
**After adding ANY handler to `main.rs` dispatch table, immediately add the corresponding entry to `settings.json` in the same commit.**

### Practice
When reviewing a PR that adds a new hook handler:
1. Check `settings.json` — is the event wired?
2. Check `hook_registry.rs` — is the hook name in `ALL_DAEMON_HOOK_NAMES`?
3. If either is missing, the handler is dead code regardless of its quality.

### Audit Command
To find handlers in main.rs that may lack settings.json entries:
```bash
grep -E '"[A-Z][a-zA-Z]+" =>' main.rs | grep -v '//' 
# compare against settings.json event names
```

### Context
Claude Code emits `Setup` on repository initialization. This feeds project metadata into the knowledge graph at the correct lifecycle point (repo init, not first tool use).
