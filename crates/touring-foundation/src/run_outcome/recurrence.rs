//! Recurrence searches over the journal TAIL: how many times an exact body ran
//! lately ([`Journal::count_prior_same_hash`]) and which earlier runs were SIMILAR
//! to it ([`Journal::find_prior_similar`]).
//!
//! Only the last [`TAIL_BYTES`] are read: older runs fade out by design — the
//! question is "is this being re-run LATELY?", not "did it ever run". Every
//! search is fail-open: an I/O problem reads as "no precedent", never as a wrong
//! count.

use std::io::{Read, Seek, SeekFrom};

use serde_json::Value;

use super::journal::Journal;

/// How much of the END of the journal the searches look at (512 KiB).
pub(super) const TAIL_BYTES: u64 = 512 * 1024;

/// Which SimHash generation a similarity search compares. Only v2 against v2
/// (and v1 against v1): a line written before a version existed lacks its field
/// and must match nothing, instead of being compared across two different votes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimhashVersion {
    /// `code_simhash` — the original vote.
    V1,
    /// `code_simhash_v2` — one vote per distinct shingle.
    V2,
}

impl SimhashVersion {
    /// The journal field that carries this version's hash.
    const fn field(self) -> &'static str {
        match self {
            Self::V1 => "code_simhash",
            Self::V2 => "code_simhash_v2",
        }
    }
}

/// One similar prior: who, how far, and whether it was harvested.
#[derive(Debug, Clone)]
pub struct PriorSimilar {
    /// `run_id` of the earlier execution (the address to investigate).
    pub run_id: String,
    /// Hamming distance to the current body (≤ τ by construction).
    pub dist: u32,
    /// The `--harvest` slug of that run, when there was one (the ready route to reuse).
    pub harvest: Option<String>,
}

/// The result of a similarity search: the priors AND the tail lines that did
/// not parse. A skipped corrupt line hides that the journal is corrupt, and a
/// partial read (a prior that did not come back) looks like "no precedent".
#[derive(Debug, Default)]
pub struct PriorSearch {
    /// Priors within τ, nearest first.
    pub priors: Vec<PriorSimilar>,
    /// Tail lines (among those carrying the field) that did not parse.
    pub malformed_lines: u64,
}

impl Journal {
    /// The last [`TAIL_BYTES`] of the journal. Fail-open: `None`.
    fn tail(&self) -> Option<Vec<u8>> {
        let mut f = std::fs::File::open(&self.path).ok()?;
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES)))
            .ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    /// How many records in the journal tail carry this exact `code_hash`.
    /// Byte-level match on the fixed JSON key — no per-line parse. Fail-open:
    /// any I/O problem reads as zero (the nudge simply does not fire, never a
    /// wrong count).
    #[must_use]
    pub fn count_prior_same_hash(&self, hash: &str) -> u32 {
        let Some(buf) = self.tail() else {
            return 0;
        };
        let needle = format!("\"code_hash\":\"{hash}\"").into_bytes();
        if needle.len() > buf.len() {
            return 0;
        }
        buf.windows(needle.len())
            .filter(|w| *w == needle.as_slice())
            .count() as u32
    }

    /// The priors within Hamming distance `tau` of `simhash` in the journal
    /// tail. Filters by the field's substring before parsing a line (the tail
    /// holds hundreds; the parse runs only on lines that carry the field).
    /// Fail-open: an empty search.
    #[must_use]
    pub fn find_prior_similar(
        &self,
        version: SimhashVersion,
        simhash: u64,
        tau: u32,
    ) -> PriorSearch {
        let field = version.field();
        let marker = format!("\"{field}\"");
        let Some(buf) = self.tail() else {
            return PriorSearch::default();
        };
        let Ok(text) = String::from_utf8(buf) else {
            return PriorSearch::default(); // the tail cut a char in half — the next run recounts
        };
        let mut priors = Vec::new();
        let mut malformed_lines = 0u64;
        for line in text.lines().filter(|l| l.contains(&marker)) {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                malformed_lines += 1;
                continue;
            };
            let Some(h) = v
                .get(field)
                .and_then(Value::as_str)
                .and_then(|x| u64::from_str_radix(x, 16).ok())
            else {
                continue;
            };
            let dist = (h ^ simhash).count_ones();
            if dist <= tau {
                priors.push(PriorSimilar {
                    run_id: v
                        .get("run_id")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string(),
                    dist,
                    harvest: v.get("harvest").and_then(Value::as_str).map(str::to_string),
                });
            }
        }
        priors.sort_by_key(|p| p.dist);
        PriorSearch {
            priors,
            malformed_lines,
        }
    }
}
