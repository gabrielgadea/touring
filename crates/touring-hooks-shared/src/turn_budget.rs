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
/// 3 000 halves the 6 650 measured on 04/09/2026. Overridable per environment
/// (`TOURING_TURN_BUDGET`) exactly like the per-call budgets, so calibration is
/// a decision someone makes and not a constant someone edits.
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

/// Keep only the directive lines of a block — what survives when the turn's
/// budget is gone.
///
/// The MUST/SHOULD lines ARE the instruction; the rest is rationale, which is
/// worth its bytes in a turn with room and is the first thing to go in one
/// without. Returns `None` when there is no directive to keep, because a block
/// with nothing to act on has nothing worth spending the last bytes on.
#[must_use]
pub fn directives_only(context: &str) -> Option<String> {
    let kept: Vec<&str> = context
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("MUST") || t.starts_with("SHOULD")
        })
        .collect();
    if kept.is_empty() {
        return None;
    }
    Some(format!(
        "{}\n  (orçamento do turno esgotado — só as diretivas; \
         `TOURING_TURN_BUDGET=<bytes>` ajusta o teto)",
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
        // 6 650 B/turno medidos em 04/09/2026 → alvo 3 000.
        assert_eq!(TURN_BUDGET_DEFAULT, 3000);
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
}
