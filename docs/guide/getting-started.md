# Getting Started with Touring (Per-Project)

> **5-minute tutorial** — Set up Touring on a fresh machine and a fresh project.
> Status: Wave W12 foundation (2026-05-23). Subtasks DONE: W12.1, W12.2, W12.4,
> W12.6, W12.7. Pending: W12.3 (toolchain install/update), W12.5 (daemon
> multi-instance), W12.8 (install.touring.dev), W12.9/10 (pilots), W12.12 (CI).

## Audience

This guide is for two readers:

1. **Existing user** who has been running Touring under the global layout
   (`~/.claude/touring/`) and now wants a per-project install.
2. **New user** starting from zero and adopting the per-project layout
   directly.

If you fall into category 1, you may want to read the migration guide first:
[`migration.md`](migration.md).

## Mental model — rustup-pattern

Touring 2026 follows the same shape as `rustup`:

| Layer | Path | Role |
|-------|------|------|
| **User-level root** | `~/.touring/` | Toolchains, default version, user-wide config |
| **Per-project** | `<project>/.touring/` | Project-isolated DBs, project config, project-local binaries |
| **Per-binary dispatcher** | `~/.claude/hooks/touring-hook` | Walks up from CWD to find the right binary |

The CLI commands are the canonical way to populate each layer. **You do not
hand-edit any of these layouts.**

## Prerequisites

- A Touring binary somewhere in `$PATH` — `touring --version` returns a value.
- (Optional) a daemon running — `touring doctor -j` returns 5/6 ok.

If you do not have a binary yet, jump to [external-client.md](external-client.md)
for `curl install.touring.dev | sh` (W12.8 — not yet shipped at the time of
writing).

## Step 1 — Initialize the user-level toolchain root

```bash
touring toolchain init
```

This scaffolds:

```text
~/.touring/
├── toolchains/      # empty until you install one (W12.3)
└── config.toml      # user-level config (User layer in detect_layered)
```

You can rerun with `--force` to overwrite a stale tree, and `TOURING_HOME=...`
overrides the location if `~/.touring/` is not where you want it.

```bash
touring toolchain list
# touring toolchain: no toolchains installed under /home/user/.touring
# Run `touring toolchain init` to scaffold the root.
```

For now, `touring toolchain install <ver>` is W12.3 (deferred). Once shipped,
you will:

```bash
touring toolchain install 0.30.0   # downloads + verifies signature
touring toolchain default 0.30.0   # sets ~/.touring/default
```

## Step 2 — Initialize the per-project tree

```bash
cd ~/projects/myproject
touring init-project
```

This scaffolds:

```text
.touring/
├── touring.toml     # per-project config (Project layer — highest precedence in detect_layered)
├── data/            # per-project DBs (symbols.db, memory.db, graph.db)
├── bin/             # per-project binary slot (read by hook walk-up shim)
└── hooks/           # project-local hook overrides (optional)
```

Flags:

- `--force` — overwrite an existing `.touring/` (default: refuse)
- `--bare` — skip the default `touring.toml` body (write your own via another tool)
- `--root=<path>` — target an explicit directory instead of the CWD

The generated `touring.toml` contains sensible defaults inherited from
`TouringConfig::default()`. Tune any field — your overrides win because the
Project layer is the highest-precedence layer below env vars.

## Step 3 — (Optional) migrate existing global data

If you already have data under `~/.claude/touring/`, copy it in:

```bash
touring migrate-from-global --dry-run
# [DRY-RUN] touring migrate-from-global: ~/.claude/touring → .touring/data
#   Copied (10):
#     would copy symbols.db
#     would copy knowledge.db
#     ...

touring migrate-from-global       # real copy; backs up existing files in dest
```

Default behavior backs up any existing dest files to `<name>.bak.<unix_ts>`.
Pass `--force` to overwrite without backup (explicit intent).

See [migration.md](migration.md) for the full migration walkthrough.

## Step 4 — (Opt-in) activate the walk-up hook shim

By default `~/.claude/hooks/touring-hook` is a symlink to the global release
binary. To activate the walk-up shim (which looks for `.touring/bin/touring-hook`
in the project tree first, then `~/.touring/toolchains/<default>/bin/`, then
falls back to the global binary):

```bash
ln -sfn ~/.claude/rust/scripts/hooks/touring-hook-shim.sh \
    ~/.claude/hooks/touring-hook
```

To revert at any time:

```bash
ln -sfn ~/.claude/rust/target/release/touring-hook \
    ~/.claude/hooks/touring-hook
```

The shim is **fail-open**: if no binary is found at any layer, it silently
exits 0 so your tool call is never blocked.

Trace the resolution chain:

```bash
TOURING_HOOK_SHIM_TRACE=1 ~/.claude/hooks/touring-hook pre-edit 2>&1
# touring-hook-shim: project_bin: /home/user/projects/myproject/.touring/bin/touring-hook
```

## Step 5 — Confirm the layered config is reading your layers

```bash
touring status -j | jq '.config_layers'  # NEW JSON field — to be added by a future wave
```

You can also test from a Rust REPL or unit test:

```rust
use touring_foundation::config::TouringConfig;
let cfg = TouringConfig::detect_layered().expect("layered detect");
println!("cache_size = {}", cfg.cache_size);
```

The precedence is documented in detail in the [API reference](#).

## What still requires manual work (as of 2026-05-23)

- **Toolchain install/update** (`touring toolchain install <ver>`) — W12.3,
  pending. For now, populate `~/.touring/toolchains/<ver>/bin/` by hand.
- **Daemon multi-instance per-project socket** — W12.5, pending. Today's
  daemon still uses the global socket; per-project isolation arrives in W12.5.
- **External installer** (`curl install.touring.dev | sh`) — W12.8, pending.
- **GitHub Actions matrix** — W12.12, pending.

## Where to go next

- [migration.md](migration.md) — Step-by-step transition from global layout
- [external-client.md](external-client.md) — Install from scratch (W12.8 ship target)
- `touring --help` — Full CLI reference
- `touring doctor -j` — Health diagnostic

## Reference

- Plan: `~/.claude/rust/docs/plans/touring-premium-refactor-2026/W12-per-project-deployment.md`
- Changelog entries: `09-CHANGELOG.md` (W12.1, W12.2, W12.4, W12.6, W12.7 — all 2026-05-23)
- Rustup pattern source: `/rust-lang/rustup` (Context7 query, 2026-05-23)
