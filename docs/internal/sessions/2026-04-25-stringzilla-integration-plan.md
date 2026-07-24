# StringZilla Integration Plan — Touring Performance & Quality Wave

> **Data**: 2026-04-25 | **Versão**: 2.0 — IMPLEMENTADO | **Nível TACO**: L4+ | **Status**: ✅ COMPLETO (1 dia)
>
> **Objetivo**: Integrar StringZilla v4.6.0 + otimizações SIMD zero-cost ao workspace Touring,
> eliminando 3.679+ chamadas `.contains()` sequenciais, 30+ `Regex::new` em hot paths, e um
> bug E0658 pré-existente — maximizando scope, performance e qualidade (sempre potencializando).

---

## 1. Análise de Impacto

### 1.1 Inventário de Bottlenecks Confirmados (VP-Scout)

| ID | Arquivo | Problema | Impacto | ROI |
|----|---------|----------|---------|-----|
| **B0** | `cli_handlers_decompose.rs:30` | E0658 `str_as_str` instável — `match token.trim().to_ascii_lowercase().as_str()` | P0 bug | Imediato |
| **B1** | `reranker.rs:get_authority()` | 8 chamadas `.contains()` sequenciais em hot path; `KeywordMatcher` existe no mesmo crate mas não está wired | P2 perf | Alto |
| **B2** | `reranker.rs:compute_keyword_match()` | Loop `keywords.iter().filter(|kw| content_lower.contains(...))` — O(n×m) sem cache | P2 perf | Alto |
| **B3** | `pre_tool_validator.rs` | 30+ `Regex::new` para padrões de prefixo fixo (`^rm\s+`, `^dd\s+`, etc.) — dispara em CADA Bash PreToolUse | P1 hot path | Crítico |
| **B4** | `async_knowledge.rs:gotcha_count_for_file` | SQL `LIKE '%' \|\| pattern \|\| '%'` — wildcard leading impede uso de índice | P2 perf | Médio |
| **B5** | `touring-generator:BkTreeFuzzyAdapter` | Vec<char> Levenshtein DP inline, O(N×m×n) por chamada — PLN2 TODO para BK-tree real | P2 perf | Alto |

### 1.2 StringZilla v4.6.0 — APIs Mapeadas

| API StringZilla | Throughput | Uso em Touring |
|-----------------|-----------|----------------|
| `sz_utf8_newline_splits` | 10+ GB/s | LLOC counting em `pre_edit` quality signals |
| `sz_utf8_whitespace_splits` | 10+ GB/s | Token counting para Halstead metrics |
| `Byteset` | 8.17 GB/s | Character class scanning em Halstead operators |
| `sz_hash` (AES-64) | 1.84 ops/unit | Checksums em file_blake3_registry, tantivy dedup |
| `sz_find / sz_rfind` | SIMD | Import detection, symbol search em source |
| `sz_edit_distance` | ~2125× vs NLTK | BkTreeFuzzyAdapter Levenshtein |
| `LevenshteinDistances` (cpus) | 3.43B CUPS | Batched Levenshtein em sugestões |
| `Fingerprints` (cpus) | MinHash | Memory dedup em tantivy |

---

## 2. Plano de Implementação por Tiers

### TIER 0 — Otimizações sem novas dependências (~1 dia)

**Zero deps adicionais. ROI máximo. Implementar primeiro.**

#### T0.0 — Bug Fix E0658 (P0, ~5 min)
- **Arquivo**: `crates/touring-hooks/src/cli_handlers_decompose.rs:30`
- **Fix**: `match token.trim().to_ascii_lowercase().as_str()` → `match &*token.trim().to_ascii_lowercase()`
- **Teste**: `cargo check --workspace` exit 0
- **Classificação**: L1 Bugfix

#### T0.1 — Wire AhoCorasick em `reranker.rs` (P2, ~30 min)
- **Arquivo**: `crates/touring-antt/src/reranker.rs`
- **Deps prontas**: `KeywordMatcher`, `ANTT_PATTERNS`, `TECHNICAL_KEYWORDS` em `keyword_matcher.rs` (mesmo crate)
- **Mudanças**:
  - `get_authority()`: substituir 8x `.contains()` por lookup em `ANTT_PATTERNS.find_matches(doc_type)` — O(1) via AhoCorasick
  - `compute_keyword_match()`: substituir loop por `TECHNICAL_KEYWORDS.find_matches(content).len()`
  - Adicionar import `use crate::keyword_matcher::{ANTT_PATTERNS, TECHNICAL_KEYWORDS};`
