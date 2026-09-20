// #tags: kind:module lang:rust domain:memory purpose:snippet-trust process:learning-reward status:active
//! W3 d2 — measured trust for snippet memories (`#kind:snippet`).
//!
//! The TanStack snippets package computes a trust level and then never reads
//! it (*"metadata only — does not currently gate execution"*), never demotes,
//! and never invalidates (`dependsOn` written, never read). This module is
//! the port that closes those holes:
//!
//! - **Trust is earned by outcome**: `untrusted` → `provisional` (≥10
//!   executions, ≥90% success) → `trusted` (≥100, ≥95%) — the TanStack
//!   thresholds, kept for comparability.
//! - **Trust is LOST by outcome**: ≥50% failures across the last 10
//!   executions demotes one level — a `trusted` snippet that starts failing
//!   does not stay ✓ forever.
//! - **Trust is void when the ground shifts**: a changed `sig_hash` (the
//!   caller's digest of whatever the snippet depends on — binding signatures,
//!   schema, code) resets the ladder to `untrusted`, the `judge_attest`
//!   pattern applied to learned code.
//!
//! Consumers gate on [`TrustLevel`]: suggesters offer only `>= Provisional`,
//! and an `Untrusted` snippet runs only behind an explicit flag. The badge
//! (`✓`/`◐`/`○`) is shown TO the model — the detail worth copying from
//! TanStack.

use rusqlite::{Connection, OptionalExtension, Result, params};

/// DDL for the snippet-stats table — single source of truth (same rule as
/// `tags::TAG_TABLES_DDL`: add columns HERE, never a second hand-written copy).
pub const SNIPPET_STATS_DDL: &str = "CREATE TABLE IF NOT EXISTS snippet_stats (
        entry_key TEXT PRIMARY KEY,
        executions INTEGER NOT NULL DEFAULT 0,
        successes INTEGER NOT NULL DEFAULT 0,
        trust_level TEXT NOT NULL DEFAULT 'untrusted',
        sig_hash TEXT NOT NULL DEFAULT '',
        failure_window TEXT NOT NULL DEFAULT '[]',
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );";

/// The measured-trust ladder. Ordering is the gate: `>= Provisional` is
/// offerable, `Trusted` is preferred, `Untrusted` needs an explicit override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustLevel {
    /// New or invalidated — runs only behind an explicit flag.
    Untrusted,
    /// ≥10 executions at ≥90% success.
    Provisional,
    /// ≥100 executions at ≥95% success.
    Trusted,
}

impl TrustLevel {
    /// Canonical storage string.
    pub fn as_str(self) -> &'static str {
        match self {
            TrustLevel::Untrusted => "untrusted",
            TrustLevel::Provisional => "provisional",
            TrustLevel::Trusted => "trusted",
        }
    }

    /// Parse the storage string; unknown values degrade to `Untrusted`
    /// (fail-closed: corrupt state never grants trust).
    pub fn from_str_ci(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "provisional" => TrustLevel::Provisional,
            "trusted" => TrustLevel::Trusted,
            _ => TrustLevel::Untrusted,
        }
    }

    /// The badge shown to the model (the TanStack detail worth copying).
    pub fn badge(self) -> &'static str {
        match self {
            TrustLevel::Untrusted => "○",
            TrustLevel::Provisional => "◐",
            TrustLevel::Trusted => "✓",
        }
    }

    fn demoted(self) -> Self {
        match self {
            TrustLevel::Trusted => TrustLevel::Provisional,
            _ => TrustLevel::Untrusted,
        }
    }
}

/// A snippet's measured state, as read back by [`trust_of`].
#[derive(Debug, Clone)]
pub struct SnippetStats {
    /// Total recorded executions.
    pub executions: u64,
    /// Successful executions.
    pub successes: u64,
    /// Current trust level (already includes demotion/invalidation).
    pub trust: TrustLevel,
    /// Digest of the snippet's dependency surface at last record.
    pub sig_hash: String,
}

impl SnippetStats {
    /// Success rate in `0,1`; 0 when never executed.
    pub fn success_rate(&self) -> f64 {
        if self.executions == 0 {
            return 0.0;
        }
        self.successes as f64 / self.executions as f64
    }
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SNIPPET_STATS_DDL)
}

