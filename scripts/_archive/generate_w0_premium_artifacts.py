#!/usr/bin/env python3
"""Generate Touring W0 Premium refactor artifacts (ADRs + Master Plan).

Renders 4 deterministic markdown files into docs/W0/:
  - ADR-001-premium-architecture.md      (13-crate target topology)
  - ADR-002-per-project-deployment.md    (rustup-like .touring/ per-project)
  - ADR-003-commercial-tiers-gtm.md      (tiers + pricing + GTM strategy)
  - MASTER-PLAN-2026.md                  (15-wave roadmap W0-W14)

The script is the single source of truth; markdown is regenerable. To update
content, edit constants below and re-run.

Examples
--------
    python3 scripts/generate_w0_premium_artifacts.py --all
    python3 scripts/generate_w0_premium_artifacts.py --only ADR-001
    python3 scripts/generate_w0_premium_artifacts.py --output-dir docs/W0

Exit codes
----------
    0 — success
    1 — runtime error
    2 — invalid arguments
"""
from __future__ import annotations

import argparse
import logging
import sys
from pathlib import Path
from typing import Sequence

LOGGER = logging.getLogger(__name__)

TODAY = "2026-05-11"
AUTHOR = "Gabriel Gadea (architect) + TACO (orchestrator)"

# =============================================================================
# ADR-001 — Premium Architecture Vision
# =============================================================================

