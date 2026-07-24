# ADR-002 — Touring Per-Project Deployment Model

> **Status**: Proposed | **Date**: 2026-05-11 | **Authors**: Gabriel Gadea (architect) + TACO (orchestrator)
> **Relates to**: ADR-001 (Architecture), ADR-003 (Commercial Tiers), MASTER-PLAN-2026 (W12)
> **Approved by Gabriel**: Per-project rustup-like AFTER architectural refactor; must serve external clients

## 1. Context

Touring currently runs as a **single global installation**:

| Artifact | Current location |
|---|---|
| Binaries | `~/.local/bin/touring` → `~/.claude/rust/target/release/touring` |
| Daemon socket | `/tmp/touring-daemon-1000.sock` (global, USER-scoped) |
| Knowledge DB | `~/.claude/touring/symbols.db` (or per-project `<ws>/.claude/touring/`) |
| Per-project cache | `<project>/.touring-cache/` (partial; only some projects) |
| Memory | `~/.claude/projects/-home-gabrielgadea/memory/` (USER-scoped) |
| Settings hooks | `~/.claude/settings.json` → `~/.claude/hooks/` |

### Problems

1. **Single binary version**: cannot run touring 1.0 and 2.0 simultaneously per project
2. **Knowledge DB pollution**: symbols of project X bleed into project Y queries
3. **Memory cross-contamination**: lessons from one project apply globally
4. **No rollback per project**: upgrading touring affects ALL projects simultaneously
5. **External clients impossible today**: no `touring install` story for non-Gabriel users
6. **Daemon contention**: `touring-hook --start-daemon` race when multiple projects open

## 2. Decision

**Adopt rustup-style toolchain manager + per-project `.touring/` data.**

### 2.1 Toolchain manager (`~/.touring/`)

```
~/.touring/
├── toolchains/
│   ├── 1.0.0/                         # Stable LTS
│   │   ├── bin/{touring,touring-hook,touring-daemon,touring-update}
│   │   ├── lib/                       # Shared rustls, etc.
│   │   ├── share/
│   │   │   ├── man/man1/touring.1
│   │   │   ├── completions/{bash,zsh,fish}/
│   │   │   └── templates/             # Project init templates
│   │   └── meta.toml                   # Build info, checksum, version
│   ├── 1.1.0/
│   └── nightly-2026-05-11/
├── default                              # File: contains "1.0.0"
├── config.toml                          # User-global defaults
├── registry/                            # crates.io mirror (enterprise: private)
│   ├── index/
│   ├── cache/
│   └── credentials                      # For `touring login`
├── env.sh                               # Source-able: PATH + completions
└── installer.sh                         # Rerun for self-update/reinstall
```

### 2.2 Per-project structure (`<project>/.touring/`)

```
<project>/.touring/
├── touring.toml                         # Project config (versioned, gitignored except this file)
├── bin/                                 # Symlinks to ~/.touring/toolchains/<v>/bin/
│   ├── touring -> ../../../~/.touring/toolchains/1.0.0/bin/touring
│   ├── touring-hook -> ...
│   └── touring-daemon -> ...
├── data/
│   ├── symbols.db                       # Project-scoped knowledge index
│   ├── memory.db                        # Project lessons (no cross-pollution)
│   ├── tantivy/                         # FTS index for this project
│   ├── vectors/                         # Vector store
│   └── learning.db                      # RL state per-project
├── cache/                               # Throwaway, regenerable
├── hooks/                               # Project-specific hook scripts (overrides global)
├── daemon.sock                          # Per-project socket
├── daemon.lock                          # PID lockfile
└── daemon.log                           # Per-project daemon log
```

### 2.3 `touring.toml` schema (project-scoped config)