- **Testes**: 5+ unit tests para `get_authority` + `compute_keyword_match` com inputs variados

#### T0.2 — `starts_with` para padrões de prefixo fixo em `pre_tool_validator.rs` (P1, ~45 min)
- **Arquivo**: `crates/touring-hooks/src/pre_tool_validator.rs`
- **Estratégia**: Criar `StaticPrefixPattern { prefix, param_pattern: Option<Regex>, reason, severity }` ao lado do `DangerousPattern { pattern: Regex, ... }` existente
- **Padrões migráveis** (29 de 30+):
  - Prefixos simples: `rm`, `dd`, `mkfs`, `fdisk`, `parted`, `pvremove`, `lvremove`, `shred`, `wipefs`, `hdparm`, `blkdiscard`, `sgdisk`, `sfdisk`, `rf`, `truncate`, `overwrite`
  - Comando + arg: `chmod 777`, `chown root`
- **Manter como Regex**: apenas padrões com lookahead/alternation complexa (ex: `(?:sudo|su)\s+`)
- **Resultado esperado**: 85%+ dos checks passam por branch `starts_with` O(m) — sem regex engine overhead
- **Testes**: 10+ unit tests cobrindo cada prefixo migrado + padrões que NÃO devem disparar

#### T0.3 — `memmem::Finder` em `gotcha_count_for_file` (P2, ~20 min)
- **Arquivo**: `crates/touring-hooks/src/async_knowledge.rs`
- **Dep**: `memchr::memmem` já disponível como dependência transitiva em `touring-hooks`
- **Estratégia**: Trocar query SQL por fetch de todos os patterns da tabela + Rust-side memmem scan
  ```rust
  // Buscar patterns do DB (cached em memory via OnceLock ou moka)
  let patterns = fetch_gotcha_patterns(&conn)?;
  let count = patterns.iter()
      .filter(|p| memmem::find(file_path.as_bytes(), p.as_bytes()).is_some())
      .count() as i64;
  ```
- **Nota**: Se `patterns.len() > 1000`, manter SQL com índice em `pattern` column
- **Testes**: 3+ unit tests cobrindo edge cases (empty, match, no-match)

---

### TIER 1 — StringZilla std features (~4 horas)

**Adds**: `stringzilla = { version = "4.6", features = ["std"] }` em workspace Cargo.toml

#### T1.1 — LLOC via `sz_utf8_newline_splits` em quality signals (P2, ~1 hora)
- **Arquivo**: `crates/touring-analysis/src/quality/complexity.rs` (função `estimate_lloc`)
- **Mudança**: Substituir `str.lines().filter(non_empty_non_comment).count()` por
  `sz_utf8_newline_splits(code).filter(...).count()` — 10+ GB/s vs std iter
- **Raciocínio**: LLOC chamado em EVERY `pre_edit` hook invocation; para arquivos grandes (>10k LOC) é bottleneck mensurável
- **Testes**: Regression test com 10k-line fixture; diff output deve ser zero

#### T1.2 — Halstead operators via `Byteset` (P2, ~1 hora)
- **Arquivo**: `crates/touring-analysis/src/quality/complexity.rs` (função `compute_halstead`)
- **Mudança**: Substituir iteração sobre chars por `Byteset::from_chars("+−∗/=<>&|^~")` + `sz_find_charset` scan
- **Benefício**: Reduce per-char comparisons de O(k×n) para O(n) single-pass SIMD

#### T1.3 — `sz_hash` para checksums de símbolos (P3, ~1 hora)
- **Arquivo**: `crates/touring-hooks/src/tantivy_store.rs` (campo `blake3_hash`)
- **Estratégia**: `sz_hash` para dedup rápido em-memória antes de blake3 final; mantém blake3 como verificação canônica
- **Nota**: sz_hash não é criptograficamente seguro — usar apenas para dedup, não como fingerprint persistido