ADR_001 = f"""# ADR-001 — Touring Premium Architecture Vision

> **Status**: Proposed | **Date**: {TODAY} | **Authors**: {AUTHOR}
> **Supersedes**: nothing (greenfield architectural redesign)
> **Relates to**: ADR-002 (Per-Project Deployment), ADR-003 (Commercial Tiers + GTM), MASTER-PLAN-2026

## 1. Context

Touring grew organically from 0 → 46 crates / ~410k LOC over 18 months. The current
workspace shows multiple severe symptoms diagnosed in the {TODAY} forensic audit
(memory: `audit:touring-arch-premium-refactor-2026-05-11`):

| Symptom | Evidence |
|---|---|
| **Macrociclo arquitetural HIGH severity** | `touring wiring cycles` reports depth=618 cycle spanning 9 crates (server↔hooks↔analysis↔cognitive↔learning↔ast↔wasm↔inferlets↔resource-monitor) |
| **Fragmentação excessiva** | 46 crates, 6 anêmicos (<1k LOC), 3 mortos (0 LOC), 1 archived spike |
| **Mega-crates concentram 69% do código** | hooks 152k, server 61k, learning 41k, cortex 32k, ast 23k |
| **Test-debt catastrófico** | cortex 0.56% ratio, 8 crates com 0 tests |
| **No semver/MSRV foundation** | 0 `[workspace.dependencies]`, 0 `version.workspace = true` |
| **Duplicação intencional documentada** | touring-ast-polyglot DOC: "Extends touring-ast" |

The decision: **transform Touring into a premium-grade product** where the architecture
itself demonstrates the quality bar the product delivers. Reduce 46 → 13 productive
crates via deliberate fusion + internal split, with modular Cargo features.

## 2. Decision

**Target topology: 13 productive crates + 2 test-only manifests, organized in 6 strict layers.**

### Layer architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│ LAYER 6 — PRODUCT  (touring-server, touring-hooks, touring-bindings)│
│   Binaries + CC interface + external API surface                    │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 5 — APPLICATION  (generator, assists, orchestration)          │
│   User-facing workflows                                             │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 4 — INTELLIGENCE  (touring-intelligence)                      │
│   Reasoning + RL + pipeline (mega-fusion to eliminate cycle)        │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 3 — DOMAIN CORE  (code, storage, analysis, offensive)         │
│   Code intelligence + storage + analysis + security                 │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 2 — KERNEL  (simd, rkyv, identity)                            │
│   Primitives without policy                                         │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 1 — FOUNDATION  (touring-foundation)                          │
│   Zero deps in touring-*; configures everything                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Crate catalog (13 productive + 2 test-only)

#### Layer 1 — Foundation

| Crate | Modules | Features | LOC src / test | MSRV |
|---|---|---|---|---|
| **touring-foundation** | alloc, cgm, char_classes, checkpoint, chunker, config, conflict, diagnostic, drift, failover, feedback, governor, hash, health, migration, plugin, profile, schema, security, shared, shutdown, telemetry, sentinel, rules, definitions, activity | tracing-otel, tracing-jaeger, gpu-embeddings, mimalloc-allocator, sentinel-psi, rules-eval | 18k / 4k (22%) | 1.83 |

Absorves: `touring-core` (rename + slim), `touring-rule-engine`, `touring-definitions`,
`touring-telemetry`, `touring-resource-monitor`, `touring-activity`.

#### Layer 2 — Kernel

| Crate | Modules | Features | LOC src / test |
|---|---|---|---|
| **touring-simd** | aco, cosine, gpu, u4_dot, quantize, bitvec, mask | gpu-cuda, gpu-vulkan, gpu-metal, simd-avx2, simd-avx512, simd-neon | 9k / 2k (22%) |
| **touring-rkyv** | transport, wire, magic, dispatch | bincode-fallback, compression-zstd | 1.5k / 600 (40%) |
| **touring-identity** | registry, schema, types, criterion, resolution | (none) | 2k / 600 (30%) |

#### Layer 3 — Domain Core

| Crate | Modules | Features | LOC src / test |
|---|---|---|---|
| **touring-code** | parsers/{{tree_sitter,ast_grep,syn}}, languages, semantics, graph, format, complexity, incremental | lang-{{rust,typescript,python,go,ruby,java,cpp}}, parser-{{tree-sitter,ast-grep,syn}}, semantic-search, incremental-salsa | 26k / 6k (23%) |
| **touring-storage** | fts (Tantivy), vec/{{sqlite,qdrant,in_memory}}, embeddings/{{candle,fastembed,voyage}}, vfs/{{mem,disk}}, salsa, hybrid_search, indexer | storage-{{fts,vec-sqlite,vec-qdrant,vec-mem,emb-candle,emb-fastembed,emb-voyage,vfs-mem,vfs-disk,salsa}} | 10k / 2.5k (25%) |
| **touring-analysis** | blast_radius, quality (TDG, Halstead, MI), wiring, health, temporal, e2e, rules, knowledge, security, cache, report, pipeline | bench-iai, cache-moka, cache-dashmap, temporal-history | 16k / 4k (25%) |
| **touring-offensive** | concolic, erickson, solver, vuln, bug_bounty | solver-z3, solver-cvc5, concolic-tracer, vuln-pattern-db | 7.5k / 2k (26%) |

Code absorves: `touring-ast`, `touring-ast-polyglot`, `touring-language`, `touring-semantics`.
Storage absorves: `touring-index`, `touring-vfs`, `touring-incremental-salsa`, `touring-vector-store`, `touring-embeddings`, `touring-search-fusion`.

#### Layer 4 — Intelligence

| Crate | Modules | Features | LOC src / test |
|---|---|---|---|
| **touring-intelligence** | reasoning (ACO, ANN, BM25, MCTS, GoT, Pensieve), rl (bandit, ACO, clustering, online_rl, ranking), pipeline (handler, fusion, scoring, fascicles, cross_audit, DSPy), ann | intel-{{reasoning,rl,pipeline,mcts,bandit,aco,ann,clustering,pensieve,got,dspy}} | 90k / 18k (20%) |

Absorves: `touring-cognitive`, `touring-cortex`, `touring-learning`, `touring-antt`.
**This fusion eliminates the depth-618 macrociclo** by collapsing the cyclical
reasoning↔learning↔pipeline dependencies into a single crate with internal pub(crate)
discipline.

#### Layer 5 — Application

| Crate | Modules | Features | LOC src / test |
|---|---|---|---|
| **touring-generator** | pipeline (Draft→Verified→Rendered→Speculated→Committed), kinds (36), vgp, render, speculate | generator-{{rust,python,typescript,tsx}}, vgp-strict | 13k / 5k (38%) |
| **touring-assists** | 10 handlers (auto_wire, extract_function, inline_call, auto_import, generate_impl, merge_imports, change_visibility, add_missing_match_arms, move_module_to_file, convert_to_guarded_return) | assist-{{rust,typescript,python}} | 2.5k / 700 (28%) |
| **touring-orchestration** | flow, tasks, decompose, session, diary, devrc | flow-dag, tasks-sqlite, decompose-mcts, session-persist | 3.5k / 900 (25%) |

Orchestration absorves: `touring-flow`, `touring-tasksfile`, `touring-devrc-adapter`.

#### Layer 6 — Product (each with internal sub-crates for modularity)

| Crate | Internal sub-crates | Features | LOC src / test |
|---|---|---|---|
| **touring-server** | server-cli, server-tools, server-reasoning, server-session, server-telemetry, server-visual | tier-{{free,standard,premium,enterprise}} | 25k / 6k (24%) |
| **touring-hooks** | hooks-core, hooks-lifecycle, hooks-cli, hooks-tools, hooks-prediction, hooks-rl | hooks-{{claude-code,mcp,prediction,rl,cortex}} | 155k / 32k (20%) |
| **touring-bindings** | bindings-{{python, wasm, capnp, web, desktop, postgis}} | bind-{{python,wasm,capnp,web,desktop,postgis}} (default = empty) | 15k / 3.5k (23%) |

Bindings absorves: `touring-python`, `touring-wasm`, `touring-capnp-server`,
`touring-web`, `touring-web-server`, `touring-desktop-ui`, `touring-geopostgis`.
**Deletes 3 dead crates**: `touring-wasm-{{client,common,server}}` (0 LOC each).

#### Test-only (preserved)

- `touring-loom-proofs` (concurrency proofs, isolated workspace)
- `touring-integration-tests` (cross-crate E2E)

### Crates removed (5 immediate dead-code purge)

1. `touring-semantic-spike` (66 LOC, 0 pub, archived per ARCHITECTURE.md)
2. `touring-wasm-client` (0 LOC)
3. `touring-wasm-common` (0 LOC)
4. `touring-wasm-server` (0 LOC)

**Net manifest reduction: 46 → 15 = -67%.**

## 3. Quality Gates (non-negotiable per crate)

Every crate in the new topology MUST meet:

| Gate | Threshold | Verification |
|---|---|---|
| **Test ratio** | tests LOC / src LOC ≥ 20% | `cargo llvm-cov` per crate |
| **Mutation kill rate** | ≥ 80% | `cargo mutants` per crate |
| **Documentation** | `#![warn(missing_docs)]` strict | `cargo doc --warnings-as-errors` |
| **API stability** | snapshot via `cargo public-api` | CI gate per PR |
| **SemVer** | `cargo-semver-checks` | CI gate before merge |
| **MSRV** | 1.83 LTS | `cargo-msrv verify` |
| **Lints** | `[workspace.lints]` strict, deny warnings | `cargo clippy -- -D warnings` |
| **Supply chain** | clean | `cargo deny check` + `cargo audit` + `cargo vet` |
| **Performance** | Criterion baseline preserved (-5% budget) | `cargo bench` regression CI |
| **No unsafe without justification** | `// SAFETY:` comment + audit | grep gate |
| **No `unwrap()` in src/** | use `?` / `.expect()` / `.unwrap_or_default()` | clippy lint enforced |

## 4. Consequences

### Positive
- **Architecture lisible**: 6 strict layers; any new contributor understands the topology in 1 hour
- **Zero cycles**: `touring wiring cycles --min-depth 2` returns 0 after refactor
- **Builds faster cold**: fewer manifests, less Cargo work; estimated 30% faster on dev machine
- **Features composable**: users opt in to exactly what they need (`tier-free` is ~30% of binary size of `tier-enterprise`)
- **Onboarding faster**: 15 manifests vs 46; new hires productive in days not weeks
- **Marketing-ready**: clean topology becomes a sellable narrative

### Negative
- **Massive refactor effort**: ~138-182 engineer-days (see MASTER-PLAN-2026 for breakdown)
- **API churn for downstream**: every consumer of old crates must update imports (touring-ast → touring-code::ast, etc.)
- **Test debt repayment** required before fusion (W6.0 cortex 0.56% → 15% precondition)
- **W6 mega-fusion (90k LOC)** is single largest risk; mitigated by W6.0 pre-test gate

### Risks
- **Build time of 90k-LOC touring-intelligence** may degrade dev iteration. Mitigation: profile.dev `incremental=false` + sccache + split-debuginfo (already in REGRA #12)
- **Reexport shims** during W4 transition may persist beyond intended sunset. Mitigation: feature-flagged deprecations with clear sunset date in CHANGELOG
- **Hook split (W8)** may introduce new internal cycles between sub-crates. Mitigation: `cargo-depgraph` CI gate validates acyclic

## 5. References

- Forensic audit: memory `audit:touring-arch-premium-refactor-2026-05-11`
- Approved decisions: memory `decision:touring-premium-roadmap-2026-05-11`
- Baselines: `docs/baselines/` (wiring, cycles, status, workspace-info, snapshot)
- Companion ADRs: ADR-002 (deployment), ADR-003 (commercial)
- Execution plan: `docs/W0/MASTER-PLAN-2026.md`
"""

