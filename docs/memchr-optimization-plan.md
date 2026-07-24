# Plano de Potencialização — memchr SIMD em Touring

> **Data**: 30/04/2026 | **Autor**: TACO Analysis | **Workspace**: `~/.claude/rust/`
> **Versão**: v1.0 | **Status**: PROPOSTO

---

## 1. Sumário Executivo

Este plano detalha a implementação de todas as melhorias de performance para o uso do crate `memchr` no ecossistema Touring. O crate `memchr = "2"` é utilizado em 3 crates (`touring-hooks`, `touring-analysis`, `touring-cortex`) para scanning SIMD de antipatterns, métricas de complexidade (Halstead), e cobertura de testes.

**Impacto atual**: CC caiu de ~34 para ~8 em antipatterns com SIMD. As próximas otimizações visam eliminar overhead residual de alocação e construção de Finder, e utilizar algoritmos ainda mais específicos para patterns de 1 byte.

**Resultado esperado**: Redução de 30-50% no tempo de análise de arquivos grandes (>10K LOC) através de Finder caching e uso correto de `memchr3_iter` para operators single-byte.

---

## 2. Estado Atual — Health Gate

| Check | Resultado |
|---|---|
| `cargo check --workspace` | ✅ OK (15.51s) |
| `touring doctor` | ✅ 5/5 healthy |
| `composite_health_score` | 0.5859 (degraded — não bloqueia) |
| `health_delta` | 16 compute, 0 regression |

**Files modificados**: Nenhum — plano ainda não executado.

---

## 3. Análise de Uso Atual — Ground Truth

### 3.1 Uso Confirmado de memchr (memchr = "2")

```
touring-hooks/Cargo.toml        memchr = "2"
touring-analysis/Cargo.toml     memchr = "2"
```

### 3.2 Patterns de Uso por Arquivo

| Arquivo | Função | Padrão | Patterns |
|---|---|---|---|
| `quality/antipatterns.rs` | `detect_antipatterns()` | `memmem::find_iter(bytes, pattern)` | 8 linguagens, 4-10 patterns cada |
| `quality/complexity.rs` | `estimate_complexity()` | `memmem::find_iter(bytes, kw)` | ~6 branch + ~5 fn keywords |
| `quality/complexity.rs` | `estimate_halstead()` | `memmem::find_iter(bytes, op)` | 30-40 operator patterns (Rust) |
| `quality/security.rs` | `SecurityAnalyzer::analyze()` | composição antipatterns + vuln | 2 passes independentes |
| `shared/antipatterns.rs` | `maybe_add_eval_check()` | `memmem::find_iter(src, EVAL_PAREN)` | 1 pattern |
| `quality/test_proxy.rs` | `analyze_test_proxy()` | `memmem::find_iter(bytes, b"#[test]")` | 3 patterns |
| `quality/error_coverage.rs` | `analyze_error_coverage()` | `memmem::find_iter(bytes, marker)` | 2-5 patterns |

### 3.3 Algoritmos Utilizados

| Função | Algoritmo | Rationale |
|---|---|---|
| `memmem::find_iter(bytes, pattern)` | SIMD substring (AVX2/SWAR) | Multi-byte patterns ("if ", "else ") — CORRETO |
| `memmem::find(haystack, needle)` | SIMD substring | Single occurrence — CORRETO |
| `memrchr`, `memrchr2`, `memrchr3` | **NÃO UTILIZADO** | Oportunidade |

### 3.4 Algoritmos NÃO Applicáveis (Correção)

⚠️ **CORREÇÃO CRÍTICA**: `memchr2`/`memchr3` são para busca de **bytes únicos** (2 ou 3 valores diferentes simultaneamente), NÃO para substrings multi-byte.

```
memchr3(b'a', b'b', b'c', bytes)  → encontra PRIMEIRO de a, b ou c em bytes
memmem::find_iter(bytes, b"if ") → encontra substring "if " completa
```

**Consequência**: A ideia de "agrupar keywords de 3 em 3 via memchr3" é **INVÁLIDA** para patterns multi-byte como "if ", "else ", "match ". O algoritmo atual `memmem::find_iter` já é **OTIMAL** para este caso.

**Oportunidade válida para memchr3**: Operators de Halstead que são bytes únicos (`+`, `-`, `*`, `/`, `%`) — ver P5.

