# Ninth Wave — Synergy Deepening (Production Wiring + Cross-Crate Reuse)

**Date**: 2026-04-26 | **Session**: TACO L4 (solo implementation) | **Skill**: Touring v4.21.0

## Objetivo

Continuação direta da Wave 8 — segundo pedido de Gabriel: "promova uma
potencialização, aperfeiçoamento e maximização das funcionalidades do Touring
através de uma maior sinergia e integração de toda a sua estrutura".

Wave 8 documentou 7 deferred opportunities. Wave 9 fecha 1 (#7) com production
wiring real, cria 2 novas integrações cross-subsystem (S8 + S9), e potencializa
3 collateral fixes (REGRA #0).

## Verdict: 3 deliverables + 3 fixes (escopo conservador, alto valor)

| ID | Deliverable | Subsistemas conectados | Valor |
|----|-------------|------------------------|-------|
| **S7** | miette + syntect production wiring | Diagnostic + cli_ast_blast + wiring orphans | ALTO |
| **S8** | composite_health_score em instructions_loaded | touring-core::health (novo) + instructions_loaded hook | ALTO |
| **S9** | touring synergy ganha gate_metrics enrichment | synergy CLI + gate-metrics counters | MÉDIO |
| **Fix1** | cargo config split-debuginfo cleanup | `.cargo/config.toml` | LOW (REGRA #0) |
| **Fix2** | devrcfile hooks → ALL_DAEMON_HOOK_NAMES const | hook_registry coverage 174→176 | LOW (REGRA #0) |
| **Fix3** | Compile error em cli_tasksfile_export | cli_handlers_decompose.rs | MEDIUM (estava bloqueando build) |

**Não-deliverables intencionais** (deferred para Wave 10+):
- Opps 1,2,3,4,5,6 do Wave 8 (sem novos drivers)

## Sumário Executivo

| Component | Arquivos modificados | LOC | Tests novos |
|-----------|---------------------|-----|-------------|
| S7 helper + production wires | `touring-core/src/diagnostic.rs` + `touring-hooks/src/cli_handlers.rs` | ~140 | 6 (4 core + 3 hooks - shared infra) |
| S8 health module + wiring | `touring-core/src/{lib,health}.rs` + `touring-server/src/cli/status.rs` + `touring-hooks/src/instructions_loaded.rs` | ~250 | 10 (7 core + 3 hooks) |
| S9 synergy enrichment | `touring-server/src/cli/synergy.rs` | ~140 | 4 |
| Fix1 cargo config | `.cargo/config.toml` | -1 | 0 |
| Fix2 devrcfile | `touring-hooks/src/hook_registry.rs` | +6 | 0 |
| Fix3 tasksfile compile | `touring-hooks/src/cli_handlers_decompose.rs` | -1 | 0 (resolved by upstream schema removal) |
| **TOTAL** | **8 arquivos** | **~534 LOC** | **20 tests** |

## Detalhes por Deliverable

### S7 — miette + syntect production wiring

**Arquivos**:
- `crates/touring-core/src/diagnostic.rs`
- `crates/touring-hooks/src/cli_handlers.rs`

Adiciona 2 APIs em `touring-core::diagnostic`:

```rust
// Top-level helper for raw-JSON diagnostic producers
pub fn read_source_snippet(file_path: &str, max_bytes: usize) -> Option<String>

// Builder method on Diagnostic struct
pub fn try_attach_source_from_file(self, file_path: &str, max_bytes: usize) -> Self
```

Ambas com:
- Hard ceiling 64 KiB (default 4 KiB para editor-window)
- UTF-8 char-boundary truncation
- Graceful degrade em I/O error (None / unchanged)
- Backfill de `file` field se ausente

**Production sites wired (Wave 9 closes Wave 8 S1 loop)**:

1. `cli_ast_blast` (linha 3401, RFC-100 B-300 BlastWarning) — diagnostic JSON
   ganha `source_snippet` quando `file_path` é legível
2. `cli_wiring_orphans` (linha 736, RFC-100 W-100 OrphanSymbol) — cada
   orphan diagnostic chama `.try_attach_source_from_file(&module_file, 4096)`

**Closes loop**: Wave 8 S1 implementou os data fields; Wave 9 S7 conecta
producers reais. Antes: 0 production callers; depois: 2 sites + helper
disponível para todos os 27 RFC-100 codes.

**Tests novos**: 6 (4 em touring-core + 3 em touring-hooks - 1 reutiliza
infra do touring-core).

### S8 — composite_health_score em instructions_loaded

**Arquivos**:
- `crates/touring-core/src/health.rs` (NOVO, ~210 LOC)
- `crates/touring-core/src/lib.rs` (+`pub mod health`)
- `crates/touring-server/src/cli/status.rs` (refator: delegate to core)
- `crates/touring-hooks/src/instructions_loaded.rs` (+`push_health_parts`)

**Refactor crítico**: `compute_composite_health_score` movido de
`touring-server/src/cli/status.rs` para `touring-core::health`. CLI
mantém wrapper para backward compat.

**Nova API em touring-core::health**:

```rust
pub fn compute_composite_health_score(combined: &Map<String, Value>) -> f64
pub const DEGRADED_SCORE_THRESHOLD: f64 = 0.5;
pub fn compose_degraded_warning(score: f64) -> Option<String>
```

**Production wiring**:

`instructions_loaded.rs::push_health_parts()` agora:
1. Captura `GateMetricsSnapshot::capture()` (cache_ratio + outstanding)
2. Lê `runtime.learning.online_rl.ema_reward()`
3. Chama `compute_composite_health_score` com map sintético
4. Se score < 0.5, push warning via `compose_degraded_warning`

**Closes loop**: Wave 8 S3 expôs o score em `touring status -j`. Wave 9 S8
torna ele PROACTIVELY VISIBLE no session start — operador sabe ANTES da
primeira edit que o sistema está degradado.

**Tests novos**: 10 (7 em touring-core::health + 3 em
instructions_loaded para validar threshold + warning composition).

### S9 — touring synergy ganha gate_metrics enrichment

**Arquivo**: `crates/touring-server/src/cli/synergy.rs`

Wave 8 S6 introduziu `touring synergy` como meta-observability. Wave 9 S9
torna ele dinâmico: por wired_pair, atribui counter live de gate_metrics.

**Novo catálogo**: `WIRED_PAIR_METRICS` (10 entries) mapeia `(producer, consumer)`
→ counter key:

```rust
("pre_edit", "TDG (touring-analysis)", "diagnostic_tdg_emitted_count"),
("pre_edit", "blast_radius cache", "blast_inject_count"),
("post_edit", "health_delta compute", "health_delta_compute_count"),
("post_edit", "query_cache invalidate", "query_cache_invalidate_count"),
// ... 6 more
```

**Nova flag CLI**: `--with-metrics`

```bash
touring synergy wired -j --with-metrics | jq '.pairs[] | select(.metrics)'
# {"producer":"pre_edit","consumer":"TDG","metrics":{"counter":"diagnostic_tdg_emitted_count","value":17}}
```

**Graceful degrade**: Daemon unreachable → enrichment silenciosamente
omitida; pairs ainda renderizam catálogo estático.

**Catalogue update**: WIRED_PAIRS expandido de 37 → 43 com 6 novos entries
documentando S7+S8+S9 (production wirings + helper module).

**Tests novos**: 4 (enrichment attaches, leaves unmapped untouched,
handles `wired` subcommand shape, no dangling entries in mapping table).

### Fix1 — cargo config split-debuginfo cleanup

**Arquivo**: `.cargo/config.toml`

`split-debuginfo = "unpacked"` estava em `[build]` — cargo 1.93.1
não reconhece sob esse path (é setting de profile). Comentário-fonte
substituído por nota explicativa.

REGRA #0 — encontrar é corrigir.

### Fix2 — devrcfile hooks no ALL_DAEMON_HOOK_NAMES const

**Arquivo**: `crates/touring-hooks/src/hook_registry.rs`

`cli-devrcfile-import` + `cli-devrcfile-export` estavam em
`all_daemon_hook_names()` (linhas 290/291) e em dispatch table (1294/1297)
mas faltavam de `ALL_DAEMON_HOOK_NAMES` const. Mesmo padrão da Wave 8
collateral fix para tasksfile.

Hook Registry: 174 → 176. Test assertion atualizado para 176.

### Fix3 — cli_tasksfile_export compile error

**Arquivo**: `crates/touring-hooks/src/cli_handlers_decompose.rs`

Compile error pré-existente: `if let Some(pg) = parallel_group` no
closure body referenciava variável não bound no tuple. Investigação
revelou que coluna `parallel_group` foi removida do schema da tabela
`decomposition_subtasks` em sessão anterior. Removidas as referências
órfãs (REVERT da minha tentativa inicial de adicionar a column).

## Methodology — Pre-Scout Ultrathink (5ª wave consecutiva)

Padrão: sequential-thinking + grep mapping + composite_health_score
analysis ANTES de scout pesado.

**Discovery crítico Wave 9**:
- `with_source_snippet` (Wave 8 S1) tem **zero production consumers** —
  apenas tests. Ergo S7 é prime closes-the-loop opportunity.
- composite_health_score=0.4894 está **abaixo do próprio threshold de 0.5
  da Wave 8 S3** — sistema reportando degradação mas sem mecanismo de
  surfacing. Driver claro para S8.
- `synergy wired` retorna `{count, pairs}` (não `wired_pairs`) — by
  design (subcommand-specific shape), não bug.

## Comparison Wave 8 vs Wave 9

| Aspect | Wave 8 (Synergy Maximization) | Wave 9 (Synergy Deepening) |
|--------|-------------------------------|----------------------------|
| Target | INTERNAL synergy gaps | DEFERRED #7 + PROACTIVE surfacing + observability of observability |
| Verdict | 4 deliverables + 2 fixes | 3 deliverables + 3 fixes |
| Code mods | ~505 LOC + 13 tests | ~534 LOC + 20 tests |
| Methodology | pre-scout ultrathink | pre-scout ultrathink + composite_health_score self-detect |
| Cross-crate refactor | Q-201/Q-202 emission only | health module promoted to touring-core |

Wave 9 promove um conceito chave de Wave 8 (composite_health_score)
para `touring-core` — primeira vez na série de waves que código de
diagnóstico é elevado a foundation crate para cross-crate reuse.

## Lições Aprendidas

1. **"Wired field" ≠ "wired site"**: Wave 8 S1 wired data fields
   (`source_snippet`); Wave 9 S7 wired production sites. Closes-the-loop
   é trabalho separado e necessário.

2. **Self-detecting health degradation**: composite_health_score=0.49
   provou seu próprio valor — ferramenta detectou seu próprio gap (não
   havia surfacing proativo). Isto valida a métrica.

3. **Cross-crate refactor para reuse genuíno**: mover
   `compute_composite_health_score` para touring-core não é over-engineering
   — é pré-requisito para wiring em touring-hooks. Sem o move, instructions_loaded
   teria que duplicar 60 linhas de logic ou sub-process daemon.

4. **REGRA #0 cascading**: 3 collateral fixes em uma wave indica que
   manutenção de invariantes (hook_registry count, schema columns,
   cargo config keys) precisa de CI guards mais agressivos. Wave 10
   candidate: gate em `cargo check --workspace` zero warnings.

5. **Drift entre `all_daemon_hook_names()` e const**: padrão repetiu
   tasksfile (Wave 8) → devrcfile (Wave 9). Wave 10 candidate: macro
   ou test que sincroniza ambas automaticamente.

## Touring CLI Changes

- **Nova flag**: `touring synergy --with-metrics` (Wave 9 S9)
- **Novo módulo**: `touring_core::health` (compute_composite_health_score
  + compose_degraded_warning + DEGRADED_SCORE_THRESHOLD)
- **Hook Registry**: 174 → 176 (cli-devrcfile-import/export)
- **WIRED_PAIRS catalogue**: 37 → 43 (S7+S8+S9 entries)
- **SYNERGY_OPPORTUNITIES**: 7 → 7 (#7 refraseado, mantido para colour render)

## Production Verification

```
cargo check --workspace        → EXIT 0, 0 warnings, 0 errors
cargo test -p touring-core     → 159 PASS
cargo test -p touring-hooks    → 3230 PASS, 1 ignored
cargo test -p touring-server   → 223 PASS
TOTAL                           → 3612 tests, 0 failures
```

## Deferred — Wave 10+

Quando drivers emergirem:
- **wiring impact transitive em pre_edit**: condicional ao saturation do
  current 14 signals
- **MCTS rollout → GranularityBandit reward**: requer actor boundary refinement
- **diary → memory ingestion**: requer schema migration
- **plan-speculate → wiring chain advisory**: requer typestate refactor
- **miette ANSI colour rendering**: requer GraphicalReportHandler composition
  no terminal-side caller
- **Generic Trace<K> primitive**: requer driver A/B test
- **FunctionApproximator trait**: requer 4º implementação como justificativa
- **CI gate**: cargo check --workspace zero warnings + hook_registry sync test

## See Also

- Wave 8 (Synergy Maximization): `~/.claude/rust/docs/2026-04-26-eighth-wave-synergy-maximization.md`
- Wave 7 (rsrl analysis): `~/.claude/rust/docs/2026-04-26-seventh-wave-rsrl.md`
- Wave 6 (BugStalker): `~/.claude/rust/docs/2026-04-26-sixth-wave-bugstalker.md`
- Reference: `touring synergy -j` para catálogo atualizado
- `~/.claude/skills/Touring/SKILL.md` v4.21.0 section
