# Phase 4: Best Practices & CI/CD — Consolidated

> Detail: `04a-rust-bestpractices.md` (F4.1–F4.6) · `04b-cicd-devops.md` (F4.7–F4.12). Severity normalized by orchestrator (4B over-escalated T2 to Critical; recalibrated to High — the full local suite is green, CI under-gating is serious but not P0 data-loss/exploit).

## Rust & Language (F4.1–F4.6) — 0 Critical · 1 High · 2 Medium · 3 Low

| # | Sev | Finding | Evidence | Fix |
|---|-----|---------|----------|-----|
| BP1 | **High** | **schemars 0.8↔1.2 duplicate** (= A1/SEC-06) | `touring-harness-mcp/Cargo.toml:21` direct `schemars="0.8"` vs workspace `Cargo.toml:257` `="1.2"`; `rmcp 1.2` schemars feature (line 20) transitively locks 0.8 → `cargo deny check bans` live-FAIL, not in skip-list | `schemars = { workspace = true }`, or drop rmcp's schemars feature if unused, or skip-list quarantine (W13 pattern) |
| BP2 | Medium | `rust-toolchain.toml` missing | (MSRV itself IS declared: `Cargo.toml:147 rust-version=1.85`) | add toolchain-pin file for reproducible CI |
| BP3 | Medium | `rustfmt.toml`/`clippy.toml` absent (relies on defaults) | repo root | pin `clippy.toml msrv=1.85` + `rustfmt edition=2024` |
| BP4 | Low | `CODEOWNERS` missing · `resolver="2"` under edition-2024 (default "3", unexplained) | `Cargo.toml` | add CODEOWNERS; document/justify resolver |

**Verified-elite:** edition **2024 uniform** (45 crates) · release profile LTO=fat/opt=s/strip/panic=abort · dev profile REGRA #12-defensive · `[workspace.lints]` deny ratchet · `block_on` discipline (correct `block_in_place` at `server/mod.rs:368`) · `#[async_trait]` correct (dyn object-safe) · 2 own `#[deprecated]` (proper since/note, 0 consumed). **Baseline corrections:** LICENSE-APACHE present (00 was stale); MSRV declared (only the toolchain *file* missing).

## CI/CD & DevOps (F4.7–F4.12) — 0 Critical · 2 High · 5 Medium · 4 Low

| # | Sev | Finding | Evidence | Fix |
|---|-----|---------|----------|-----|
| CD1 | **High** | **CI gates only `cargo test --workspace --lib`** — 177 integration + 89 e2e + 34 doctests NOT run (= T2) | `ci.yml:79-80` | add `cargo nextest run --workspace --no-fail-fast` + `cargo test --doc` job |
| CD2 | **High** | schemars bans failure surfaces in CI deny step (= BP1/A1/SEC-06) | `cargo deny check bans` | one-line dep unify |
| CD3 | Medium | No `actionlint`; verify all actions SHA-pinned; incident runbooks incomplete | `.github/workflows/` | actionlint job; runbook dir |
| CD4 | Medium | Public-release readiness: most crates `publish=false`; release awaits Gabriel repo-publish+tag (B-W1) | `release.yml` present (musl+macOS aarch64, checksums, SBOM, smoke, cosign OIDC + SLSA) | tag `v*` when ready |

**Verified-elite (USP-class):** **cargo-deny LIVE** (advisories pass with 4 *documented* ignores: bincode-unmaintained, instant, pyo3-optional, proc-macro-error2-dev) · **GitHub Actions least-privilege + cosign OIDC + SLSA attestation** · **11/13 elite gates wired in CI** (sync_metrics, file_size, wiring_integrity, perf_p99 hdrhistogram, scalability, extensibility, ux, craftsmanship, elite_aggregate) · **`touring gate-metrics` observability** (real hdrhistogram p99, ~250 `tracing::` instrumentation points, live counters) · graceful degradation (CI fail-open when touring absent) · **CEG circuit-breaker + graceful shutdown** (flushes WAL/LinUCB/CRDT — partial auto-recovery) · `deny.toml` 23KB curated · env credential-strip (`ENV_ALLOWLIST`).

## Convergence
- **schemars** = A1 = SEC-06 = BP1 = CD2 — **one 1-line fix** clears an architecture finding, a supply-chain finding, a Rust-deps finding, and a CI red. Highest leverage-per-effort in the review.
- **CI under-gating (CD1/T2)** is the single biggest "claims vs enforced" gap: the suite is green locally but CI only proves `--lib`.