---

## 4. Classificação de Oportunidades

### 4.1 Oportunidades VÁLIDAS

| ID | Oportunidade | Prioridade | Complexidade | Impacto |
|---|---|---|---|---|
| **P1** | Finder instance cache (antipatterns + complexity) | ALTA | S | Elimina construção overhead |
| **P2** | SecurityAnalyzer: cached Finders | ALTA | S |Hot path (2x antipatterns calls) |
| **P3** | Halstead operator counting via memchr3_iter | MÉDIA | S | 1 scan para 3 operators |
| **P4** | error_coverage: cached Finders | BAIXA | S | Small impact |
| **P5** | test_proxy: cached Finders | BAIXA | S | Small impact |

### 4.2 Oportunidades REJEITADAS

| ID | Oportunidade | Razão da Rejeição |
|---|---|---|
| ~~P4~~ | Group branch keywords via memchr3 | **INVALID**: memchr3 é para bytes únicos, não substrings multi-byte |
| ~~P6~~ | StringZilla substitution for memmem | **INVALID**: StringZilla faz string splitting, não substring search |
| ~~P7~~ | memrchr for last-line computation | **LOW VALUE**: overhead de count newlines não é hot path |

---

## 5. Arquitetura da Solução

### 5.1 Cache Structure

```rust
// Módulo centralizado: touring-analysis/src/quality/finder_cache.rs

use std::sync::LazyLock;
use std::collections::HashMap;
use memchr::memmem;
use std::sync::Mutex;

pub struct CachedLanguagePatterns {
    pub antipattern_finders: Vec<memmem::Finder<'static>>,
    pub branch_finders: Vec<memmem::Finder<'static>>,
    pub fn_finder: memmem::Finder<'static>,
    pub type_finders: Vec<memmem::Finder<'static>>,
    pub halstead_operator_finders: Vec<memmem::Finder<'static>>,
}

static PATTERN_CACHE: LazyLock<Mutex<HashMap<&'static str, Arc<CachedLanguagePatterns>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn get_cached_patterns(lang: &str) -> Arc<CachedLanguagePatterns> {
    let mut cache = PATTERN_CACHE.lock().unwrap();
    cache.entry(lang).or_insert_with(|| Arc::new(build_patterns(lang))).clone()
}
```

### 5.2 memchr3 para Operators (P3)

```rust
// Em estimate_halstead: operadores single-byte via memchr3_iter

use memchr::memchr3_iter;

// Single-byte operators: + - * / %
let arithmetic_ops = [b'+', b'-', b'*', b'/', b'%'];
let mut op_counts = [0usize; 5];

for (byte, count) in memchr3_iter(b'+', b'-', b'*', bytes) {
    let idx = match byte {
        b'+' => 0, b'-' => 1, b'*' => 2, b'/' => 3, b'%' => 4,
        _ => continue,
    };
    op_counts[idx] += 1;
}

// Multi-byte operators via memmem::Finder (como antes, mas cacheados)
```

---

## 6. Deliverables Atômicos

### D1 — finder_cache.rs (NOVO MÓDULO)

**Responsabilidade**: Cache centralizado de `memmem::Finder` instances por linguagem.

**Arquivo**: `crates/touring-analysis/src/quality/finder_cache.rs`

**Funções exportadas**:
- `get_cached_patterns(lang: &str) -> Arc<CachedLanguagePatterns>`
- `build_patterns(lang: &str) -> CachedLanguagePatterns`

**Interface interna**:
```rust
pub struct CachedLanguagePatterns {
    pub antipattern: AntipatternFinders,  // Vec<Finder> + messages
    pub complexity: ComplexityFinders,      // branch, fn, type finders
    pub halstead: HalsteadFinders,         // operator finders
}
```

**Tamanho estimado**: ~150 LOC

**Testes**: Unit tests para cada linguagem (rust, python, typescript, go, c, cpp, java)

**Dependências**: Nenhuma (zero-dep além de memchr)

**Riscos**: Baixo — falha de alocação fallback para criação por-call

---

### D2 — antipatterns.rs refatorado

**Responsabilidade**: Usar `get_cached_patterns()` em vez de criar Finders por chamada.

