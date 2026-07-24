---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
type: "deployment"
created: "2026-05-11"
relates_to:
  - 01-ARCHITECTURE.md
  - 03-COMMERCIAL.md
  - W12-per-project-deployment.md
---
# 02-DEPLOYMENT — Touring Per-Project Deployment Model

> **Status**: Proposed | **Date**: 2026-05-11
> **Approved by**: Gabriel Gadea (architect) — "rustup-like AFTER architectural refactor; must serve external clients"

## 1. Current state (problem)

Touring runs as **single global installation**:
- Binaries: `~/.local/bin/touring` → `~/.claude/rust/target/release/touring`
- Daemon socket: `/tmp/touring-daemon-1000.sock` (global, USER-scoped)
- Knowledge DB: `~/.claude/touring/symbols.db`
- Memory: `~/.claude/projects/-home-gabrielgadea/memory/`

**6 problems**: (1) single binary version; (2) knowledge DB pollution; (3) memory
cross-contamination; (4) no rollback per project; (5) external clients impossible;
(6) daemon contention multi-project.

## 2. Target state: rustup-style toolchain + per-project `.touring/`

### 2.1 `~/.touring/` (user toolchain manager)

```
~/.touring/
├── toolchains/
│   ├── 1.0.0/  bin/{touring,touring-hook,touring-daemon,touring-update}
│   │           lib/   share/{man,completions,templates}
│   │           meta.toml (build info, checksum, version)
│   ├── 1.1.0/
│   └── nightly-2026-05-11/
├── default                          # File: contains "1.0.0"
├── config.toml                      # User-global defaults
├── registry/                        # crates.io mirror (enterprise private)
│   ├── index/  cache/  credentials
├── env.sh                           # Source-able: PATH + completions
└── installer.sh                     # Rerun for self-update/reinstall
```

### 2.2 Per-project `<project>/.touring/`

```
<project>/.touring/
├── touring.toml                    # Project config (versioned in git)
├── bin/                             # Symlinks → ~/.touring/toolchains/<v>/bin/
├── data/
│   ├── symbols.db                  # Project knowledge (isolated)
│   ├── memory.db                   # Project lessons (isolated)
│   ├── tantivy/                    # FTS index
│   ├── vectors/                    # Vector store
│   └── learning.db                 # RL state per-project
├── cache/                          # Throwaway, regenerable
├── hooks/                          # Project-specific overrides
├── daemon.sock                     # Per-project socket
├── daemon.lock
└── daemon.log
```

## 3. `.touring/touring.toml` schema v1.0

```toml
[touring]
schema_version = "1.0"
version_constraint = "^1.0"
tier = "premium"
default_toolchain = "1.0.0"

[features]
intelligence = true
generator = true
assists = true
offensive = false
orchestration = true
bindings = ["python", "web"]

[languages]
enabled = ["rust", "python", "typescript", "go"]

[daemon]
socket = ".touring/daemon.sock"
log_path = ".touring/daemon.log"
log_level = "info"
idle_timeout_secs = 0
rayon_threads = "auto"
tokio_workers = 4

[memory]
isolation = "project"            # project | user | global
retention_days = 90
fts_engine = "tantivy"
vector_backend = "sqlite-vec"
embedding_provider = "candle"

[telemetry]
opt_in = false                   # Premium default OFF
endpoint = ""
include_pii = false              # NEVER true by default

[hooks]
claude_code = true
project_specific = true
mcp_server = true

[enterprise]                     # Ignored if tier < enterprise
registry_url = ""
sso_provider = ""
audit_log_path = ""
license_key_file = ".touring/license.key"
```

## 4. Daemon discovery (walk-up + fallback)

```
1. From CWD, walk up looking for `.touring/touring.toml`
2. If found:
   a. Read config, resolve `default_toolchain` → ~/.touring/toolchains/<v>
   b. Socket = <found_dir>/.touring/daemon.sock
   c. Connect; if dead, spawn `touring-daemon --config <found_dir>/.touring/touring.toml`
3. If NOT found:
   a. ~/.touring/config.toml (user-global default)
   b. Socket /tmp/touring-daemon-<UID>-default.sock
   c. Spawn with default toolchain
4. Fallback: hardcoded defaults
```

## 5. CLI surface (canonical)

```bash
# Lifecycle
touring init [--tier <T>] [--features <F>] [--languages <L>] [--toolchain <V>]
touring uninstall [--purge]
touring migrate [--from-global]

# Toolchain
touring update [version]                  # rustup-like
touring update --rollback
touring toolchain {list,install,remove,default}

# Components
touring component {list,add,remove}

# Inspection
touring which
touring config {get,set,edit}

# Daemon control
touring daemon {start,stop,restart,status,logs}

# Registry (enterprise)
touring {login,logout}
touring registry {list,sync}
```

## 6. External installer (install.touring.dev)

```bash
$ curl -sSf https://install.touring.dev | sh
# Steps:
# 1. Download installer script (sigstore-signed)
# 2. Verify signature + SHA-256
# 3. Detect OS/arch (linux/macos/win; x86_64/aarch64)
# 4. Download binary tarball + SBOM (CycloneDX)
# 5. Extract → ~/.touring/toolchains/<version>/
# 6. Create symlinks ~/.local/bin/ (or %USERPROFILE%\.touring\bin)
# 7. Generate ~/.touring/env.sh (or env.ps1)
# 8. Write completions to ~/.bashrc / ~/.zshrc / fish_config
# 9. Print getting-started tutorial
```

Mirrors: install.touring.dev, get.touring.dev, GitHub Releases.

## 7. Migration tool (`touring migrate --from-global`)

```
Steps performed:
1. Detect existing .claude/touring/ symbol DB → copy to .touring/data/symbols.db
2. Copy relevant memory entries (filtered by project tag) → .touring/data/memory.db
3. Copy learning state filtered by project → .touring/data/learning.db
4. Generate .touring/touring.toml from inferred features
5. Update .gitignore to ignore .touring/ EXCEPT touring.toml
6. Optionally update ~/.claude/settings.json to use project-scoped hooks
```

## 8. Hook dispatcher (Claude Code backward compat)

```sh
#!/bin/sh
# ~/.claude/hooks/touring-hook (smart dispatcher)
DIR=$PWD
while [ "$DIR" != "/" ]; do
  if [ -x "$DIR/.touring/bin/touring-hook" ]; then
    exec "$DIR/.touring/bin/touring-hook" "$@"
  fi
  DIR=$(dirname "$DIR")
done
exec "$HOME/.touring/toolchains/$(cat $HOME/.touring/default)/bin/touring-hook" "$@"
```

## 9. Multi-daemon coexistence

- Each project: `.touring/daemon.sock` (not /tmp)
- PID file: `.touring/daemon.lock`
- Resource: ~92 MB RSS per daemon
- 16 GB workstation: up to ~50 projects without swap
- Auto-shutdown opt-in via `daemon.idle_timeout_secs > 0`

## 10. Rollout strategy (sequential)

1. **W0-W11**: architectural refactor (13 crates)
2. **W12**: per-project deployment implementation
3. **W13**: publishing (docs.rs, semver-check, sigstore, SBOM)
4. **W14**: commercial tiers + external distribution

Backward compat: feature flag `--legacy-global` (ON in 0.x, OFF in 1.0, removed 1.5).

## 11. References

- W12 wave file: `W12-per-project-deployment.md` (detailed subtasks)
- Memory: `decision:touring-premium-roadmap-2026-05-11`
