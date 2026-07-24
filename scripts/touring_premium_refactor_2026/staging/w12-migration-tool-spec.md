# `touring migrate` — Migration Tool Spec (W12.x)

## Use case
User wants to migrate from global deployment (`~/.claude/rust/` shared) to
per-project mode (`<project>/.touring/`).

## Subcommands

```bash
touring migrate from-global --to <project>     # copy + filter global DBs
touring migrate from-global --dry-run          # preview without copying
touring migrate to-global --from <project>     # reverse (merge to global)
```

## Migration steps (from-global)

1. **Inspect** existing global state:
   - `~/.claude/projects/-home-gabrielgadea/memory/` (memory tier=*)
   - `~/.claude/rust/.touring-cache/` (knowledge_db, tantivy)
2. **Filter** by project path: keep only entries with path-prefix matching `<project>`
3. **Copy** filtered subset to `<project>/.touring/data/`:
   - `memory.db` (SQLite, filtered subset)
   - `tantivy/` (FTS5 index, rebuilt for project scope)
   - `knowledge.db` (semantic memories)
4. **Generate** `<project>/.touring/touring.toml`:
   - `channel = "stable-X.Y.Z"`
   - `[storage] backend = "sqlite"`
   - `[features] tier = "free"` (default)
5. **Verify** post-migration:
   - `touring doctor` from project dir
   - Compare memory count: global vs project-filtered

## Risks

| Risk | Mitigation |
|---|---|
| Lose cross-project memory | `--keep-global` flag (don't delete source) |
| Index rebuild slow (~5min) | Background job + progress bar |
| Partial migration corrupts state | Atomic: write to .touring.tmp then rename |