---

### TIER 2 — StringZilla cpus feature / BkTree real (~6 horas)

**Adds**: `features = ["std", "cpus"]` + `allocator-api2 = "0.3"` para stable Rust polyfill

#### T2.1 — `sz_edit_distance` em `BkTreeFuzzyAdapter` (P2, ~3 horas)
- **Arquivo**: `crates/touring-generator/src/core/context.rs` (struct `BkTreeFuzzyAdapter`)
- **Dependência**: feature gate `simd-fuzzy` (já existe em `Cargo.toml:21`)
- **Mudanças**:
  1. Substituir `levenshtein_dist(a, b)` inline Vec<char> por `stringzilla::sz_edit_distance(a, b)`
  2. Implementar BK-tree real: `struct BkNode { symbol: String, children: BTreeMap<usize, BkNode> }`
  3. `BkTree::insert(sym)` + `BkTree::query(query, max_dist) -> Vec<&str>`
  4. `BkTreeFuzzyAdapter::top_k(query, k, max_dist)` → BK-tree query com sz_edit_distance
- **Ganho esperado**: ~2125× vs implementação atual Vec<char> para batches de sugestões
- **LOC estimado**: ~200 linhas novas
- **Testes**: 10+ unit tests cobrindo insert, query, top_k, edge cases (empty, exact match, unicode)

#### T2.2 — `LevenshteinDistances` batched para sugestões em massa (P3, ~2 horas)
- **Arquivo**: novo `crates/touring-generator/src/core/batch_fuzzy.rs`
- **API**: `batch_levenshtein(query: &str, candidates: &[&str], max_dist: usize) -> Vec<(usize, &str)>`
- **Usa**: `stringzilla::LevenshteinDistances` (feature cpus) — 3.43B CUPS, 2125× mais rápido
- **Wiring**: exposto em `GeneratorContext::suggest_symbols_fuzzy_batch`
- **Testes**: 5+ unit tests

#### T2.3 — MinHash para dedup de symbols em memory store (P3, ~1 hora)
- **Arquivo**: `crates/touring-hooks/src/memory_store.rs`
- **API**: `stringzilla::Fingerprints` — sketch MinHash para detect near-duplicates antes de INSERT
- **Wiring**: `MemoryStore::store()` → MinHash check → skip se Jaccard > 0.9

---

### TIER 3 — Features Estratégicas (~4 horas)

#### T3.1 — Case-insensitive symbol search via `sz_find` (P3, ~1 hora)
- **Arquivo**: `crates/touring-hooks/src/cli_handlers_index.rs`
- **Mudança**: Adicionar flag `--ignore-case` em `touring index find` e `touring tantivy search`
- **Implementação**: Byteset lowercase normalization + sz_find em lowercase cache

#### T3.2 — Import detection via `sz_matches` (P3, ~1 hora)
- **Arquivo**: `crates/touring-analysis/src/quality/complexity.rs`
- **Mudança**: Substituir `str.lines().any(|l| l.starts_with("use "))` por `sz_matches(code, "use ")` count
- **Benefício**: Detecta imports em qualquer posição sem line-by-line overhead

#### T3.3 — AhoCorasick routing em `suggest_next` (P2, ~2 horas)
- **Arquivo**: `crates/touring-hooks/src/cli_handlers_suggest.rs`
- **Problema atual**: 18 chamadas `.contains()` sequenciais para routing de sugestões
- **Solução**: Static AhoCorasick (`Lazy<AhoCorasick>`) com patterns de routing; primeiro match define route
- **Wiring**: Lazy static inicializada uma vez, ~0 overhead em hot path

---

## 3. Ordem de Execução e DAG

```
T0.0 (bug fix) ──► validate cargo check OK
                           │
          ┌────────────────┼─────────────────┐
          ▼                ▼                 ▼
      T0.1 (reranker)  T0.2 (validator)  T0.3 (gotcha)
          │                │                 │
          └────────────────┼─────────────────┘
                           ▼
                    validate all TIER 0
                           │
         ┌─────────────────┼──────────────┐
         ▼                 ▼              ▼
     T1.1 (LLOC)       T1.2 (Halstead)  T1.3 (sz_hash)
         └─────────────────┴──────────────┘
                           │
                    validate TIER 1 (cargo test)
                           │
              T2.1 (BkTree) → T2.2 (batch) → T2.3 (MinHash)
                           │
                    validate TIER 2
                           │
              T3.1 + T3.2 + T3.3 (paralelo)
                           │
                    validate TIER 3 (full suite)
```