/// Number of trailing outcomes the demotion window looks at.
const WINDOW: usize = 10;
/// Failures within the window that trigger a one-level demotion.
const WINDOW_FAILURES_TO_DEMOTE: usize = 5;

fn ladder(executions: u64, successes: u64, current: TrustLevel) -> TrustLevel {
    let rate = if executions == 0 {
        0.0
    } else {
        successes as f64 / executions as f64
    };
    // Monotonic single-step promotion, same thresholds as TanStack.
    match current {
        TrustLevel::Untrusted if executions >= 10 && rate >= 0.90 => TrustLevel::Provisional,
        TrustLevel::Provisional if executions >= 100 && rate >= 0.95 => TrustLevel::Trusted,
        other => other,
    }
}

/// Record one execution outcome for `entry_key` and return the resulting
/// trust level. `sig_hash` is the caller's digest of the snippet's dependency
/// surface — when it differs from the stored one, the ladder RESETS to
/// `untrusted` before the outcome is counted (invalidation-on-change).
pub fn record_execution(
    conn: &Connection,
    entry_key: &str,
    success: bool,
    sig_hash: &str,
) -> Result<TrustLevel> {
    ensure_schema(conn)?;
    let row: Option<(i64, i64, String, String, String)> = conn
        .query_row(
            "SELECT executions, successes, trust_level, sig_hash, failure_window
             FROM snippet_stats WHERE entry_key = ?1",
            params![entry_key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;

    let (mut executions, mut successes, mut trust, stored_sig, window_json) = match row {
        Some((e, s, t, sig, w)) => (
            e.max(0) as u64,
            s.max(0) as u64,
            TrustLevel::from_str_ci(&t),
            sig,
            w,
        ),
        None => (0, 0, TrustLevel::Untrusted, String::new(), "[]".into()),
    };

    // Invalidation: the dependency surface changed → the history no longer
    // vouches for THIS snippet-world; restart the ladder (judge_attest).
    let mut window: Vec<bool> = if !stored_sig.is_empty() && stored_sig != sig_hash {
        executions = 0;
        successes = 0;
        trust = TrustLevel::Untrusted;
        Vec::new()
    } else {
        serde_json::from_str(&window_json).unwrap_or_default()
    };

    executions += 1;
    if success {
        successes += 1;
    }
    window.push(success);
    if window.len() > WINDOW {
        let excess = window.len() - WINDOW;
        window.drain(..excess);
    }

    trust = ladder(executions, successes, trust);
    // Demotion by recent-failure window — the hole TanStack admits: without
    // this, a trusted snippet that starts failing 100% stays ✓ forever.
    let recent_failures = window.iter().filter(|ok| !**ok).count();
    if window.len() >= WINDOW && recent_failures >= WINDOW_FAILURES_TO_DEMOTE {
        trust = trust.demoted();
        window.clear();
    }

    conn.execute(
        "INSERT INTO snippet_stats
            (entry_key, executions, successes, trust_level, sig_hash, failure_window, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
         ON CONFLICT(entry_key) DO UPDATE SET
            executions = excluded.executions,
            successes = excluded.successes,
            trust_level = excluded.trust_level,
            sig_hash = excluded.sig_hash,
            failure_window = excluded.failure_window,
            updated_at = excluded.updated_at",
        params![
            entry_key,
            executions as i64,
            successes as i64,
            trust.as_str(),
            sig_hash,
            serde_json::to_string(&window).unwrap_or_else(|_| "[]".into()),
        ],
    )?;
    Ok(trust)
}

/// The canonical prefix of a snippet's `entry_key`.
///
/// SINGLE SOURCE for the pair (writer, predicate). The W3 harvest hint told
/// the model to store the snippet under a FREE slug, while the reward bridge
/// only recognised keys starting with `snippet:` — so a snippet harvested by
/// the code-mode path could never reach this ladder. Both sides now derive
/// from here: whoever mints a key calls [`harvest_key`], whoever recognises
/// one calls [`is_snippet_key`].
pub const SNIPPET_KEY_PREFIX: &str = "snippet:";

/// Mint the `entry_key` of a snippet harvested from an executed program.
///
/// Idempotent: a slug that already carries the prefix is returned unchanged,
/// so re-harvesting the same snippet cannot fork the ladder into two rows.
pub fn harvest_key(slug: &str) -> String {
    let slug = slug.trim();
    if slug.starts_with(SNIPPET_KEY_PREFIX) {
        slug.to_string()
    } else {
        format!("{SNIPPET_KEY_PREFIX}{slug}")
    }
}

/// The predicate that recognises a snippet key — the mirror of [`harvest_key`].
pub fn is_snippet_key(entry_key: &str) -> bool {
    entry_key.starts_with(SNIPPET_KEY_PREFIX)
}

/// Digest of a snippet's BODY, used both as the ladder's `sig_hash` (so an
/// edited snippet is invalidated rather than inheriting its predecessor's
/// trust) and as the re-identification key of [`by_sig`].
///
/// Normalisation is deliberately shallow — trailing whitespace and blank
/// lines only. Anything deeper (comment stripping, formatting) would make two
/// programs that behave differently share one ladder.
pub fn code_sig(code: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for line in code.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

/// Re-identify a snippet from the body being executed: the ladder's own
/// `sig_hash` column IS the index, so recognising a re-run costs one indexed
/// lookup and needs no second table to drift out of sync.
///
/// Returns the `entry_key` of the matching snippet, or `None` when the body
/// was never harvested. Ties (two snippets with an identical body) resolve to
/// the most recently updated one — they are the same program either way.
pub fn by_sig(conn: &Connection, sig_hash: &str) -> Result<Option<String>> {
    ensure_schema(conn)?;
    if sig_hash.is_empty() {
        return Ok(None);
    }
    conn.query_row(
        "SELECT entry_key FROM snippet_stats
         WHERE sig_hash = ?1 ORDER BY updated_at DESC LIMIT 1",
        params![sig_hash],
        |r| r.get::<_, String>(0),
    )
    .optional()
}

/// What the trust ladder holds, in three numbers.
///
/// The reuse ruler needs to tell PERSISTING a block from REUSING one, and the
/// ladder is the only place that knows: a body is enrolled the first time it
/// runs and its `executions` grows on every later run of the SAME body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LadderTotals {
    /// Distinct bodies enrolled in the ladder.
    pub entries: u64,
    /// Executions summed over every enrolled body.
    pub executions: u64,
    /// Executions that RE-ran a body already enrolled (`executions - 1` each).
    ///
    /// This, not the enrolment count, is reuse: enrolling is the first time,
    /// which is by definition not a reuse of anything.
    pub reused: u64,
}

/// Aggregate the whole ladder in one pass.
///
/// Measured on 19/09/2026: 369 bodies enrolled, 364 of them run exactly once —
/// 9 re-executions in total. The library fills up and is never read back, which
/// is why `code_mode_reuse` reports FAIL for a behaviour, not for a bug.
pub fn ladder_totals(conn: &Connection) -> Result<LadderTotals> {
    ensure_schema(conn)?;
    conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(MAX(executions, 0)), 0), \
         COALESCE(SUM(MAX(executions - 1, 0)), 0) FROM snippet_stats",
        [],
        |r| {
            Ok(LadderTotals {
                entries: r.get::<_, i64>(0)?.max(0) as u64,
                executions: r.get::<_, i64>(1)?.max(0) as u64,
                reused: r.get::<_, i64>(2)?.max(0) as u64,
            })
        },
    )
}

/// One enrolled body that resembles the code about to run.
#[derive(Debug, Clone, PartialEq)]
pub struct SnippetMatch {
    /// The ladder key — what `touring memory recall <key>` fetches.
    pub entry_key: String,
    /// Jaccard overlap of identifier tokens, in `[0.0, 1.0]`.
    pub similarity: f64,
    /// The block's measured standing.
    pub trust: TrustLevel,
    /// How many times it has run.
    pub executions: u64,
}

/// Identifier-ish tokens of a program, lowercased, short ones dropped.
///
/// Deliberately crude: the question is "have I written this before?", which
/// shared identifiers answer well and an embedding would answer at 100× the
/// cost on a path that runs on every execution.
fn tokens(code: &str) -> std::collections::BTreeSet<String> {
    code.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 3)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Jaccard similarity of two token sets; 0.0 when either is empty.
fn jaccard(
    a: &std::collections::BTreeSet<String>,
    b: &std::collections::BTreeSet<String>,
) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 { 0.0 } else { inter / union }
}

