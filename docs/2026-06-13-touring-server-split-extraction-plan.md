# touring-server Split — Extraction Plan (touring-cli-app + touring-mcp + touring-server-core)

> **Authored**: 2026-06-13 (TACO, `/goal` continuation; Gabriel chose "Split touring-server")
> **Level**: L4 XL crate extraction (point-of-no-return physical move) | **Verdict**: multi-session, ~7 engineer-days
> **Scout**: code-explorer agent, VP-Scout file:line-verified (read-only) | **Report items**: A1 / A6 / SEC#1
> **Precedent**: `daemon-lib-rearch` + W9 (`touring-server-{reasoning,visual,session}`) + `touring-ceg` (recursive-cuddling-blossom.md)
> **⚠ PREREQUISITE (R6)**: the concurrent `mcp-curated` default flip (W2, in-progress) MUST land first — do NOT run the split and the mcp-curated migration in the same session.

---

## Goal

Split `touring-server` (~67.9k LOC, ~160 files, the workspace's largest crate; binary name `touring`) into:
- **`touring-cli-app`** (~38.5k) — the `touring` binary + all `cli/` subcommand handlers (pure daemon-socket RPC clients).
- **`touring-mcp`** (~26k) — the MCP server (`TouringServer`, 42+ `#[tool]`s, ingest, graph_service, plugins).
- **`touring-server-core`** (~4.5k, new) — shared infra used by both (telemetry_init, memory_store, context_compiler, output, projects, agent_diary, knowledge_adapter, observation_masker). The old `touring-server` package becomes a thin re-export façade (W9 precedent).

## Verified baseline (file:line evidence)

- Binary dispatch: `src/main.rs:108-113` (comm-name), `:215-261` (serve vs CLI), `:163` (tokio runtime built unconditionally), `:49-50` (`#[global_allocator]`).
- **Only** server→cli coupling: `src/server/tools_core.rs:2` + `src/server/tools_activity.rs:10` import `crate::cli::daemon_query`. `grep "crate::cli" src/server/` is otherwise empty.
- Zero cli→server imports (confirmed by grep).
- Tool-router merge (10 sub-routers): `src/server/mod.rs:419-451`; mcp-curated gate `:444-449`, `tools_new.rs:15`, `tools_status.rs:32`.
- `system_info` inline module with `server::tools_*` deps: `src/lib.rs:20-156` (must move to own file → touring-mcp).
- **Zero out-of-crate `use touring_server::`** in non-server workspace crates (only comments / mirror-pattern). External isolation is clean → the façade re-export suffices; no external test edits needed.
- W9 façade precedent: root `Cargo.toml:211-213`, `lib.rs:185-197`. `#![deny(missing_docs)]` at `lib.rs:4`.

## Session A — reversible seam inversion (single binary preserved, fully reversible)

- **A1** (highest value): extract `daemon_query` + `daemon_socket_path`/`libc_getuid`/retry from `src/cli/mod.rs:155` into new `src/daemon_client.rs`; `pub mod daemon_client` in lib.rs; repoint the 2 `server/` imports + `cli/common.rs:6`. **Gate**: `grep "crate::cli" src/server/` → empty (cli/ ↔ server/ fully decoupled).
- **A2**: `src/shared/mod.rs` re-exporting the 8 future-core modules (compile-time manifest of the boundary).
- **A3**: confirm `mcp-legacy`/`mcp-curated` feature guards carry to touring-mcp (no-op now).
- **A4/A5**: annotate `pub mod cli` (`lib.rs:213`) and the allocator decl (`main.rs:49-50`) with `// MOVE-TO:` markers; dry-run verify allocator under `--no-default-features --features jemalloc`.
- **A6**: move `system_info` (`lib.rs:20-156`) to its own file; annotate `// MOVE-TO: touring-mcp`.
- **End-of-A gate**: `cargo check --workspace` 0; `grep "crate::cli" src/server/` empty. Reversible rollback point.

## Session B — physical move (POINT OF NO RETURN; each step `cargo check --workspace`-gated)

- **B1**: `taco-forge perfect-create-crate touring-server-core`; copy the 8 shared modules; `cargo check -p touring-server-core`.
- **B2**: touring-server depends on core; replace module bodies with `pub use touring_server_core::<m>;`. `cargo check --workspace` 0.
- **B3**: `taco-forge perfect-create-crate touring-mcp`; move `server/`, `tools/`, `ingest/`, `graph_service.rs`, `plugins/`, `rl_mapping.rs`; replicate `rmcp` + feature deps + mcp-legacy/curated. `cargo check -p touring-mcp`.
- **B4**: `taco-forge perfect-create-crate touring-cli-app`; move `cli/`, `main.rs`, `daemon_client.rs`; `[[bin]] name="touring"`. `cargo check -p touring-cli-app`.
- **B5**: resolve serve coupling — add `pub async fn run_mcp_server()` to touring-mcp; cli-app calls it (cli-app depends on touring-mcp → carries the heavy dep tree, acceptable since the binary already does).
- **B6**: thin `touring-server` façade re-exporting all three; remove moved sources. `cargo test --workspace` full.
- **B7**: move `[[bin]] name="touring"` from touring-server (`Cargo.toml:9-11`) to touring-cli-app; add to workspace members; verify `target/debug/touring` exists.
- **B8**: update `update-touring` / CI build invocations (`cargo build -p touring-server` → `-p touring-cli-app`); binary path `target/release/touring` unchanged → symlinks/settings.json untouched.

## Re-export strategy (external consumers)

Zero external `use touring_server::` imports exist (scout-verified). The 6 in-crate test files + bench resolve via the façade re-exports — **no test-file edits required**. Façade: `pub use touring_server_core::*; pub use touring_mcp::{server, tools, system_info};` + per-module re-exports matching the current `lib.rs` surface.

## Risk register

| # | Risk | Sev | Mitigation |
|---|---|---|---|
| R1 | Binary name test coupling (6 E2E spawn `target/{debug,release}/touring`) | HIGH | add touring-cli-app to workspace default-members; update CI `cargo build -p` |
| R2 | `graph_service_e2e` hang (pre-existing — NEVER run) | HIGH | preserve `#[ignore]`/exclusion; don't surface in new context |
| R3 | 30+ feature matrix (mutually-exclusive allocators `main.rs:37-42`) | MED | feature-matrix diff old vs new before declaring complete; allocators land ONLY in cli-app |
| R4 | `system_info` inline w/ server deps | LOW | A6 moves to own file; façade re-export |
| R5 | `daemon_query` server→cli coupling | LOW | resolved in A1 |
| R6 | concurrent mcp-curated flip (today) | MED | **complete mcp-curated first**, then split |
| R7 | `#![deny(missing_docs)]` on new crates | LOW | add to each new lib.rs from day one |
| R8 | `touring_wasm::MAX_FUEL` unconditional import (`server/mod.rs:62`) under `wasm-plugins` | MED | verify cfg-gate before B3; declare touring-wasm dep on touring-mcp |

## Effort + session strategy

| Session | Work | ed | Gate |
|---|---|---|---|
| 0 (pre-req) | finish mcp-curated default flip | 0.5 | `cargo check --workspace` 0 |
| A | seam inversion A1-A6 (reversible) | 1 | `grep crate::cli src/server/` empty |
| B1 | touring-server-core | 1 | `cargo check --workspace` 0 |
| B2 | touring-mcp (most complex) | 2 | `cargo check -p touring-mcp` 0; 10 routers present |
| B3 | touring-cli-app + binary | 1 | `target/debug/touring` exists; binary e2e pass |
| B4 | façade + cleanup | 1 | `cargo test --workspace` pass |
| B5 | CI + update-touring | 0.5 | `update-touring` exit 0; `doctor` 5/5 |

**Total ~7 ed. Multi-session (3+ dedicated sessions).** Single-session is rejected: 160 files, 30+ feature matrix, 6 binary-name-coupled E2E tests, and every step must keep a working binary via incremental `cargo check` — would overrun a single context.

**Hardest parts**: (1) feature-matrix replication without tripping `compile_error!` allocator guards; (2) `system_info` inline-module relocation; (3) the `run_serve` binary/MCP coupling (B5) — cli-app ends up nearly as heavy as the original binary.
