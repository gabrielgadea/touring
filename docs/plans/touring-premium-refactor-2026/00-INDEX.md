---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
created: "2026-05-11"
type: "index"
files:
  - W0-prep-&-safety-net.md
  - W1-dead-code-purge.md
  - W2-tooling-foundation.md
  - W3-layer-1-2-stabilization.md
  - W4-touring-code-fusion.md
  - W5-touring-storage-fusion.md
  - W6-touring-intelligence-fusion.md
  - W7-touring-bindings-fusion.md
  - W8-touring-hooks-internal-split.md
  - W9-touring-server-internal-split.md
  - W10-touring-orchestration-fusion.md
  - W11-test-debt-repayment.md
  - W12-per-project-deployment.md
  - W13-publishing-pipeline.md
  - W14-product-tiers-&-distribution.md
  - CROSS-AUDIT.md
flags:
  - --sequential-thinking
  - --ultrathink
  - --touring-cli
  - --code-generator
cila: "L4+"
checkpoint_format: ".toon"
total_days_min: 138
total_days_max: 182
---
# touring-premium-refactor-2026 — Touring Premium Refactor 2026

> **Objetivo**: Transformar Touring (46 crates fragmentados, 410k LOC, macrociclo
> depth 618) em produto premium com 13 crates produtivos, per-project deployment
> rustup-like, 4 tiers comerciais, e gates de qualidade não-negociáveis.
> **Total**: 138-182 engineer-days.

## Flags de Execução

`--sequential-thinking` `--ultrathink` `--touring-cli` `--code-generator`

## Waves