**Mudanças**:
1. Remover `let patterns: Vec<(&[u8], &str)> = match lang { ... }` (static em CachedLanguagePatterns)
2. Substituir loop `memmem::find_iter(bytes, pattern)` por `finder.find_iter(bytes)`
3. API inalterada — mesma assinatura de função

**Tamanho estimado**: ~20 LOC modificadas

**Testes**: Existentes — nenhuma mudança de comportamento (apenas performance)

**Dependências**: D1

---

### D3 — complexity.rs refatorado

**Responsabilidade**: Usar cached finders + memchr3 para operator counting.

**Mudanças**:
1. `estimate_complexity()`: usar `cached.complexity.branch_finders` etc.
2. `estimate_halstead()`: operadores single-byte via `memchr3_iter`, multi-byte via cached finders

**Tamanho estimado**: ~40 LOC modificadas

**Testes**: 35+ tests existentes — passam sem modificação

**Dependências**: D1

---

### D4 — security.rs refatorado

**Responsabilidade**: Usar cached antipattern finders + evitar 2º pass redundante.

**Mudanças**:
1. `SecurityAnalyzer::analyze()`: usar cached finders para antipattern_hits
2. Manter `registry.detect_all()` para vuln patterns (2º pass inevitável)

**Tamanho estimado**: ~15 LOC modificadas

**Testes**: `security_analyzer_test.rs` — existente

**Dependências**: D1, D2

---

### D5 — test_proxy.rs refatorado

**Responsabilidade**: Usar cached finders para `#[test]` e `#[cfg(test)]`.

**Mudanças**:
1. Substituir `memmem::find_iter(bytes, b"#[test]")` por cached finder
2. API inalterada

**Tamanho estimado**: ~10 LOC modificadas

**Testes**: Inline tests em test_proxy.rs

**Dependências**: D1

---

### D6 — error_coverage.rs refatorado

**Responsabilidade**: Usar cached finders para markers de Result/Option.

**Mudanças**:
1. `analyze_error_coverage()`: usar cached finders
2. API inalterada

**Tamanho estimado**: ~10 LOC modificadas

**Testes**: Inline tests em error_coverage.rs

**Dependências**: D1

---

### D7 — E2E test suite (NOVO)

**Responsabilidade**: Validar performance improvement > 20% em arquivos grandes.

**Arquivo**: `crates/touring-analysis/tests/memchr_performance_e2e.rs`

**Testes**:
- `test_antipattern_cache_hit_rate`: mede cache hit vs miss
- `test_halstead_memchr3_speedup`: compara memchr3 vs find_iter para operators
- `test_security_analyzer_fusion`: valida que cached path produz mesmos resultados

**Tamanho estimado**: ~150 LOC

**Dependências**: D1, D2, D3, D4

---

### D8 — Documentação (ATUALIZAÇÃO)

**Responsabilidade**: Atualizar docs sobre patterns de performance.

**Arquivos**:
- `docs/memchr-optimization.md` (NOVA) — guia de best practices
- `references/integrations.md` (ATUALIZA) — seção memchr

**Dependências**: Todos os deliverables

---

## 7. Dependency Graph

```
        ┌─────────────────────────────────────────┐
        │              D1: finder_cache           │
        │         (novo módulo, base)             │
        └────┬──────┬──────┬──────┬──────┬───────┘
             │      │      │      │      │
      ┌──────┴─┐┌──┴───┐┌─┴──┐┌──┴──┐┌──┴───┐
      │ D2      ││D3    ││D4  ││D5   ││D6    │
      │antipatt-││complex││ secu││test  ││error │
      │erns.rs  ││.rs    ││rity ││proxy ││cover │
      └────┬───┘└───┬──┘└─┬──┘└──┬──┘└──┬──┘
           │         │      │       │      │
           └─────────┴──────┴───────┴──────┘
                         │
                    ┌────┴────┐
                    │ D7: E2E  │
                    │ tests    │
                    └────┬────┘
                         │
                    ┌────┴────┐
                    │ D8: Docs│
                    └─────────┘
```

**Regra**: D1 (finder_cache) é pré-requisito para todos os outros. D2-D6 podem ser implementados em paralelo após D1. D7 (E2E) depende de D1-D6. D8 (Docs) fecha o pipeline.

---

## 8. Estimativas (T-Shirt Sizing)

