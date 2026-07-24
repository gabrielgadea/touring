# Touring Productization — Pln2 · Progress Index

> **Plan**: `~/.claude/plans/giggly-drifting-kahn.md` (6-phase arc: singleton global → installable, versioned, per-project system)
> **Validator per phase**: `validate_phaseN.sh` in this directory (cross-audit gate, run after each phase's deploy)

## Phase status

| Fase | Título | Status | Data | Evidência |
|---|---|---|---|---|
| **0** | Fundação: desacoplar fonte canônica + version-pin | ✅ **DONE** | 01/07/2026 | `validate_phase0.sh` (5 checks) + gates verdes |
| **4′** | Repo canônico `~/projects/touring` (**move-first**, D1 aprovado) | ✅ **DONE** | 24/07/2026 | `validate_phase4.sh` **8/8 PASS** · e2e 0.8749 PASS do novo root · daemon/symlinks no novo binário · 93 configs co-evoluídos (backup em `backups/`) · `~/.claude/rust` congelado (`FROZEN-2026-07-24.md`) · DAG `task_1784891751372651360` |
| **1** | Daemon multi-instância per-project (lock per-socket) | ✅ **DONE** | 24/07/2026 | `validate_phase1.sh` **8/8 PASS** · E2E 2/2 (RED→GREEN) · unit 4/4 · clippy 0 · pid file REGRA #19 vivo · DAG recriado `task_1784899384232534031` (gotcha flush 2×, → F1-FLUSH) · `phases/F1.md` |
| **2** | Lifecycle: popular `.touring/bin` + ativar hook shim | ✅ **DONE** | 24/07/2026 | `validate_phase2.sh` **7/7 PASS** · `populate_bin` (toolchain pinada → dev fallback, 11/11) · shim W12.6 **ATIVADO** em `~/.claude/hooks/touring-hook` (layers 2/4 provadas TRACE) · `phases/F2.md` |
| **3** | `touring update` + `touring component` (propagação) | ✅ **DONE** | 24/07/2026 | `validate_phase3.sh` **8/8 ALL PASS** (binário deployado) · RED→GREEN E2E 5/5 · lib 1416/1416 · clippy 0 · 50-dim 0.898-0.953 (project_toolchain 💎) · 6 P0 Pass · lockfile requested-vs-resolved + `--rollback` determinístico · `--from-source`/`--from-url` · daemon per-project respawn no binário novo · `phases/F3.md` |
| **PILOT** | Piloto konverter (D3) — 1º projeto per-project | ✅ **DONE** | 24/07/2026 | `validate_pilot.sh` **9/9 ALL PASS** · `~/.touring` criado do zero + toolchain 30.3.0 `--from-source` (imutável) · konverter pinado, shim→project_bin em sessão real, daemon próprio no binário pinado · **bug real corrigido**: spawn per-project agora pina root derivado do socket (contaminação cruzada eliminada, provado via `/proc`) · finding: `doctor project_db` é client-side · CLAUDE.md camadas 2+3 co-evoluídos · `phases/PILOT.md` |
| **5** | Distribuição & versionamento GA | ✅ **DONE** (metade não-git) | 24/07/2026 | `validate_phase5.sh` **10/10 ALL PASS** · `install.touring.dev.sh` ATIVADO (3 fontes, sha256 fail-closed, cosign pre-GA warn, tamper/bogus recusados) · `package_release.sh` + release.yml layout `bin/` CI-idêntico · cargo-deny remediado REAL (crossbeam-epoch CVE + 2 spin yanked) · deploy + konverter propagado via `touring update` (dogfooding) · **git boundary Gabriel**: promover `release-plz.yml`/`docs-rs-mirror.yml`, tag SemVer, publicar artefatos + DNS · ASK: licença comercial (5.3) · `phases/F5.md` |

> **Human gate 24/07/2026**: D1 move-first ✅ · D2 `~/projects/touring` ✅ ·
> D3 piloto konverter ✅ · D4 `~/.claude/rust` congelado até E2E (descarte =
> decisão futura de Gabriel). Execução detalhada: `log.md`.

## Fase 0 — o que foi entregue (01/07/2026)

- **0.1 Hardcodes runtime desacoplados** (`TOURING_WORKSPACE_ROOT` env → fallback histórico):
  - `touring-storage/src/knowledge_wiring.rs` + `touring-hooks-core/src/knowledge_wiring.rs`: `WORKSPACE_ROOT_MARKER` const → `workspace_root_marker()` (OnceLock, trailing-`/` garantido)
  - `touring-cli/src/cli/gotcha.rs` (default gotcha dir) + `touring-hook-handlers/src/hooks/session_hooks.rs` (session-start gotcha sync)
  - **+1 site descoberto além do plano**: `touring-server/src/cli/profile.rs` `PARCER_SCHEMA` → `parcer_schema_path()` (schema vive no workspace e move com a fonte; `PARCER_DIR` fica — é user config)
- **0.2 Version-pin per-project**: `[toolchain] channel = "30.3.0"` no `DEFAULT_TOURING_TOML` (init-project) + campo `TouringConfig.toolchain: Option<ToolchainPin>` lido por `detect_layered` (unit test `test_layered_reads_project_toolchain_pin`)
- **0.3 Versão única**: `[workspace.package] version = "30.3.0"` (era 0.1.0); `touring-server` herda via `version.workspace = true` (era 30.0.0 próprio); binário deriva `CARGO_PKG_VERSION`; OTel scope `touring.ceg.gate_metrics` idem; 2 deps internas com pin `0.1.0` redundante removido (`touring-ast`, `touring-ast-polyglot`)
- **Gates**: cargo check 0 · testes 6 crates 0 fail · clippy `-D warnings` 0 · fmt 0 · 50-dim 8/8 ≥ Platinum (4 Diamond) · 6 P0 Pass

## Próxima sessão

Fase 1 (daemon multi-instância): começar pelo RED test `w12_5_per_project_daemon_e2e.rs` (2 TempDir-projects → 2 sockets), depois per-socket lock em `ipc.rs::daemon_lock_path()`. Opt-in `[daemon] per_project=true` default OFF — zero disrupção do vivo.