| Wave | Nome | Depende De | Mudanças | Dias | Status |
|---|---|---|---|---|---|
| [W0](W0-prep--safety-net.md) | Prep & Safety Net | — | ZERO | 5-7 | DONE (2026-05-11) — plan structure rebuilt (26 markdown 232 KB), 16 python scripts 400 KB, deterministic generator via `generate_w0_premium_artifacts.py`. Memory key: `w0:premium-refactor-plan-structure-2026-05-11` |
| [W1](W1-dead-code-purge.md) | Dead Code Purge | W0 | DELETION | 3-4 | DONE (2026-05-12) — 62 orphan files deleted in `crates/touring-core/src/`. Memory key: `refactor:premium_2026:W3.x_cleanup_done_2026_05_12` |
| [W2](W2-tooling-foundation.md) | Tooling Foundation | W0, W1 | REFACTOR | 4-5 | DONE (2026-05-12 + W2.4 ultrathink 2026-05-14) — W2.1 forward-fixed + W2.3 215 rewrites + W2.4 helper_fn pattern fixed + W2.5-2.8 lints/deny/cargo machete. State: `[workspace.dependencies]` 205 entries. Memory keys: `refactor:premium_2026:W2_complete_2026_05_12` + `refactor:premium_2026:W2.4_ultrathink_inline_complete_2026_05_14` |
| [W3](W3-layer-1-2-stabilization.md) | Layer 1-2 Stabilization | W2 | REFACTOR + ABSORVE | 8-10 | DONE (2026-05-12) — W3.2 phase 1 boundary marker + EmbedderConfig in touring-foundation::embedding. W3.x cleanup of 62 orphan files in touring-core. Memory keys: `refactor:premium_2026:W3.2_phase1_done_2026_05_12` + `refactor:premium_2026:W3.x_cleanup_done_2026_05_12` |
| [W4](W4-touring-code-fusion.md) | touring-code Fusion | W3 | FUSION | 12-15 | DONE (2026-05-15) |
| [W5](W5-touring-storage-fusion.md) | touring-storage Fusion | W3 | FUSION | 10-12 | DONE (2026-05-15) |
| [W6](W6-touring-intelligence-fusion.md) | touring-intelligence Fusion | W3, W4 | MEGA-FUSION | 15-20 | DONE (2026-05-15) |
| [W7](W7-touring-bindings-fusion.md) | touring-bindings Fusion | W3 | FUSION + DELETE | 8-10 | DONE (2026-05-15) |
| [W8](W8-touring-hooks-internal-split.md) | touring-hooks Internal Split | W4, W5, W6, W7 | SPLIT | 15-20 | DONE — pragmatic 3-crate (2026-05-15); 8-crate infeasible (4 Cargo cycles) |
| [W9](W9-touring-server-internal-split.md) | touring-server Internal Split | W8 | SPLIT | 10-12 | DONE — pragmatic 3-crate (2026-05-15); SCC {cli,server,tools} stays |
| [W10](W10-touring-orchestration-fusion.md) | touring-orchestration Fusion | W9 | FUSION | 5-7 | DONE — 3-crate fusion (2026-05-15); decompose/session part superseded by W9 |
| [W11](W11-test-debt-repayment.md) | Test Debt Repayment | W6, W7, W8, W9, W10 | TESTS-ONLY | 5-8 (re-scoped) | SUBSTANTIALLY DONE (2026-05-23) — W11.6 fuzz DONE (8 targets) · B-FUZZ-002 FIXED via Wave 5 mossy-crunching-owl (ast-grep 0.36→0.42.3 + tree-sitter 0.24→0.26 + 2 Go regression tests + S-13/S-14 docs) · W11.4 + W11.2 = advisory baselines (non-blocking) |
| [W12](W12-per-project-deployment.md) | Per-Project Deployment | W11 | ADDITIVE | 1-3 (re-est) | IN PROGRESS (2026-05-23) — **9/12 subtasks DONE + W12.5 wired into daemon**: W12.1 (8/8), W12.2+W12.3 (28/28 cli::toolchain — init/list/default/install/remove), W12.4 (5/5), **W12.5 wired** — `ipc::daemon_socket_path()` agora tem 4-layer chain (TOURING_DAEMON_SOCKET → TOURING_DAEMON_SOCK → walk-up `.touring/daemon.sock` → global `/tmp` fallback). Workspace cargo check 0 errors. W12.6 (4/4), W12.7 (10/10), W12.11 (3 guides 611 LOC), W12.12 (CI template). Pending: W12.5 full daemon spawn detection + W12.8 install.touring.dev + W12.9/10 pilots. |
| [W13](W13-publishing-pipeline.md) | Publishing Pipeline | W12 | DOCS + CI | 8-10 | SKELETON + 4 SLICES (2026-05-23) — **W13.1+W13.2+W13.3+W13.4 done**: README+missing_docs(gap=346)+docs.rs all-features(cargo doc PASS)+cargo-semver-checks PR-gate+**W13.4 cargo-msrv verify** (Job 7, foundation rust-version=1.75 drift vs workspace=1.80 documented for separate wave). YAML valid: 7 jobs (build-and-test, shellcheck-shim, lint, shim-e2e, msrv-check, semver-checks, docs-lint). Roadmap for W13.5-W13.6 in `W13-SKELETON-2026-05-23.md`. |
| [W14](W14-product-tiers--distribution.md) | Product Tiers & Distribution | W13 | ADDITIVE + DISTRO | 10-15 | SKELETON + **8 SLICES** done (2026-05-23) — `touring-license` crate (16/16 tests): W14.1 Tier enum + 4 `tier-*` features, W14.2 License parse + grace, W14.3-W14.6 partial policy fns. **W14.7 partial** `scripts/packaging/install.touring.dev.sh` (POSIX sh, --dry-run + OS/arch detection + canonical install plan, shellcheck clean). **W14.8 partial** Homebrew Formula `scripts/packaging/homebrew/touring.rb` (macOS arm/intel + Linux arm/intel) + Scoop manifest `scripts/packaging/scoop/touring.json` (Windows x64+arm64, autoupdate). **W14.9 partial** Docker `scripts/docker/Dockerfile.{distroless,alpine}` (~80 MiB / ~15 MiB final). All artifacts placeholder URLs/hashes — activated by W13.6 release-plz + W13.5 sigstore. JWT crypto verify + commercial decisions remain blockers. |

---

## Discovery Updates (2026-05-11) — Plan Adjustments After Forensic Scripts

A wave de auto-scripts executados em 2026-05-11 (16 sub-scripts REAL impl + 70 pytest tests) trouxe descobertas que alteram o plano original. Cada wave doc tem sua própria `## Discovery Updates` section detalhada.

### Resumo cross-wave

