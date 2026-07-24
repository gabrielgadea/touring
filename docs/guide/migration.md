# Migration Guide — Global → Per-Project

> **Audience**: existing Touring user moving from the global layout
> (`~/.claude/touring/`) to the per-project layout (`<project>/.touring/`).
> Status: Wave W12 partial (W12.7 `migrate-from-global` shipped 2026-05-23).

## Why migrate?

The historical global layout puts everything for every project under
`~/.claude/touring/`. This works for one project but causes three pain points
as soon as you have two:

1. **DB pollution** — symbols from project A appear when you search in project B.
2. **No version pinning** — both projects use whatever Touring binary is in `$PATH`.
3. **No per-project tuning** — `cache_size`, `embedding_dim`, etc. are one global value.

The per-project layout (rustup-pattern) fixes all three:

| | Global (legacy) | Per-project (W12) |
|---|---|---|
| Symbols DB | `~/.claude/touring/symbols.db` (shared) | `<proj>/.touring/data/symbols.db` (isolated) |
| Toolchain version | `$PATH` lookup | `.touring/bin/touring-hook` walk-up + `~/.touring/default` |
| Config tuning | `~/.claude/touring/config.toml` (one) | 4-layer: project < user < system < hardcoded |

## Prerequisites

- Touring binary in `$PATH` with W12 subcommands available
  (`touring init-project`, `touring toolchain`, `touring migrate-from-global`).
- An existing `~/.claude/touring/` directory with the DBs you want to keep
  (`symbols.db`, `memory.db`, `knowledge.db`, etc.).
- A backup plan — `migrate-from-global` defaults to backup-on-overwrite, but
  if you have spare disk space, a tar snapshot is cheap insurance:

  ```bash
  tar -czf ~/touring-global-backup-$(date +%Y%m%d).tar.gz ~/.claude/touring/
  ```

## Step 1 — dry-run

**Always dry-run first.** Confirm the source and destination paths and the
file list match your expectations.

```bash
cd ~/projects/myproject
touring init-project              # creates .touring/data/ (W12.1)
touring migrate-from-global --dry-run
```

Output:

```text
[DRY-RUN] touring migrate-from-global: /home/user/.claude/touring → /home/user/projects/myproject/.touring/data
  Copied (10):
    would copy symbols.db
    would copy knowledge.db
    would copy memory.db
    would copy graph.db
    would copy semantic_recall.db
    would copy rlm_memory.db
    would copy ann_memory.db
    would copy touring_knowledge.db
    would copy touring_pipeline.db
    would copy got_snapshots.db
```

If anything looks wrong (typo in destination, missing source, surprise files):
fix it before running for real.

## Step 2 — first migration

```bash
touring migrate-from-global
```

This copies each known DB byte-for-byte into `.touring/data/`. Existing files
in the destination are renamed to `<name>.bak.<unix_ts>` (visible at
destination dir).

Output:

```text
touring migrate-from-global: /home/user/.claude/touring → /home/user/projects/myproject/.touring/data
  Copied (10):
    copied symbols.db
    ...
```

## Step 3 — verify

The DBs are now under `.touring/data/`. Confirm:

```bash
ls -la .touring/data/
# -rw-r--r-- 1 user user 65M mai 23 14:30 symbols.db
# -rw-r--r-- 1 user user 12M mai 23 14:30 memory.db
# ...
```

Check that the per-project layered config is reading them:

```rust
let cfg = TouringConfig::detect_layered().expect("layered");
// Note: detect_layered does NOT auto-rewrite the db path fields to point at
// .touring/data/ — that is the daemon's responsibility (W12.5). For now you
// can override via env: TOURING_DB_PATH=.touring/data/symbols.db
```

> **Caveat (W12.5 pending)**: the daemon does not yet auto-discover the
> per-project DBs via walk-up. Until W12.5 ships, you must point the running
> daemon at the new paths via env vars or stop/restart with explicit paths.

## Step 4 — second migration (overwrite scenario)

If you re-migrate later (e.g., after editing more code under the global layout
to catch up on indexes), `migrate-from-global` again backs up the project DBs
before overwriting:

```bash
touring migrate-from-global
ls .touring/data/
# symbols.db
# symbols.db.bak.1748033400
# memory.db
# memory.db.bak.1748033400
# ...
```

