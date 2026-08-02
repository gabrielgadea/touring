//! Database performance (D20 / F2.7) — polyglot N+1 and over-fetch smells.
//!
//! D20 asks whether the database is accessed efficiently. The single most
//! expensive, most common mistake is the **N+1 query**: issuing one query *per
//! item of a loop* (`for u in users { db.query(u.id) }`) instead of one batched
//! query (`WHERE id IN (…)`, a JOIN, or an ORM `include`). The second is
//! **over-fetch**: `SELECT *` pulls every column when a few are needed. The
//! idiomatic *good* forms (a batched `IN (…)` / `include` / `relationLoadStrategy:
//! "join"`, an explicit column list) cannot be proven present by a scanner, so
//! this engine detects the two high-confidence **anti-patterns**:
//!   1. **N+1** — a curated DB-execution token (`.execute(`/`.query(`/`.fetch_*`/
//!      `.findMany(`/`.Query(`/…) inside a *loop body* (`for`/`while`, brace-matched
//!      for brace languages, indent-scoped for Python).
//!   2. **`SELECT *`** — fetching every column instead of the needed ones.
//!
//! | Lang | N+1 loop form | Idiomatic fix |
//! |------|---------------|---------------|
//! | Rust | `for id in ids { conn.execute("DELETE … WHERE id = ?") }` | one `… WHERE id IN (…)` |
//! | Python | `for u in users:` ⇒ `cursor.execute(…)` | a JOIN / batched `IN` |
//! | TS/JS | `for (const u of users) { await prisma.post.findMany(…) }` | `include` / `in` filter |
//! | Go | `for _, id := range ids { db.Query(…) }` | a single `… IN (…)` |
//! | Java/C++ | `for (…) { stmt.executeQuery(…) }` | a batched query / JOIN |
//!
//! It is disjoint from **F2.1 OWASP** (which scores SQL *injection* — a
//! quote-break in a string sink — via the `SecurityAnalyzer`): F2.7 scores
//! *performance* (the same SQL string is read for `SELECT *`, never for a
//! quote-break). Comments and `#[cfg(test)]`/test regions are excluded via
//! `super::code_regions` (production string literals are deliberately *not*
//! suppressed, so a real `"SELECT * FROM …"` in code is still seen). Rolls up as
//! `AggKind::WeightedLoc`. ADVISORY-tier. Zero non-std deps beyond `memchr`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// Canonical language bucket (collapses extension aliases).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    TsJs,
    Go,
    Java,
    Cpp,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" => Lang::TsJs,
        "go" => Lang::Go,
        "java" => Lang::Java,
        "cpp" | "c++" | "cc" | "cxx" | "c" | "h" | "hpp" => Lang::Cpp,
        _ => Lang::Other,
    }
}

/// Curated DB-execution tokens — a call to one of these *inside a loop body* is
/// the N+1 signature. Kept DB-unambiguous on purpose (collection methods like
/// `.get(`/`.find(`/`.iter(` are deliberately excluded): `.execute(`/`.query(`
/// (sqlx / rusqlite / SQLAlchemy / JDBC), `.query_row(`/`.query_map(` (rusqlite),
/// `.fetch_one(`/`.fetch_all(`/`.fetch_optional(` (sqlx), `.findOne(`/`.findMany(`/
/// `.findUnique(`/`.findFirst(` (Prisma / Mongoose), `.Query(`/`.QueryRow(`/`.Exec(`
/// (Go `database/sql`).
const DB_TOKENS: &[&[u8]] = &[
    b".execute(",
    b".query(",
    b".query_row(",
    b".query_map(",
    b".fetch_one(",
    b".fetch_all(",
    b".fetch_optional(",
    b".findOne(",
    b".findMany(",
    b".findUnique(",
    b".findFirst(",
    b".Query(",
    b".QueryRow(",
    b".Exec(",
];

/// `SELECT *` over-fetch needles (the two common casings).
const SELECT_STAR: &[&[u8]] = &[b"SELECT *", b"select *"];

/// `true` if any `DB_TOKENS` entry occurs in `[s, e)` in executable (non-region)
/// position.
fn body_has_db_token(bytes: &[u8], regions: &[(usize, usize)], s: usize, e: usize) -> bool {
    let body = &bytes[s..e];
    for tok in DB_TOKENS {
        for off in memmem::find_iter(body, tok) {
            if !offset_suppressed(s + off, regions) {
                return true;
            }
        }
    }
    false
}

