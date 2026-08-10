//! A2 — the real tokenizer for the context-savings ledger.
//!
//! `gate_metrics::record_compression_savings` counts bytes exactly and asks a
//! registered counter for tokens. This module is that counter for the **daemon**
//! — the process where compression and routing actually happen, and therefore
//! the only process whose counters mean anything.
//!
//! # Why it lives here and not in `touring-cortex`
//!
//! `cl100k_base` already exists in `touring_cortex::enrichment::count_tokens`,
//! but `touring-cortex` depends on `touring-hooks`, so the daemon cannot call
//! it without inverting the graph. Rather than move the tokenizer, the ledger
//! exposes a `fn` seam and each process installs whatever it can afford. The
//! encoding is the same (`cl100k_base`), so the numbers are comparable.
//!
//! # Cost
//!
//! `cl100k_base()` builds a BPE table — a one-off cost paid on the FIRST
//! savings event, never at startup, and never at all in a daemon whose routing
//! subsystem stays dormant. Set `TOURING_TOKEN_METER=0` to keep tokens
//! unmeasured (the ledger then reports `token_method: "not_measured"`, which is
//! the honest answer, not a zero).

use std::sync::OnceLock;

/// The lazily-built encoder. `None` = the build failed; every subsequent call
/// reports "not measurable" instead of a fabricated count.
static ENCODER: OnceLock<Option<tiktoken_rs::CoreBPE>> = OnceLock::new();

/// Counts `cl100k_base` tokens, or `None` when no encoder could be built.
fn count_tokens(text: &str) -> Option<usize> {
    ENCODER
        .get_or_init(|| match tiktoken_rs::cl100k_base() {
            Ok(bpe) => Some(bpe),
            Err(e) => {
                tracing::warn!(
                    target: "touring::token_meter",
                    err = %e,
                    "cl100k_base unavailable — context savings will report tokens as not measured"
                );
                None
            }
        })
        .as_ref()
        .map(|bpe| bpe.encode_ordinary(text).len())
}

/// Installs the counter unless `TOURING_TOKEN_METER=0`.
///
/// Returns `true` when this call installed it. Idempotent: a second call is a
/// no-op returning `false` (the ledger's `OnceLock` keeps the first writer).
pub fn install() -> bool {
    if std::env::var("TOURING_TOKEN_METER").unwrap_or_default() == "0" {
        return false;
    }
    crate::shared::gate_metrics::set_token_counter(count_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cl100k_counts_are_real_not_a_byte_heuristic() {
        // The whole reason A2 exists: bytes/4 is not a token count. Pick text
        // where the two visibly disagree.
        let text = "fn main() { println!(\"{}\", 42); }";
        let tokens = count_tokens(text).expect("cl100k must be available in-tree");
        assert!(tokens > 0);
        assert_ne!(
            tokens,
            text.len() / 4,
            "if these ever coincide the assertion is vacuous — pick other text"
        );
    }

    #[test]
    fn empty_text_is_zero_tokens_not_a_failure() {
        assert_eq!(count_tokens(""), Some(0));
    }
}