# =============================================================================
# ADR-002 — Per-Project Deployment Model
# =============================================================================

ADR_002 = f"""# ADR-002 — Touring Per-Project Deployment Model

> **Status**: Proposed | **Date**: {TODAY} | **Authors**: {AUTHOR}
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
│   │   ├── bin/{{touring,touring-hook,touring-daemon,touring-update}}
│   │   ├── lib/                       # Shared rustls, etc.
│   │   ├── share/
│   │   │   ├── man/man1/touring.1
│   │   │   ├── completions/{{bash,zsh,fish}}/
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
overrides = {{ go = {{ tier = 1 }} }}     # Promote Go to tier 1 in this project

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
touring toolchain {{list,install,remove,default}}

# Component management
touring component {{list,add,remove}}     # add intelligence | offensive | bind-python | ...

# Inspection
touring which                             # Show binary path + config path resolved
touring config {{get,set,edit}}           # Manipulate touring.toml

# Daemon control
touring daemon {{start,stop,restart,status,logs}}

# Registry (enterprise)
touring {{login,logout}}
touring registry {{list,sync}}
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
# 6. Create symlinks in ~/.local/bin/ (Linux/macOS) or %USERPROFILE%\\.touring\\bin (Windows)
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
"""

# =============================================================================
# ADR-003 — Commercial Tiers + GTM Strategy
# =============================================================================