/// Detector 1 — N+1: count loop bodies (from the shared [`super::loop_blocks`]
/// finder) that contain a DB-execution token.
fn n_plus_one(bytes: &[u8], regions: &[(usize, usize)], lang: &str) -> usize {
    super::loop_blocks::loop_bodies(bytes, regions, lang)
        .into_iter()
        .filter(|&(s, e)| body_has_db_token(bytes, regions, s, e))
        .count()
}

/// Detector 2 — `SELECT *` over-fetch (production strings only; comment/test
/// occurrences are region-excluded).
fn select_star(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    for needle in SELECT_STAR {
        n += memmem::find_iter(bytes, needle)
            .filter(|&off| !offset_suppressed(off, regions))
            .count();
    }
    n
}

/// Per-file DB-performance analysis (parallel shape to [`super::idioms::IdiomReport`]).
#[derive(Debug, Clone, Default)]
pub struct DbPerfReport {
    /// Total DB-performance anti-patterns found in production code.
    pub violations: usize,
    /// Weighted sum (each anti-pattern scaled by its category weight).
    pub weighted_total: f32,
    /// Production lines considered (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired category, for evidence (highest count first).
    pub findings: Vec<(String, usize)>,
}

impl DbPerfReport {
    /// Record `count` occurrences of one anti-pattern category with `weight`.
    /// A zero count is a no-op.
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count == 0 {
            return;
        }
        self.violations += count;
        self.weighted_total += weight * count as f32;
        self.findings.push((message.to_string(), count));
    }
}

/// Analyze DB-performance anti-patterns for `lang`. Unknown languages still get
/// the language-agnostic `SELECT *` check (SQL is embedded in strings) but no
/// loop analysis.
#[must_use]
pub fn analyze_db_perf(source: &str, lang: &str) -> DbPerfReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();
    let canon = canonical_lang(lang);
    let mut report = DbPerfReport {
        total_lines: source.lines().count(),
        ..DbPerfReport::default()
    };

    let n1 = if canon == Lang::Other {
        0
    } else {
        n_plus_one(bytes, &regions, lang)
    };
    report.push(
        "DB query inside a loop (likely N+1) -> batch with `WHERE id IN (…)`, a JOIN, or an ORM `include`",
        n1,
        1.0,
    );
    report.push(
        "`SELECT *` fetches every column -> select only the needed columns",
        select_star(bytes, &regions),
        0.6,
    );

    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// D20 DB-performance score: `1 - density * SCALE`, where density is the weighted
