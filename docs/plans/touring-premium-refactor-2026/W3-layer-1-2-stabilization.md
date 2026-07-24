---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W3"
name: "Layer 1-2 Stabilization"
phase: "F3-STABILIZATION"
depends_on:
  - W2
parallel_with: []
status: "PENDING"
created: "2026-05-11"
cila: "L3"
rust_changes: "REFACTOR + ABSORVE"
estimated_days: "8-10"
checkpoint: "touring_premium_W3_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W3.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W0-*.md
  - W1-*.md
  - W2-*.md
  - W4-*.md
  - W5-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W3: Layer 1-2 Stabilization

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F3-STABILIZATION
> **Contribuição para resultado final**: Foundation é o anchor de TODOS os crates. Se ela está kitchen-sink, todos herdam complexidade. Slim foundation = todas waves seguintes começam mais limpas.

---

## Contexto e Dependências

- **Depende de**: W2
- **Paralelo com**: Nenhuma
- **CILA**: `L3`
- **Mudanças Rust**: `REFACTOR + ABSORVE`
- **Estimativa**: 8-10 dias
- **Checkpoint**: `touring_premium_W3_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W3.py`

---

## Descrição

Renomear touring-core → touring-foundation (slim). Absorver 6 crates anêmicos: touring-rule-engine, touring-definitions, touring-telemetry, touring-resource-monitor, touring-activity (+ extrair embedding/, mvkl/ → preparação para W5 storage). Identity + simd + rkyv permanecem standalone (kernel layer 2). Tests +25%/+30% LOC ratio.

---

## Efeitos no Sistema

- touring-core renomeado para touring-foundation (re-export shim)
- 5 crates anêmicos absorvidos como submódulos de foundation
- embedding/ extraído (vai para touring-storage em W5)
- Foundation atinge ≥ 25% LOC test ratio
- Identity atinge ≥ 30% LOC test ratio
- Macrociclo de 618 reduzido (crates absorvidos saem do grafo)

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W3.1: Rename touring-core → touring-foundation (+ shim)

**Descrição**: Cargo.toml name field updated. crates/touring-core/ → crates/touring-foundation/. Re-export shim 'pub use touring_foundation::* as touring_core;' em um stub crate touring-core mantido por 2 versões.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - touring tantivy search 'touring_core::'
  - grep -rn 'touring_core::' crates/ | wc -l

**Critério de validação**: cargo check --workspace exit 0; consumers ainda compilam via shim.

---

### W3.2: Slim foundation: extract embedding/ (→ W5 storage)

**Descrição**: Mover crates/touring-foundation/src/embedding/ para diretório temporário scripts/touring_premium_refactor_2026/staging/embedding/. Atualizar consumers para tipo trait abstrato. Implementação concreta vai para touring-storage em W5.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - touring ast blast crates/touring-foundation/src/embedding/mod.rs
  - touring wiring impact 'foundation::embedding' --depth 2

**Critério de validação**: cargo check exit 0; embedding/ não mais em foundation/src/.

---

### W3.3: Extract mvkl/ (multi-version key list) — keep in foundation

**Descrição**: mvkl/ é primitive (não embedding-related). Mantém em foundation/.

**Dias estimados**: 0.5

**Critério de validação**: mvkl/ presente em foundation/src/.

---

### W3.4: Absorve touring-rule-engine → foundation/rules/

**Descrição**: 443 LOC anêmicos. Mover para foundation/src/rules/. Atualizar 1-2 consumers que importam direto. Delete crate antigo.

**Dias estimados**: 0.5

**DISCOVER obrigatório**:
  - touring wiring impact 'touring_rule_engine' --depth 2

**Critério de validação**: cargo check exit 0; foundation::rules pública.

---

### W3.5: Absorve touring-definitions → foundation/types/

**Descrição**: 1.1k LOC. Mover para foundation/src/types/.

**Dias estimados**: 0.5

**Critério de validação**: cargo check exit 0; foundation::types pública.

---

### W3.6: Absorve touring-telemetry → foundation/telemetry/

**Descrição**: 990 LOC. Mover para foundation/src/telemetry/.

**Dias estimados**: 0.5

**Critério de validação**: cargo check exit 0; foundation::telemetry pública.

---

### W3.7: Absorve touring-resource-monitor → foundation/sentinel/

**Descrição**: 2.4k LOC. Mover para foundation/src/sentinel/. Feature 'sentinel-psi' para gating Linux-only.

**Dias estimados**: 1.0

**Critério de validação**: cargo check exit 0; foundation::sentinel pública.

---

### W3.8: Absorve touring-activity → foundation/activity/