ADR_003 = f"""# ADR-003 — Touring Commercial Tiers + Go-To-Market Strategy

> **Status**: Proposed | **Date**: {TODAY} | **Authors**: {AUTHOR}
> **Relates to**: ADR-001 (Architecture), ADR-002 (Deployment), MASTER-PLAN-2026 (W14)
> **Approved by Gabriel**: Tiers integrated into roadmap as W14

## 1. Context

Touring is being transformed from internal tool into a **premium commercial product**.
The architecture itself (ADR-001) demonstrates the quality bar. Now we define:

- **What is sold** (tiers + features)
- **How it is priced** (per-developer, per-team, enterprise)
- **Who is targeted** (segments)
- **How they discover and buy** (channels, sales motion)
- **What success looks like** (KPIs, financial forecast)

## 2. Decision

### 2.1 Four-tier model

| Tier | Target | Telemetry | Support | License |
|---|---|---|---|---|
| **Free** | Solo, students, OSS contributors | ON (metrics only, no PII) | Community (GitHub issues) | MIT OR Apache-2.0 |
| **Standard** | Active OSS maintainers (registered) | ON (metrics, opt-out) | GitHub + Discord community | MIT OR Apache-2.0 |
| **Premium** | Senior solo devs, small teams | **OFF by default** | 24h SLA email + private Discord | Commercial |
| **Enterprise** | Regulated industries, 200+ devs | OFF + audit logs to SIEM | 4h SLA + office hours + dedicated CS | Commercial + custom MSA |

### 2.2 Feature matrix by subsystem

| Subsystem | Free | Standard | Premium | Enterprise |
|---|---|---|---|---|
| Languages (touring-code) | rust+py | + ts+go | + java/cpp/swift | full polyglot |
| Storage backends | sqlite + tantivy | + candle emb | + qdrant + voyage | + on-prem registry |
| Analysis quality | basic blast | + TDG + Halstead | + cross-feature + temporal | + custom rules engine |
| Offensive security | ✗ | ✗ | concolic + solver | + bug-bounty + private vuln-db |
| Intelligence | basic reasoning | + RL + bandit | + MCTS + GoT + Pensieve | + DSPy + custom strategies |
| Generator kinds | 8 | 24 | 36 | + custom templates |
| Assists | 3 | 7 | 10 | + custom assist plugins |
| Orchestration | basic DAG | + decompose MCTS | + session persistence | + multi-user sync |
| Hooks | CC integration | + RL hooks | + prediction (L7-B) | + custom hook handlers |
| Bindings | python | + wasm | + web + capnp | + desktop + postgis + custom |
| SSO/Audit/Registry/On-prem | — | — | — | ✓ |

### 2.3 Cargo features → tier mapping

```toml
[features]
default = ["tier-free"]

tier-free = [
  "touring-foundation/full",
  "touring-code/lang-rust", "touring-code/lang-python",
  "touring-analysis/blast-basic",
  "touring-hooks/claude-code",
  "touring-storage/storage-vec-sqlite", "touring-storage/storage-fts",
]

tier-standard = [
  "tier-free",
  "touring-code/lang-typescript", "touring-code/lang-go",
  "touring-code/parser-ast-grep",
  "touring-analysis/quality-tdg", "touring-analysis/quality-halstead",
  "touring-generator/generator-rust", "touring-generator/generator-python",
  "touring-generator/generator-typescript",
  "touring-assists/assist-rust", "touring-assists/assist-typescript",
  "touring-intelligence/intel-rl", "touring-intelligence/intel-bandit",
  "touring-storage/storage-emb-candle",
  "touring-hooks/hooks-rl",
  "touring-orchestration/decompose-mcts",
]

tier-premium = [
  "tier-standard",
  "touring-code/lang-java", "touring-code/lang-cpp", "touring-code/parser-syn",
  "touring-analysis/quality-mi", "touring-analysis/temporal-history",
  "touring-analysis/cross-feature",
  "touring-offensive/concolic-tracer", "touring-offensive/solver-z3",
  "touring-intelligence/intel-mcts", "touring-intelligence/intel-got",
  "touring-intelligence/intel-pensieve",
  "touring-storage/storage-vec-qdrant", "touring-storage/storage-emb-voyage",
  "touring-generator/generator-tsx", "touring-generator/vgp-strict",
  "touring-hooks/hooks-prediction", "touring-hooks/hooks-cortex",
  "touring-bindings/bind-wasm", "touring-bindings/bind-web",
  "telemetry-off-default",
]

tier-enterprise = [
  "tier-premium",
  "enterprise-sso", "enterprise-audit", "enterprise-registry",
  "enterprise-custom-rules", "enterprise-custom-templates",
  "enterprise-mcp-plugins", "enterprise-onprem",
  "touring-bindings/bind-desktop", "touring-bindings/bind-postgis",
  "touring-bindings/bind-capnp",
  "touring-intelligence/intel-dspy", "touring-intelligence/intel-clustering",
  "touring-offensive/solver-cvc5", "touring-offensive/vuln-pattern-db",
]
```

### 2.4 License key system (JWT ed25519)

License key location: `~/.touring/license.key` (user-scoped) or
`<project>/.touring/license.key` (project override).

Format: JWT signed with **ed25519**; public key embedded in binary for offline
verification.

```json
{{
  "sub": "user@company.com",
  "iss": "license.touring.dev",
  "iat": 1736000000,
  "exp": 1767536000,
  "tier": "premium",
  "features": ["intel-dspy", "bind-postgis"],
  "max_projects": 10,
  "trial": false
}}
```

Verification flow:
- Binary validates JWT on startup → enables tier
- No key → graceful tier-free
- Expired → fallback to tier-free + warning + 30-day grace
- Corrupted → clear error + support link

### 2.5 Pricing matrix

| Plan | Annual price | Monthly price | Billing |
|---|---|---|---|
| Free | $0 | $0 | — |
| Standard | $0 (registered) | $0 | — |
| **Premium Individual** | **$348/yr** ($29/mo) | $39/mo | Stripe self-service |
| **Premium Team (5-30 seats)** | $288/seat/yr ($24/seat/mo) | $32/seat/mo | Stripe |
| **Business (30-200 seats)** | $228/seat/yr ($19/seat/mo) | $26/seat/mo | Stripe + invoice |
| **Enterprise (200+)** | Custom ($60-120k base + $35-50/seat/mo) | Custom | MSA |
| **Enterprise On-Prem** | Custom ($150-300k/yr + setup) | Custom | MSA |
| **OEM/Embedded** | Custom rev-share or flat license | — | Partnership |

#### Discount policy

| Case | Discount |
|---|---|
| Annual vs monthly | -25% |
| 5+ seats team rate | -17% baseline |
| OSS projects (verified) | -50% |
| Education (.edu verified) | Free Premium (up to 5 seats) |
| Non-profit (501c3 verified) | -50% |
| Volume 50 seats | -20% |
| Volume 100 seats | -25% |
| Volume 500 seats | -30% |

#### Example enterprise quote (bank, 300 devs, on-prem)

| Component | $ |
|---|---|
| Base platform license | $60,000 |
| 300 seats × $400 | $120,000 |
| Private registry hosting | $12,000 |
| SSO setup (one-time) | $5,000 |
| Audit log SIEM integration | $8,000 |
| On-prem add-on | $90,000 |
| Dedicated CS 40h | included |
| **Annual ARR** | **$290,000** + $5k one-time |

## 3. Competitive landscape

| Product | Focus | Price | Overlap |
|---|---|---|---|
| GitHub Copilot | LLM autocomplete + chat | $10/$19/$39 | LIMITED |
| Cursor | IDE-fork VSCode w/ AI | $20/$40 | LIMITED |
| **Sourcegraph Cody** | Code search + AI ent | Free/$9 Pro/Custom | **HIGH** |
| **Continue.dev** | OSS AI assistant | Free/Ent custom | **HIGH** |
| Aider | CLI git-native AI pair | OSS | MEDIUM |
| Codeium/Windsurf | Autocomplete + agent | Free/$15/Custom | LIMITED |
| Tabnine | Autocomplete enterprise | $12/$39 | LIMITED |
| JetBrains AI | Plugin IDE | $10 add-on | LIMITED |
| rust-analyzer + clippy | LSP + linter Rust | OSS | NICHE |
| ast-grep | AST search/rewrite polyglot | OSS | NICHE |
| **Semgrep** | Static analysis polyglot | Free/$40/Ent | **HIGH** |
| Snyk Code | Security DAST/SAST | Enterprise | LIMITED |

### Touring positioning

> **"Premium AI-native code intelligence platform — deep code understanding + agentic execution + persistent learning."**

Differentiation (7 moats):

1. **Not "AI assistant"** primarily — code intelligence platform with AI agentic capabilities
2. **Don't sell inference** — BYOK (Anthropic, OpenAI, Voyage, local Ollama)
3. **CLI-native, daemon-based, IDE-agnostic** — works with CC, Cursor, Cline, Aider, any MCP client
4. **Deep Rust (syn) + polyglot (tree-sitter + ast-grep)** — uniquely both
5. **Offensive security included** (concolic, erickson, vuln-db) — first in category
6. **Memory + RL persistent** across sessions and projects
7. **OSS-first with premium tiers** — hybrid (Sourcegraph + Continue model)

## 4. Sales motion + distribution

### 4.1 Three-tier motion

| Motion | Target | Cycle | Team | Channel |
|---|---|---|---|---|
| **PLG self-service** | Free → Premium → Business | 30-90 days | None | GitHub, content, community |
| **SLG inside sales** | Business 30-200 devs | 60-90 days | 1 SDR/AE | PQL inbound, demos, ROI |
| **SLG enterprise** | 200+ devs | 6-12 months | AE + SE + CS | RFP, ABM, executive briefings |

### 4.2 Acquisition channels (ranked by LTV/CAC)

| Channel | Cost | LTV/CAC est. | Volume |
|---|---|---|---|
| GitHub organic (stars + README) | $0 | ∞ | Limited |
| Hacker News / Lobste.rs | $0 | 50× | Bursty |
| Reddit (r/rust, r/programming) | $0 | 40× | Risk anti-promo |
| Podcast appearances | $0-500/ep | 30× | High-quality slow |
| Twitter/X dev community | $200/mo content | 25× | Steady |
| YouTube tutorials | $1k/video | 15× | Slow compound |
| Dev.to / Medium | $500/mo writer | 12× | SEO compound |
| Sourcegraph integration | strategic | high | Cross-pollination |
| Conference sponsorships | $5-25k/event | 5× | Niche (RustConf, RustNation) |
| Paid Google ads | $3-8/click | 3-5× | Volume, lower quality |

### 4.3 Distribution channels

```
curl install.touring.dev | sh        70%  PLG primary
brew install touring                 15%  macOS power users
docker pull touring/touring           3%  CI/CD
apt install touring                   5%  Debian/Ubuntu PPA
rpm install touring                   2%  RHEL/Fedora
scoop install touring                 3%  Windows
nix flake                             1%  NixOS devs
enterprise on-prem installer        custom  Enterprise SLG
```

### 4.4 Partner ecosystem

| Partner type | Examples | Touring offering |
|---|---|---|
| IDE integrations | VSCode, Cursor, Cline, Aider, JetBrains | Free MCP integration |
| LLM providers | Anthropic, OpenAI, Voyage, Cohere | BYOK; no markup |
| Cloud marketplaces | AWS, Azure, GCP | Listed, 5% rev share |
| Consultancies | Dev shops, agencies | 15% referral first year |
| Training partners | Educative, Frontend Masters | Co-marketing, custom curricula |
| OSS projects | Tokio, Rust Foundation, Bevy, Linkerd | Free Enterprise tier + brand |
| Security firms | Trail of Bits, Sourcegraph | Co-marketing on offensive |

### 4.5 Telemetry tier matrix

| Metric | Free | Standard | Premium | Enterprise |
|---|---|---|---|---|
| Command frequency | ON | ON | OFF | OFF |
| Error rate aggregate | ON | ON | OFF | OFF |
| Latency P50/P99 | ON | ON | OFF | OFF |
| Symbol counts | OFF | ON opt-out | OFF | OFF |
| Daemon uptime | OFF | ON opt-out | OFF | OFF |
| User identifier | ❌ NEVER | ❌ NEVER | ❌ NEVER | hash(email) only |
| Code content | ❌ NEVER | ❌ NEVER | ❌ NEVER | ❌ NEVER |
| Audit log entries | — | — | — | ✓ self-hosted SIEM |

## 5. Success metrics + financial forecast

### KPIs by horizon

| KPI | T0 | M3 | M6 | M12 | M24 |
|---|---|---|---|---|---|
| GitHub stars | 1k | 5k | 15k | 35k | 80k |
| DAU active installs | 100 | 1k | 5k | 20k | 60k |
| Free→Premium conversion | — | 0.5% | 1% | 1.5% | 2% |
| Premium MRR | $0 | $5k | $25k | $120k | $400k |
| Enterprise ARR | $0 | $0 | $300k | $1.5M | $5M |
| **Total ARR** | $0 | $60k | $600k | **$2.9M** | **$9.8M** |
| Premium subs | 0 | 150 | 700 | 3,000 | 9,000 |
| Enterprise accounts | 0 | 0 | 1 | 5 | 17 |
| Monthly churn | — | 8% | 5% | 3% | 2% |
| NPS | — | 30 | 45 | 55 | 60+ |
| Test coverage workspace | 20% | 25% | 30% | 40% | 50% |
| External contributors | 1 | 10 | 50 | 200 | 500 |
| Conference talks | 0 | 2 | 5 | 12 | 25 |

### Financial forecast (5 years)

| Year | Revenue | Costs | Net | Headcount | Notes |
|---|---|---|---|---|---|
| Y1 (2027) | $1.2M | $1.8M | -$600k | 6 | Bootstrap; founders + 4 |
| Y2 (2028) | $5.8M | $5.0M | +$800k | 18 | Profitable; Series A optional |
| Y3 (2029) | $15M | $12M | +$3M | 40 | Series A confirmed |
| Y4 (2030) | $35M | $26M | +$9M | 80 | Series B optional |
| Y5 (2031) | $70M | $50M | +$20M | 130 | IPO-ready or M&A |

### Unit economics

| Metric | Value |
|---|---|
| Cost per premium user/month | ~$11 (infra $0.30 + license $0.05 + support $1.50 + sales $4 + marketing CAC $5) |
| Revenue per premium | $29/mo |
| **Gross margin** | **~62%** |
| LTV premium individual | $740 (3yr × 85% retention) |
| LTV premium team | $5,300/seat (4yr × 92%) |
| LTV enterprise | $1.38M/account (5yr × 95%) |
| **LTV/CAC premium** | **9.3×** (bench 3×) |
| **LTV/CAC enterprise** | **34.5×** (bench 3-5×) |

### SLA matrix

| Severity | Free | Standard | Premium | Enterprise |
|---|---|---|---|---|
| Critical (prod down) | community | community | 24h | **4h** |
| High (regression) | community | community | 48h | 8h |
| Medium (bug) | community | community | 5d | 1d |
| Low (enhancement) | community | community | 2 weeks | 1 week |
| Office hours | — | — | — | Weekly 1h |
| Dedicated CS | — | — | — | ✓ |

## 6. OKRs Year 1

**O1**: Establish Touring as the premium code intelligence platform for serious Rust + polyglot developers.
- KR1: 15k GitHub stars by M12
- KR2: 700 premium subs by M12 ($25k MRR)
- KR3: 5 enterprise pilots by M9
- KR4: NPS ≥ 45 by M9

**O2**: Build a world-class engineering foundation that itself demonstrates Touring quality.
- KR1: Test coverage ≥ 30% workspace-wide by M12
- KR2: Zero cycles workspace by M6
- KR3: docs.rs 100% green
- KR4: cargo-deny / audit / vet clean continuously

**O3**: Create defensible moats via community + ecosystem.
- KR1: 50 external contributors by M9
- KR2: 10 partner integrations live
- KR3: 5 published conference talks
- KR4: 20 customer case studies

## 7. Strategic risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Copilot adds code intelligence | High | Medium | Polyglot + agentic; Copilot vendor-locked |
| Cursor pivots to platform | Medium | High | IDE-agnostic; we sell the infra they'd use |
| Sourcegraph adds AI agent | High | High | Deeper rust+offensive; Cody is search-first |
| LLM commoditization | High | Low | We don't compete with autocomplete; we sell infra |
| Hostile OSS fork | Medium | Medium | License chooser: foundation MIT, premium Commercial |
| Talent hiring | High | High | Remote-first, OSS pipeline, equity-heavy comp |
| Compliance failure | Low | Catastrophic | SOC2 Year 1 priority; legal counsel from start |
| BR-hostile capital market | Medium | Medium | Delaware-flip if raising US VC |

## 8. References

- ADR-001: Architecture topology
- ADR-002: Per-project deployment
- MASTER-PLAN-2026: W14 detailed subtasks
- Memory: `decision:touring-premium-roadmap-2026-05-11`
"""