```toml
[touring]
schema_version = "1.0"                   # Allows future migrations
version_constraint = "^1.0"               # Toolchain compatibility range
tier = "premium"                          # free | standard | premium | enterprise
default_toolchain = "1.0.0"               # Resolves to ~/.touring/toolchains/<v>

[features]
intelligence = true
generator = true
assists = true
offensive = false
orchestration = true
bindings = ["python", "web"]              # Opt-in binding list

[languages]
enabled = ["rust", "python", "typescript", "go"]
overrides = { go = { tier = 1 } }     # Promote Go to tier 1 in this project

[daemon]
socket = ".touring/daemon.sock"          # Relative to project root
log_path = ".touring/daemon.log"
log_level = "info"                        # error | warn | info | debug | trace
idle_timeout_secs = 0                     # 0 = no watchdog (workstation default)
rayon_threads = "auto"                    # "auto" | <integer>
tokio_workers = 4
blocking_workers = 16
mcp_workers = 4

[memory]
isolation = "project"                     # project | user | global
retention_days = 90
fts_engine = "tantivy"                    # tantivy | sqlite-fts5
vector_backend = "sqlite-vec"             # sqlite-vec | qdrant | in-memory
embedding_provider = "candle"             # candle | fastembed | voyage

[telemetry]
opt_in = false                            # Premium DEFAULT off; standard default ON
endpoint = ""                             # Used only if opt_in = true
export_otlp = false
export_jaeger = false
include_pii = false                       # NEVER true by default

[hooks]
claude_code = true                        # Register in ~/.claude/settings.json
project_specific = true                   # Use .touring/hooks/ overriding global
mcp_server = true                         # Register touring MCP server

[enterprise]                              # Ignored if tier < enterprise
registry_url = ""                         # Private registry
sso_provider = ""                         # okta | google | github
audit_log_path = ""                       # SIEM endpoint
license_key_file = ".touring/license.key"
```

### 2.4 Daemon discovery (walk-up + fallback)

```
1. Starting from CWD, walk up looking for `.touring/touring.toml`
2. If found:
   a. Read config, resolve `default_toolchain` to ~/.touring/toolchains/<v>
   b. Socket = <found_dir>/.touring/daemon.sock
   c. Connect; if dead, spawn `touring-daemon --config <found_dir>/.touring/touring.toml`
3. If NOT found:
   a. Check ~/.touring/config.toml (user-global default)
   b. Use socket /tmp/touring-daemon-<UID>-default.sock
   c. Spawn with default toolchain
4. Fallback: hardcoded defaults (last resort)
```

### 2.5 CLI surface (canonical)

```bash
# Lifecycle
touring init [--tier <T>] [--features <F>] [--languages <L>] [--toolchain <V>]
touring uninstall [--purge]               # purge = also delete .touring/data/
touring migrate [--from-global]           # migrate ~/.claude/touring/ → .touring/

# Toolchain management
touring update [version]                  # rustup-like
touring update --rollback                 # revert to previous toolchain
touring toolchain {list,install,remove,default}

# Component management
touring component {list,add,remove}     # add intelligence | offensive | bind-python | ...

# Inspection
touring which                             # Show binary path + config path resolved
touring config {get,set,edit}           # Manipulate touring.toml

# Daemon control
touring daemon {start,stop,restart,status,logs}

# Registry (enterprise)
touring {login,logout}
touring registry {list,sync}
```

### 2.6 External installer (install.touring.dev)

```bash
$ curl -sSf https://install.touring.dev | sh

# Steps performed:
# 1. Download installer script (signed)
# 2. Verify sigstore signature + SHA-256
# 3. Detect OS/arch (linux/macos/windows; x86_64/aarch64)
# 4. Download appropriate binary tarball + SBOM
# 5. Extract to ~/.touring/toolchains/<version>/
# 6. Create symlinks in ~/.local/bin/ (Linux/macOS) or %USERPROFILE%\.touring\bin (Windows)
# 7. Generate ~/.touring/env.sh (or env.ps1)
# 8. Write completions to ~/.bashrc / ~/.zshrc / fish_config
# 9. Print getting-started tutorial
```

