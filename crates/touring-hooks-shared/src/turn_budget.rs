//! P3/S-3.3 (2026-09-04) — the injection budget of a TURN, which is the unit the
//! window is actually paid in.
//!
//! [`crate::cila`] caps a single call (800 / 2 000 / 4 000 chars by level, and
//! it is tested). It has never bounded a turn, and a turn is many calls:
//! measured over 10 transcripts / 406 user turns, hook injection was **26,6 % of
//! everything that crossed the window — 6 650 bytes per turn, ~1 662 tokens**,
//! while no single call came close to its own ceiling. A per-call cap on a
//! per-turn cost bounds nothing.
//!
//! The ceiling here is the plan's target (halve the measured baseline), and the
//! ORDER of what gets dropped is the policy, stated once so it can be audited:
//!
//! ```text
//! gate deny  >  structural enrichment  >  pillar nudge  >  post-tool echo
//! ```
//!
//! A deny never reaches this module — the gates return before any budget is
//! consulted, which is the right structure: a refusal that a budget could
//! silence would be a gate in name only.

use std::path::{Path, PathBuf};

/// Default ceiling in bytes of injected context per user turn.
///
/// 3 000 halved the 6 650 measured on 04/09/2026; the S8 remeasurement of
/// 08/10/2026 (163 turns, 12 transcripts: p50 = 16 072 B/turn, p90 = 129 820)
/// lowered it to 2 000 — with section-level dedupe the legitimate spend of a
/// turn is the NEW sections (~2 blocks × ~900 B), and 2 000 is 12,4 % of the
/// median turn, the same halving policy over the new ruler. Overridable per
/// environment (`TOURING_TURN_BUDGET`) exactly like the per-call budgets, so
/// calibration is a decision someone makes and not a constant someone edits.
pub const TURN_BUDGET_DEFAULT: usize = touring_foundation::cila::INJECTION_CEIL_BYTES_PER_TURN;

/// A turn with no observed end is bounded by this instead, so a session that
/// never fires the reset cannot hold a budget open forever.
const TURN_TTL_SECS: u64 = 900;

/// Where a session's turn spending lives.
///
/// A FILE, not a process cache, and the reason is topological: the suggester
/// runs in the daemon while `touring-hook prompt-enhance` — the only event that
/// means "the human spoke again" — runs as an ephemeral process. A `OnceLock`
/// cache would give each of them its own copy, and the reset would be a wire
/// that LOOKS connected and is not, which is worse than no reset at all. The
/// file is the same idiom the bypass budget already uses for the same reason.
fn turn_file(root: &Path, session: &str) -> PathBuf {
    let safe: String = session
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    root.join(".claude/touring")
        .join(format!("turn-{safe}.txt"))
}

/// Read `(spent, age_secs)`; a file older than the TTL reads as a fresh turn.
fn read_turn(root: &Path, session: &str) -> usize {
    let path = turn_file(root, session);
    let Ok(meta) = std::fs::metadata(&path) else {
        return 0;
    };
    let stale = meta
        .modified()
        .ok()
        .and_then(|m| m.elapsed().ok())
        .is_some_and(|age| age.as_secs() > TURN_TTL_SECS);
    if stale {
        return 0;
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

/// The ceiling in force, honouring `TOURING_TURN_BUDGET`.
#[must_use]
pub fn turn_budget() -> usize {
    std::env::var("TOURING_TURN_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(TURN_BUDGET_DEFAULT)
}

/// A new user turn began: the budget starts over.
///
/// Fail-open: an unwritable path simply leaves the previous count, which caps
/// harder than intended and never blocks the session.
pub fn reset_turn(root: &Path, session: &str) {
    let _ = std::fs::remove_file(turn_file(root, session));
}

/// Charge `bytes` to this turn and report what the turn has spent INCLUDING them.
pub fn charge_turn(root: &Path, session: &str, bytes: usize) -> usize {
    let total = read_turn(root, session) + bytes;
    let path = turn_file(root, session);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, total.to_string());
    total
}

/// What the turn has spent so far, without charging anything.
#[must_use]
pub fn spent_this_turn(root: &Path, session: &str) -> usize {
    read_turn(root, session)
}

/// True when a directive's program CLOSES on its own line.
///
/// Medido no corpus (400 sessões, S-7, 26/10/2026): 58,0 % das ordens MUST da
/// família `code-mode-contract` chegavam sintaticamente inválidas — 1.070 com
/// aspas ímpares, 513 com heredoc sem terminador — porque o emissor original
/// corta o payload no meio do programa e esta função mantinha a linha inteira.
/// O modelo não executa metade de um programa, e a "não-adesão" medida era de
/// ordem quebrada, não de modelo que ignora. Precedente do harness2
/// (`spec.py:163-167`): truncamento que fabrica defeito é pior que ausência.
/// Ambos os sinais erram na direção SEGURA: em dúvida, omite.
fn directive_program_closes(line: &str) -> bool {
    !has_heredoc_operator(line) && quotes_balance(line)
}

/// O terminador de um heredoc tem de estar sozinho na própria linha, e uma
/// diretiva É uma linha — um `<<` aqui significa que o corpo ficou de fora.
/// `<<<` (herestring) fecha na própria linha, então não conta.
fn has_heredoc_operator(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'<' {
            if bytes.get(i + 2) == Some(&b'<') {
                i += 3; // <<< herestring: consome o trio inteiro, nao o re-le
                continue;
            }
            return true;
        }
        i += 1;
    }
    false
}

/// Aspas abrem e fecham aos pares. O ideoma shell `'\''` é uma aspa ESCAPADA
/// dentro de aspas simples — some antes de contar — e `\` torna literal a aspa
/// seguinte.
fn quotes_balance(line: &str) -> bool {
    let normalized = line.replace("'\\''", "");
    let b = normalized.as_bytes();
    let (mut singles, mut doubles) = (0usize, 0usize);
    let mut j = 0;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 1,
            b'\'' => singles += 1,
            b'"' => doubles += 1,
            _ => {}
        }
        j += 1;
    }
    singles % 2 == 0 && doubles % 2 == 0
}

