---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
type: "rollback"
created: "2026-05-11"
---
# 07-ROLLBACK — Rollback Procedures

> **REGRA #11**: Git é PROIBIDO neste workspace. Rollback usa snapshots
> Touring + tar archives + cargo trustworthy state restoration.
> **Constraint absoluto**: NUNCA `git stash`, NUNCA `git reset --hard`.

## 1. Pre-refactor snapshot (W0.1)

```bash
# Captured during W0
SNAPSHOT=docs/baselines/touring-snapshot-pre-refactor-2026-05-11.tar.gz
SHA256=$(cat docs/baselines/touring-snapshot-pre-refactor-2026-05-11.sha256)

# Full restoration (LAST RESORT)
sha256sum -c docs/baselines/touring-snapshot-pre-refactor-2026-05-11.sha256
tar -xzf "$SNAPSHOT" -C /tmp/restore/
rsync -avz --delete /tmp/restore/.claude/rust/crates/ ~/.claude/rust/crates/
# then: cargo check --workspace
```

## 2. Per-wave rollback procedures

### W0 — Prep & Safety Net
- **State changed**: docs/baselines/, docs/plans/, scripts/touring_premium_refactor_2026/
- **Rollback**: `rm -rf docs/plans/touring-premium-refactor-2026 scripts/touring_premium_refactor_2026 docs/baselines/touring-*` + delete checkpoint
- **Reversibility**: TRIVIAL (zero code changes)

### W1 — Dead Code Purge
- **State changed**: 4 crates deleted from workspace + Cycle #1 fixed
- **Rollback**: tar -xz dos diretórios deletados de pre-refactor snapshot + revert Cargo.toml members
- **Reversibility**: TRIVIAL (crates eram 0 LOC ou archived; cycle fix isolado)

### W2 — Tooling Foundation
- **State changed**: Cargo.toml workspace.dependencies + workspace.package + workspace.lints + deny.toml + machete.toml + CI workflow
- **Rollback**: Substituir Cargo.toml por backup + apagar deny.toml/machete.toml + revert workflow YAML
- **Reversibility**: MÉDIA (42 crate Cargo.toml mudaram para .workspace = true — reverter via sed pode poluir)
- **Mitigation**: backup explícito de cada Cargo.toml em scripts/touring_premium_refactor_2026/staging/w2-backup/

### W3 — Layer 1+2 Stabilization
- **State changed**: touring-core renomeado para touring-foundation + 5 crates absorvidos + tests +4.5k LOC
- **Rollback**: Restaurar via snapshot dos 6 crates absorvidos + reverter rename
- **Reversibility**: ALTA mas tem custo (precisa de full snapshot pré-W3)
- **Pre-wave checkpoint**: tar archive em docs/baselines/pre-W3.tar.gz (criar antes de W3.1)

### W4 — touring-code FUSION
- **State changed**: touring-code criado (26k LOC) + 4 crates deletados + 38 consumers updated + bench baseline atualizado
- **Rollback**: Restaurar 4 crates source + reverter 38 consumer imports + delete touring-code
- **Reversibility**: ALTA via shim crates (W4 mantém shims `pub use touring_code::ast::* as touring_ast` por 2 versões → reverter é "delete shim, restore old crate from snapshot")
- **Pre-wave checkpoint**: tar archive obrigatório (W4 é fusão grande)

### W5 — touring-storage FUSION
- **Similar a W4**: shim crates + pre-wave snapshot. Custo médio.

### W6 — touring-intelligence FUSION (HIGHEST RISK)
- **State changed**: touring-intelligence criado (90k LOC) + cortex test debt repagado + 4 crates absorvidos + 12 consumers updated + macrociclo 618 eliminado
- **Rollback**: COMPLEXO. Test debt repayment (W6.0) é additive, fica. Mas fusão dos 4 crates requer restore completo de cognitive/cortex/learning/antt + reverter 12 consumers + reverter wiring.
- **Pre-wave checkpoint**: tar archive OBRIGATÓRIO em docs/baselines/pre-W6.tar.gz
- **Fallback strategy**: Se W6 falha, pausar e revisar. NÃO descartar trabalho de cortex test debt (W6.0) — ele beneficia tudo.

### W7 — touring-bindings FUSION
- **Similar a W5/W4** mas com 0% test ratio absorvidos exigem tests adicionados em W7. Rollback similar.