/// Below this overlap two programs are simply different work.
///
/// Calibrated against the live library on 20/09/2026: the corpus is 371 bodies
/// and a threshold under ~0.3 starts matching any two scripts that both import
/// `json` and open a database.
pub const SIMILARITY_FLOOR: f64 = 0.35;

/// Blocks already in the library that resemble `code`, best first.
///
/// This is the missing half of the trust ladder. Persisting worked — 371 bodies
/// enrolled — but 364 of them ran exactly once (measured 19/09/2026), because
/// nothing ever told the author "you already wrote this". `code_mode_reuse`
/// reports that gap as a FAIL and its own docstring names the cause: the
/// problem is DISCOVERY, not persistence.
///
/// Bodies live in `memory_entries`; the ladder holds their standing. A body
/// identical to `code` is excluded — that is a re-run, which `by_sig` already
/// recognises, not a discovery.
pub fn similar_snippets(conn: &Connection, code: &str, limit: usize) -> Result<Vec<SnippetMatch>> {
    ensure_schema(conn)?;
    let mine = tokens(code);
    if mine.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let own_sig = code_sig(code);

    let mut stmt = conn.prepare(
        "SELECT s.entry_key, s.executions, s.trust_level, s.sig_hash, m.value \
         FROM snippet_stats s JOIN memory_entries m ON m.key = s.entry_key \
         WHERE m.value IS NOT NULL AND m.value != ''",
    )?;
    let mut achados: Vec<SnippetMatch> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?.max(0) as u64,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .filter_map(std::result::Result::ok)
        .filter(|(_, _, _, sig, _)| sig != &own_sig)
        .filter_map(|(entry_key, executions, trust, _sig, body)| {
            let similarity = jaccard(&mine, &tokens(&body));
            (similarity >= SIMILARITY_FLOOR).then(|| SnippetMatch {
                entry_key,
                similarity: (similarity * 1000.0).round() / 1000.0,
                trust: TrustLevel::from_str_ci(&trust),
                executions,
            })
        })
        .collect();

    // Best overlap first; ties go to the block with the longer record.
    achados.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.executions.cmp(&a.executions))
    });
    achados.truncate(limit);
    Ok(achados)
}