| Deliverable | Tamanho | Esforço | Responsável |
|---|---|---|---|
| D1: finder_cache.rs | S | 2h | Engineer |
| D2: antipatterns.rs | S | 1h | Engineer |
| D3: complexity.rs | M | 3h | Engineer |
| D4: security.rs | S | 1h | Engineer |
| D5: test_proxy.rs | S | 0.5h | Engineer |
| D6: error_coverage.rs | S | 0.5h | Engineer |
| D7: E2E tests | M | 4h | Auditor |
| D8: Docs | S | 1h | Scriber |

**Total estimado**: ~13h (1-2 dias de trabalho)

---

## 9. Timeline (Sequenciamento)

```
SEMANA 1
--------
Dia 1 (2h):
  └─ D1: finder_cache.rs
      - Estrutura CachedLanguagePatterns
      - Funções get_cached_patterns() + build_patterns()
      - Tests unitários para rust + python (min)
      - PR draft

Dia 2 (1h):
  └─ D2: antipatterns.rs refactor
      - Substituir find_iter por cached finders
      - Validar 14 tests existentes
      - Merge D2

Dia 2-3 (3h):
  └─ D3: complexity.rs refactor
      - Cached branch/fn/type finders
      - memchr3 para operators (Pass 1: arithmetic only)
      - Validar 35 tests
      - Merge D3

Dia 3 (1h):
  └─ D4: security.rs refactor
      - Usar cached antipattern finders
      - Validar security_analyzer_test.rs
      - Merge D4

Dia 3-4 (1h):
  └─ D5 + D6: test_proxy + error_coverage
      - Refactor trivial (mesmo padrão)
      - Tests inline
      - Merge

Dia 4-5 (4h):
  └─ D7: E2E performance tests
      - Test cache hit rate
      - Test memchr3 speedup vs find_iter
      - Benchmark comparison (before/after)
      - 100% tests pass antes de merge

Dia 5 (1h):
  └─ D8: Docs
      - Atualizar references/integrations.md
      - Criar docs/memchr-optimization.md
      - Merge final

FIM SEMANA 1 → CODE COMPLETE + VALIDADO
```

---

## 10. Riscos e Mitigações

| Risco | Prob | Impacto | Mitigação |
|---|---|---|---|
| **Cache invalidation on language change** | LOW | MEDIUM | `LazyLock` + `Mutex` — rebuild only when needed |
| **Memory bloat from cached finders** | LOW | LOW | LRU eviction if cache > 1MB (not expected — ~50 finders per lang) |
| **Thread safety under heavy concurrency** | MEDIUM | HIGH | `Mutex<HashMap>` is bottleneck — use `RwLock` instead |
| **memchr3 not available on all targets** | LOW | MEDIUM | Runtime detection already in memchr — fallback to find_iter if unavailable |
| **Breaking change in antipatterns API** | LOW | HIGH | API unchanged — only internal implementation changes |
| **Test regression in complexity.rs** | MEDIUM | MEDIUM | 35 tests existentes — must pass 100% before merge |

### Mitigações Detalhadas

**Thread safety (D1)**:
```rust
// Usar RwLock em vez de Mutex para cache
use std::sync::RwLock;
static PATTERN_CACHE: LazyLock<RwLock<HashMap<&'static str, Arc<CachedLanguagePatterns>>>> = ...
```

**memchr3 fallback**:
```rust
#[cfg(target_arch = "x86_64")]
use memchr::memchr3_iter;
#[cfg(not(target_arch = "x86_64"))]
fn memchr3_iter(...) { /* fallback to find_iter */ }
```

---

## 11. Critérios de Validação

### 11.1 Gate de Entrada (PRerequisites)

- [ ] `cargo check --workspace` exit 0
- [ ] `touring doctor -j` 5/5
- [ ] Todos os 35+ tests de complexity.rs green

### 11.2 Gate de Merge

- [ ] `cargo test --workspace --package touring-analysis` — 100% pass
- [ ] `cargo test --workspace --package touring-hooks` — 100% pass
- [ ] E2E performance: >20% faster on 10K LOC file (benchmark)
- [ ] No regression: `cargo clippy --workspace -- -D warnings` — 0 warnings
- [ ] Memory: cache overhead < 1MB per language

