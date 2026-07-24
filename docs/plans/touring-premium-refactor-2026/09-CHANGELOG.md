---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
type: "changelog"
created: "2026-05-11"
---
# 09-CHANGELOG — touring-premium-refactor-2026

> Conventional changelog following [Keep a Changelog](https://keepachangelog.com/)
> and [SemVer](https://semver.org/). Per-wave entries added during refactor;
> per-crate CHANGELOG.md generated via release-plz starting at W13.

## [Unreleased]

### [shim-elimination-W4-W6-2026-05-31] — Wave (d) closed: code-consolidation complete (39 → 36 crates)

#### Removed
- **`touring-language`, `touring-semantics`** (W4) — pure re-export shims of
  `touring_code::{languages,semantics}`. Deleted after migrating consumers.
- **`touring-index`** (W6) — pure re-export shim of `touring_intelligence::index`
  (incremental indexing + file watch + LRU cache). Deleted after migrating consumers.

#### Changed
- Consumers migrated (re-measured per L6, not trusted from plan):
  `touring_language::` → `touring_code::languages::` (1 file: `touring-server/cli/language.rs`);
  `touring_semantics::` → `touring_code::semantics::` (2: `touring-hooks/cli_handlers_semantics.rs`,
  `touring-intelligence/src/index/mod.rs`);
  `touring_index::` → `touring_intelligence::index::` (6: `touring-server` `lib.rs`/`graph_service.rs`/`server/mod.rs`,
  `touring-generator` `vgp/engine.rs`/`core/context.rs`/`tests/e2e_pipeline.rs`).
- Cargo dep swaps: `touring-hooks` + `touring-intelligence` + `touring-server` → direct `touring-code`;
  `touring-generator` + `touring-server` → direct `touring-intelligence` (feature passthrough
  `simd-similarity`+`smart-cache` preserved). `touring-server`'s separate semantics+language
  deps folded into a single `touring-code` dep (dedup).

#### Anti-theater note (C08 / VP-Scout Cadeia 7)
- 14 grep false-positives correctly **excluded** from the rename: EntityId string literals
  in `touring-identity/tests/d510_pilot.rs` (`"touring_semantics::Definition"` — renaming would
  break REGRA #17 deterministic-derivation assertions); `touring_index_status`/`_find`/`_search`
  tool-name strings; the `bash_find_maps_to_glob_or_touring_index_files` fn name.

#### Validation
- `cargo check --workspace --tests` exit 0 (26.96s incremental). Only 2 pre-existing
  `refining_impl_trait` warnings (capnp 0.24 test files) — not introduced by this wave.
- 0 residual deps on the 3 removed crates (workspace-wide grep). Structurally no new orphans.

### [dep-hygiene-2026-05-31] — Resolved 3 of 5 tracked RUSTSEC advisories (real upgrades)

#### Security
- **validator 0.18 → 0.20** — single bump removes BOTH `idna 0.5.0` (RUSTSEC-2024-0421)
  and `proc-macro-error 1.0.4` (RUSTSEC-2024-0370). The 36 `#[validate(length(...))]`
  sites in `touring-hooks` migrated with zero changes (length API stable across versions).
- **capnp + capnp-futures + capnp-rpc + capnpc 0.20 → 0.24** (RUSTSEC-2025-0143) —
  required migrating the Cap'n Proto RPC server receiver `&mut self` →
  `self: ::capnp::capability::Rc<Self>` across 12 methods in
  `touring-bindings/src/capnp/{holon_impl,server,generator_health}.rs` (capnp 0.21+
  Rc-based dispatch). `bind-capnp` compiles clean.

#### Changed
- `deny.toml` advisory ignore list 8 → 5 (3 unmaintained kept; the 3 fixed advisories
  removed entirely — their crates are absent/upgraded). 4 stale skip entries pruned.

#### Deferred (kept in `deny.toml` ignore with refined rationale — parent major-bump required)
- RUSTSEC-2025-0140 `gix-date 0.9.4` — build-time only (vergen-gix git metadata);
  fix needs gix 0.71→0.72 ecosystem (no vergen-gix release uses gix 0.72 within `^1.0`).
- RUSTSEC-2026-0002 `lru 0.12.5` — internal to tantivy 0.22's cache; fix needs
  tantivy 0.23+ (Tantivy schema v5 deeply wired; high-risk search-engine bump).

#### Metrics

| Gate | Pre | Post |
|---|---|---|
| RUSTSEC advisories failing (when un-ignored) | 5 tracked | **3 fixed, 2 deferred** |
| `cargo deny check` | exit 0 (5 ignored) | **exit 0 (5 → 5 entries: 3 unmaintained + 2 deferred)** |
| `cargo check --workspace` | exit 0 | exit 0 |

> LESSON: sibling advisories often share a parent — validator 0.20 cleared 2 advisories
> in one bump. Feasibility triage: `cargo update -p X --precise <fix> --dry-run` →
> "failed to select version" = parent bump required; decide fix-vs-defer by severity × risk.

### [W13.5+W13.6-2026-05-31] — Repository-premium: release-plz + cargo-deny GREEN + sigstore

#### Security (cargo-deny: advisories+bans+licenses+sources FAILED → all GREEN)
- **wasmtime 42.0.2 → 44.0.2** — fixes RUSTSEC-2026-0114 (sandbox runtime CVE).
  Real upgrade: `touring-wasm` + `touring-bindings` compile with zero API change.
- **rustls-webpki 0.103.12 → 0.103.13** (RUSTSEC-2026-0104) + **rand 0.8.5 → 0.8.6** (RUSTSEC-2026-0097).
- **fastembed → rustls**: `default-features = false` + `ort-download-binaries-rustls-tls`
  + `hf-hub-rustls-tls` + `image-models`. Eliminates `openssl`/`openssl-sys`/`native-tls`/
  `tokio-native-tls`/`hyper-tls` entirely (0 in lockfile) — the banned-crate failures.
- 5 transitive major-bump advisories tracked in `deny.toml` ignore with upgrade
  paths (capnp→0.24, gix-date→0.12, idna→1.0, lru→0.16, proc-macro-error unmaintained).

#### Added
- `release-plz.toml` (repo root) — binary-product release automation: `publish = false`
  workspace-wide; only `touring-server` releases (version + CHANGELOG + tag `vX.Y.Z` +
  GitHub release). No crates.io, no `CARGO_REGISTRY_TOKEN`.
- Staged workflows (`scripts/.../staging/w13-github-workflows/`, promote to `.github/workflows/`):
  `release-plz.yml` (release-pr + release jobs), `sigstore-release.yml` (keyless cosign
  sign+verify + SHA256SUMS + CycloneDX SBOM, 4-target matrix), `PROMOTION-README.md`.

#### Changed
- `deny.toml` refreshed: 125 skip entries (deterministic all-but-newest), BSL-1.0
  allowed (ryu/xxhash-rust), inferno CDDL-1.0 scoped exception, benches licensed.

#### Metrics

| Gate | Pre | Post |
|---|---|---|
| `cargo deny check` | exit 7 (3 gates FAILED) | **exit 0 (all 4 GREEN)** |
| RUSTSEC advisories failing | 7 | **0** (2 patched + 1 upgraded + 4 tracked + proc-macro-error) |
| openssl/native-tls in graph | yes (banned) | **0** |
| `cargo check --workspace` | exit 0 | exit 0 |

> W13.5 (sigstore) + W13.6 (release-plz) materialized. Keyless OIDC unblocked the
> "HIGH external deps" note (no account/token needed). Promotion: hook blocks direct
> `.github/workflows/` writes → see `PROMOTION-README.md` (Gabriel promotes via `cp`+git).

### [shim-elimination-2026-05-30] — Crate consolidation: 48 → 42 (WA zero-risk + W5 storage family)

#### Removed
- `touring-desktop-ui` + `touring-geopostgis` — dead transparent lib shims
  (`pub use touring_bindings::{desktop,postgis}::*`), 0 workspace consumers.
- `touring-vfs`, `touring-vector-store`, `touring-search-fusion`,
  `touring-embeddings` — W5 compatibility shims, consolidated into
  `touring-storage::{vfs,vec,hybrid_search,embeddings}`.

#### Changed
- Consumers migrated to the canonical `touring-storage` namespace:
  `touring-code/src/ast/file_heat.rs`,
  `touring-server/src/{cli/search_unified,cli/find_code,tools/search_tools,main}.rs`,
  `benches/{Cargo.toml + throughput,hybrid_search_bench,keyword_search_bench,semantic_search_bench}.rs`.
- `touring-code` + `touring-server` + `benches` Cargo.toml deps swapped to
  `touring-storage` (default features = exact union the 4 shims forwarded:
  storage-{vfs,vec-sqlite,emb-fastembed,hybrid}).
- 2 stale doc-comments updated (`touring_embeddings::` → `touring_storage::embeddings::`).

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| Crate count | 48 | 42 | **-6** |
| `cargo check --workspace --tests --benches` | exit 0 | exit 0 | green |
| Lingering removed-namespace refs | — | 0 | clean |

> KEEP (not debt): `touring-web` + `touring-web-server` (Leptos+Axum product,
> `[[workspace.metadata.leptos]]`), `touring-capnp-server` (Cap'n Proto product),
> `touring-loom-proofs` (concurrency proofs). Next: W4-code / W6-intel / W10-orch
> shim families (see `~/.claude/plans/touring-47-to-13-residual/plan.md`).

### [W12.3-simplified-2026-05-23] — Wave W12 partial: toolchain install/remove (local tarball, no download/sigstore)

#### Added
- `cli::toolchain::ToolchainCmd::Install { version, from_tarball, force }`
  variant + parser support for `touring toolchain install --from-tarball
  <path> <version> [--force]` (both `--from-tarball X` and `--from-tarball=X`
  forms accepted).
- `cli::toolchain::ToolchainCmd::Remove { version }` variant + parser support
  for `touring toolchain remove <version>` / `touring toolchain uninstall
  <version>` (alias).
- `pub fn cli::toolchain::install_toolchain_from_tarball(home, version,
  tarball, force) -> Result<()>` — extracts a `.tar.gz` into
  `home/toolchains/<version>/` via the system `tar -xzf` binary (no new
  workspace dep). Writes `meta.toml` (version + installed_at unix-ts + source)
  at the toolchain root for future inspection. Cleans up partial extraction
  on failure.
- `pub fn cli::toolchain::remove_toolchain(home, version) -> Result<()>` —
  removes the toolchain dir. **Safety**: refuses to remove the currently
  active default (user must `touring toolchain default <other>` first).
- 12 new unit tests covering: parse install (separate args + equals form +
  force), parse missing tarball, parse remove + uninstall alias, extract
  tarball into toolchain dir, refuse existing without force, force overwrite,
  error when root missing, error when tarball missing, remove extracts dir,
  refuse active default, error on uninstalled.

#### Implementation choice
- Uses system `tar` (POSIX-standard on Linux + macOS, tier-1 for W12) via
  `std::process::Command`. Avoids adding `tar` + `flate2` crates to workspace
  deps just for this feature. The W12 spec also notes Windows is W14
  territory (distro packages), so the POSIX assumption is consistent.

#### Deferred to W12.8 (install.touring.dev)
- URL download (`touring toolchain install <ver>` without `--from-tarball` —
  needs HTTP + release-server URL convention)
- sigstore signature verification
- SHA-256 checksum verification
- The local-tarball path implemented here is the FOUNDATION the future
  installer wraps after download + verify.

#### Composes with prior W12 work
- `touring toolchain init` (W12.2) creates the root structure.
- `touring toolchain install --from-tarball X 0.30.0` (this) populates
  `~/.touring/toolchains/0.30.0/`.
- `touring toolchain default 0.30.0` (W12.2) writes `~/.touring/default`.
- The walk-up hook shim (W12.6) Layer 3 now resolves to a real binary at
  `~/.touring/toolchains/<default>/bin/touring-hook`.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| W12 subtasks DONE | 8/12 | 9/12 | +1 |
| `cli::toolchain` test count | 16 | 28 (+12) | +12 |
| `cli::toolchain` pub fns | 5 | 7 (+install, +remove) | +2 |
| `cargo check --workspace` errors | 0 | 0 | 0 |

### [W12.5-partial-2026-05-23] — Wave W12 partial: daemon socket path resolver (foundation)

#### Added
- `touring_foundation::config::TouringConfig::resolve_daemon_socket_path()` —
  production entry point. Reads `TOURING_DAEMON_SOCKET` env override and CWD
  walk-up.
- `TouringConfig::resolve_daemon_socket_path_from(start_dir)` — accepts an
  explicit start directory (or `None`); reads the env var internally.
- `TouringConfig::resolve_daemon_socket_path_inner(start_dir, env_override)` —
  **pure function**; takes both the start dir and the env override explicitly.
  Used by unit tests to avoid env-var race conditions with parallel test runs.
- 5 unit tests covering: env override wins, walk-up from nested subdir resolves
  to project sock, falls back to `/tmp/touring-daemon-<uid>.sock`, no start
  dir falls to global, empty-string env override treated as unset. All PASS.

#### Resolution chain
1. **Layer 1**: `TOURING_DAEMON_SOCKET` env var (explicit override, for testing
   and ops debugging)
2. **Layer 2**: walk-up from CWD (or `$CLAUDE_PROJECT_DIR` if set) looking for
   `<dir>/.touring/daemon.sock`, stopping at filesystem root
3. **Layer 3**: global fallback `/tmp/touring-daemon-<uid>.sock` (matches the
   current production daemon spawn convention — REGRA #2.5 backward compat)

#### What W12.5 partial does NOT do (full W12.5 still ahead)
- Does NOT spawn a daemon at the resolved socket — only resolves the path.
- Does NOT bind a socket — just returns where the path would live.
- Does NOT modify the running daemon's socket behavior — production daemon
  continues to use `/tmp/touring-daemon-<uid>.sock` until W12.5 full ships.

This is the **foundation** for full W12.5 (daemon multi-instance per-project
socket bind/spawn). Production callers can begin migrating to
`resolve_daemon_socket_path()` even before full W12.5 ships — the global
fallback ensures behavior is unchanged until per-project sockets exist.

#### Composes with W12.1 + W12.6
- `touring init-project` (W12.1) creates `.touring/` — the walk-up target dir.
- The walk-up shim (W12.6) dispatches binaries via the same `.touring/bin/`
  walk-up pattern; this resolver does the equivalent for `.touring/daemon.sock`.
- Together, binary + socket resolution share a single architectural pattern.

#### Lesson — race-condition gotcha (Rust parallel tests)
- Initial implementation read `std::env::var("TOURING_DAEMON_SOCKET")` inside
  the testable variant — parallel tests setting/unsetting the env var raced
  with the read, causing intermittent failures. Refactored into
  `_inner(start_dir, env_override: Option<&str>)` which takes the env value
  explicitly. The public `_from()` wraps it with the env read. Tests now use
  `_inner` directly — race-free.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| W12 subtasks DONE | 7/12 | 8/12 | +1 |
| `touring-foundation` test count | 464 | 469 (+5) | +5 |
| W12.5 tests | n/a | 5/5 PASS | — |
| `cargo check --workspace` errors | 0 | 0 | 0 |

### [W12.12-2026-05-23] — Wave W12 partial: CI matrix template (Linux + macOS, 5 jobs)

#### Added
- `scripts/ci/per-project-deployment.yml.template` (NEW, ~190 LOC) — ready-to-
  copy GitHub Actions workflow that exercises all W12 deliverables end-to-end.
  YAML validated via `python3 -c "import yaml; yaml.safe_load(...)"` — 5 jobs
  parsed cleanly.
- 5 CI jobs:
  1. **`build-and-test`** (matrix: ubuntu-latest + macos-latest) —
     `cargo check --workspace --all-targets` + the 4 W12 unit-test buckets
     (init_project / toolchain / config::test_layered / migrate_from_global).
  2. **`shellcheck-shim`** — `shellcheck scripts/hooks/touring-hook-shim.sh`
     (W12.6 lint gate).
  3. **`lint`** — `cargo fmt --check` (scoped to W12 files) + `cargo clippy -D
     warnings` on `touring-foundation` and `touring-server`.
  4. **`shim-e2e`** (4 inline scenarios) — force_bin override, fail-open exit 0,
     CLAUDE_PROJECT_DIR walk-up, TRACE-to-stderr logging. Each scenario asserts
     the resolved output matches expected. Mirrors the manual W12.6 tests.
  5. **`docs-lint`** — verifies the 3 W12.11 guides exist + each has ≥150 LOC.
- Triggers: `push` to `main` / `wave/W12*`, `pull_request` to `main` (scoped to
  W12 paths so unrelated PRs don't pay the cost), and `workflow_dispatch`.

#### Why template (not direct `.github/workflows/`)
- The workspace's `security_reminder_hook.py` blocks direct Writes into
  `.github/workflows/` to enforce review of new CI surface. The template lives
  outside that path so it can be edited freely; activation is a single
  `cp scripts/ci/...template .github/workflows/...yml`.
- Security review checklist embedded in the template header:
  ✅ no untrusted `github.event.*` contexts in `run:` blocks (no PR title /
  body / commit message / branch name interpolation), ✅ env vars used only
  for safe values, ✅ `paths:` filter scoped to W12 deliverables, ✅
  `shellcheck` gate runs before the e2e job.

#### Windows intentionally out of scope
- Per the W12 spec, Linux + macOS are tier-1; Windows is W14 (distro
  packages). Matrix has explicit comment marker for future Windows addition.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| W12 subtasks DONE | 6/12 | 7/12 | +1 |
| CI templates | 0 | 1 (template ready) | +1 |
| YAML jobs defined | 0 | 5 | +5 |
| YAML validated | n/a | `yaml.safe_load` PASS | — |

### [W12.11-2026-05-23] — Wave W12 partial: documentation (3 user guides, 611 LOC)

#### Added
- `docs/guide/getting-started.md` (185 LOC) — 5-minute tutorial for the
  rustup-pattern per-project layout. Covers `touring toolchain init`,
  `touring init-project`, `touring migrate-from-global`, opt-in walk-up shim,
  layered config verification. Explicit "what still requires manual work" list
  with status of W12.3/5/8/12.
- `docs/guide/migration.md` (255 LOC) — Step-by-step transition from global
  layout (`~/.claude/touring/`) to per-project (`.touring/data/`). Covers
  dry-run, first migration, force mode, rollback (3 options: .bak files, tar
  snapshot, re-pull from global), clean-up of global layout, exhaustive
  file list table, troubleshooting (`permission denied` on running daemon
  etc.).
- `docs/guide/external-client.md` (171 LOC) — Future spec for
  `curl install.touring.dev | sh` (W12.8 target) + the *manual install
  interim* recipe for current users without W12.8 yet. Documents the
  rustup-style security model (TLS 1.2 pin, SHA-256, sigstore signature)
  that W12.8 will implement.

#### Audience
- New users (start at getting-started.md)
- Migrating users (start at migration.md)
- External users / cross-machine install (external-client.md)

#### Composes with
- All 5 W12 code subtasks (W12.1, W12.2, W12.4, W12.6, W12.7) are now
  user-documented. The guides exercise the canonical end-to-end flow:
  `toolchain init` → `init-project` → `migrate-from-global` → opt-in shim →
  layered config reads project layer.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| W12 subtasks DONE | 5/12 | 6/12 | +1 |
| `docs/guide/` files | 0 | 3 (NEW dir) | +3 |
| Documentation LOC | 0 (W12) | 611 | +611 |

### [W12.7-2026-05-23] — Wave W12 partial: `touring migrate-from-global` (global → per-project DB migration)

#### Added
- `crates/touring-server/src/cli/migrate_from_global.rs` (NEW, ~310 LOC) —
  implements `touring migrate-from-global [--from DIR] [--to DIR] [--dry-run]
  [--force]`. NEW subcommand, distinct from existing `touring migrate` (which
  remains DB-schema consolidation) per REGRA #0.
- Copies known DB files from `~/.claude/touring/` (or `--from`) into project's
  `.touring/data/` (or `--to`):
  - `MIGRATE_FILES`: symbols.db, knowledge.db, memory.db, graph.db,
    semantic_recall.db, rlm_memory.db, ann_memory.db, touring_knowledge.db,
    touring_pipeline.db, got_snapshots.db (10 files)
- Safety: refuses to silently overwrite existing dest files. Default mode
  renames existing → `<name>.bak.<unix_ts>`. `--force` overwrites without
  backup.
- `--dry-run` walks the file list and reports what *would* be copied, mutating
  nothing.
- `pub struct cli::migrate_from_global::MigrationReport` (5 fields: source,
  destination, copied, skipped_missing, backed_up, dry_run).
- `pub fn migrate_from_global_in(source, destination, dry_run, force)` —
  testable pure-IO core.
- 10 unit tests covering: 3 parse scenarios, copies only known DBs, skipped
  missing tracking, dry-run writes nothing, backup-without-force, force-no-
  backup, error when source missing, byte-content preservation (binary payload
  with `\x00` and `\xff`). All PASS.

#### Changed
- `crates/touring-server/src/cli/mod.rs` — added `pub mod migrate_from_global;`
  (line 20, alphabetic order).
- `crates/touring-server/src/cli/common.rs` — registered `CommandDescriptor`
  for `"migrate-from-global"` in `command_table()`.

#### REGRA #0 honored
- Existing `touring migrate` (`migrate.rs`, 1331L — DB consolidation) preserved
  intact. `migrate-from-global` is a NEW sibling subcommand, NOT replacement.
- No existing semantics disturbed.

#### Composes with W12.1 + W12.4 + W12.6
- `touring init-project` (W12.1) creates `.touring/data/` (the destination).
- `touring migrate-from-global` populates `.touring/data/` with the user's
  existing DBs.
- After migration, the daemon (W12.5 future) walks-up to `.touring/touring.toml`
  (W12.4 Project layer), reads the per-project paths, and operates on the
  migrated DBs — **isolated** from other projects.

#### Future enhancement
- DB **content filtering** (per-project subset extraction via project-tag
  columns) is not yet implemented — current behavior is byte-for-byte file
  copy. Correct for the common single-project-under-global case; needs schema
  changes for multi-project filtering.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| W12 subtasks DONE | 4/12 | 5/12 | +1 |
| `touring-server` cli modules | 67 | 68 (+1: migrate_from_global) | +1 |
| W12.7 tests | n/a | 10/10 PASS | — |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `cargo check -p touring-server` time | n/a | 6.73s (cached) | — |

### [W12.2-2026-05-23] — Wave W12 partial: `touring toolchain` CLI (~/.touring/ user-level manager)

#### Added
- `crates/touring-server/src/cli/toolchain.rs` (NEW, ~340 LOC) — implements
  `touring toolchain {init|list|default <ver>}` subcommands. Rustup-pattern
  user-level toolchain root manager.
- `pub fn cli::toolchain::toolchain_home()` — resolves `~/.touring/` honoring
  `TOURING_HOME` env override.
- `pub enum cli::toolchain::ToolchainCmd { Init {force}, List, Default {version}, Help }`
  + `pub fn parse(&[String]) -> Self`.
- `pub fn init_toolchain_root(home, force)` — scaffolds `~/.touring/{toolchains/, config.toml}`
  with the default `config.toml` body (User layer in `detect_layered()`).
- `pub fn list_installed_toolchains(home) -> Vec<String>` — sorted enumeration
  of `home/toolchains/<version>/` subdirs.
- `pub fn current_default(home) -> Option<String>` — reads `home/default`.
- `pub fn set_default(home, version)` — refuses uninstalled versions (catches
  typos before the shim silently falls through).
- 16 unit tests covering: 5 parse scenarios, 3 init scenarios (create / refuse
  existing / force overwrite), 3 list scenarios (empty / sorted / missing-dir),
  4 default scenarios (none initially / set after install / refuse uninstalled /
  refuse empty), 1 env-override scenario. All PASS.

#### Changed
- `crates/touring-server/src/cli/mod.rs` — added `pub mod toolchain;` (line 20).
- `crates/touring-server/src/cli/common.rs` — registered `CommandDescriptor` for
  `"toolchain"` in `command_table()`.

#### Composes with W12.1 + W12.4 + W12.6
- `~/.touring/config.toml` (created by W12.2 `init`) is the User layer that
  `TouringConfig::detect_layered()` (W12.4) reads.
- `~/.touring/toolchains/<default>/bin/touring-hook` is Layer 3 of the walk-up
  shim (W12.6) — `toolchain default <ver>` writes the version that the shim
  reads via `~/.touring/default`.
- `touring init-project` (W12.1) creates the **project** layer; `touring toolchain
  init` creates the **user** layer. Together with `/etc/touring/config.toml`
  (System) and hardcoded defaults, all 4 W12.4 layers are now scaffoldable
  via canonical CLI subcommands.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| W12 subtasks DONE | 3/12 | 4/12 | +1 |
| `touring-server` cli modules | 66 | 67 (+1: toolchain) | +1 |
| `touring-server` test count | n/a | +16 | +16 |
| W12.2 tests | n/a | 16/16 PASS | — |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `cargo check -p touring-server` time | n/a | 6.46s (cached) | — |

### [W12.6-2026-05-23] — Wave W12 partial: hook dispatcher walk-up shim (rustup-style)

#### Added
- `crates/touring-rust/scripts/hooks/touring-hook-shim.sh` (NEW, ~80 LOC bash)
  — rustup-pattern hook dispatcher with 4-layer resolution chain:
  1. `TOURING_HOOK_SHIM_FORCE_BIN` env var (explicit override, for testing)
  2. Per-project walk-up looking for `.touring/bin/touring-hook` from CWD
     (or `$CLAUDE_PROJECT_DIR` if set), stopping at filesystem root
  3. User-default toolchain: `~/.touring/toolchains/<default>/bin/touring-hook`
     (reads version from `~/.touring/default` file)
  4. Global fallback: `~/.claude/rust/target/release/touring-hook` (current
     production binary)
  5. Fail-open: silent `exit 0` if no binary found (honors hook contract)
- Environment knobs documented inline: `TOURING_HOOK_SHIM_TRACE=1` (stderr
  trace each lookup step), `TOURING_HOOK_SHIM_FORCE_BIN=<path>` (testing),
  `TOURING_HOME=<dir>` (toolchain root override), `CLAUDE_PROJECT_DIR=<dir>`
  (walk-up start override).

#### Side-by-side install (opt-in, not yet active)
- The shim is NOT (yet) symlinked into `~/.claude/hooks/touring-hook` — the
  current symlink still points to the global release binary, so production
  behavior is unchanged. To opt in (Gabriel's call):
  ```bash
  ln -sfn ~/.claude/rust/scripts/hooks/touring-hook-shim.sh \
      ~/.claude/hooks/touring-hook
  ```
- To revert: re-point symlink to `~/.claude/rust/target/release/touring-hook`.

#### Tested
- 4 manual scenarios PASS:
  1. `TOURING_HOOK_SHIM_FORCE_BIN` → executes forced bin (proof of Layer 1)
  2. Walk-up from nested subdir without `.touring/bin/` → falls through to
     Layer 4 global binary (proof of fail-through)
  3. Zero binaries anywhere → exit 0 (proof of fail-open)
  4. `CLAUDE_PROJECT_DIR` override + walk-up 2 levels → executes per-project
     bin (proof of Layer 2 + override interaction)
- `shellcheck scripts/hooks/touring-hook-shim.sh` → 0 issues.

#### Composes with W12.1 + W12.4
- W12.1 (`touring init-project`) creates `.touring/bin/` — the shim's Layer 2.
- W12.4 (`detect_layered`) reads `.touring/touring.toml` — the shim
  complements by reading `.touring/bin/touring-hook` (the runtime binary, not
  the config).
- Three subtasks now COMPOSE: init-project produces, detect_layered consumes
  config, shim dispatches to the right binary.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| W12 subtasks DONE | 2/12 | 3/12 | +1 |
| `shellcheck` issues | n/a | 0 | clean |
| Manual scenario tests | n/a | 4/4 PASS | — |
| Production symlink touched | no | no | unchanged (opt-in) |

### [W12.1-2026-05-23] — Wave W12 partial: `touring init-project` CLI (rustup-pattern per-project scaffolder)

#### Added
- `crates/touring-server/src/cli/init_project.rs` (NEW, ~230 LOC) — implements
  `touring init-project` subcommand that scaffolds `.touring/{touring.toml,
  data/,bin/,hooks/}` under the current working directory. Mirrors the rustup
  per-project pattern (`rust-toolchain.toml`-style pinning). Coexists with the
  existing `touring init` subcommand (TOML profile preset) per REGRA #0.
- `cli::init_project::InitProjectArgs` (pub struct) with `--force` / `--bare` /
  `--root=PATH` flag support.
- `cli::init_project::run(args)` (pub fn) — CLI dispatch entry.
- `cli::init_project::init_project_in(root, args)` (pub fn) — testable core
  scaffolder.
- New `CommandDescriptor` for `"init-project"` in `cli::common::command_table`
  (immediately after `init`).
- 8 unit tests covering: parse defaults, parse --force/-f, parse --bare/--root,
  create full tree (`.touring/{touring.toml,data,bin,hooks}` all present), --bare
  skips toml body, refuse-existing-without-force, --force wipes sentinel +
  recreates, --root targets explicit dir (outer tmp untouched). All PASS.

#### Changed
- `crates/touring-server/src/cli/mod.rs` — added `pub mod init_project;`
  declaration (line 19, immediately after `pub mod init;`).

#### Discovered
- The existing `touring init` (`init.rs`, 942L) and `touring migrate`
  (`migrate.rs`, 1331L) are NOT to be replaced — they implement different
  semantics (TOML profile preset / DB consolidation). W12 spec items
  W12.1 + W12.7 require ADDING new sibling subcommands (`init-project` +
  `migrate-from-global`) per REGRA #0 potentialize.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| `touring-server` cli modules | 65 | 66 (+1: init_project) | +1 |
| `touring-server` test count | n/a | +8 | +8 |
| W12.1 tests | n/a | 8/8 PASS | — |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `cargo check -p touring-server` time | n/a | 31.53s (first), 6.98s (cached) | — |

### [W12.4-2026-05-23] — Wave W12 partial: layered config loader (Hardcoded < System < User < Project)

#### Added
- `touring_foundation::config::TouringConfig::detect_layered()` — production
  entry point reading `/etc/touring/config.toml` (System) →
  `~/.touring/config.toml` (User) → `.touring/touring.toml` (Project, walk-up
  from CWD) with hardcoded defaults as base. Rustup-pattern adapted (Context7
  `/rust-lang/rustup` 2026-05-23). Last-write-wins per key via recursive
  `toml::Value` merge.
- `touring_foundation::config::TouringConfig::detect_layered_from(system, user, project)` —
  testable variant accepting explicit layer paths.
- `touring_foundation::config::TouringConfig::find_project_toml_walk_up()` —
  walks CWD up to filesystem root looking for `.touring/touring.toml`.
- `merge_toml(base, overlay)` private helper — recursive last-write-wins TOML
  merge (tables merged recursively, scalars/arrays replaced wholesale).
- 5 unit tests covering: hardcoded-only fallback, user-overrides-hardcoded,
  project-overrides-user, full 3-layer chain, malformed-file-falls-through.
  All PASS.

#### Discovered
- `touring-server/src/cli/init.rs` (942L) and `cli/migrate.rs` (1331L) ALREADY
  exist but with semantics that DIFFER from the W12 plan spec:
  - `init.rs` = TOML profile preset (`--profile`/`--list-profiles`/`--cc-setup`)
  - `migrate.rs` = DB consolidation migration (8 legacy DBs → 3)
  W12 implementations MUST extend (REGRA #0 potentialize), not replace. Plan:
  add `init-project` + `migrate-from-global` as NEW subcommands.

#### Metrics

| Gate | Pre | Post | Delta |
|---|---|---|---|
| `touring-foundation` test count | 459 | 464 (+5) | +5 |
| W12.4 tests | n/a | 5/5 PASS | — |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `cargo check -p touring-foundation` time | n/a | 2.08s | — |

### [W11-2026-05-23] — Wave W11 closure (via Wave 5 mossy-crunching-owl)

#### Added
- `crates/touring-code/src/polyglot/search.rs` — 2 B-FUZZ-002 regression tests
  (`test_go_polyglot_search_post_abi_v15_returns_ok_on_arbitrary_input` +
  `test_go_polyglot_search_handles_minimal_and_unusual_sources_without_panic`).
  12/12 polyglot::search PASS post ABI v15 alignment.
- `crates/touring-code/src/polyglot/lang.rs` — S-14 status block documenting
  why `Lang::Md` is NOT wired: `ast-grep-language 0.42.3 SupportLang::all_langs()`
  exposes 27 variants — none is Markdown. Wiring would require forking
  `ast-grep-language` OR violating the "thin wrapper" architecture. Deferred
  until upstream exposes Markdown.
- `crates/touring-hooks/src/shared/bash_ast_validator.rs` — S-13 closure
  rationale documenting that the bash tokenizer is NOT removed (REGRA #0
  violation). The tokenizer is the chosen design for bash structural
  validation; removal would forfeit 22-test structural coverage, simpler
  fail-open contract, and shell-quoting robustness. AST path remains
  available as complement via `touring-code::polyglot::search`.

#### Changed
- Root `Cargo.toml`: `ast-grep-core = "=0.42.3"` (was `=0.36.0`),
  `ast-grep-language = "=0.42.3"`, `tree-sitter = "0.26"` (was `0.24`).
  Lockstep alignment to ABI v15.
- `crates/touring-code/src/polyglot/search.rs:1` — `StrDoc` import moved
  from crate root to `ast_grep_core::tree_sitter::StrDoc` (0.42 namespace
  reorganization).

#### Fixed
- **B-FUZZ-002 PRODUCTION FIX** — `tree-sitter-go` ABI v15 incompatibility
  with `ast-grep-core =0.36.0` (`node.rs:73 .expect("should parse")` abort
  in release) eliminated. Go polyglot search/rewrite now operational
  in production. Lesson key: `wave-5-mossy-crunching-owl-S9-S13-S14-closure:2026-05-23`.

#### Discovered
- ast-grep 0.36→0.42.3 was a 1-line API edit (`StrDoc` namespace) plus a
  workspace-wide tree-sitter 0.24→0.26 bump — no cascading API breaks.
  Wave 5 (mossy-crunching-owl) executed end-to-end: 42/42 CEG audit PASS
  · 100% health · 0 GAP · 0 regressions across 56+12+22=90 session tests.

#### Metrics

| Gate | Pre-Wave-5 | Post-Wave-5 | Delta |
|---|---|---|---|
| ast-grep-core version | =0.36.0 | =0.42.3 | upgrade |
| tree-sitter version | 0.24 | 0.26 | upgrade |
| B-FUZZ-002 (Go polyglot crash) | PRESENT | ELIMINATED | ✅ |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| polyglot::search regression tests | 10 | 12 (+2) | +2 |
| CEG audit PASS rate | 41/42 (97%) | 42/42 (100%) | +1 PASS |

#### W11 Remaining (advisory baselines, non-blocking)

- W11.4 `cargo-mutants` advisory baseline per-crate (`touring-foundation`,
  `touring-intelligence`, `touring-code`). Tracked in
  `.touring-cache/mutation-test/`. Skip-friendly: workspace-wide mutation
  is hours/days; advisory mode is a snapshot, NOT a gate.
- W11.2 `touring-bindings` per-feature coverage measurement
  (`cargo llvm-cov -p touring-bindings --features bind-<feat>`). 185 tests
  already exist behind `bind-*` features — measurement is documentary,
  not new-test work.

### [W11-2026-05-15] — Wave W11: Test Debt Repayment — re-scoped + W11.6 fuzz

#### Added
- `fuzz/` — cargo-fuzz crate at the workspace root with 8 VGP-verified fuzz
  targets: `fuzz_rust_syn`, `fuzz_rust_public_api` (touring-code syn parsing);
  `fuzz_polyglot_search_{rust,python,typescript,go}` + `fuzz_polyglot_rewrite`
  (touring-ast-polyglot tree-sitter + ast-grep); `fuzz_rkyv_deserialize`
  (`touring_rkyv::check_archived_root`). `cargo +nightly fuzz build` exit 0.
- 2 regression tests in `touring-code::polyglot` — `search` / `rewrite`
  reject malformed patterns without panicking.

#### Changed
- Root `Cargo.toml` `[workspace]`: added `exclude = ["fuzz"]` (the fuzz crate
  needs nightly + libfuzzer; excluded so `cargo check --workspace` is unaffected).

#### Fixed
- `touring-code::polyglot` `search()` / `rewrite()` built the ast-grep matcher
  with the infallible `Pattern::new` (`.unwrap()` internally) → panic on a
  malformed pattern. Switched to fallible `Pattern::try_new`, mapping the error
  into `Error::InvalidPattern` (both fns already return `Result`). Surfaced by
  the new fuzz targets.

#### Discovered
- W11 plan premises (test ratios) were STALE — written 2026-05-11, before the
  W4-W10 fusions. Ground-truth re-measurement (`cargo llvm-cov --json`):
  touring-intelligence **83.14%** line coverage (plan said "15%→20%"),
  touring-foundation **77.73%** (plan said "15%→22%"). W11.1/W11.3 obsolete.
  W11.5 already met — 89 proptest properties exist (plan asked ≥50).
  touring-bindings is feature-gated (`default=[]`) — naive coverage is
  unmeasurable; 185 tests exist behind `bind-*` features. W11 re-scoped
  10-15d → 5-8d. Detail in `W11-test-debt-repayment.md` Discovery Updates.
- **B-FUZZ-001** (deferred — debug-only, severity corrected 2026-05-15) —
  `ast-grep-core` `match_tree/mod.rs:82` is a `debug_assert!(false, "Ellipsis
  should be matched in parent level")`, NOT a `panic!`. Compiled out in release
  (`debug-assertions = false`) — fires only under debug / cargo-fuzz builds; in
  production the branch just returns `Some(())`. A text guard
  `is_degenerate_ellipsis_pattern` was added to `polyglot/search.rs`+`rewrite.rs`
  (25 polyglot tests) — partial mitigation; the ellipsis node is produced by
  ast-grep's internal parse, so a text heuristic is inherently incomplete (the
  python fuzz target still trips it with a zero-`$` input). Not a production crash.
- **B-FUZZ-002** (deferred) — tree-sitter-go grammar ABI v15 incompatible with
  pinned `ast-grep-core =0.36.0` → panic in polyglot search for Go.
- Both B-FUZZ bugs → one follow-up: evaluate upgrading `ast-grep-core` from
  `=0.36.0` (Cargo.toml:399) to `0.38.7`/`0.42.1` (already in the registry).

#### Metrics

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Fuzz targets | 0 | 8 (build-clean) | +8 |
| `cargo +nightly fuzz build` | n/a | exit 0 | — |
| Fuzz targets smoke-clean | n/a | 4 / 8 | — |
| Bugs surfaced by fuzzing | — | 5 (1 fixed, 4 deferred) | — |
| `touring-code` polyglot tests | 15 | 17 (+2 regression) | +2 |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| W11-introduced regressions | — | 0 | 0 |

### [W10-2026-05-15] — Wave W10: touring-orchestration Fusion completed

#### Added
- `touring-orchestration` crate — unified orchestration layer (18 files,
  ~2,629 LOC) fusing three standalone crates into the modules `flow`
  (declarative dataflow pipeline, ex touring-flow), `tasks` (Tasksfile YAML
  DSL + compiler, ex touring-tasksfile), `devrc` (Devrcfile adapter, ex
  touring-devrc-adapter). Feature surface: `yaml`, `templates`,
  `http-client` (`default = yaml + templates`). 79 tests PASS.

#### Changed
- `touring-flow`, `touring-tasksfile`, `touring-devrc-adapter` reduced to
  1-file shim crates (`pub use touring_orchestration::<module>::*`). The
  shims preserve their feature interface (`touring-tasksfile/templates`,
  etc.) by propagating to `touring-orchestration`. External consumers
  (touring-hooks, touring-server) are unchanged.
- 42 intra-crate `crate::` references rewritten to `crate::{flow,tasks,
  devrc}::` (no string-literal `"crate::"` existed — zero false-positive
  risk); `touring_tasksfile::` → `crate::tasks::` in the fused `devrc`
  module (the old devrc → tasksfile crate edge became intra-crate).
- 4 pre-existing `unused_variable` warnings fixed in moved test code
  (`touring-flow` / `touring-tasksfile` lacked `[lints] workspace` before
  the fusion, so the fused crate's strict lints surfaced them).

#### Discovered
- The W10 plan's "extract decompose / session / diary from touring-server"
  is **superseded by W9**: W9 already extracted `session` →
  `touring-server-session` and `reasoning` (TaskDecomposer) →
  `touring-server-reasoning`. W10 ships the **3-crate fusion** only; the
  server-side extraction is W9's outcome.
- The 3 fused crates form a clean internal DAG (`devrc` → `tasksfile`,
  `flow` independent) — fusing them turns the cross-crate edge into an
  intra-crate `crate::tasks` reference, no cycle.

#### Metrics

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Productive orchestration crates | 3 | 1 (+3 shims) | -2 boundaries |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `cargo check --workspace --tests` errors | 0 | 0 | 0 |
| `touring-orchestration` + shim tests | n/a | 79 PASS | — |
| clippy `-D warnings` (orchestration) | n/a | 0 | — |
| W10-introduced regressions | — | 0 | 0 |

### [W9-2026-05-15] — Wave W9: touring-server Internal Split (pragmatic 3-crate) completed

#### Added
- `touring-server-reasoning` crate — reasoning layer extracted from
  touring-server (5 files, ~4,483 LOC): decomposer, granularity adapter,
  inference handlers. Cycle-free **leaf**. 103 tests PASS.
- `touring-server-visual` crate — graph visualization formatters (9 files,
  ~2,766 LOC): DOT, Mermaid, JSON, flow viz, edge bundling. Cycle-free
  **leaf**. 89 tests PASS.
- `touring-server-session` crate — session management layer (2 files,
  ~686 LOC): session manager + state handling. Cycle-free **leaf**.
  35 tests PASS.

#### Changed
- `touring-server` `lib.rs` re-exports the 3 extracted modules verbatim
  (`pub mod reasoning;` → `pub use touring_server_reasoning::reasoning;`,
  etc.). The `touring` binary and the external API are unchanged — verified
  by `cargo check --workspace --tests` (exit 0).
- 22 cross-crate visibility promotions `pub(crate)` → `pub`: 16 on
  `TaskDecomposer` / `SubTask` / `DecomposeValidationMetrics` in
  `reasoning::decomposer` + 6 on `SessionManager` in `session::manager`
  (consumed by the SCC `server/` modules across the new crate boundary).
- 5 pre-existing latent issues fixed in moved code: 4 clippy errors in
  `visual` (`should_implement_trait`, `unused_enumerate_index`,
  `if_same_then_else`, `derivable_impls`) + 1 `unused_comparisons` test
  warning in `flow.rs`.

#### Discovered
- SCC analysis of touring-server's 23 modules: only **{cli, server, tools}**
  form a strongly-connected component (116 files). The other 20 modules are
  a clean DAG — touring-server is genuinely layered (unlike touring-hooks in
  W8, where 7/10 buckets collapsed into one SCC). The W9 plan's 6-bucket
  split: `server-cli`/`server-tools` always belonged in the SCC with
  `server`; `reasoning`/`session`/`visual` are pure leaves that extract
  cleanly.
- `touring-server-telemetry` deferred — `telemetry/` + `telemetry_init.rs`
  are dependency-leaves but entangled with 6 observability feature flags
  (`console`/`otlp`/`file-logs`/`tracy`/`dhat-heap`/`ebpf-telemetry`) + 8
  optional deps + the binary's allocator/startup story. Mechanical but
  error-prone — a focused follow-up (like W8's `rl` deferral). W9 ships
  **3 crates**, not the planned 4.
- `src/snapshot/` (3 files, 724 LOC) is dead code — never `mod`-declared at
  crate root, 0 `crate::snapshot` references. Left in place (W1 scope).

#### Metrics

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Productive crates in server layer | 1 | 4 | +3 internal |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `cargo check --workspace --tests` errors | 0 | 0 | 0 |
| New-crate tests | n/a | 227 PASS | — |
| clippy `-D warnings` (new crates) | n/a | 0 | — |
| New Cargo cycles | — | 0 | 0 |
| W9-introduced regressions | — | 0 | 0 |

### [W8-2026-05-15] — Wave W8: touring-hooks Internal Split (pragmatic 3-crate) completed

#### Added
- `touring-hooks-shared` crate — cycle-free **LEAF** utilities crate (15
  modules, ~4,882 LOC) extracted from touring-hooks: `errors`, `metrics`,
  `plugin`, `query_dsl`, `rfc100_emission`, `idempotency`,
  `got_snapshot_store`, `mcp_overhead`, `memory_finding`, `n1_bridge`,
  `pattern_bandit`, `precomputed_signals`, `qa_syntax`, `reranked_context`,
  `user_filters`. Depends on NO other touring-hooks-* crate. 186 tests +
  1 doctest PASS.
- `touring-hooks-prediction` crate — predictive/classification layer (7
  modules, 8 files, ~5,375 LOC) extracted from touring-hooks: `classifier`,
  `pii`, `llm_judge`, `tfidf_retriever`, `layer7_prediction`,
  `semantic_classifier`, `ann_memory`. Depends only on `touring-hooks-shared`
  (no SCC back-edge). 108 tests PASS.

#### Changed
- `touring-hooks` reduced to a **façade** for the 22 extracted modules — each
  `pub mod X;` became `pub use touring_hooks_{shared,prediction}::X;`. External
  API (`touring_hooks::errors`, `::classifier`, `::pii`, …) and the root
  re-exports (`pub use classifier::{…}`, `errors::{…}`, `metrics::…`,
  `pii::…`) are byte-identical — verified by `cargo check --workspace --tests`.
- `query_hash_embedding` in `ann_memory` promoted `pub(crate)` → `pub`
  (cross-crate access from the SCC façade after extraction).
- 2 pre-existing latent issues fixed in moved code: clippy
  `explicit_auto_deref` in `mcp_overhead.rs`; `non_snake_case` test fn in
  `idempotency.rs`.

#### Discovered
- **The planned 8-crate split is architecturally infeasible.** The
  `w8_hooks_split_planner.py` v5 forensic output itself reported **4 REAL
  Cargo cycles** between the proposed buckets. Strongly-connected-component
  analysis showed 7 of 10 buckets collapse into ONE SCC
  (`core+lifecycle+tools+infra+cli+misc+shared`), with `core↔lifecycle`
  alone carrying 41 cross-bucket edges. Cargo forbids circular crate
  dependencies — topic-keyword bucketing does not survive contact with the
  real call graph. Approved pivot (Gabriel, 2026-05-15): extract only the
  cycle-free crates now; the SCC stays in `touring-hooks` with internal
  module layering.
- `touring-hooks-rl` (`agentic_rl.rs`) is NOT cleanly extractable — it
  references `crate::HookRuntime` (the planner's `crate::module::` regex
  missed this root-level edge). Deferred — needs a 1-edge dependency
  inversion. W8 ships **3 crates** (façade + 2), not the planned 4.
- 3 of the 19 shared-bucket files (`inventory_registry`, `throttle`,
  `wave3_extended`) reference SCC modules — kept in `touring-hooks`.
  `lib_off.rs` is dead (never `mod`-declared) — left in place, out of scope.

#### Metrics

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Productive crates in hooks layer | 1 | 3 | +2 internal |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `cargo check --workspace --tests` errors | 0 | 0 | 0 |
| New-crate tests | n/a | 294 + 1 doctest PASS | — |
| clippy `-D warnings` (new crates) | n/a | 0 | — |
| New Cargo cycles | — | 0 | 0 |
| W8-introduced regressions | — | 0 | 0 |

### [W7-2026-05-15] — Wave W7: touring-bindings Fusion completed

#### Added
- `touring-bindings` crate — unified language-bindings layer (14,651 LOC,
  84 files) fusing seven crates into the modules `python` (PyO3), `wasm`
  (wasm-bindgen + inferlet runtime), `capnp` (Cap'n Proto RPC), `web`
  (Leptos + Axum, with `web::server` ex touring-web-server), `desktop`
  (Tauri), `postgis` (geozero EWKB).
- 6-feature surface: `bind-python`, `bind-wasm`, `bind-capnp`, `bind-web`,
  `bind-desktop`, `bind-postgis` — all opt-in, `default = []` (tier-free
  builds skip pyo3/wasm-bindgen/tauri/axum compile cost).
- `crate-type = ["rlib", "cdylib"]` on `touring-bindings`.
- `scripts/touring_premium_refactor_2026/w7_rewrite_crate_paths.py`.

#### Changed
- `touring-python`, `touring-wasm`, `touring-capnp-server`, `touring-web`,
  `touring-web-server`, `touring-desktop-ui`, `touring-geopostgis` reduced
  to 1-file shim crates (`pub use touring_bindings::<module>::*`).
- `touring-python` shim preserves `[lib] name = "claude_learning_kernel"` +
  `crate-type = ["cdylib","rlib"]`. `touring-web-server` shim keeps its
  `[[bin]]` (`main.rs` → `touring_bindings::web::server::run`).
- `touring-wasm` / `touring-capnp-server` shims default-enable
  `bind-wasm` / `bind-capnp` (their consumers need the symbols unconditionally).

#### Discovered
- All 7 crates fused with **no exclusion** — verified cycle-safe: the 7
  depend only on {foundation, simd, intelligence, code}; none depends on
  hooks/server/cortex. The `hooks → touring-bindings → {low layers}` DAG
  has no back-edge. (Contrast: W5 excluded touring-index, W6 excluded
  touring-cortex.)
- `cargo test --features bind-python` fails at link (libpython) — inherent
  to PyO3 extension-module crates, not a W7 regression. `bind-wasm` /
  `bind-desktop` need wasm32 / Tauri targets. All deferred to W11.

#### Metrics

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Productive bindings crates | 7 | 1 (+7 shims) | -6 boundaries |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `touring-bindings` tests | n/a | 53 + 1 doctest PASS | — |
| clippy errors (bindings + shims) | n/a | 0 (11 style warnings) | — |
| Wiring cycles (min-depth 2) | 2 | 2 | 0 |
| W7-introduced regressions | — | 0 | — |

### [W6-2026-05-15] — Wave W6: touring-intelligence Fusion completed

#### Added
- `touring-intelligence` crate — unified intelligence layer (63,971 LOC,
  162 files) fusing four crates into the modules `reasoning` (ex
  touring-cognitive), `rl` (ex touring-learning), `ann` (ex touring-antt),
  `index` (ex touring-index).
- 14-feature surface: `analysis-bridge`, `esaa`, `simple-clustering`,
  `leiden-clustering`, `hnsw-working-memory`, `ast-features`, `ftrl`,
  `u4-quantization`, `gpu-compute`, `async-memory`, `semantic-embeddings`,
  `ebpf`, `simd-similarity`, `smart-cache`.
- 15 integration test files + 6 benches relocated into `touring-intelligence`.
- `scripts/touring_premium_refactor_2026/w6_rewrite_crate_paths.py` —
  string/comment-aware Rust path rewriter for the fusion.

#### Changed
- `touring-cognitive`, `touring-learning`, `touring-antt`, `touring-index`
  reduced to 1-file shim crates (`pub use touring_intelligence::<module>::*`)
  for one minor version.
- `touring-index` (deferred from W5) absorbed into `touring-intelligence::index`.

#### Discovered
- **`touring-cortex` NOT fused** — it depends on `touring-hooks` via 6 hook
  handler modules (`pre_read`/`pre_edit`/`pre_bash`/`post_read`/`post_edit`/
  `post_bash`) + `HookRuntime` + `IntentClassifier` + `PIIScanner`; fusing it
  would create the Cargo cycle `touring-intelligence ↔ touring-hooks`. cortex
  is an orchestration-layer crate — deferred to W10 (touring-orchestration).
  W6 fuses **4** crates, not 4+cortex.
- The 621-module macrocycle is **not** eliminated by W6 — it is a
  workspace-wide module-coupling problem spanning ~15 crates, not a
  cognitive/learning issue. No W6 variant addresses it.
- `--no-default-features` has 18 errors — pre-existing feature-gating debt
  from `touring-learning` (bare build never validated upstream); deferred
  to W11.

#### Metrics

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Productive intelligence crates | 4 | 1 (+4 shims) | -3 boundaries |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `touring-intelligence` tests | n/a | 1,758 + 26 doctests PASS | — |
| clippy (intelligence + shims) | n/a | 0 issues | — |
| Wiring cycles (min-depth 2) | 2 | 2 | 0 |
| W6-introduced regressions | — | 0 | — |

### [W5-2026-05-15] — Wave W5: touring-storage Fusion completed

#### Added
- `touring-storage` crate — unified storage layer (6,046 LOC, 36 files)
  fusing five crates into the modules `vfs`, `salsa`, `vec`, `embeddings`,
  `hybrid_search`.
- Feature surface: `storage-vfs`, `storage-vfs-watcher`, `storage-salsa`,
  `storage-vec-{sqlite,qdrant,postgres,mem}`,
  `storage-emb-{candle,fastembed,voyage}`, `storage-hybrid` (heavy backends
  opt-in via `dep:` mapping to internal feature names).
- `scripts/touring_premium_refactor_2026/w5_rewrite_crate_paths.py` —
  generalized string/comment-aware Rust path rewriter (per-module + the
  intra-fusion `touring_*::` cross-crate rewrites for `hybrid_search`).
- 4 integration test files relocated into `touring-storage/tests/`.

#### Changed
- `touring-vfs`, `touring-incremental-salsa`, `touring-vector-store`,
  `touring-embeddings`, `touring-search-fusion` reduced to 1-file shim
  crates (`pub use touring_storage::<module>::*`) for one minor version.
- `voyage` embedding provider fixed — added the missing `#[async_trait]`
  on `impl EmbeddingProvider for VoyageProvider` (pre-existing E0195 bug).
- 8 pre-existing clippy issues fixed in fused `vfs`/`salsa` code (those
  crates lacked `[lints] workspace = true` and were never lint-checked).

#### Removed
- `touring-incremental-salsa` `queries_bench` — dead-on-arrival bench
  (referenced the nonexistent symbol `FileId`, wrong `FileText::new`
  arity, and the nonexistent `Throughput::throughput` API; never compiled).

#### Discovered
- **`touring-index` NOT fused** — fusing it would create the Cargo cycle
  `touring-code → touring-vfs → touring-storage → touring-ast →
  touring-code`. It is an intelligence-layer crate (depends on
  `touring-ast`/`touring-semantics`) and is deferred to W6. W5 fuses **5**
  crates, not 6.
- `qdrant` (11 errors) and `candle-bge` (5 errors) optional backends were
  already broken in the origin crates (API drift) — preserved identically,
  deferred to W11.
- Plan premise "0% tests in search-fusion/salsa" was stale — both already
  had tests (40 + 11 fns).

#### Metrics

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Productive storage crates | 6 | 1 (+5 shims) | -5 boundaries |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `touring-storage` tests | n/a | 141 PASS | — |
| clippy (storage + shims) | n/a | 0 issues | — |
| Wiring cycles (min-depth 2) | 2 | 2 | 0 |
| W5-introduced regressions | — | 0 | — |

### [W4-2026-05-15] — Wave W4: touring-code Fusion completed

#### Added
- `touring-code` crate — unified code intelligence crate (25,754 LOC) fusing
  the `ast`, `polyglot`, `languages`, `semantics` modules.
- Feature flags: `lang-{rust,typescript,python,go,ruby,java,cpp}`,
  `parser-{tree-sitter,ast-grep,syn}`, `semantic-search`, `incremental-salsa`,
  plus inherited `simd-search`/`ann`/`more-languages`/`async-pipeline`.
- `scripts/touring_premium_refactor_2026/w4_rewrite_crate_paths.py` — string-
  and comment-aware Rust mini-lexer that rewrote 119 `crate::` references while
  preserving 16 string-literal `"crate::"` test fixtures in `wiring.rs`.
- 19 integration test files + 6 benches relocated into `touring-code`.

#### Changed
- `touring-ast`, `touring-ast-polyglot`, `touring-language`, `touring-semantics`
  reduced to 1-file shim crates (`pub use touring_code::<module>::*`) — kept for
  one minor version for backward compatibility.
- 2 stale workspace-structure test fixtures corrected (`touring-core` →
  `touring-foundation`, post-W3 rename).

#### Removed
- `crates/touring-ast/src/quality.rs.bak` (36 KB dead file).

#### Metrics

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Productive crates touching code-intel | 4 | 1 (+4 shims) | -3 boundaries |
| `cargo check --workspace` errors | 0 | 0 | 0 |
| `touring-code` tests | n/a | 612 PASS | — |
| Wiring cycles (min-depth 2) | 2 | 2 | 0 |
| W4-introduced regressions | — | 0 | — |

### Planning phase (pre-W0)

### Added
- **2026-05-11** — Forensic architectural audit completed. Findings:
  46 crates, ~410k LOC, macrociclo depth 618, 5 mega-crates (69% código),
  cortex test ratio 0.56%, 4 dead crates. Memory: `audit:touring-arch-premium-refactor-2026-05-11`.
- **2026-05-11** — Decision approved by Gabriel: 13-crate target topology + rustup-like
  per-project deployment + 4-tier commercial model + W14 commercial integration.
  Memory: `decision:touring-premium-roadmap-2026-05-11`.
- **2026-05-11** — Plan structure created at `docs/plans/touring-premium-refactor-2026/`:
  - 00-INDEX.md (master index)
  - 01-ARCHITECTURE.md (full 13-crate breakdown)
  - 02-DEPLOYMENT.md (per-project + toolchain manager)
  - 03-COMMERCIAL.md (tiers + GTM)
  - 04-GLOSSARY.md (terminology)
  - 05-RISKS.md (cross-wave risk register, 70+ risks)
  - 06-METRICS.md (KPIs + quality gates)
  - 07-ROLLBACK.md (per-wave rollback procedures)
  - 08-CONTRIBUTING.md (dev setup + gates + PR template)
  - 09-CHANGELOG.md (this file)
  - W0-prep--safety-net.md through W14-product-tiers--distribution.md (15 wave files)
  - CROSS-AUDIT.md (E2E 10-dimension audit)
- **2026-05-11** — Scripts created at `scripts/touring_premium_refactor_2026/`:
  - generate_plan.py (N1 generator, data-driven)
  - validate_W0.py through validate_W14.py (15 validators, auto-generated)
  - cross_audit_e2e.py (10-dimension cross-validation)

## Template for future entries

### [WX-YYYY-MM-DD] — Wave WX: <Name> completed

#### Added
- <New features, modules, tests>

#### Changed
- <Refactored or relocated items>

#### Deprecated
- <Items scheduled for removal in next major>

#### Removed
- <Items deleted (e.g., dead crates)>

#### Fixed
- <Bug fixes>

#### Security
- <Security patches>

#### Performance
- <Bench results: speedups, regressions within budget>

#### Metrics (per gates)

| Gate | Pre-wave | Post-wave | Delta |
|---|---|---|---|
| Crate count | N | N-X | -X |
| Cycle count | C | C-Y | -Y |
| Workspace test ratio | R% | R'% | +Z% |
| composite_health | H | H' | +Δ |

## SemVer guidance

| Wave | Likely impact | Version bump |
|---|---|---|
| W0-W2 | Internal only, no public API touched | 0.x.y → 0.x.(y+1) |
| W3 | Foundation rename (with shim) | 0.x.y → 0.(x+1).0 |
| W4 | touring-ast → touring-code rename (with shim) | 0.x.y → 0.(x+1).0 |
| W5-W7 | New crates, new features, shims provide compat | 0.x.y → 0.(x+1).0 |
| W8-W10 | Internal splits, façade preserved | 0.x.y → 0.(x+1).0 |
| W11 | Tests only, no API touched | 0.x.y → 0.x.(y+1) |
| W12 | New CLI (touring init, etc.), backward compat via --legacy-global | 0.x.y → 0.(x+1).0 |
| W13 | Publishing infra, no functional changes | 0.x.y → 0.x.(y+1) |
| **W14** | **1.0.0 GA** | 0.(x+1).0 → **1.0.0** |

## References

- SemVer: https://semver.org/
- Keep a Changelog: https://keepachangelog.com/en/1.1.0/
- Conventional Commits: https://www.conventionalcommits.org/en/v1.0.0/
- release-plz: https://release-plz.dev/ (used from W13)