| Wave | Descoberta | Impacto |
|---|---|---|
| **W1** | 4 KNOWN_DEAD crates confirmados (0 LOC ou 0 consumers) | ✅ Plano confirmado |
| **W2** | **135 unique external deps + 12 version conflicts** descobertos | ⚠️ Pre-W2.1 dedup obrigatório |
| **W3** | Top sub-mods consumidos: `TouringConfig=45 uses`, `TouringError=42`. Estimate: 1.45 engineer-days | ✅ |
| **W3.2** | Anemic absorption tem **overlap 100% com W1** — todos candidates já KNOWN_DEAD | ⚠️ **W3.2 pode ser removido** ou re-escopado |
| **W4** | **77 consumer files** (2× expected ~38). touring-ast = 68 files em 11 crates | ⚠️ Re-estimate W4 hours |
| **W5** | Top 3 storage hotspots: `reasoning/persistence.rs` (60 hits), `hooks/knowledge.rs` (59), `server/tools_infra.rs` (51) | ✅ Skeleton ready |
| **W6.0** | **Premissa "0.56% test ratio" estava errada** — cortex JÁ tem 236% pub-ratio e 73% loc-ratio | ⚠️ **W6.0 pode ser removido** do critical path |
| **W7** | 12 multi-feature crates discovered for powerset validation | ✅ Skeleton + validator ready |
| **W8** | 4 iterações (v1→v4) — descoberta: misc residual = shared types implícitos. **8º bucket** `touring-hooks-shared` necessário | ⚠️ Plan ajustado: 7→**8 sub-crates** |
| **W9** | Mirror do W8 — mesma estratégia + 6 sub-crates | ✅ |
| **W11** | **Re-medido 2026-05-15** (`cargo llvm-cov`): intelligence 83%, foundation 78% cov — premissas "15%" stale. bindings feature-gated (`default=[]`, 185 tests). proptest 89 (>50 já). fuzz 0/8 = gap real. | ⚠️ **RE-SCOPED 10-15→5-8d** — W11.1/W11.3 obsoletos, W11.5 atingido; ver `W11-*.md` Discovery Updates |

### Adjustments no total_days

- **W3**: original 8-10 dias → revisado **5-7 dias** (W3.2 removido/re-escopado)
- **W6**: original 15-20 dias → revisado **10-13 dias** (W6.0 removido)
- **W8**: original 15-20 dias → revisado **18-23 dias** (8 sub-crates, não 6)

- **W11**: original 10-15 dias → revisado **5-8 dias** (re-medição 2026-05-15: W11.1/W11.3 obsoletos — cov já 77-83%; W11.5 já atingido)

**Total revisado**: **128-169 engineer-days** (original 138-182, -10 dias por escopo refinado em W3/W6/W11)

### Sub-scripts disponíveis (16 REAL impl + 22 stubs)

```bash
# W0
python3 scripts/touring_premium_refactor_2026/w0_snapshot_tar.py
python3 scripts/touring_premium_refactor_2026/w0_capture_baselines.py
# W1
python3 scripts/touring_premium_refactor_2026/w1_audit_dead_code.py
python3 scripts/touring_premium_refactor_2026/w1_clean_reexports.py
python3 scripts/touring_premium_refactor_2026/w1_fix_cycle1.py
# W2
python3 scripts/touring_premium_refactor_2026/w2_centralize_workspace_deps.py
python3 scripts/touring_premium_refactor_2026/w2_propagate_workspace_inherit.py
# W3
python3 scripts/touring_premium_refactor_2026/w3_rename_core_to_foundation.py
python3 scripts/touring_premium_refactor_2026/w3_absorve_anemic_crates.py
# W4
python3 scripts/touring_premium_refactor_2026/w4_consumer_audit.py
python3 scripts/touring_premium_refactor_2026/w4_consumer_migrate.py
# W5
python3 scripts/touring_premium_refactor_2026/w5_storage_skeleton.py
# W6
python3 scripts/touring_premium_refactor_2026/w6_cortex_test_debt_repay.py
# W7
python3 scripts/touring_premium_refactor_2026/w7_bindings_skeleton.py
python3 scripts/touring_premium_refactor_2026/w7_features_powerset_check.py
# W8
python3 scripts/touring_premium_refactor_2026/w8_hooks_split_planner.py --emit-cargo --emit-evidence
# W9
python3 scripts/touring_premium_refactor_2026/w9_server_split_planner.py --emit-evidence
# W11
python3 scripts/touring_premium_refactor_2026/w11_mutation_kill_rate_audit.py --cache-only
```

### Test coverage atual

- 70 pytest tests em `scripts/touring_premium_refactor_2026/tests/`
- All passing em < 0.20s
- 5 sub-scripts com test suite (w0_snapshot, w0_capture, w1_audit, w1_fix, w2_centralize)