/// Read a snippet's measured state; `None` when never recorded.
pub fn trust_of(conn: &Connection, entry_key: &str) -> Result<Option<SnippetStats>> {
    ensure_schema(conn)?;
    conn.query_row(
        "SELECT executions, successes, trust_level, sig_hash
         FROM snippet_stats WHERE entry_key = ?1",
        params![entry_key],
        |r| {
            Ok(SnippetStats {
                executions: r.get::<_, i64>(0)?.max(0) as u64,
                successes: r.get::<_, i64>(1)?.max(0) as u64,
                trust: TrustLevel::from_str_ci(&r.get::<_, String>(2)?),
                sig_hash: r.get(3)?,
            })
        },
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        Connection::open_in_memory().expect("in-memory db")
    }

    #[test]
    fn snippet_stats_progression_untrusted_to_provisional_to_trusted() {
        let conn = mem();
        let mut last = TrustLevel::Untrusted;
        for _ in 0..10 {
            last = record_execution(&conn, "snippet:a", true, "sig1").expect("record");
        }
        assert_eq!(
            last,
            TrustLevel::Provisional,
            "10 successes at 100% promote"
        );
        for _ in 0..90 {
            last = record_execution(&conn, "snippet:a", true, "sig1").expect("record");
        }
        assert_eq!(last, TrustLevel::Trusted, "100 at 100% promote to trusted");
        let stats = trust_of(&conn, "snippet:a").expect("read").expect("some");
        assert_eq!(stats.executions, 100);
        assert!(stats.success_rate() > 0.99);
    }

    #[test]
    fn trusted_demotes_on_failure_window() {
        let conn = mem();
        for _ in 0..100 {
            record_execution(&conn, "snippet:b", true, "sig1").expect("record");
        }
        assert_eq!(
            trust_of(&conn, "snippet:b").unwrap().unwrap().trust,
            TrustLevel::Trusted
        );
        // 10 straight failures fill the window → demote one level.
        let mut last = TrustLevel::Trusted;
        for _ in 0..10 {
            last = record_execution(&conn, "snippet:b", false, "sig1").expect("record");
        }
        assert_eq!(
            last,
            TrustLevel::Provisional,
            "a trusted snippet that starts failing must NOT stay trusted (the TanStack hole)"
        );
    }

    #[test]
    fn sig_change_invalidates_trust() {
        let conn = mem();
        for _ in 0..10 {
            record_execution(&conn, "snippet:c", true, "sig1").expect("record");
        }
        assert_eq!(
            trust_of(&conn, "snippet:c").unwrap().unwrap().trust,
            TrustLevel::Provisional
        );
        // The dependency surface changed: history restarts.
        let level = record_execution(&conn, "snippet:c", true, "sig2").expect("record");
        assert_eq!(level, TrustLevel::Untrusted, "sig change resets the ladder");
        let stats = trust_of(&conn, "snippet:c").unwrap().unwrap();
        assert_eq!(stats.executions, 1, "history restarted");
        assert_eq!(stats.sig_hash, "sig2");
    }

    #[test]
    fn unknown_trust_string_degrades_to_untrusted() {
        assert_eq!(TrustLevel::from_str_ci("weird"), TrustLevel::Untrusted);
        assert_eq!(TrustLevel::from_str_ci("TRUSTED"), TrustLevel::Trusted);
    }

    #[test]
    fn badges_match_levels() {
        assert_eq!(TrustLevel::Trusted.badge(), "✓");
        assert_eq!(TrustLevel::Provisional.badge(), "◐");
        assert_eq!(TrustLevel::Untrusted.badge(), "○");
    }

    // ---- W3b: the harvest path reaches the ladder -------------------------

    #[test]
    fn every_minted_key_is_recognised_by_the_predicate() {
        // THE contract that was broken: the W3 hint minted a free slug while
        // the bridge only accepted `snippet:`-prefixed keys, so a code-mode
        // harvest could never be graded. Whatever `harvest_key` mints,
        // `is_snippet_key` MUST accept — asserted over the shapes a slug
        // actually takes, so a future minting rule cannot silently drift.
        for slug in [
            "scan-crates",
            "snippet:scan-crates",
            "  padded-slug  ",
            "with/slash#and-hash",
            "acentuação-e-hífen",
        ] {
            let key = harvest_key(slug);
            assert!(
                is_snippet_key(&key),
                "minted key {key:?} escapes its own predicate"
            );
        }
    }

    #[test]
    fn harvest_key_is_idempotent() {
        // Re-harvesting the same snippet must not fork the ladder into
        // `snippet:x` and `snippet:snippet:x` — two rows, two half-histories.
        let once = harvest_key("scan-crates");
        assert_eq!(once, "snippet:scan-crates");
        assert_eq!(harvest_key(&once), once);
    }

    /// Seed a body into BOTH the library and the ladder, as a harvest does.
    fn enrol(conn: &Connection, key: &str, body: &str, runs: u32) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory_entries (key TEXT PRIMARY KEY, value TEXT);",
        )
        .expect("memory table");
        conn.execute(
            "INSERT OR REPLACE INTO memory_entries (key, value) VALUES (?1, ?2)",
            params![key, body],
        )
        .expect("insert body");
        let sig = code_sig(body);
        for _ in 0..runs {
            record_execution(conn, key, true, &sig).expect("record");
        }
    }

    #[test]
    fn similar_snippets_finds_the_block_already_written() {
        let conn = mem();
        enrol(
            &conn,
            "snippet:censo-do-dag",
            "import sqlite3\ndef censo(caminho):\n    conn = sqlite3.connect(caminho)\n    \
             linhas = conn.execute('SELECT status FROM decomposition_subtasks')\n    \
             return list(linhas)\n",
            3,
        );
        enrol(
            &conn,
            "snippet:desenha-grafico",
            "import matplotlib\ndef plota(series):\n    fig = matplotlib.figure()\n    \
             fig.plot(series)\n    return fig\n",
            1,
        );

        // A new program that does the same DAG census with different names.
        let novo = "import sqlite3\ndef contar(caminho):\n    conn = sqlite3.connect(caminho)\n    \
                    linhas = conn.execute('SELECT status FROM decomposition_subtasks')\n    \
                    return len(list(linhas))\n";
        let achados = similar_snippets(&conn, novo, 5).expect("search");

        assert!(!achados.is_empty(), "the library holds this work already");
        assert_eq!(
            achados[0].entry_key, "snippet:censo-do-dag",
            "the census block must rank first, not the plotting one: {achados:?}"
        );
        assert!(achados[0].similarity >= SIMILARITY_FLOOR);
        assert_eq!(achados[0].executions, 3);
    }

    #[test]
    fn unrelated_work_matches_nothing() {
        let conn = mem();
        enrol(
            &conn,
            "snippet:desenha-grafico",
            "import matplotlib\ndef plota(series):\n    return matplotlib.figure().plot(series)\n",
            1,
        );

        let nada_a_ver = "fn main() { println!(\"hello\"); }\n";
        assert!(
            similar_snippets(&conn, nada_a_ver, 5)
                .expect("search")
                .is_empty(),
            "a floor that matches anything is worse than no search at all"
        );
    }

    /// An identical body is a RE-RUN (`by_sig` handles it), never a discovery —
    /// offering it back would tell the author to reuse what they just typed.
    #[test]
    fn an_identical_body_is_not_offered_as_a_discovery() {
        let conn = mem();
        let corpo = "import sqlite3\ndef censo(caminho):\n    return sqlite3.connect(caminho)\n";
        enrol(&conn, "snippet:censo", corpo, 2);

        assert!(
            similar_snippets(&conn, corpo, 5).expect("search").is_empty(),
            "the exact same body is a re-run, not something to discover"
        );
    }

    #[test]
    fn an_empty_library_or_an_empty_program_finds_nothing() {
        let conn = mem();
        assert!(similar_snippets(&conn, "x = 1", 5).expect("search").is_empty());
        enrol(&conn, "snippet:a", "def f():\n    return 1\n", 1);
        assert!(similar_snippets(&conn, "", 5).expect("search").is_empty());
        assert!(
            similar_snippets(&conn, "def f():\n    return 1\n", 0)
                .expect("search")
                .is_empty(),
            "limit 0 asks for nothing"
        );
    }

    #[test]
    fn code_sig_ignores_only_cosmetic_difference() {
        let a = "def f(x):\n    return x\nf(1)\n";
        // Trailing whitespace and blank lines are cosmetic: same program.
        let b = "def f(x):   \n\n    return x\nf(1)\n\n";
        assert_eq!(code_sig(a), code_sig(b));
        // A changed body is a DIFFERENT program — it must not inherit trust.
        assert_ne!(code_sig(a), code_sig("def f(x):\n    return x + 1\nf(1)\n"));
    }

    #[test]
    fn by_sig_reidentifies_a_rerun_and_ignores_unknown_bodies() {
        let conn = mem();
        let sig = code_sig("def f(x):\n    return x\nf(1)\n");
        record_execution(&conn, "snippet:scan", true, &sig).expect("record");

        assert_eq!(
            by_sig(&conn, &sig).expect("lookup"),
            Some("snippet:scan".to_string()),
            "a re-run of a harvested body must be recognised — this is what \
             lets the library grade itself from ordinary use"
        );
        assert_eq!(
            by_sig(&conn, &code_sig("print(1)")).expect("lookup"),
            None,
            "an unknown body must NOT be attributed to some existing snippet"
        );
        assert_eq!(
            by_sig(&conn, "").expect("lookup"),
            None,
            "an empty signature must never match the default-empty column"
        );
    }

    #[test]
    fn reruns_of_a_recognised_snippet_climb_the_ladder() {
        // The end-to-end property the whole elo exists for: executing the same
        // body repeatedly, with no hand-typed reward anywhere, promotes it.
        let conn = mem();
        let body = "def f(x):\n    return x\nf(1)\n";
        let sig = code_sig(body);
        let key = harvest_key("scan");
        for _ in 0..10 {
            let found = by_sig(&conn, &sig)
                .expect("lookup")
                .unwrap_or_else(|| key.clone());
            record_execution(&conn, &found, true, &sig).expect("record");
        }
        let stats = trust_of(&conn, &key).expect("read").expect("recorded");
        assert_eq!(stats.executions, 10);
        assert_eq!(stats.trust, TrustLevel::Provisional);
    }
}