# =============================================================================
# MASTER-PLAN-2026 — 15-wave roadmap
# =============================================================================

MASTER_PLAN = f"""# MASTER-PLAN-2026 — Touring Premium Refactor

> **Status**: Proposed | **Date**: {TODAY} | **Authors**: {AUTHOR}
> **Total effort**: 138-182 engineer-days (~6-9 months sustained for 1 senior engineer)
> **References**: ADR-001 (architecture), ADR-002 (deployment), ADR-003 (commercial)

## Executive summary

15 waves (W0-W14) transform Touring from 46-crate fragmented workspace into a
13-productive-crate premium product with rustup-style deployment + 4 commercial
tiers. Critical path is **W0 → W1 → W2 → W3 → W4 → W6 → W8 → W12 → W13 → W14**
(~95-126 days). Parallelism opportunities reduce to ~120-158 days with 2 engineers.

## Wave sequencing

```
F1 PREP (W0-W2)               12-16 days
F2 FUSIONS (W4, W5, W6, W7)   45-57 days
F3 STABILIZATION (W3, W8-W10) 38-49 days
F4 QUALITY (W11)              10-15 days
F5 DEPLOYMENT (W12)           15-20 days
F6 PUBLISHING (W13)            8-10 days
F7 PRODUCT (W14)              10-15 days
─────────────────────────────────────────
TOTAL                        138-182 days
```

```
W0 → W1 → W2 → W3 → W4 → W6 → W8 → W12 → W13 → W14
                ↓         ↓         ↓
              W5||W7    W9||W10   W11 (||W12)
```

## W0 — Prep & Safety Net (5-7 days · zero edits)

| Subtask | Action | Days |
|---|---|---|
| W0.1 | Snapshot tar pre-refactor + SHA-256 | 0.5 |
| W0.2 | Bench baseline `cargo bench --workspace --save-baseline pre-refactor` | 1 |
| W0.3 | CI baseline (cargo check / test --no-run + timing logs) | 0.5 |
| W0.4 | Coverage baseline `cargo llvm-cov --workspace --json` | 1 |
| W0.5 | Wiring/cycle snapshot (`touring wiring audit -j` + `cycles --format json`) | 0.5 |
| W0.6 | ADR-001 Premium Architecture Vision | 1 |
| W0.7 | ADR-002 Per-Project Deployment Model | 1 |
| W0.8 | ADR-003 Commercial Tiers + GTM Strategy | 0.5 |
| W0.9 | MASTER-PLAN-2026 (this document) | 1 |

**Gate W0→W1**: ADRs approved + baselines committed to docs/baselines/.

## W1 — Dead Code Purge (3-4 days)

| Subtask | Action | Days |
|---|---|---|
| W1.1 | DELETE `touring-semantic-spike` (66 LOC archived, 0 pub) | 0.5 |
| W1.2 | DELETE `touring-wasm-{{client,common,server}}` (0 LOC each) | 0.5 |
| W1.3 | Audit + remove dead `pub use` re-exports | 1 |
| W1.4 | `cargo check --workspace` + `test --no-run` pass | 0.5 |
| W1.5 | Fix Cycle #1 (intra-server `file_tools.rs ↔ project_tools.rs`) | 1 |
| W1.6 | `touring wiring cycles` → Cycle #1 GONE | 0.5 |

**Gate**: -4 dead crates, -1 cycle, all checks green.

## W2 — Tooling Foundation (4-5 days)

| Subtask | Action | Days |
|---|---|---|
| W2.1 | `[workspace.dependencies]` centralization (~60 external deps) | 1.5 |
| W2.2 | `[workspace.package]` shared metadata (license, edition, MSRV 1.83) | 0.5 |
| W2.3 | Update 42 Cargo.toml: `<dep>.workspace = true` everywhere | 1.5 |
| W2.4 | `[workspace.lints]` strict (deny warnings + pedantic + nursery) | 0.5 |
| W2.5 | cargo-deny config (bans, advisories, sources, licenses) | 0.5 |
| W2.6 | cargo-machete CI gate (0 unused deps) | 0.5 |
| W2.7 | cargo-mutants per-crate config (50% initial, 80% by W11) | 0.5 |
| W2.8 | CI workflow: deny + machete + mutants smoke + msrv verify | 1 |

**Gate**: 1 source of truth for external deps; `cargo deny check` + `machete` clean.

## W3 — Layer 1+2 Stabilization (8-10 days)

| Subtask | Action | Days |
|---|---|---|
| W3.1 | Rename `touring-core` → `touring-foundation` (+ re-export shim) | 1 |
| W3.2 | Slim foundation: extract `embedding/` → touring-storage (prep W5) | 1 |
| W3.3 | Extract `mvkl/` → foundation submodule | 0.5 |
| W3.4 | Absorve `touring-rule-engine` (443L) → foundation/rules/ | 0.5 |
| W3.5 | Absorve `touring-definitions` (1.1k) → foundation/types/ | 0.5 |
| W3.6 | Absorve `touring-telemetry` (990L) → foundation/telemetry/ | 0.5 |
| W3.7 | Absorve `touring-resource-monitor` (2.4k) → foundation/sentinel/ | 1 |
| W3.8 | Absorve `touring-activity` (781L) → foundation/activity/ | 0.5 |
| W3.9 | Foundation tests reach ≥ 25% LOC ratio | 2 |
| W3.10 | Identity tests reach ≥ 30% ratio | 0.5 |
| W3.11 | Cycle re-check; macrociclo reduction expected | 0.5 |

**Gate**: foundation slim ≤ 18k LOC, identity OK, 6 crates absorbed.

## W4 — touring-code Fusion (12-15 days) [LARGE]

| Subtask | Action | Days |
|---|---|---|
| W4.1 | Create `crates/touring-code/` skeleton + Cargo.toml | 0.5 |
| W4.2 | Move `touring-ast/src/*` → `touring-code/src/parsers/tree_sitter/` + ast deep | 2 |
| W4.3 | Move `touring-ast-polyglot/src/*` → `touring-code/src/parsers/ast_grep/` | 1 |
| W4.4 | Move `touring-language/src/*` → `touring-code/src/languages/` | 0.5 |
| W4.5 | Move `touring-semantics/src/*` → `touring-code/src/semantics/` | 0.5 |
| W4.6 | Define features: `lang-{{rust,typescript,python,go,ruby,java,cpp}}` + `parser-{{tree-sitter,ast-grep,syn}}` | 0.5 |
| W4.7 | Update 25 consumer crates: `touring_ast::X` → `touring_code::ast::X` | 3 |
| W4.8 | Update 8 consumers: `touring_ast_polyglot::X` → `touring_code::polyglot::X` | 1 |
| W4.9 | Update 3 consumers: `touring_language::X` → `touring_code::languages::X` | 0.5 |
| W4.10 | Update 2 consumers: `touring_semantics::X` → `touring_code::semantics::X` | 0.5 |
| W4.11 | Bench parsing: assert < 5% regression vs baseline | 1 |
| W4.12 | Tests pass + cycle re-check | 1 |
| W4.13 | Delete old crates (ast, ast-polyglot, language, semantics) | 0.5 |
| W4.14 | Update workspace Cargo.toml members | 0.2 |

**Gate**: touring-code 26k LOC, 6 lang features, 3 parser features, ≥ 23% test ratio, perf < 5% regression.

## W5 — touring-storage Fusion (10-12 days, ‖ W7)

6 crates → 1: index, vfs, salsa, vector-store, embeddings, search-fusion.

| Subtask | Action | Days |
|---|---|---|
| W5.1-W5.6 | Move 6 crates into touring-storage submodules | 4 |
| W5.7 | Features: storage-{{fts, vec-sqlite, vec-qdrant, vec-mem, emb-candle, emb-fastembed, emb-voyage, vfs-mem, vfs-disk, salsa}} | 1 |
| W5.8 | Update 15 consumers | 3 |
| W5.9 | Add +500 LOC tests for 0%-ratio crates (search-fusion, salsa) | 2 |
| W5.10 | Bench query latency < 5% regression | 1 |
| W5.11 | Delete old crates + workspace update | 1 |

**Gate**: touring-storage 10k LOC, 11 features, 25% test ratio.

## W6 — touring-intelligence Fusion (15-20 days) [LARGEST RISK]

cognitive + cortex + learning + antt → touring-intelligence.

| Subtask | Action | Days |
|---|---|---|
| **W6.0** | **PRE-TEST DEBT REPAYMENT**: cortex 0.56% → 15% ratio (BLOCKER for W6.1+) | **5** |
| W6.1 | Create skeleton + Cargo.toml | 0.5 |
| W6.2 | Move touring-cognitive → src/reasoning/ | 2 |
| W6.3 | Move touring-learning → src/rl/ | 2 |
| W6.4 | Move touring-cortex → src/pipeline/ | 2 |
| W6.5 | Move touring-antt → src/ann/ | 1 |
| W6.6 | Features: 11 intel-* | 1 |
| W6.7 | Update 12 consumers | 3 |
| W6.8 | Bench MCTS rollout / ANN query / bandit P99 — < 5% regression | 2 |
| W6.9 | Tests pass; cycle re-check | 1 |
| W6.10 | Delete old crates + workspace update | 0.5 |

**Gate**: touring-intelligence 90k LOC, 11 features, ≥ 20% test ratio, **macrociclo of 618 ELIMINATED**.

## W7 — touring-bindings Fusion (8-10 days, ‖ W5)

8 crates → 1: python, wasm, capnp-server, web, web-server, desktop-ui, geopostgis (+ 3 dead wasm crates DELETED).

| Subtask | Action | Days |
|---|---|---|
| W7.1 | Create skeleton + Cargo.toml with features 100% opt-in (default = empty) | 0.5 |
| W7.2-W7.7 | Move 6 bindings into submodules | 5 |
| W7.8 | Features bind-* mutually compatible | 1 |
| W7.9 | Add +1k LOC tests for 0%-ratio (web, python, desktop, postgis) | 2 |
| W7.10 | `cargo check` per feature combination | 1 |
| W7.11 | Delete old crates + workspace update | 0.5 |

**Gate**: touring-bindings 15k LOC, 6 features opt-in, 23% test ratio.

## W8 — touring-hooks Internal Split (15-20 days) [CRITICAL]

Internal split into 6 sub-crates; external façade preserved.

| Subtask | Action | Days |
|---|---|---|
| W8.1 | Create 6 internal sub-crates (workspace members) | 1 |
| W8.2 | Move hooks/core/* → touring-hooks-core (handler trait, runtime, context) | 2 |
| W8.3 | Move lifecycle/* → touring-hooks-lifecycle | 2 |
| W8.4 | Move cli_handlers/* → touring-hooks-cli (70+ files split by subdomain) | 4 |
| W8.5 | Move tools/* → touring-hooks-tools (MCP wiring) | 2 |
| W8.6 | Move layer7_prediction → touring-hooks-prediction | 1 |
| W8.7 | Move rl-related → touring-hooks-rl | 1 |
| W8.8 | Façade touring-hooks re-exports everything | 0.5 |
| W8.9 | Tests reorganize per sub-crate | 1.5 |
| W8.10 | Bench hook hot-path (pre-edit, post-edit) | 1 |
| W8.11 | Cycle re-check — expect ZERO cycles | 0.5 |
| W8.12 | Validation: TACO full wave run (24 hook events) | 1.5 |

**Gate**: touring-hooks split into 6 internal sub-crates, 0 cycles workspace-wide, hooks performance < 5ms P99 pre-edit.

## W9 — touring-server Internal Split (10-12 days, ‖ W10)

| Subtask | Action | Days |
|---|---|---|
| W9.1-W9.6 | Split into 6 sub-crates (cli, tools, reasoning, session, telemetry, visual) | 6 |
| W9.7 | Façade touring-server keeps binary | 0.5 |
| W9.8 | Tests reorganize | 1.5 |
| W9.9 | Bench CLI dispatch latency | 1 |
| W9.10 | Validation: 82 CLI commands smoke test | 1 |

**Gate**: server reduced to 25k LOC façade, 6 internal sub-crates.

## W10 — touring-orchestration Fusion (5-7 days, ‖ W9)

flow + tasksfile + devrc-adapter + decompose extracts + session + diary.

| Subtask | Action | Days |
|---|---|---|
| W10.1-W10.4 | Move flow + tasksfile + devrc-adapter | 2 |
| W10.5 | Extract decompose from touring-server | 1 |
| W10.6 | + session + diary | 1 |
| W10.7 | Features and tests | 1.5 |
| W10.8 | Update consumers + delete old | 0.5 |

## W11 — Test Debt Repayment (10-15 days, possibly ‖ W12)

| Target | Current | Goal | Days |
|---|---|---|---|
| touring-intelligence (cortex inherited) | 15% (after W6.0) | 20% | 3 |
| touring-bindings (web/python/desktop) | 8% (after W7) | 18% | 3 |
| touring-foundation (sentinel/telemetry) | 15% (after W3) | 22% | 2 |
| **Mutation kill rate** workspace-wide | ~50% | **≥ 80%** | 3 |
| Proptest for key types (Identity, Plan, Definition) | 0 | 50 properties | 1.5 |
| Fuzz targets (parsers, serializers) | 0 | 8 targets | 2.5 |

**Gate W11**: NO crate < 20% test ratio. Mutation kill rate ≥ 80%. Proptest + fuzz in CI.

## W12 — Per-Project Deployment (15-20 days) [LARGE]

| Subtask | Action | Days |
|---|---|---|
| W12.1 | Implement `touring init` CLI | 2 |
| W12.2 | Implement `~/.touring/` toolchain manager | 3 |
| W12.3 | Implement `touring update/toolchain/component` | 2 |
| W12.4 | Implement layered config loader (project ← user ← system) | 1 |
| W12.5 | Daemon multi-instance: per-project socket | 2 |
| W12.6 | Hook dispatcher (CWD walk-up shim) | 1 |
| W12.7 | Implement `touring migrate --from-global` | 2 |
| W12.8 | External installer script (install.touring.dev) | 1.5 |
| W12.9 | Pilot: install in konverter, validate all workflows | 1 |
| W12.10 | Pilot: install in analise, validate | 1 |
| W12.11 | Documentation: getting started + migration guide | 2 |
| W12.12 | Cross-platform testing (Linux + macOS; Windows later) | 1.5 |

**Gate**: 2 pilot projects running per-project; backward compat with `--legacy-global` works.

## W13 — Publishing Pipeline (8-10 days)

| Subtask | Action | Days |
|---|---|---|
| W13.1 | README per crate + `#![warn(missing_docs)]` all | 2 |
| W13.2 | docs.rs build all feature combinations | 1 |
| W13.3 | semver-check in CI | 0.5 |
| W13.4 | cargo-msrv verify per crate | 0.5 |
| W13.5 | Sigstore signing pipeline | 1 |
| W13.6 | SBOM (CycloneDX) per release | 1 |
| W13.7 | Telemetry privacy doc + opt-out UX | 1 |
| W13.8 | CHANGELOG.md per crate (release-plz config) | 1 |
| W13.9 | Release candidate `1.0.0-rc.1` | 1 |

**Gate**: release tooling working, RC1 published in internal registry.

## W14 — Product Tiers & Distribution (10-15 days)

| Subtask | Action | Days |
|---|---|---|
| W14.1 | Tiers as Cargo features (tier-{{free,standard,premium,enterprise}}) | 2 |
| W14.2 | License key system (JWT ed25519 + local validation) | 2 |
| W14.3 | Telemetry tiered (free/std ON, premium/ent OFF) | 1 |
| W14.4 | Private registry support (enterprise) | 2 |
| W14.5 | SSO scaffold (Okta/Google/GitHub) | 2 |
| W14.6 | Audit log SIEM export (enterprise) | 1.5 |
| W14.7 | Pricing + license validation flow | 1.5 |
| W14.8 | install.touring.dev + binary releases CI/CD | 2 |
| W14.9 | Distro packages (deb, rpm, brew, scoop) | 2 |
| W14.10 | Docker images (alpine, debian-slim, distroless) | 1 |

**Gate W14**: 1.0.0 GA published. install.touring.dev functional. 4 tiers activatable.

## Risk register (per-wave mitigations)

| Wave | Risk | Mitigation |
|---|---|---|
| W4 | 38 consumers break on import path change | Re-export shim `pub use touring_code::ast::* as touring_ast` for 2 versions |
| W6 | Cortex test-debt 0.56% pollutes intelligence | **W6.0 mandatory** before W6.1+ (+5 days budgeted) |
| W6 | 90k LOC build time explodes | profile.dev `incremental=false` + split-debuginfo + sccache verified (REGRA #12) |
| W8 | Hook split breaks Claude Code at runtime | Feature `--legacy-monolith` keeps old behavior for 2 versions |
| W12 | Daemon can't find project | Walk-up + fallback default toolchain + explicit error messages |
| W14 | License JWT compromised | ed25519 key rotation + online revocation + 30-day grace |

## Critical path summary

```
Sequential (1 engineer):           ~138-182 days
With parallelism W5||W7 + W9||W10: ~120-158 days (2 engineers in F2/F3)
```

## References

- ADR-001: Premium architecture (13-crate topology)
- ADR-002: Per-project deployment (rustup-like)
- ADR-003: Commercial tiers (free/standard/premium/enterprise + GTM)
- Memory: `audit:touring-arch-premium-refactor-2026-05-11`
- Memory: `decision:touring-premium-roadmap-2026-05-11`
- Baselines: `docs/baselines/{{wiring,cycles,status,workspace-info,cargo-check}}-pre-refactor-2026-05-11.{{json,log}}`
- Snapshot: `docs/baselines/touring-snapshot-pre-refactor-2026-05-11.tar.gz` (97 MB, SHA-256 0b3934ce…)
"""


