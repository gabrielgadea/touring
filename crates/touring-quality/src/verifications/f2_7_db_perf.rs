//! F2.7 — Database Performance verifier (D20).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_db_perf`] — a polyglot detector of the
//! two highest-confidence DB-performance anti-patterns across **7 languages**:
//! **N+1** (a curated DB-execution token — `.execute(`/`.query(`/`.fetch_*`/
//! `.findMany(`/`.Query(`/… — inside a `for`/`while` loop body, brace-matched for
//! brace languages and indent-scoped for Python) and **`SELECT *`** over-fetch.
//! It is disjoint from F2.1 OWASP (which scores SQL *injection* via the
//! `SecurityAnalyzer`) — F2.7 scores *performance*. The idiomatic fixes (a
//! batched `WHERE id IN (…)` / JOIN / ORM `include`; an explicit column list)
//! cannot be proven present, so the engine scores the anti-patterns.
//!
//! This replaces a stub that scored the ratio of `.query_async(`/`.fetch(` to
//! `.query(`/`.execute(`. Comments and `#[cfg(test)]`/test regions are excluded
//! via `code_regions` (production string literals are *not* suppressed, so a real
//! `"SELECT * FROM …"` is still seen).
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled `SELECT *`
//! density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F2.7 verifier — Database Performance.
#[allow(non_camel_case_types)]
pub struct F2_7_DbPerf;

impl Verification for F2_7_DbPerf {
    fn id(&self) -> DimId {
        DimId::F2_7
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_db_perf_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: polyglot DB-performance anti-pattern detection ───────────────
#[cfg(feature = "workspace-integration")]
fn analyze_db_perf_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_db_perf, score_db_perf};

    // The engine (`db_perf.rs`) embeds every DB token (`b".execute("`, `b".query("`,
    // `b"SELECT *"`, the loop headers `b"for "`/`b"while "`, …) as data, so scanning
    // its own source is a self-match false positive.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.7: detector own source (DB-perf needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_db_perf(&raw, lang);

    let value = score_db_perf(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F2.7: {} DB-performance anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_db_perf: N+1 query-in-loop / SELECT* over-fetch){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the DB-perf needle vocabulary (`.execute(`,
/// `.query(`, `SELECT *`, the loop headers, …) as detection data, so scoring their
/// own source is a self-match false positive. Mirrors
/// `f1_5_tech_debt::is_detector_own_source` (test/bench dirs + the quality engine
/// + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: SELECT * density heuristic ───────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_db_perf_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let over_fetch = raw.matches("SELECT *").count() + raw.matches("select *").count();
    let value = (1.0 - (over_fetch as f32 / lines) * 4.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{over_fetch} `SELECT *` over {} lines (heuristic; build --features \
         workspace-integration for polyglot N+1 / DB-performance analysis)",
        lines as usize
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_ext(content: &str, suffix: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_db_perf_returns_valid_score() {
        let f = write_temp_ext("conn.execute(\"DELETE WHERE id IN (?)\", p)?;\n", ".rs");
        let s = F2_7_DbPerf.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_db_perf_empty_file() {
        let f = write_temp("");
        let s = F2_7_DbPerf.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// A batched query (no loop) → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_batched_query_high() {
        let f = write_temp_ext(
            "fn purge(conn: &Conn, ids: &[i64]) -> Result<()> {\n    conn.execute(\"DELETE FROM t WHERE id IN (?)\", params![ids])?;\n    Ok(())\n}\n",
            ".rs",
        );
        let s = F2_7_DbPerf.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "batched query should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: an N+1 query-in-loop scores below a batched query —
    /// the stub (async ratio) was blind to it.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_n_plus_1_scores_lower() {
        let bad = write_temp_ext(
            "fn purge(conn: &Conn, ids: &[i64]) {\n    for id in ids {\n        conn.execute(\"DELETE FROM t WHERE id = ?\", params![id]).ok();\n    }\n}\n",
            ".rs",
        );
        let good = write_temp_ext(
            "fn purge(conn: &Conn, ids: &[i64]) {\n    conn.execute(\"DELETE FROM t WHERE id IN (?)\", params![ids]).ok();\n}\n",
            ".rs",
        );
        let sb = F2_7_DbPerf.check(bad.path()).expect("check");
        let sg = F2_7_DbPerf.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "N+1 ({}) < batched ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: Python indented query-in-loop flagged; a comprehension is not.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_python_n_plus_1_polyglot() {
        let bad = write_temp_ext(
            "def sync(users):\n    for u in users:\n        cursor.execute(\"SELECT 1 WHERE id = %s\", (u.id,))\n",
            ".py",
        );
        let good = write_temp_ext(
            "def names(users):\n    return [u.name for u in users]\n",
            ".py",
        );
        let sb = F2_7_DbPerf.check(bad.path()).expect("check");
        let sg = F2_7_DbPerf.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "Python N+1 ({}) must score below a comprehension ({})",
            sb.value,
            sg.value
        );
    }

    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/db_perf.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-storage/src/vec.rs"
        )));
    }
}
