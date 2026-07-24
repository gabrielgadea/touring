# Phase 3: Testing & Documentation — Consolidated

> Detail: `03a-testing.md` (F3.1–F3.7) · `03b-documentation.md` (F3.8–F3.13). Testing authored by orchestrator (test-automator subagent thrashed context ×2 — gathered via capped CLI). Docs by subagent + **in-session remediation R2**.

## Testing (F3.1–F3.7) — 0 Critical · 2 High · 3 Medium · 2 Low

Pyramid ✅ ELITE: 11,666 unit · 177 integration · 89 e2e · 30 criterion benches · 5 loom proofs.

| # | Sev | Finding | Evidence | Fix |
|---|-----|---------|----------|-----|
| T1 | **High** | Untrusted-input parsers (ast-polyglot, ast, **rkyv IPC**) have **0 proptest, 0 fuzz** workspace-wide | `grep proptest!`=0; `find -name fuzz`=0 | cargo-fuzz + proptest roundtrip on polyglot parser + rkyv decode |
| T2 | **High** | CI gates only `cargo test --workspace --lib` (`ci.yml:80`) — 177 integration + 89 e2e + doctests NOT run in CI; `nextest.toml` exists but unused | `ci.yml:79-80` | CI job: `cargo nextest run --workspace --no-fail-fast` + `cargo test --doc` |
| T3 | Medium | p99 guards exist (8 asserts) but none on `hook_dispatch_latency` (the P-1 199ms path) | `latency_p99_guard.rs`, `predictive_wave_p99_guards.rs` | `hook_dispatch_p99_guard` <50ms |
| T4 | Medium | SEC-02 web bind has no negative security test | `touring-bindings/tests/` listing | assert loopback-default + 401-on-unauth |
| T5 | Medium | cargo-mutants advisory, no score floor | `Cargo.toml:113` "(advisory)" | mutation-score floor on critical crates |
| T6 | Low | 1 `#[ignore]` masks a real deferred bug (`cli_decompose_ready` Wave-8 shape) | ignore reason "needs investigation" | fix + re-enable or track |

**Verified-elite:** healthy pyramid · 30 benches + baseline regression gate · **5 loom proofs** (model-checking) · SEC-01 5 negative tests · CI clippy `--all-targets -D warnings`.

## Documentation (F3.8–F3.13) — 0 Critical · 3 High (**ALL REMEDIATED in-session R2**) · 4 Medium · 3 Low

| # | Sev | Finding | Status |
|---|-----|---------|--------|
| D1 (was DOC-ARCH-1) | High | ARCHITECTURE.md self-contradictory (45 vs 49 crates), `sync_metrics --check` RED | ✅ **R2 DONE** — `sync_metrics --sync` → "OK crates=45 loc_src=537343 in sync" |
| D2 (was DOC-ACC-1) | High | Composite **measured Gold 0.8856, not Diamond** (`06_documentation=0.00 FAIL`) | ✅ **R2 DONE** — `gen_reference.py` → `06_documentation=1.00 PASS`, composite **Diamond 0.9703** restored |
| D3 (was DOC-ACC-2) | High | `gen_reference --validate` → modules.md out of sync | ✅ **R2 DONE** — regenerated 164 mcp + 218 hooks + 337 modules, "OK in sync" |
| D4 | Medium | No ADRs + 0 mermaid/C4 in ARCHITECTURE.md; CONTRIBUTING.md:43 points to non-existent `docs/rfcs/` | open | add ADR dir (MADR) + C4 mermaid; fix the dead pointer |
| D5 | Medium | Doctests (34 `rust` examples) never run in CI | open (overlaps T2) | `cargo test --doc` in CI |
| D6 | Medium | README hook-count self-contradicts (198 vs 218 vs 140) | open | single-source from `gen_reference` (218 authoritative) |
| D7 | Low | CHANGELOG top zone = checkpoint dump, not Keep-a-Changelog; A2/A5 + schemars 0.8→1.2 have no consumer migration entry | open | Keep-a-Changelog + migration note |

**Verified-strong:** `sync_metrics.py` metrics-as-code drift gate wired into CI = **genuine USP** · **47/48 lib crates enforce `#![deny(missing_docs)]`** · Diátaxis docs + 124-file `rust/docs/` + 5 RFCs + Constitution · substantive SECURITY.md/CONTRIBUTING.md · README structurally best-in-class.

## R2 remediation note (honesty)
The composite had regressed Diamond→Gold during this session: the dead-file removal (Q1) shifted the module inventory, which — together with pre-existing ARCHITECTURE.md crate-count staleness — tripped the two doc-sync block gates. **Fixed in-session (REGRA #21) via the repo's own no-code tools** (`sync_metrics --sync` + `gen_reference.py`): composite back to **Diamond 0.9703**, both doc gates PASS. The structural recommendation (keep `sync_metrics --check` + add `gen_reference --validate` binding in CI — the former is already at `ci.yml:88`) stands.

## Critical issues for Phase 4 (Best Practices & CI/CD) context
- **CI under-gates tests (T2)** — biggest CI/CD finding; integration+e2e+doctests not run.
- **Missing `rust-toolchain.toml`** (MSRV pin — RBP-04) + `rustfmt.toml`/`clippy.toml`/`CODEOWNERS`.
- **release.yml present** but repo-publish/tag is a Gabriel action (B-W1) — most crates `publish=false` (no semver-governable public API yet).
- **231 `Result<_,String>`** (RBP-03 incomplete) — SDK-readiness blocker.
- **The lint ratchet IS elite** (`[workspace.lints]` clippy::all=deny + RBP-11) — credit, don't re-flag.
- **schemars pin (A1/SEC-06)** — the one open supply-chain hygiene item.