# =============================================================================
# Rendering / CLI
# =============================================================================

ARTIFACTS: dict[str, tuple[str, str]] = {
    "ADR-001": ("ADR-001-premium-architecture.md", ADR_001),
    "ADR-002": ("ADR-002-per-project-deployment.md", ADR_002),
    "ADR-003": ("ADR-003-commercial-tiers-gtm.md", ADR_003),
    "MASTER": ("MASTER-PLAN-2026.md", MASTER_PLAN),
}


def build_parser() -> argparse.ArgumentParser:
    """Build the CLI argument parser."""
    parser = argparse.ArgumentParser(
        prog="generate_w0_premium_artifacts",
        description="Generate Touring W0 Premium refactor artifacts (4 ADRs + Master Plan).",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("docs/W0"),
        help="Output directory for generated markdown files.",
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--only",
        choices=sorted(ARTIFACTS.keys()),
        help="Generate only the specified artifact.",
    )
    group.add_argument(
        "--all",
        action="store_true",
        help="Generate all 4 artifacts (default if no flag passed).",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="Enable debug logging.",
    )
    return parser


def write_artifact(output_dir: Path, name: str, filename: str, content: str) -> Path:
    """Write a single artifact to disk and return the path."""
    path = output_dir / filename
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


def main(argv: Sequence[str] | None = None) -> int:
    """CLI entry point."""
    parser = build_parser()
    args = parser.parse_args(argv)

    log_level = logging.DEBUG if args.verbose else logging.INFO
    logging.basicConfig(
        level=log_level,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    LOGGER.debug("parsed args: %s", args)

    try:
        if args.only:
            selection = [(args.only, ARTIFACTS[args.only])]
        else:
            selection = list(ARTIFACTS.items())

        output_dir = args.output_dir
        output_dir.mkdir(parents=True, exist_ok=True)
        LOGGER.info("output directory: %s", output_dir.resolve())

        total_bytes = 0
        for name, (filename, content) in selection:
            path = write_artifact(output_dir, name, filename, content)
            size = len(content.encode("utf-8"))
            total_bytes += size
            print(f"✓ {name:8} → {path} ({size:>7,} bytes)")

        print(f"\nGenerated {len(selection)} artifact(s) · total {total_bytes:,} bytes")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted by user")
        return 130
    except Exception:  # noqa: BLE001 — top-level guard
        LOGGER.exception("unhandled error generating artifacts")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