/// Keep only the directive lines of a block — what survives when the turn's
/// budget is gone.
///
/// The MUST/SHOULD lines ARE the instruction; the rest is rationale, which is
/// worth its bytes in a turn with room and is the first thing to go in one
/// without. A directive whose program does not close (heredoc cortado, aspa
/// órfã) é OMITIDA — meia ordem não executa, e a ausência é declarada no
/// rodapé. Returns `None` when there is no directive to keep, because a block
/// with nothing to act on has nothing worth spending the last bytes on.
#[must_use]
pub fn directives_only(context: &str) -> Option<String> {
    let mut omitted = 0usize;
    let kept: Vec<&str> = context
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("MUST") || t.starts_with("SHOULD")
        })
        .filter(|l| {
            let closes = directive_program_closes(l);
            omitted += usize::from(!closes);
            closes
        })
        .collect();
    if kept.is_empty() {
        return None;
    }
    let omission_note = if omitted > 0 {
        format!("; {omitted} diretiva(s) truncada(s) omitida(s) — programa não fecha")
    } else {
        String::new()
    };
    Some(format!(
        "{}\n  (orçamento do turno esgotado — só as diretivas; \
         `TOURING_TURN_BUDGET=<bytes>` ajusta o teto{omission_note})",
        kept.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charging_accumulates_and_resetting_starts_the_turn_over() {
        let d = tempfile::tempdir().expect("tempdir");
        let r = d.path();
        assert_eq!(spent_this_turn(r, "s1"), 0);
        assert_eq!(charge_turn(r, "s1", 1000), 1000);
        assert_eq!(charge_turn(r, "s1", 500), 1500, "o turno soma as chamadas");
        reset_turn(r, "s1");
        assert_eq!(spent_this_turn(r, "s1"), 0, "o turno novo comeca do zero");
    }

    #[test]
    fn one_sessions_spending_is_never_charged_to_another() {
        let d = tempfile::tempdir().expect("tempdir");
        let r = d.path();
        charge_turn(r, "sessao-a", 5000);
        assert_eq!(
            spent_this_turn(r, "sessao-b"),
            0,
            "duas sessoes CC concorrentes nao partilham orcamento"
        );
    }

    /// O estado do turno atravessa PROCESSOS: o suggester vive no daemon e o
    /// prompt-enhance e' efemero. Um cache de processo daria a cada um a sua
    /// copia, e o reset seria um fio que parece ligado.
    #[test]
    fn the_turn_state_survives_a_fresh_reader_because_it_lives_on_disk() {
        let d = tempfile::tempdir().expect("tempdir");
        let r = d.path();
        charge_turn(r, "cross-proc", 2500);
        // Nenhum estado em memoria compartilhado: so' o arquivo.
        assert_eq!(spent_this_turn(r, "cross-proc"), 2500);
        assert!(turn_file(r, "cross-proc").exists());
    }

    #[test]
    fn the_ceiling_is_the_measured_baseline_halved_and_is_overridable() {
        // 6 650 B/turno medidos em 04/09/2026 → alvo 3 000; S8 (08/10/2026):
        // p50 do turno = 16 072 B e, com a dedupe por seção, o gasto legítimo
        // é ~2 blocos novos × ~900 B → 2 000 (12,4 % do p50, mesma política).
        assert_eq!(TURN_BUDGET_DEFAULT, 2000);
        assert_eq!(turn_budget(), TURN_BUDGET_DEFAULT);
    }

    #[test]
    fn only_the_directives_survive_and_the_elision_says_so() {
        let block = "[TOURING SUGGEST]\n  MUST touring doctor -j\n            // razao longa\n  SHOULD touring status -j\n  MAY grep -rn x\n  Reason: prosa extensa que explica\n  sig=outcome:bash:cat";
        let cut = directives_only(block).expect("ha' diretivas");
        assert!(cut.contains("MUST touring doctor -j"));
        assert!(cut.contains("SHOULD touring status -j"));
        assert!(
            !cut.contains("Reason: prosa"),
            "a racionalizacao e' a 1a a cair"
        );
        assert!(!cut.contains("MAY grep"), "MAY cai antes de MUST/SHOULD");
        assert!(
            cut.contains("orçamento do turno esgotado"),
            "a elisao e' declarada"
        );
        assert!(cut.len() < block.len());
    }

    #[test]
    fn a_block_with_no_directive_has_nothing_worth_the_last_bytes() {
        assert_eq!(directives_only("[generic]\n  algum eco"), None);
    }

    /// S-7 (26/10/2026): 58,0 % das MUSTs de code-mode-contract chegavam
    /// quebradas (1.070 aspas ímpares, 513 heredocs sem terminador). A ordem
    /// truncada é omitida — o modelo não executa metade de um programa — e a
    /// omissão é declarada no rodapé.
    #[test]
    fn a_directive_cut_mid_program_is_omitted_and_the_omission_is_declared() {
        let block = "[CODE MODE — contrato]\n  \
            MUST    touring run --lang bash --code 'mkdir -p /tmp/c3 && cat > /tmp/c3/a.py <<'\\''PYEOF'\\''\n  \
            MUST    touring ast tdg crates/foo/src/lib.rs\n  \
            SHOULD  touring index find Foo";
        let cut = directives_only(block).expect("uma diretiva valida sobrevive");
        assert!(
            !cut.contains("heredoc") && !cut.contains("PYEOF"),
            "a ordem com heredoc sem terminador nao chega ao modelo: {cut}"
        );
        assert!(
            cut.contains("MUST    touring ast tdg"),
            "a ordem que FECHA fica"
        );
        assert!(cut.contains("SHOULD  touring index find Foo"));
        assert!(
            cut.contains("1 diretiva(s) truncada(s) omitida(s)"),
            "a omissao e' declarada: {cut}"
        );
        // Contrato do cli_suggester_e2e.rs e do injection_family (S-3):
        // o prefixo do rodapé e' a assinatura do caminho — intocado.
        assert!(
            cut.contains("orçamento do turno esgotado — só as diretivas; `TOURING_TURN_BUDGET=<bytes>` ajusta o teto"),
            "o prefixo exato do rodape e' preservado: {cut}"
        );
    }

    #[test]
    fn odd_quotes_mark_a_truncated_program_and_the_line_falls() {
        let block = "  MUST    touring run --lang bash --code 'ls -la\n  \
                     SHOULD  touring doctor -j";
        let cut = directives_only(block).expect("SHOULD valida");
        assert!(
            !cut.contains("ls -la"),
            "aspa orfa = programa cortado: {cut}"
        );
        assert!(cut.contains("touring doctor -j"));
    }

    #[test]
    fn the_shell_idiom_and_the_herestring_close_on_their_own_line() {
        // `'\''` e' aspa escapada dentro de aspas simples — o programa fecha.
        // Texto shell real: bash -c 'echo 'it'\''s''  (a sequencia de 4 chars
        // quote-backslash-quote-quote e' o ideoma normalizado pelo conserto).
        let idiom = "  MUST    bash -c 'echo 'it'\\''s''";
        let cut = directives_only(idiom).expect("o ideoma shell e' valido");
        assert!(cut.contains("echo"), "o ideoma '\\'' sobrevive: {cut}");
        // herestring e' de linha unica — nao e' heredoc.
        let herestring = "  MUST    bash -c 'cat <<< \"done\"'";
        assert!(
            directives_only(herestring).is_some(),
            "<<< fecha na propria linha"
        );
        // aspas escapadas por barra nao contam como abertura/fechamento.
        let escaped = "  MUST    bash -c \"echo \\\"ok\\\"\"";
        assert!(directives_only(escaped).is_some());
    }

    #[test]
    fn when_every_directive_is_truncated_there_is_nothing_to_keep() {
        let block = "  MUST    touring run --lang bash --code 'cat <<'\\''EOF\n  \
                     SHOULD  touring run --lang bash --code 'echo";
        assert_eq!(
            directives_only(block),
            None,
            "bloco sem diretiva executavel nao gasta os ultimos bytes"
        );
    }
}