/// anti-pattern count per production line. A query-efficient file is `1.0`. SCALE
/// is the style-tier `6.0` (ADVISORY); `WeightedLoc` roll-up.
#[must_use]
pub fn score_db_perf(report: &DbPerfReport) -> f32 {
    const SCALE: f32 = 6.0;
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Rust ──────────────────────────────────────────────────────────────────
    #[test]
    fn rust_n_plus_1_flagged_batched_clean() {
        // The real-world shape: one DELETE per id (the sqlite_vec.rs pattern).
        let bad = analyze_db_perf(
            "for id in ids {\n    conn.execute(\"DELETE FROM t WHERE id = ?\", params![id])?;\n}\n",
            "rust",
        );
        assert!(
            bad.violations >= 1,
            "query-in-loop must flag N+1: {:?}",
            bad.findings
        );
        assert!(
            bad.findings.iter().any(|(m, _)| m.contains("N+1")),
            "evidence names N+1: {:?}",
            bad.findings
        );
        // A single batched query (no loop) is clean.
        let good = analyze_db_perf(
            "conn.execute(\"DELETE FROM t WHERE id IN (?)\", params![ids])?;\n",
            "rust",
        );
        assert_eq!(
            good.violations, 0,
            "batched query is clean: {:?}",
            good.findings
        );
        assert!((score_db_perf(&good) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rust_loop_without_query_clean() {
        let r = analyze_db_perf("for x in items {\n    total += x.len();\n}\n", "rust");
        assert_eq!(
            r.violations, 0,
            "a loop without a DB call is not N+1: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_closure_in_iterator_expr_not_misscoped() {
        // The `{ y + 1 }` closure brace is inside the iterator expr (paren depth>0);
        // the loop body `{ total += y }` has no DB call → clean (no false N+1).
        let r = analyze_db_perf(
            "for y in xs.iter().map(|y| { y + 1 }) {\n    total += y;\n}\n",
            "rust",
        );
        assert_eq!(
            r.violations, 0,
            "closure brace must not misscope the body: {:?}",
            r.findings
        );
    }

    #[test]
    fn select_star_flagged_case_insensitive_columns_clean() {
        let bad = analyze_db_perf("let q = \"SELECT * FROM users\";\n", "rust");
        assert!(
            bad.violations >= 1,
            "SELECT * must flag: {:?}",
            bad.findings
        );
        let bad2 = analyze_db_perf("let q = \"select * from t\";\n", "rust");
        assert!(
            bad2.violations >= 1,
            "lowercase select * must flag: {:?}",
            bad2.findings
        );
        let good = analyze_db_perf("let q = \"SELECT id, email FROM users\";\n", "rust");
        assert_eq!(
            good.violations, 0,
            "explicit columns are clean: {:?}",
            good.findings
        );
    }

    // ── Python ────────────────────────────────────────────────────────────────
    #[test]
    fn python_n_plus_1_indent_flagged_comprehension_clean() {
        let bad = analyze_db_perf(
            "for u in users:\n    cursor.execute(\"SELECT 1 WHERE id = %s\", (u.id,))\n",
            "python",
        );
        assert!(
            bad.violations >= 1,
            "indented query-in-loop must flag: {:?}",
            bad.findings
        );
        // A list comprehension is not a loop body (the `for` is mid-line).
        let good = analyze_db_perf("rows = [process(x) for x in items]\n", "python");
        assert_eq!(
            good.violations, 0,
            "comprehension is not an N+1 loop: {:?}",
            good.findings
        );
    }

    // ── Go ──────────────────────────────────────────────────────────────────────
    #[test]
    fn go_three_clause_for_n_plus_1_flagged() {
        // Go `for init; cond; post {` has no parens and `;` at depth 0 — the
        // detector must still find the body.
        let bad = analyze_db_perf(
            "for i := 0; i < len(ids); i++ {\n    db.Query(\"SELECT 1 WHERE id = $1\", ids[i])\n}\n",
            "go",
        );
        assert!(
            bad.violations >= 1,
            "Go 3-clause for + .Query must flag: {:?}",
            bad.findings
        );
    }

    // ── TypeScript / JavaScript ─────────────────────────────────────────────────
    #[test]
    fn ts_for_of_n_plus_1_flagged() {
        let bad = analyze_db_perf(
            "for (const u of users) {\n  await prisma.post.findMany({ where: { authorId: u.id } });\n}\n",
            "typescript",
        );
        assert!(
            bad.violations >= 1,
            "for-of + findMany must flag: {:?}",
            bad.findings
        );
    }

    // ── C-style for ─────────────────────────────────────────────────────────────
    #[test]
    fn c_style_for_paren_semicolons_handled() {
        // `for (i=0; i<n; i++)` — the header semicolons are at paren depth 1.
        let bad = analyze_db_perf(
            "for (int i = 0; i < n; i++) {\n    stmt.execute(\"DELETE WHERE id = ?\");\n}\n",
            "java",
        );
        assert!(
            bad.violations >= 1,
            "C-style for + execute must flag: {:?}",
            bad.findings
        );
    }

    // ── Cross-cutting ──────────────────────────────────────────────────────────
    #[test]
    fn comments_and_tests_excluded() {
        // The smells live only in a comment and a #[cfg(test)] module.
        let src = "// for id in ids { conn.execute(\"SELECT * FROM t\") } — just docs\nfn prod() -> bool { true }\n#[cfg(test)]\nmod tests {\n    fn t() { for id in ids { conn.execute(\"SELECT * FROM x\"); } }\n}\n";
        let r = analyze_db_perf(src, "rust");
        assert_eq!(
            r.violations, 0,
            "comment/test smells excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn collection_methods_not_db_tokens() {
        // `.get(`/`.iter(`/`.find(` are collection methods, not DB calls.
        let r = analyze_db_perf(
            "for x in items {\n    let _ = map.get(&x.id);\n    let _ = v.iter().find(|y| **y == x);\n}\n",
            "rust",
        );
        assert_eq!(
            r.violations, 0,
            "collection methods are not N+1: {:?}",
            r.findings
        );
    }

    #[test]
    fn unknown_language_only_select_star() {
        // No loop analysis for unknown langs, but SELECT * (SQL in strings) still counts.
        let r = analyze_db_perf(
            "for u in users { conn.query(\"SELECT * FROM t\") }",
            "haskell",
        );
        assert_eq!(
            r.violations, 1,
            "only SELECT* (not the loop) for unknown lang: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_is_monotone_in_density() {
        let mk = |w: f32| DbPerfReport {
            weighted_total: w,
            total_lines: 100,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for w in [0.0, 1.0, 3.0, 6.0, 12.0] {
            let s = score_db_perf(&mk(w));
            assert!(s <= prev, "more DB-perf debt must not raise the score");
            prev = s;
        }
    }
}