---

## 4. Mudanças em Cargo.toml

### workspace Cargo.toml — novas deps

```toml
# TIER 1 (std features)
stringzilla = { version = "4.6", features = ["std"] }

# TIER 2 (cpus feature — allocator-api2 polyfill para stable Rust)
allocator-api2 = { version = "0.3", features = ["alloc"] }
# stringzilla features atualizado para ["std", "cpus"]
```

### Crates que recebem deps diretas

| Crate | stringzilla feature | Motivo |
|-------|--------------------|---------| 
| `touring-analysis` | std | LLOC, Halstead, complexity metrics |
| `touring-generator` | std + cpus | BkTreeFuzzyAdapter, batch Levenshtein |
| `touring-hooks` | std | sz_hash em tantivy, MinHash em memory_store |

---

## 5. Acceptance Gate (Definition of Done)

```
□ T0.0 — cargo check --workspace EXIT:0 (E0658 eliminado)
□ T0.1 — reranker tests: 5+ PASS, zero .contains() em get_authority/compute_keyword_match
□ T0.2 — validator: 10+ tests PASS, 85%+ patterns migrados para starts_with
□ T0.3 — gotcha: 3+ tests PASS, SQL LIKE '%' eliminado de gotcha_count_for_file
□ T1.x — cargo test --workspace --exclude touring-python PASS (sem regressão)
□ T2.1 — BkTree: 10+ tests PASS, sz_edit_distance wired, feature simd-fuzzy ativa
□ T2.x — cargo test --workspace PASS
□ T3.x — cargo test --workspace PASS
□ touring doctor -j: 5/5 ok
□ Zero novos orphan pub symbols (touring wiring orphans contagem estável)
□ Hard Rule #11: zero git commands usados
```

---

## 6. Riscos e Mitigações

| Risco | Probabilidade | Mitigação |
|-------|---------------|-----------|
| `stringzilla` não suporta wasm32 target | Baixa | Feature gate `#[cfg(not(target_arch = "wasm32"))]` |
| `cpus` feature exige `allocator-api2` — conflito com outros | Baixa | Pin `allocator-api2 = "0.3.x"` exato |
| `sz_edit_distance` diverge de `levenshtein_dist` para Unicode | Média | Regression test com corpus Unicode |
| SQL gotcha cache invalidation | Baixa | OnceLock com `Arc<Vec<String>>` — rebuild quando `touring gotcha add` é chamado |
| BK-tree delete não suportado (insert-only) | Aceito | Use-case é append-only (symbols são adicionados, não removidos) |

---

## 7. Estimativas de Performance

| Mudança | Métrica Atual | Métrica Esperada | Ganho |
|---------|--------------|-----------------|-------|
| T0.1 reranker | ~8 sequential .contains() | 1 AhoCorasick scan | ~8× |
| T0.2 validator | 30 Regex eval por Bash hook | 1 starts_with per pattern | ~15× |
| T0.3 gotcha SQL | Leading wildcard LIKE | memmem O(n×m) | elimina full-scan |
| T1.1 LLOC | str.lines() iteration | sz_utf8_newline 10+ GB/s | ~3-5× |
| T2.1 BkTree | Vec<char> O(N×m×n) | BK-tree + sz_edit O(log N) | ~2125× |

---

## 8. Referências

- StringZilla v4.6.0 API: https://github.com/ashvardanian/StringZilla
- allocator-api2 polyfill: https://docs.rs/allocator-api2/0.3/
- memchr memmem: https://docs.rs/memchr/latest/memchr/memmem/
- AhoCorasick em touring-antt: `crates/touring-antt/src/keyword_matcher.rs`
- BkTreeFuzzyAdapter PLN2 TODO: `crates/touring-generator/src/core/context.rs:55`