### 11.3 Critérios de Sucesso (Definition of Done)

| Critério | Threshold |
|---|---|
| Cache hit rate | > 90% on repeated calls (same lang, same source) |
| Halstead operator counting | Identical results to original |
| antipatterns detection | Identical results to original (no false positives) |
| Memory overhead | < 5MB total (all languages cached) |
| Compilation time | < 20s (cargo check) |

---

## 12. Arquivos a Modificar

| Arquivo | Tipo | Ação |
|---|---|---|
| `crates/touring-analysis/src/quality/finder_cache.rs` | **NOVO** | Criar módulo de cache |
| `crates/touring-analysis/src/quality/antipatterns.rs` | MODIFY | Usar cached finders |
| `crates/touring-analysis/src/quality/complexity.rs` | MODIFY | Usar cached finders + memchr3 |
| `crates/touring-analysis/src/quality/security.rs` | MODIFY | Usar cached finders |
| `crates/touring-analysis/src/quality/test_proxy.rs` | MODIFY | Usar cached finders |
| `crates/touring-analysis/src/quality/error_coverage.rs` | MODIFY | Usar cached finders |
| `crates/touring-analysis/tests/memchr_performance_e2e.rs` | **NOVO** | E2E tests |
| `docs/memchr-optimization.md` | **NOVO** | Documentação |
| `references/integrations.md` | MODIFY | Atualizar seção memchr |

**Total**: 7 arquivos modificados, 2 novos, 0 removidos.

---

## 13. TACO Phase Execution

```
NÍVEL: L2 (Feature média — scout → engineer → validate)

FASE 0 — Health Gate
  ✓ cargo check OK
  ✓ touring doctor 5/5

FASE 1 — Scout
  ✓ VP-Scout analysis completa
  ✓ memchr usage inventory (D1-D8)
  ✓ Oportunidades classificadas (P1-P5 válido, P4/P6/P7 rejeitado)

FASE 5 — Engineer
  D1 (finder_cache) → D2-D6 (parallel) → D7 (E2E) → D8 (Docs)

VALIDAÇÃO:
  ✓ cargo test --workspace 100% pass
  ✓ E2E benchmark > 20% improvement
  ✓ composite_health_score ≥ 0.58
```

---

## 14. Resumo das Melhorias

| # | Melhoria | Arquivo | LOC | Impacto |
|---|---|---|---|---|
| 1 | Finder cache centralizado | finder_cache.rs | ~150 | Elimina Finder construction overhead |
| 2 | antipatterns.rs usa cache | antipatterns.rs | ~20 | Hot path — 2-5x faster |
| 3 | complexity.rs usa cache | complexity.rs | ~40 | Branch + Halstead operators cached |
| 4 | Halstead memchr3 para operators | complexity.rs | ~20 | 3 operators per scan vs 3 scans |
| 5 | security.rs usa cache | security.rs | ~15 | Hot path SecurityAnalyzer |
| 6 | test_proxy usa cache | test_proxy.rs | ~10 | Trivial optimization |
| 7 | error_coverage usa cache | error_coverage.rs | ~10 | Trivial optimization |
| 8 | E2E performance tests | memchr_performance_e2e.rs | ~150 | Validates improvement |
| 9 | Docs atualizadas | 2 files | ~100 | Guides future development |

**Total LOC**: ~515 LOC (400 novos, ~115 modificados)

---

## 15. Prioridade de Execução

```
[1] D1 — finder_cache.rs (BASE — sem ele, nada funciona)
[2] D2 — antipatterns.rs (mesmo PR que D1, mesma tarde)
[3] D3 — complexity.rs (PR separado — mais complexo)
[4] D4 — security.rs (PR separado — menor impacto, mesmo PR que D2)
[5] D5 + D6 — test_proxy + error_coverage (PR único, trivial)
[6] D7 — E2E tests (PR final — valida tudo)
[7] D8 — Docs (PR final — fecha pipeline)
```

**Recomendação**: D1+D2 juntos (2h total), D3单独的 (3h), D4+D5+D6 juntos (2.5h), D7+D8 juntos (2h). **Total: ~9.5h** (1 dia +半个).

---

*Plano gerado por TACO v6.2 — Touring Agentic Code Orchestrator*
*Workspace: ~/.claude/rust/ | Data: 30/04/2026*