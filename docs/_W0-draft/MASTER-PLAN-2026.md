# MASTER-PLAN-2026 — Touring Premium Refactor

> **Status**: Proposed | **Date**: 2026-05-11 | **Authors**: Gabriel Gadea (architect) + TACO (orchestrator)
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
| W1.2 | DELETE `touring-wasm-{client,common,server}` (0 LOC each) | 0.5 |
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
| W4.6 | Define features: `lang-{rust,typescript,python,go,ruby,java,cpp}` + `parser-{tree-sitter,ast-grep,syn}` | 0.5 |
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
| W5.7 | Features: storage-{fts, vec-sqlite, vec-qdrant, vec-mem, emb-candle, emb-fastembed, emb-voyage, vfs-mem, vfs-disk, salsa} | 1 |
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
| W14.1 | Tiers as Cargo features (tier-{free,standard,premium,enterprise}) | 2 |
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
- Baselines: `docs/baselines/{wiring,cycles,status,workspace-info,cargo-check}-pre-refactor-2026-05-11.{json,log}`
- Snapshot: `docs/baselines/touring-snapshot-pre-refactor-2026-05-11.tar.gz` (97 MB, SHA-256 0b3934ce…)
