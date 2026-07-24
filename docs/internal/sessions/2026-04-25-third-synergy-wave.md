# Third Synergy Wave — 3 RFC-100 Diagnostic Emission Sites

**Date**: 2026-04-25 | **Session**: TACO L4+ | **Skill**: Touring v4.15.0

## Objetivo

Potencializar 3 gaps identificados por VP-Scout após a segunda Synergy Wave (v4.14.0).
Todos os building blocks (structs RFC-100, métodos, bandit API) já existiam; apenas as
conexões com os sites de emissão faltavam.

## Sumário Executivo

| ID | Synergy | Files Modified | Tests Added |
|----|---------|---------------|-------------|
| G1 | Decompose Create + GranularityBandit | `cli_handlers.rs` | 1 |
| G2 | Pre-Edit Blast + BlastWarning B-300 | `pre_edit.rs` | 2 |
| G3 | Memory Recall + MemoryFinding M-5xx | `cli_handlers.rs` | 2 |
| **TOTAL** | | **2 files** | **5 tests** |

## Resultados FASE 6

- `cargo check --workspace`: EXIT:0
- `touring-hooks` --lib: **3220 PASS, 0 failed** (era 3217, +3 novos)
- `touring-server` --lib: **408 PASS, 0 failed**
- Total: **3628 PASS, 0 failed**
- Orphan baseline: **9106** (preservado — zero novos orphans)
- Hook Registry: **172** (sem novos handlers — wiring in-process)

## Detalhes por Synergy

### G1 — Decompose Create + GranularityBandit Advisory

**Arquivo**: `crates/touring-hooks/src/cli_handlers.rs`
**Função**: `cli_decompose_create()` (linha 2108)

Gap: `GranularityBandit::select_split()` existia desde Wave C1 (2026-04-20) mas
nunca era chamado em `cli_decompose_create`. A função retornava `cila_level` do payload
(ou default=3) sem consultar o bandit que aprende empiricamente qual granularidade
produz melhores resultados.

Fix:
```rust
// G1: GranularityBandit advisory — when cila_level not explicitly provided,
// consult the bandit for a recommended optimal subtask split.
let cila_provided = payload.get("cila_level").is_some();
let (bandit_split_factor, bandit_subtasks) = if !cila_provided {
    let factor = rt.select_task_split(0, "general", cila_level.min(4) as u8);
    let count = factor.subtask_count() as i64;
    (format!("{factor:?}"), count)
} else {
    (String::from("explicit"), cila_level)
};
```

Resposta agora inclui:
```json
{
  "task_id": "task_...",
  "cila_level": 3,
  "bandit_split_factor": "Split3",
  "bandit_subtasks": 3
}
```

**Por que**: `GranularityBandit` aprende via RL reward quais splits produzem melhor
qualidade. Sem ser consultado no momento de criação de tasks, o aprendizado acumulado
ficava invisível ao fluxo de decomposição principal.

### G2 — Pre-Edit Blast + BlastWarning B-300 RFC-100

**Arquivo**: `crates/touring-hooks/src/pre_edit.rs`
**Função**: `run_returning()` (linha 134, blast signal block)

Gap: `BlastWarning::HighBlast` (RFC-100 B-300) existia em `touring-analysis` com
`to_diagnostic()`, `code_str()`, `severity_class()` mas nunca era emitido em produção.
O pre_edit já calculava o blast radius (via cache de session_bus ou SymbolIndex),
mas não surfaceava isso como diagnostic estruturado RFC-100.

Fix:
```rust
if count > 10 {
    use touring_analysis::blast_radius::BlastWarning;
    let w = BlastWarning::HighBlast {
        symbol: rel_path.clone(),
        affected_files: count,
        threshold: 10,
    };
    tracing::warn!(
        code = w.code_str(),
        message = %format!("{count} files affected by blast from {rel_path} (threshold=10)"),
        severity = "warning",
        file_path = %rel_path,
    );
}
```

**Por que**: RFC-100 B-300 é o código canônico para avisar que um edit terá alto impacto
transitivo. O pre_edit é exatamente o momento certo para emitir este warning — antes
do edit ser aplicado. O blast radius em cache (populado pelo pre_read) está disponível
como `Option<usize>` neste ponto.

### G3 — Memory Recall + MemoryFinding M-5xx RFC-100

**Arquivo**: `crates/touring-hooks/src/cli_handlers.rs`
**Função**: `cli_memory_recall()` (linha 1600)

Gap: `MemoryFinding` (M-500..M-530) existia com 4 variantes diagnósticas mas nunca
era chamado em nenhum site de produção. O `cli_memory_recall` já executava recall
SQL + ANN + TF-IDF + RRF fusion, mas não emitia diagnósticos estruturados sobre
o que acontecia internamente.

Fix (3 emissões):
```rust
// G3: RFC-100 M-5xx — emit structured MemoryFinding diagnostics at recall time.
if merged_entries.is_empty() {
    let f = MemoryFinding::RecallEmpty { query: query.to_string() };
    tracing::info!(code = f.code_str(), %query, "recall empty for query");
}
if !tfidf_results.is_empty() {
    let f = MemoryFinding::TfidfActivated {
        candidate_count: tfidf_results.len(),
        corpus_size: entries_len,
    };
    tracing::debug!(code = f.code_str(), candidate_count = tfidf_results.len(), "tfidf activated");
}
if !ann_results.is_empty() || !tfidf_results.is_empty() {
    let source_count = usize::from(entries_len > 0)
        + usize::from(!ann_results.is_empty())
        + usize::from(!tfidf_results.is_empty());
    let f = MemoryFinding::RrfFusion { source_count, merged_count: merged_entries.len() };
    tracing::debug!(code = f.code_str(), source_count, "rrf fusion");
}
```

Requerido `let entries_len = entries.len()` antes do merge (evitar borrow-after-move).

**Por que**: `MemoryFinding` é o subsistema RFC-100 para observabilidade do memory recall.
Sem emissões, os códigos M-500..M-530 ficavam como dead code diagnostico — definidos
mas nunca observados em produção. Agora cada recall emite diagnósticos estruturados
capturáveis via `tracing::Subscriber` (OpenTelemetry, console, etc.).

## FP Evitado

- **G4 (api_cascade SubtaskProposal)**: VP-Scout Chain 3 confirmou `plan_api_cascade`
  já wired via `cascade_queue.rs` + `cli_cascade_queue_drain`. Falso positivo evitado.

## Lições Aprendidas

1. **Dead code diagnóstico**: RFC-100 structs sem emission sites = observabilidade nula.
   O pattern correto: definir o struct RFC-100 + wire em todos os sites relevantes.
2. **Borrow-before-move**: quando um `Vec` é movido em um `if-else`, capturar `.len()`
   antes da expressão condicional para usar em código posterior.
3. **Advisory vs decisivo**: bandit advisory (G1) adiciona contexto sem forçar override —
   `cila_provided` check preserva a intenção explícita do caller.
4. **Blast threshold = 10**: threshold selecionado conservador (SKILL.md recomenda
   cuidado para blast_radius > 10). Consistência com a tabela de thresholds existente.

## Touring CLI Changes

Nenhuma nova CLI command adicionada. `cli_decompose_create` agora retorna
`bandit_split_factor` + `bandit_subtasks`. `cli_memory_recall` emite M-5xx diagnostics.
`pre_edit::run_returning` emite B-300 warning via tracing para blast alto.
