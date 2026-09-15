//! CILA (Cognitive Intent-Level Architecture) token budgets — THE source.
//!
//! Lives in `touring-foundation` because two crates were computing it
//! independently and disagreeing: `touring-hooks-shared::cila` (read
//! 800/2000/4000) and `touring-cortex::enrichment::compute_context_budget`
//! (L2 = 1200, L4-5 = 3200, L6+ = 4800) — different numbers at 3 of 5 levels,
//! for the same question. Both crates depend on this one, so the answer can be
//! given once. `touring-hooks-shared::cila` re-exports these, which keeps its
//! thirteen call sites untouched.
//!
//! Measured consequence of the split (04/09/2026): tightening one scale never
//! moved the other, so the dial appeared not to work.

/// Teto de bytes de injeção por turno — o contrato da economia de contexto.
///
/// Cross-audit 04/09/2026: este número vivia em DUAS fontes — `TURN_BUDGET_DEFAULT`
/// (o executor que corta o enriquecimento) e `INJECTED_BYTES_PER_TURN_CEIL` (a régua
/// que reprova o turno). Duas fontes para um número é o defeito que esta sessão
/// inteira persegue: o executor podia cortar em 3000 enquanto a régua reprovava em
/// outro valor, e nada os reconciliava. Aqui é o teto; os dois leem daqui.
pub const INJECTION_CEIL_BYTES_PER_TURN: usize = 3000;

/// Fração máxima de bytes injetados que pode ser repetição literal dentro da janela.
pub const INJECTION_DUPLICATE_RATIO_CEIL: f64 = 0.05;

/// Fração mínima das diretivas MUST emitidas que o modelo precisa seguir para que a
/// injeção esteja se pagando. Abaixo disso, o que entra na janela é custo sem retorno.
/// Baseline medida: 0,492.
///
/// Este piso é, por construção, uma medição de LIMITE SUPERIOR: um MUST conta como
/// "seguido" quando o próximo comando Bash casa com ele — o que também aconteceria se
/// o modelo fosse rodar aquele comando de qualquer jeito. O número só pode superestimar
/// o efeito do nudge, nunca subestimá-lo. Vale dizer em voz alta, porque uma razão que
/// parece saudável aqui ainda pode não estar comprando nada.
pub const INJECTION_FOLLOW_RATIO_FLOOR: f64 = 0.60;

#[inline]
/// Resolve a CILA token budget given an env-var key prefix and default values.
///
/// Checks `<env_prefix>_L0`, `<env_prefix>_L2`, `<env_prefix>_L4` for overrides.
/// Falls back to the provided defaults when no env var is set.
pub fn cila_budget(cila_level: u8, env_prefix: &str, low: usize, mid: usize, high: usize) -> usize {
    let env_key = match cila_level {
        0 | 1 => format!("{env_prefix}_L0"),
        2 | 3 => format!("{env_prefix}_L2"),
        _ => format!("{env_prefix}_L4"),
    };
    if let Ok(val) = std::env::var(&env_key)
        && let Ok(n) = val.parse::<usize>()
    {
        return n;
    }
    match cila_level {
        0 | 1 => low,
        2 | 3 => mid,
        _ => high,
    }
}

/// Budget for `pre_read` hooks (conservative — read context is smaller).
///
/// L0-L1: 800 | L2-L3: 2000 | L4+: 4000
#[inline]
pub fn cila_budget_read(cila_level: u8) -> usize {
    cila_budget(cila_level, "TOURING_CILA_BUDGET", 800, 2000, 4000)
}

/// Budget for `pre_edit` hooks (50% larger than read — edit context needs more).
///
/// L0-L1: 1200 | L2-L3: 3000 | L4+: 6000
#[inline]
pub fn cila_budget_edit(cila_level: u8) -> usize {
    cila_budget(cila_level, "TOURING_CILA_BUDGET_EDIT", 1200, 3000, 6000)
}

/// Budget for `pre_write` hooks (same as edit).
///
/// L0-L1: 1200 | L2-L3: 3000 | L4+: 6000
#[inline]
pub fn cila_budget_write(cila_level: u8) -> usize {
    cila_budget(cila_level, "TOURING_CILA_BUDGET_WRITE", 1200, 3000, 6000)
}