Binary tarball signed with **sigstore (cosign)** + SBOM (CycloneDX) attached.
Mirrors: install.touring.dev, get.touring.dev, GitHub Releases.

### 2.7 Migration tool (`touring migrate --from-global`)

```bash
$ touring migrate --from-global --source ~/.claude/touring/ --target .

# Steps:
# 1. Detect existing .claude/touring/ symbol DB → copy to .touring/data/symbols.db
# 2. Copy relevant memory entries (filtered by project tag) to .touring/data/memory.db
# 3. Copy learning state filtered by project → .touring/data/learning.db
# 4. Generate .touring/touring.toml from inferred features
# 5. Update .gitignore to ignore .touring/ EXCEPT touring.toml
# 6. Optionally update ~/.claude/settings.json to use project-scoped hooks
```

### 2.8 Hook dispatcher (Claude Code backward compat)

`~/.claude/hooks/touring-hook` becomes a **smart dispatcher**:

```sh
#!/bin/sh
# Walk up CWD to find .touring/bin/touring-hook
DIR=$PWD
while [ "$DIR" != "/" ]; do
  if [ -x "$DIR/.touring/bin/touring-hook" ]; then
    exec "$DIR/.touring/bin/touring-hook" "$@"
  fi
  DIR=$(dirname "$DIR")
done
# Fallback to default toolchain
exec "$HOME/.touring/toolchains/$(cat $HOME/.touring/default)/bin/touring-hook" "$@"
```

Preserves Claude Code compatibility — same `settings.json` paths, but routing
follows project context.

### 2.9 Multi-daemon coexistence

- Each project has its own `.touring/daemon.sock` (not /tmp)
- PID file: `.touring/daemon.lock`
- Resource usage: ~92 MB RSS per daemon (measured RUST_LOG=info)
- 16 GB workstation: up to ~50 projects simultaneously without swap
- Auto-shutdown opt-in via `daemon.idle_timeout_secs > 0`

## 3. Rollout strategy

Deployment is **sequential with architectural refactor first** (Gabriel's
explicit constraint):

1. **W0-W11 (architectural refactor)**: complete topology to 13 crates, all gates green
2. **W12 (per-project deployment)**: implement toolchain manager + .touring/ + migration tool
3. **W13 (publishing)**: docs.rs, semver-check, sigstore, SBOM, install.touring.dev
4. **W14 (commercial tiers)**: tiered licensing + private registry + SSO + audit

Backwards compatibility during W12:
- Feature flag `--legacy-global` (default ON in 0.x, default OFF in 1.0, removed in 1.5)
- Old global installation continues to work until 1.5
- `touring migrate` automates the transition

## 4. Consequences

### Positive
- **Per-project isolation**: no cross-pollution of knowledge, memory, learning state
- **Multiple toolchains**: pin 1.0.0 on stable projects, test 2.0-beta on others
- **External adoption**: clients can install via `curl install.touring.dev | sh`
- **Enterprise on-prem**: private registry + audit logs without polluting public binaries
- **Rollback per-project**: bad toolchain upgrade → `touring update --rollback`

### Negative
- **Disk usage**: each project ~100 MB (.touring/data/) + each toolchain ~200 MB shared in ~/.touring/
- **Migration friction**: existing users must run `touring migrate`
- **Complexity**: more configuration surface to document

### Risks
- **Hook dispatcher walk-up bug** could break Claude Code integration. Mitigation: pilot on konverter + analise before broader rollout
- **License key sync** for enterprise tier requires reliable phone-home. Mitigation: 30-day grace period offline; cache last validation

## 5. References

- ADR-001: Architecture topology
- ADR-003: Commercial tiers
- MASTER-PLAN-2026: W12 detailed subtasks
- Memory: `decision:touring-premium-roadmap-2026-05-11`
