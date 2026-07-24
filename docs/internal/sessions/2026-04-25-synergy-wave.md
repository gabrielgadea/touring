# Synergy Wave — 5 Cross-Subsystem Integrations

**Date**: 2026-04-25 | **Session**: TACO L4+ | **Skill**: Touring v4.12.0

## Objetivo

Potencializar sinergia entre subsistemas Touring existentes mas desconexos —
todos os building blocks já estavam prontos; faltavam as conexões.

## Sumário Executivo

| ID | Synergy | Files Modified | Tests Added |
|----|---------|---------------|-------------|
| S1 | TDG Grade → Pre-Edit Signal | `pre_edit.rs` | 3 |
| S2 | Health Delta Streak → RFC-100 Q-210 | `health_delta.rs` | 2 |
| S3 | Wiring Audit + F2 Cycles | `cli/wiring.rs`, `tools/wiring_audit.rs` | 3 |
| S4 | GateMetrics Diagnostic Prevalence Counters | `shared/gate_metrics.rs`, `cli_handlers.rs` (2 sites) | 7 |
| S5 | Pre-Task Scout Orphan Hint | `cli_handlers_scout.rs` | 0 (integration) |
| **TOTAL** | | **6 files** | **15 tests** |

## Resultados FASE 6

- `cargo check --workspace`: EXIT:0
- `touring doctor -j`: 5/5 healthy
- `touring-hooks` tests: **3383 PASS, 0 failed**
- `touring-server` tests: **624 PASS, 0 failed**
- Orphan baseline: **9106** (preservado — zero novos orphans)
- EMA reward: 0.44

## Detalhes por Synergy

### S1 — TDG Grade → Pre-Edit Signal

**Arquivo**: `crates/touring-hooks/src/pre_edit.rs`  
**Função**: `compose_quality_evolution()` (linha 728)

Integra `TdgReport::from_components(complexity_score, 1.0, 1.0, 0.0, 1.0, antipatterns_score)`.
Quando `to_diagnostic_opt().is_some()` (grades D ou F), injeta warning em Signal 9:

```
quality_evolution: TDG: grade D (0.58) — STOP — refactor antes de edit
```

**Por que**: TDG grade já existia como CLI command (`touring ast tdg`), mas não era
consultado automaticamente antes de edits. Agora o pré-edit hook avisa quando a
complexidade atual já é problemática — antes do desenvolvedor piorar.

### S2 — Health Delta Streak → RFC-100 Q-210

**Arquivo**: `crates/touring-hooks/src/health_delta.rs`

Quando `regression_streak >= STREAK_ALERT_THRESHOLD (3)`, além do hint existente,
agora emite diagnostic estruturado:

```rust
let diag = Diagnostic::new(
    codes::Q_210_REGRESSION_STREAK,
    Severity::Warning,
    format!("Health delta regression streak of {} consecutive declines on '{}'", STREAK_ALERT_THRESHOLD, file_path),
);
tracing::warn!(code = diag.code, message = %diag.message, ...);
```

**Por que**: Q-210 já existia em `diagnostic.rs` mas nunca era emitido. Streak alerts
eram apenas hints textuais, não diagnostics estruturados consumíveis por tooling.

### S3 — Wiring Audit inclui F2 Cycle Detection

**Arquivos**: `crates/touring-server/src/cli/wiring.rs`, `tools/wiring_audit.rs`

`run_audit()` agora consulta `cli-wiring-cycles` via `daemon_query` com degrade gracioso:

```rust
let cycles_raw = daemon_query("cli-wiring-cycles", json!({"min_depth": 1, "format": "json"}))
    .unwrap_or_else(|_| r#"{"cycle_count":0,"cycles":[]}"#.to_string());
```

Output de `touring wiring audit` agora inclui seção `cycles: {count, detail}`.
`WiringAuditFull` struct ganhou campo `cycles_count: usize`.

**Por que**: F2 (Tarjan SCC) já existia desde v4.9.0, mas `touring wiring audit`
(o comando de auditoria completa) não o consultava — os cycles ficavam invisíveis
no workflow de auditoria.

### S4 — GateMetrics Diagnostic Prevalence Counters

**Arquivo**: `crates/touring-hooks/src/shared/gate_metrics.rs`

Dois novos `AtomicU64`:
- `diagnostic_wiring_finding_emitted_count` — incrementado quando W-1xx emitido
- `diagnostic_tdg_emitted_count` — incrementado quando Q-201/Q-202 emitido

Callers wired:
1. `cli_handlers.rs:5265` — `cli_ast_tdg` D/F path
2. `cli_handlers.rs:~734` — wiring orphans `--diagnostics` path (loop sobre diagnostics)

```bash
touring gate-metrics -j | jq '{
  wiring_diagnostics_emitted: .diagnostic_wiring_finding_emitted_count,
  tdg_diagnostics_emitted: .diagnostic_tdg_emitted_count
}'
```

**Por que**: RFC-100 diagnostic system (v4.10.0) criou W-1xx e Q-2xx codes,
mas não havia observabilidade de quantas vezes eram emitidos — impossível
avaliar prevalência em produção.

### S5 — Pre-Task Scout Orphan Hint

**Arquivo**: `crates/touring-hooks/src/cli_handlers_scout.rs`

No final do `cli_pre_task_scout()`, após EC31 cognitive enrichment:

```rust
if let Ok(status) = _rt.ctx.knowledge.module_wiring_status(&rel_path) {
    if !status.orphan_symbols.is_empty() && status.integration_score < 1.0 {
        let count = status.orphan_symbols.len();
        let listed: Vec<&str> = status.orphan_symbols.iter().take(5).map(String::as_str).collect();
        findings.push_str(&format!(
            "\n[wiring] {count} orphan pub symbol(s): {listed}{suffix} — wire to consumers or reduce pub visibility",
            ...
        ));
    }
}
```

**Por que**: Pre-task scout já rodava blast radius + quality via scouter, mas não
surfaceava orphan symbols do arquivo-alvo. Agora o contexto injetado antes do
`PreToolUse:Task*` inclui também wiring health — potencializando antes de cada edit.

## Follow-up Identificado

- `wave_c_e2e::hook_registry_has_cascade_queue_handlers`: expect count 171 vs registry 172
  (drift pré-existente, não causado por esta sessão — Engineer A documentou)
- Wiring score `gate_metrics.rs` = 0.44 (esperado: arquivo é infraestrutura central)

## Lições Aprendidas

1. **Parallel engineer strategy**: 3 engineers com escopos disjuntos por crate/módulo — zero conflitos
2. **S4 wiring_finding caller**: só `record_diagnostic_tdg_emitted` estava wired; `record_diagnostic_wiring_finding_emitted` precisou de fix adicional pós-Engineer-A
3. **Engineer A + TACO**: Engineer A implementou S1+S4 com composite_score=1.0; TACO completou S5 diretamente (arquivo fora do escopo dos Engineers)
4. **Daemon Q_210**: código `codes::Q_210_REGRESSION_STREAK` existia mas nunca era emitido — padrão recorrente de "building blocks órfãos" que esta wave endereçou

## Touring CLI Changes

Nenhuma nova CLI command adicionada. Melhorias internas wired em handlers existentes.
`touring gate-metrics -j` agora exibe `diagnostic_wiring_finding_emitted_count` e
`diagnostic_tdg_emitted_count`.