The backups are timestamped — collect them periodically if disk space matters.

## Step 5 — force mode (CI / scripted)

In CI or other scripted contexts where you don't want stale backups
accumulating, use `--force`:

```bash
touring migrate-from-global --force
```

This overwrites the destination files without creating `.bak.<ts>` copies.
**Use sparingly** — once a `.bak` is missing, you cannot roll back without
your own snapshot.

## Step 6 — rollback

If the migration produced unwanted state, rollback options:

### Option A — restore from `.bak.<ts>` files

```bash
cd .touring/data/
mv symbols.db.bak.1748033400 symbols.db   # restore the previous version
```

### Option B — restore from the tar snapshot

If you took the tar snapshot in Prerequisites:

```bash
rm -rf .touring/data/
tar -xzf ~/touring-global-backup-20260523.tar.gz -C /
# Re-runs the original global state.
```

### Option C — re-pull from global

If `~/.claude/touring/` is still intact (the migration only **copies**, never
deletes the source):

```bash
touring migrate-from-global --force   # re-copy from global
```

## Step 7 — clean up the global layout (optional)

Once you have validated that the per-project layout works end-to-end (after
W12.5 daemon multi-instance ships), you can remove the global tree:

```bash
# Verify per-project is healthy first
touring doctor -j

# Then remove the global tree
mv ~/.claude/touring/ ~/.claude/touring.archived-$(date +%Y%m%d)/
# Keep the archive for a few weeks before truly deleting it.
```

**Don't `rm -rf ~/.claude/touring/` yet** until you have a daemon running
per-project and have confirmed indexing/memory/wiring all work against the
migrated DBs.

## What `migrate-from-global` does NOT do

- **Does not delete the source** — `~/.claude/touring/` is left intact. Manual
  cleanup is yours.
- **Does not filter by project** — every DB is copied byte-for-byte. If
  `symbols.db` contains symbols from project A and project B, **both** end up
  in project A's `.touring/data/symbols.db`. Per-project filtering requires
  schema changes (see "Future enhancement" in the W12.7 changelog entry).
- **Does not migrate ad-hoc files** — only the 10 known DB files in
  `MIGRATE_FILES`. Logs, caches, custom configs, etc. are left alone.

## File list migrated

| File | Purpose |
|---|---|
| `symbols.db` | AST symbol index (file → kind → location) |
| `knowledge.db` | Consolidated symbols + file knowledge + wiring (post-W6) |
| `memory.db` | RLM episodic + semantic recall + ANN embeddings |
| `graph.db` | GoT sessions + RL pipeline + hook events |
| `semantic_recall.db` | (legacy, may be empty post-consolidation) |
| `rlm_memory.db` | (legacy) |
| `ann_memory.db` | (legacy) |
| `touring_knowledge.db` | (legacy) |
| `touring_pipeline.db` | (legacy) |
| `got_snapshots.db` | (legacy) |

Legacy DBs are migrated for completeness; they may be empty if the daemon
already migrated to the consolidated schema (see `touring migrate run` —
distinct from `migrate-from-global`).

## Troubleshooting

### "source `/home/user/.claude/touring/` does not exist"

You have no global layout to migrate from. This is the "new user" case — go
back to [getting-started.md](getting-started.md) Step 2.

### "permission denied" on a `.db` file

The DB is likely held by a running daemon. Stop the daemon first:

```bash
pkill -TERM -f "touring-hook --start-daemon"
touring migrate-from-global
# Then restart the daemon — see touring-rebuild.md
```

### Destination already has `.bak.<ts>` files from a prior run

Safe. The new run creates `.bak.<new_ts>` so you have multiple snapshots.
Clean up older `.bak.*` files at your leisure.

## Next steps

After migration:

1. Activate the hook shim ([getting-started.md](getting-started.md) Step 4)
2. Wait for W12.5 (daemon multi-instance) for per-project daemon isolation
3. Open the project in Claude Code and verify hooks see the project DBs

## Reference

- Implementation: `crates/touring-server/src/cli/migrate_from_global.rs`
- Changelog: `09-CHANGELOG.md` § `[W12.7-2026-05-23]`
- Plan: `W12-per-project-deployment.md` § W12.7