### W8 — touring-hooks INTERNAL SPLIT (CRITICAL — CC integration)
- **State changed**: 6 sub-crates criados + 224 files realocados + 32k tests redistribuídos + façade reexports
- **Rollback**: Restaurar touring-hooks/src/ monolítico via snapshot + delete 6 sub-crates
- **Reversibility**: MÉDIA (façade preserva API mas internal layout muda muito)
- **Pre-wave checkpoint**: tar OBRIGATÓRIO
- **Production gate**: Antes de declarar W8 complete, smoke test 24 hook events em sessão TACO real

### W9 — touring-server INTERNAL SPLIT
- **Similar a W8** mas menor (61k vs 152k LOC). 82 CLI commands smoke test gate.

### W10 — touring-orchestration FUSION
- **State changed**: 3 crates absorvidos + decompose/session/diary extraídos de touring-server
- **Rollback**: Restaurar 3 crates + mover decompose/session/diary de volta para touring-server
- **Reversibility**: MÉDIA

### W11 — Test Debt Repayment
- **State changed**: Tests adicionados (+ proptest + fuzz) — ADDITIVE only
- **Rollback**: Trivial (deletar tests adicionados)
- **Reversibility**: TRIVIAL (zero functional code changed)

### W12 — Per-Project Deployment (LARGE)
- **State changed**: touring init + toolchain manager + .touring/ + daemon multi-instance + hook dispatcher + 2 pilots
- **Rollback**: Feature flag `--legacy-global` default ON. `touring config set deployment.mode global`. Pilots: `cd ~/projects/{konverter,analise}; touring uninstall --purge`.
- **Reversibility**: ALTA (feature flag controlado, pilots isolados)
- **Critical gate**: Validar que Claude Code segue funcionando após W12.6 (hook dispatcher walk-up)

### W13 — Publishing Pipeline
- **State changed**: docs.rs config + semver-check CI + sigstore signing + SBOM gen + CHANGELOG + RC1 published
- **Rollback**: Yank RC1 (cargo yank). Revert CI workflow YAML. Restore CHANGELOG anterior.
- **Reversibility**: MÉDIA (RC1 yank é registrado em crates.io history)

### W14 — Product Tiers & Distribution
- **State changed**: tier features in Cargo + license key system + telemetry tiered + private registry + SSO + audit + binary releases CI/CD + distro packages + Docker images
- **Rollback**: License key system add-on; pode ser desligado via feature flag `enterprise-disable`. Distro packages yank. Docker images delete tags.
- **Reversibility**: BAIXA-MÉDIA (1.0.0 GA published; rollback significa publicar 1.0.1 com fixes ou yank major)

## 3. Bisect strategy

Quando algo quebra mas não sabemos qual wave introduziu:

1. **List checkpoint snapshots**: `ls docs/baselines/pre-W*.tar.gz`
2. **Binary search**: extract snapshot do meio do range, validar
3. **Validate gate**: `cargo check --workspace && cargo test --workspace --no-run && touring wiring cycles`
4. **Refine**: continuar bisect até identificar wave culpada
5. **Postmortem**: documentar root cause em memory.db tier=semantic; criar gotcha entry

## 4. Gotchas registradas durante refactor

Cada wave que produzir um postmortem deve criar:

```bash
touring gotcha add "gotcha:wave-WX-<symptom>" \
  --file <affected_file> \
  --pattern "<regex>" \
  --severity high \
  --remediation "<actionable fix>"
```

## 5. Disaster recovery

**Cenário catastrófico**: refactor corrompeu workspace, snapshots perdidos, daemon morto.

1. **Recovery binary**: backup binário `~/.local/bin/touring` em `~/.touring/recovery/touring-pre-refactor`
2. **Recovery DB**: SQLite knowledge DB backup diário via cron em `~/.touring/backups/symbols-YYYY-MM-DD.db`
3. **Memory recovery**: `touring memory recall` queries persistem em SQLite — backup junto
4. **Re-bootstrap**: instalar Touring from scratch via install.touring.dev + migrate knowledge from backup
5. **DO NOT**: usar `git` (REGRA #11). Use Touring memory + snapshots para state.

## 6. References

- REGRA #11 (git proibido): `~/.claude/CLAUDE.md`
- Pre-refactor snapshot: `docs/baselines/touring-snapshot-pre-refactor-2026-05-11.tar.gz`
- Cross-wave risks: `05-RISKS.md`