**Descrição**: 781 LOC. Mover para foundation/src/activity/.

**Dias estimados**: 0.5

**Critério de validação**: cargo check exit 0; foundation::activity pública.

---

### W3.9: Foundation tests ≥ 25% LOC ratio

**Descrição**: Atual foundation ratio ~9% (1.2k / 13.6k). Após absorções, total ~18k src. Adicionar tests até atingir 25% (~4.5k tests). Focar em modules de alto blast_radius.

**Dias estimados**: 2.0

**TDD RED** (escrever ANTES do código):
```python
def test_foundation_coverage_25pct():
    """RED: tests/src LOC ratio < 25%."""
```

**Critério de validação**: wc -l foundation/src/**/*.rs vs tests/ ≥ 0.25.

---

### W3.10: Identity tests ≥ 30% ratio

**Descrição**: Atual identity ratio ~45% (720/1599) — já bom. Manter ou aumentar. Garantir RFC-004 invariants cobertos por proptest.

**Dias estimados**: 0.5

**Critério de validação**: identity tests ≥ 30%; proptest para EntityId determinism.

---

### W3.11: Cycle re-check

**Descrição**: touring wiring cycles --min-depth 2 → comparar com W0 baseline. Esperado: macrociclo de 618 menor por absorção (menos crates no grafo).

**Dias estimados**: 0.5

**Critério de validação**: cycle depth max < 618 (redução documentada).

---

## Gate de Saída

touring-foundation ≤ 18k LOC; 5 crates absorvidos; identity standalone OK; test ratio ≥ 25% foundation, ≥ 30% identity; cycle reduction vs W0 baseline documentada.

## Riscos Específicos

- Renomear touring-core afeta hooks em ~/.claude/settings.json → manter shim crate por 2 versões
- Absorver resource-monitor pode quebrar feature gating sentinel-psi → validar em CI Linux + macOS

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

---

## Discovery Updates (2026-05-11) — Rename Estimate + W3.2 Overlap

Dois auto-scripts executados — `w3_rename_core_to_foundation.py` e `w3_absorve_anemic_crates.py`. Ambos retornam descobertas que alteram o escopo de W3.

### W3.1 — Rename touring-core → touring-foundation

| Métrica | Valor |
|---|---|
| Cargo.toml files declaring `touring-core` | depende do scan |
| Source files using `touring_core::` | depende do scan |
| Total `use` statements | depende do scan |
| **Total estimate** | **~1.45 engineer-days** |

Top sub-modules consumidos (orientam a documentação de migração):

| Sub-module | Use count |
|---|---|
| `touring_core::TouringConfig` | 45 |
| `touring_core::TouringError` | 42 |
| `touring_core::diagnostic` | 28 |
| `touring_core::schema` | 27 |
| `touring_core::truncate_str` | 17 |

**Ast-grep rewrite script** auto-gerado em `staging/w3-ast-grep-rewrites.sh` (idempotente).

### W3.2 — Anemic crates absorption (REVISÃO MAJOR)

**Descoberta**: top 5 anemic crates são **EXATAMENTE OS MESMOS** que `w1_audit_dead_code` já marca como dead:

| Crate | LOC | Pub | Consumers |
|---|---|---|---|
| `touring-loom-proofs` | 11 | 0 | 0 |
| `touring-semantic-spike` | 67 | 0 | 0 |
| `touring-wasm-client` | 0 | 0 | 0 |
| `touring-wasm-common` | 0 | 0 | 0 |
| `touring-wasm-server` | 0 | 0 | 0 |

**Implicação**: W3.2 (absorção) overlap completo com W1.1 (dead-code purge). Essas crates serão **deletadas em W1**, não absorvidas em W3. W3.2 pode ser **completamente removido** ou re-escopado para crates legitimamente anemic mas com consumers (currently zero).

### Ação revisada para W3

1. **W3.1**: ✅ Estimate confirmado (~1.5 dias). Migration script pronto.
2. **W3.2**: ⚠️ **REVISAR ESCOPO** — overlap 100% com W1. Possíveis caminhos:
   - (a) Remover W3.2 do critical path
   - (b) Re-escopar W3.2 para "promover types/protocol crates a workspace inheritance"
   - (c) Manter W3.2 como safety-net check para anemic crates que escapem do W1 audit

### Forensic outputs disponíveis

- `data/w3-touring-core-consumer-map.json` — full rename scope
- `data/w3-anemic-crates-map.json` — anemic audit (todos KNOWN_DEAD)
- `staging/w3-rename-migration-plan.md` — checklist humano
- `staging/w3-ast-grep-rewrites.sh` — script idempotente
