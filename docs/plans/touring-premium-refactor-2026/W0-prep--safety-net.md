---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W0"
name: "Prep & Safety Net"
phase: "F1-PREP"
depends_on: []
parallel_with: []
status: "PENDING"
created: "2026-05-11"
cila: "L2"
rust_changes: "ZERO"
estimated_days: "5-7"
checkpoint: "touring_premium_W0_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W0.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W1-*.md
  - W2-*.md
  - W3-*.md
  - W4-*.md
  - W5-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W0: Prep & Safety Net

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F1-PREP
> **Contribuição para resultado final**: Sem baselines, qualquer regressão é invisível. Sem ADRs, decisões arquiteturais ficam vulneráveis a drift. Esta wave imuniza o refactor contra retrocesso silencioso.

---

## Contexto e Dependências

- **Depende de**: Nenhuma
- **Paralelo com**: Nenhuma
- **CILA**: `L2`
- **Mudanças Rust**: `ZERO`
- **Estimativa**: 5-7 dias
- **Checkpoint**: `touring_premium_W0_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W0.py`

---

## Descrição

Capturar snapshots completos, baselines de bench/test/coverage, e produzir 4 ADRs + Master Plan como constituição do refactor. Zero alterações de código de produção; somente leituras, medições, e documentação. Define a linha de base contra a qual TODAS as waves posteriores serão comparadas.

---

## Efeitos no Sistema

- Snapshot tar pre-refactor (97 MB) + SHA-256
- Bench baseline para regression budget de ±5%
- Coverage baseline para gate de ≥20% per crate
- Wiring/cycles snapshot (2 cycles, depth max 618)
- ADR-001 (Architecture), ADR-002 (Deployment), ADR-003 (Commercial)
- MASTER-PLAN-2026 com 15 waves + critical path + DAG
- Touring memory lessons persistidas (tier=semantic)

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W0.1: Snapshot tar pre-refactor + SHA-256

**Descrição**: tar -czf crates+Cargo.{toml,lock} → docs/baselines/. Excluir target/, .touring-cache/, .git/, __pycache__/. SHA-256 hex armazenado em .sha256 sibling file.

**Dias estimados**: 0.5

**DISCOVER obrigatório**:
  - touring memory recall 'snapshot pre-refactor'

**Critério de validação**: touring-snapshot-pre-refactor-<DATE>.tar.gz existe + .sha256 sidecar; tamanho 80-150 MB esperado.

---

### W0.2: Bench baseline

**Descrição**: cargo bench --workspace --save-baseline pre-refactor-<DATE>. Output: target/criterion/* + docs/baselines/bench-pre-refactor-<DATE>.log.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - touring tantivy search 'criterion benchmark'
  - ls crates/*/benches/ → existing bench targets

**Critério de validação**: docs/baselines/bench-pre-refactor-<DATE>.log existe; exit code 0.

---

### W0.3: CI baseline (cargo check + test --no-run)

**Descrição**: cargo check --workspace --all-targets → log. cargo test --workspace --no-run --all-targets → log. Captura tempos e warnings.

**Dias estimados**: 0.5

**Critério de validação**: 2 logs em docs/baselines/, ambos exit 0.

---

### W0.4: Coverage baseline (cargo llvm-cov)

**Descrição**: cargo llvm-cov --workspace --json --output-path docs/baselines/coverage-pre-refactor-<DATE>.json. Aceita falhas em crates anêmicos.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - which cargo-llvm-cov || cargo install cargo-llvm-cov

**Critério de validação**: coverage-pre-refactor-<DATE>.json existe + parseável.

---

### W0.5: Wiring/cycle snapshot

**Descrição**: touring wiring audit -j > wiring-pre.json (~29 MB). touring wiring cycles --min-depth 2 --format json > cycles-pre.json. touring status -j > status-pre.json. touring ast workspace-info > workspace-info-pre.json.

**Dias estimados**: 0.5

**Critério de validação**: 4 JSON files em docs/baselines/; cycle_count documentado.

---

### W0.6: ADR-001 Premium Architecture Vision

**Descrição**: Documento canônico da topologia alvo: 13 crates produtivos em 6 layers (Foundation → Kernel → Domain Core → Intelligence → Application → Product). Inclui mapa de absorções (46 → 13).

**Dias estimados**: 1.0

**Critério de validação**: docs/plans/.../01-ARCHITECTURE.md existe; ≥ 600 LOC; menciona todos os 13 crates target + 46 atuais.

---

### W0.7: ADR-002 Per-Project Deployment Model

**Descrição**: .touring/touring.toml schema + ~/.touring/toolchains/ layout + daemon discovery walk-up + CLI surface + external installer (install.touring.dev) + migration tool (touring migrate --from-global).

**Dias estimados**: 1.0

**Critério de validação**: docs/plans/.../02-DEPLOYMENT.md existe; ≥ 400 LOC.

---

### W0.8: ADR-003 Commercial Tiers + GTM Strategy

**Descrição**: 4 tiers (free/standard/premium/enterprise) + Cargo features mapping + JWT ed25519 license + pricing matrix + competitive landscape + sales motion + 5-year financial forecast + OKRs Y1.

**Dias estimados**: 0.5

**Critério de validação**: docs/plans/.../03-COMMERCIAL.md existe; ≥ 500 LOC.

---

### W0.9: MASTER-PLAN-2026 + 15 wave files + cross-cutting docs

**Descrição**: Rodar generate_plan.py --all. Emite 26 markdown (00-INDEX + 9 cross-cutting + 15 waves + CROSS-AUDIT) + 15 validate_WX.py + cross_audit_e2e.py.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - python3 generate_plan.py --check (verify scaffold exists)

**Critério de validação**: 26 .md em docs/plans/touring-premium-refactor-2026/; 16 .py em scripts/touring_premium_refactor_2026/.

---

## Gate de Saída

ADRs aprovados por Gabriel; baselines committed; cycle_count registrado (esperado: 2, depth max 618); cargo check exit 0.

## Riscos Específicos

- Snapshot tar muito grande (>200 MB) → revisar exclusions
- cargo llvm-cov ausente → fallback para cargo tarpaulin ou skip W0.4

## Checklist de Conclusão

- [ ] Todos os subtasks implementados
- [ ] Todos os testes TDD GREEN
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test --workspace --no-fail-fast` pass
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `touring wiring cycles --min-depth 2` no new cycles
- [ ] `touring wiring orphans -j` no new orphans (REGRA #0)
- [ ] Bench regression < 5%
- [ ] Test ratio ≥ 20% per touched crate
- [ ] Checkpoint `.toon` salvo
- [ ] Memory lesson persistida (`touring memory store --tier semantic`)
- [ ] RL reward injetado (`touring learning reward orchestrate <val>`)
- [ ] Documentação atualizada (se necessário)
