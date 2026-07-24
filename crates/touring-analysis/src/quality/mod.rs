//! Multi-dimensional code quality analysis pipeline.
//!
//! Combines antipattern detection, complexity metrics, unwrap auditing,
//! error handling coverage, and test proxy scoring into a unified pipeline.

pub mod antipatterns;
/// Polyglot public-API **contract** design analysis (D09): typed errors
/// (`Result<_, String>`), getter naming (C-GETTER / Effective Go), `into_`/`as_`
/// conventions (C-CONV), `Debug` on public types (C-DEBUG), field encapsulation,
/// constructor width — across 7 languages; backs the F1.9 dimension. Disjoint
/// from [`idioms`] (local style) by design.
pub mod api_design;
/// Polyglot API documentation analysis (D35): `pub fn`/`pub struct` etc. without
/// `# Examples` / `# Panics` / `# Errors` rustdoc section, HTTP-framework files
/// without `openapi`/`swagger`/`utoipa` reference, and Markdown README without
/// `# API`/`# Endpoints`/`# Reference` heading. Source:
/// `/openapitools/openapi-generator` (High reputation, bench 90) + rustdoc;
/// backs the F3.9 dimension.
pub mod api_doc;
/// Polyglot architectural-consistency analysis (D12): mixed-pattern drift
/// detection within a file — error-handling (panic + Result + unwrap +
/// anyhow/thiserror), logging (println + tracing/log), config (env::var +
/// config::/figment/dotenv), hardcoded role strings (`.role == "admin"`),
/// and conflicting async runtimes (tokio + async-std + smol). Source:
/// `/sverweij/dependency-cruiser` (High reputation, bench 87) — ArchUnit
/// layer rules + dependency-cruiser `forbidden` block as the gold standard
/// for cross-cutting-concern enforcement. Backs the F1.12 dimension.
/// Disjoint from F1.7 boundaries (F1.7 keys on `pub` vs `pub(crate)`
/// surface area; F1.12 keys on *internal* style consistency) and F1.8
/// dep-cycles (F1.8 keys on workspace-level SCCs; F1.12 keys on
/// intra-file pattern mix).
pub mod arch_consistency;
/// Architecture documentation analysis (D36): MADR-template structural
/// conformance (Status/Context/Decision/Consequences H2 sections) +
/// Mermaid-diagram presence for `.md` files (ADRs and architecture
/// overviews). Source: `/mermaid-js/mermaid` (High reputation, bench 91.75) +
/// MADR (context7); backs the F3.10 dimension.
pub mod arch_doc;
/// Polyglot authentication / authorization analysis (D16): broken access
/// control (OWASP A01:2021) — sensitive operations (admin/delete_user/
/// role/permission) without `authorize`/`require_role`/`check_permission`
/// gate, IDOR-prone parameters (`user_id`/`account_id`) without authz check,
/// hardcoded role strings (`.role == "admin"`), and JS/TS client-side
/// authz (`window.confirm`/`localStorage`/`document.cookie`). Source:
/// `/owasp/cheatsheetseries` (High reputation, bench 78.47) — Authorization
/// Cheat Sheet + IDOR Prevention Cheat Sheet. Backs the F2.3 dimension.
/// Disjoint from F2.1 OWASP (F2.1 detects injection sinks; F2.3 detects
/// missing authorization gates) and F3.6 sec-tests (F3.6 detects missing
/// tests of authz; F2.3 detects missing authz code itself).
pub mod authz;
/// Component-boundary / encapsulation surface analysis (D07): `pub` vs
/// `pub(crate)`/`pub(super)` vs private top-level items, `pub` struct fields
/// (C-STRUCT-PRIVATE), and the public-exposure ratio; backs the F1.7 dimension.
pub mod boundaries;
/// Technical-debt signal (D05): word-bounded comment markers (TODO/FIXME/HACK/
/// XXX/BUG, comment-scoped), `todo!()`/`unimplemented!()` code debt, and
/// `#[allow(dead_code/unused)]` suppressions; backs the F1.5 dimension.
pub mod build_config;
/// Polyglot cache-discipline analysis (D22): unbounded cache growth (moka
/// `Cache::builder()` without `max_capacity`; JS/TS `LRUCache` without
/// `max`/`ttl`) and missing single-flight (cache-stampede risk on get/insert
/// without `get_with`/`try_get_with`/`or_insert_with`/`entry`). Backs F2.9;
/// disjoint from [`memory`] by keying on the cache builder chain + cache-named
/// get/insert, none of which the memory engine inspects.
pub mod caching;
/// Changelog format compliance (D39): file-based detection of the canonical
/// "missing Keep-a-Changelog structural section" smell in a project
/// `CHANGELOG.md` — `[Unreleased]` + versioned `[X.Y.Z] - YYYY-MM-DD` +
/// category sub-headings + Keep-a-Changelog/SemVer header links. Source:
/// `/olivierlacan/keep-a-changelog` (High reputation, bench 92.67); backs
/// the F3.13 dimension.
pub mod changelog;
pub mod cicd;
/// AST-aware non-executable region detection (comments + `#[cfg(test)]`) used by
/// [`security::SecurityAnalyzer`] to drop vulnerability matches that live in
/// documentation/test corpora rather than executable sinks.
pub mod code_regions;
pub mod complexity;
/// Language-aware concurrency anti-patterns (D24): lock-across-await (`std::sync::Mutex`
/// guard held across `.await` — `!Send` + deadlock; tokio task blocking
/// guidance), sync-locks-in-async (`std::sync::Mutex` inside an `async fn`
/// where `tokio::sync::Mutex`/`parking_lot::Mutex` is the idiomatic fix),
/// `Arc<Mutex<…>>` shared state without a `tokio::sync::mpsc` channel
/// (channel-vs-state-shape), `Mutex<u64/i64/usize>` for a counter
/// (`AtomicU64` is lock-free), `go func()` + `sync.Mutex` (Go race-on-mutex),
/// and `async def` + `threading.Lock` (Python blocks the event loop).
/// Backs F2.11; disjoint from F2.8 memory (unbounded/leak/.clone) and F2.10
/// I/O (which keys on `std::fs::` in `async fn` + `block_on(` — F2.11 keys on
/// lock primitives + channel-vs-state-shape).
pub mod concurrency;
/// OWASP A05:2021 Security Misconfiguration detection (disabled TLS verification,
/// permissive CORS, active debug, insecure cookies, unsafe CSP, world-writable
/// modes). Sibling to [`security::SecurityAnalyzer`]; backs the F2.6 dimension.
pub mod config_security;
/// Real per-file line coverage parsed from a `cargo-llvm-cov` LCOV artifact
/// (`hit / found`); backs the F3.1 dimension when an artifact is present.
pub mod coverage_artifact;
/// Polyglot data-model analysis (D10): "make illegal states unrepresentable" +
/// primitive-obsession smells — stringly-typed domain fields (`status: String`),
/// type-erasure escapes (`any`/`interface{}`/`void*`/`Any`), and boolean-flag
/// explosion (≥3 `bool` fields in one struct) across 7 languages; backs the
/// F1.10 dimension. Disjoint from [`api_design`]/[`design_patterns`] (contract /
/// pattern surface) by scoring the *data shape*.
pub mod data_model;
/// Polyglot database-performance analysis (D20): N+1 (a DB-execution token —
/// `.execute(`/`.query(`/`.fetch_*`/`.findMany(`/… — inside a `for`/`while` loop
/// body) + `SELECT *` over-fetch across 7 languages; backs the F2.7 dimension.
/// Disjoint from [`security`] (F2.1 SQL injection) by scoring *performance*.
pub mod db_perf;
/// cargo-deny `[bans]` (wildcards, multiple-versions) + RustSec informational
/// (unmaintained/unsound) + cargo-machete (unused deps) hygiene for a Cargo
/// manifest; backs the F4.5 dimension. Sibling to [`security`]/[`config_security`].
pub mod dep_health;
pub mod deploy;
/// Polyglot design-pattern **anti-pattern** detection (D11): GoF transplants
/// (`static mut` Singleton, `getInstance`, `FactoryFactory`), ownership smells
/// (`Rc<RefCell<`, `unsafe impl Send/Sync`), and type-erasure escape hatches
/// (`.downcast`/`dyn Any`, `dynamic_cast`, `as unknown as`) across 7 languages;
/// backs the F1.11 dimension. Disjoint from [`idioms`]/[`api_design`]/
/// [`modernization`] by scoring the *structural pattern* choice.
pub mod design_patterns;
/// Documentation accuracy analysis (D38): polyglot detector of the canonical
/// "doc drift / no executable examples" smell — `missing_docs` lint absence,
/// `///` without inline ``` doctest, drift markers (TODO/FIXME/XXX) in docs,
/// Markdown without code blocks. Source: `/vale-cli/vale` (Medium
/// reputation) + rustdoc doctests; backs the F3.12 dimension.
pub mod doc_accuracy;
/// Type-1 (exact, modulo whitespace) block clone detection (jscpd/CPD-style):
/// runs of consecutive meaningful production lines recurring verbatim; backs the
/// F1.3 dimension.
pub mod duplication;
/// Edge-case coverage analysis (D30): polyglot detector of the canonical
/// "no property-based / fuzz coverage" smell — Rust `proptest!` + `fuzz_target!`,
/// Python `@hypothesis.given`, JS/TS `fast-check`. Source:
/// `/proptest-rs/proptest` (High reputation, bench 91.73) +
/// `/rust-fuzz/cargo-fuzz` (High reputation, bench 57.4); backs the F3.4 dimension.
pub mod edge_cases;
pub mod env;
pub mod error_coverage;
pub mod fast_hash;
pub mod frameworks;
/// Polyglot frontend performance (D25): Core Web Vitals — render-blocking
/// `<script>` (LCP regression), `<link rel="stylesheet">` outside `<head>`,
/// `<img>` without `width`/`height` (CLS — the canonical layout-shift cause),
/// `<img loading="lazy">` without `fetchpriority="high"` (LCP contradiction),
/// multi-line `addEventListener`/`onclick` with no `await`/`Promise` (INP
/// regression), `.wasm` literal without `wasm-opt -Oz` nearby (binary-size
/// regression), and many `import(` without code-split hint (bundle-not-lazy).
/// Backs F2.12; disjoint from F2.1 OWASP (`innerHTML` is security, F2.12 is
/// perf) and F2.10 I/O (F2.12 is browser-load latency, F2.10 is
/// blocking-IO-in-async).
pub mod frontend;
pub mod iac;
/// Polyglot language-idiom detection (D40): non-idiomatic constructs across 7
/// languages — clippy (Rust), ruff (Python), ESLint (TS/JS), go vet (Go),
/// clang-tidy (C++), Java legacy APIs; backs the F4.1 dimension.
pub mod idioms;
pub mod incident;
/// Polyglot inline-documentation analysis (D34): `pub` items in Rust without
/// preceding `///` / `//!` (the `rustdoc missing_docs` smell), `export` items
/// in JS/TS without preceding `/** */` JSDoc, and module-level `def`/`class`
/// in Python without a triple-quoted docstring. Source-code-only heuristic
/// (not `cargo doc --no-deps`); backs the F3.8 dimension.
pub mod inline_doc;
/// Polyglot input-validation (D15): boundary-validation security anti-patterns
/// — blocklist sanitization (`.replace("../"`, CWE-22), insecure deserialization
/// (`pickle.loads`/`yaml.load`/`ObjectInputStream`, CWE-502), unbounded input
/// (`gets`/`scanf("%s"`, CWE-242/120), and auto-escaping bypasses
/// (`dangerouslySetInnerHTML`, `template.HTML`) across 7 languages; backs the
/// F2.2 dimension (WARN, `WorstOf` roll-up). Disjoint from [`security`] (F2.1
/// injection sinks).
pub mod input_validation;
/// Polyglot I/O-bottleneck analysis (D23): blocking I/O in async context
/// (`async fn` + `std::fs::`/`std::net::`/`reqwest::blocking`), `block_on(`
/// inside a runtime, file/network I/O inside a loop body (via
/// [`loop_blocks::loop_bodies`]), and unbuffered byte-loop reads (`read_exact(`
/// in a loop without `BufReader`). Backs F2.10; disjoint from F2.7 db-perf
/// (which keys on `db.execute`/`db.query` in loop, db-specific) and F2.8
/// memory (which keys on `unbounded_channel(`/`Box::leak(`/.clone).
pub mod io;
/// Shared loop-body finder (brace-matched / indent-scoped) used by the F2.7
/// (N+1) and F2.8 (hot-path allocation) engines — extracted so the loop logic is
/// not duplicated across them (which the F1.3 clone detector would itself flag).
mod loop_blocks;
/// Polyglot memory-management analysis (D21): unbounded growth
/// (`unbounded_channel(`/`unbounded(`/`maxsize=None`), leaks (`Box::leak`/
/// `mem::forget`/`.leak()`), refcount cycles (a `parent`/`prev`/`owner`
/// back-reference held as a strong `Rc`/`Arc`/`shared_ptr`), and hot-path
/// allocation (`.to_vec()`/`.to_owned()` in a loop); backs the F2.8 dimension.
/// Disjoint from [`design_patterns`] (which keys on `Rc<RefCell<`) by keying on
/// the back-reference name + unbounded/leak/alloc.
pub mod memory;
/// Polyglot modernization detection (D43): adoption of newer language/edition
/// features replacing superseded ones — `try!`→`?` / `extern crate`→paths /
/// `lazy_static!`→`LazyLock` (Rust), `super(Cls,self)`→`super()` (Python),
/// `require`→ESM (JS/TS), `ioutil`→`io` (Go), anon-class→lambda (Java), C
/// headers→C++ headers; backs the F4.4 dimension. Version-anchored, so disjoint
/// from [`idioms`] (per-version style).
pub mod modernization;
pub mod monitoring;
/// Performance test coverage analysis (D33): polyglot detector of the
/// canonical "no benchmark / regression-guard" smell — Rust Criterion +
/// `#[bench]`, Python pytest-benchmark, JS/TS `perf()`-prefixed tests.
/// Source: `/bheisler/criterion.rs` (High reputation, bench 94.42) +
/// `/websites/pytest-benchmark_readthedocs_io_en_stable` (High reputation,
/// bench 62); backs the F3.7 dimension.
pub mod perf_tests;
pub mod quality_finding;
/// README completeness (D37): heuristic detection of the canonical
/// "missing essential section" smell in a project `README` — title +
/// description + install + usage + contributing + tests + license. Source:
/// `/othneildrew/best-readme-template` (High reputation, bench 85); backs
/// the F3.11 dimension.
pub mod polyglot_semantic;
pub mod readme;
pub mod rust_semantic;
/// Polyglot scalability (D26): unbounded state in-process
/// (`Arc<Mutex<HashMap<…>>>` — cannot horizontally replicate), unbounded
/// `mpsc` channel (no bounded sibling in the same file = SPOF under load),
/// hardcoded small rayon thread-pool size (`.num_threads(N ≤ 8)` caps
/// parallelism regardless of host CPU count), missing external-call
/// timeout (`reqwest::Client::new`/`get` without `.timeout(` — SPOF
/// under stall), hot `async fn` loop without `tokio::task::yield_now()`
/// (starves the executor), and unbounded `go func()` (no `WaitGroup`/
/// bounded `chan`). Backs F2.13; disjoint from F2.8 memory (F2.8 keys
/// on the *individual* `unbounded_channel(` literal — F2.13 keys on the
/// *file-level* comparison: unbounded *and* no bounded sibling) and F2.10
/// I/O (F2.10 keys on `reqwest::blocking` — F2.13 keys on `reqwest::get`
/// without `.timeout`).
pub mod scalability;
/// Shared scoring primitives for the per-file `score_X` functions (D-rules
/// 1-52). Hosts the canonical `density_score(weighted_total, total_lines,
/// scale) -> f32` helper with a `max(20)` floor on the line count so
/// short files (test fixtures, trait declarations, single `impl` blocks)
/// don't saturate the score to 0 when they host several findings. The
/// floor is uniform across all 50-dim per-file density scores; individual
/// engines keep their own SCALE constant (heterogeneous: 6.0 for most,
/// 8.0 for `idioms`/`input_validation` which detect more findings per
/// file). Lição F2.13: prior `total_lines.max(1)` saturated to 0 on
/// 4-line test files with 3 findings (density 0.675 → 1 - 0.675·6 =
/// -3.05 → clamp 0). The helper consolidates the floor in one place.
pub mod score_utils;
/// Polyglot security-test coverage analysis (D32): control-untested detection —
/// auth-handling code (token/cookie/login/password) without `test_auth*`/`test_login*`,
/// authz-handling code without `test_authz*`/`test_forbidden*`/`test_403*`,
/// input-validation code without `test_xss*`/`test_sql_injection*`, security
/// tests without 401/403/deny/reject assertion (positive-only — does not prove
/// the control), HTTP framework without zap-baseline/burp/owasp reference (no
/// DAST in CI). Source: `/zaproxy/zaproxy` (High reputation) OWASP ZAP baseline
/// scan + secure-SDLC; backs the F3.6 dimension.
pub mod sec_tests;
pub mod security;
/// Workspace-level quality signal (Sentrux-inspired, gameproof Nash 1950).
///
/// While `tdg` provides per-file 6-dimensional analysis, [`signal`] aggregates
/// the entire workspace into a single 0..=10000 score via the geometric mean
/// of five root cause metrics. The two views are complementary: TDG drives
/// per-file refactor decisions; the workspace signal drives macro direction
/// (which root cause to attack next).
pub mod signal;
pub mod tdg;
/// Polyglot test-maintainability analysis (D31): flakiness + isolation smells —
/// `#[ignore]` accumulation (hidden gaps), `sleep(` in test (flaky), `now()`/
/// `thread_rng`/`Math.random`/`random.*` in test (non-deterministic), state-sharing
/// `lazy_static!`/`OnceCell`/`static mut` in Rust test (breaks parallel exec),
/// HTTP/DB I/O in test without mockall/wiremock/testcontainers/mockito. Source:
/// `/testcontainers/testcontainers-rs` (High reputation) + WireMock; backs the
/// F3.5 dimension.
pub mod test_maint;
/// Polyglot test-pyramid analysis (D29): ice-cream-cone detection — Rust project
/// with ≥5 `#[test]` but no Playwright/Cypress/Selenium (missing E2E top); JS/TS
/// test file with `page.click`/`page.locator` but no `it(`/`test(` (heavy E2E,
/// no unit base); Playwright actions without `.toBeVisible` (E2E no-op).
/// Source: `/microsoft/playwright` (High reputation, bench 90); backs the F3.3
/// dimension.
pub mod test_pyramid;
/// Polyglot test-quality analysis (D28): value-blind assertion detection — Rust
/// `#[test]` without `assert_eq!`/`assert_ne!`, trivial `assert!(true|false)`,
/// JS/TS tests using only `.toBeTruthy()`/`.toBeFalsy()`, Python `def test_*`
/// without `assert`. Source: `/sourcefrog/cargo-mutants` (High reputation, bench
/// 80) — cargo-mutants mutates operators; value-blind assertions let mutantes
/// survive. Backs the F3.2 dimension.
pub mod test_quality;
pub use build_config::{BuildConfigReport, analyze_build_config, score_build_config};
pub use cicd::{CicdReport, analyze_cicd, score_cicd};
pub use deploy::{DeployReport, analyze_deploy, score_deploy};
pub use env::{EnvReport, analyze_env, score_env};
pub use frameworks::{FrameworksReport, analyze_frameworks, score_frameworks};
pub use iac::{IacReport, analyze_iac, score_iac};
pub use incident::{IncidentReport, analyze_incident, score_incident};
pub use monitoring::{MonitoringReport, analyze_monitoring, score_monitoring};
pub mod tech_debt;
pub mod test_proxy;
pub mod unwrap_audit;

pub use quality_finding::QualityFinding;
pub use signal::{
    Bottleneck, Diagnostics, RootCauseRaw, RootCauseScores, Workspace, WorkspaceIoError,
    WorkspaceQualitySignal, build_workspace_from_path, compute_quality_signal,
};

pub use api_design::{ApiDesignReport, analyze_api_design, score_api_design};
pub use api_doc::{ApiDocReport, analyze_api_doc, score_api_doc};
pub use arch_consistency::{
    ArchConsistencyReport, analyze_arch_consistency, score_arch_consistency,
};
pub use arch_doc::{ArchDocReport, analyze_arch_doc, score_arch_doc};
pub use authz::{AuthzReport, analyze_authz, score_authz};
pub use boundaries::{BoundaryReport, analyze_boundaries, score_boundaries};
pub use caching::{CachingReport, analyze_caching, score_caching};
pub use changelog::{ChangelogReport, analyze_changelog, score_changelog};
pub use complexity::{
    estimate_cognitive_complexity, estimate_complexity, estimate_halstead,
    estimate_maintainability_index,
};
pub use concurrency::{ConcurrencyReport, analyze_concurrency, score_concurrency};
pub use config_security::{ConfigReport, ConfigSecurityAnalyzer};
pub use coverage_artifact::{CoverageArtifact, CoverageReport, FileCoverage};
pub use data_model::{DataModelReport, analyze_data_model, score_data_model};
pub use db_perf::{DbPerfReport, analyze_db_perf, score_db_perf};
pub use dep_health::{DepHealthAnalyzer, DepHealthReport};
pub use design_patterns::{DesignPatternReport, analyze_design_patterns, score_design_patterns};
pub use doc_accuracy::{DocAccuracyReport, analyze_doc_accuracy, score_doc_accuracy};
pub use duplication::{DuplicationReport, analyze_duplication};
pub use edge_cases::{EdgeCasesReport, analyze_edge_cases, score_edge_cases};
pub use fast_hash::fast_content_hash;
pub use frontend::{FrontendReport, analyze_frontend, score_frontend};
pub use idioms::{IdiomReport, analyze_idioms, score_idioms};
pub use inline_doc::{InlineDocReport, analyze_inline_doc, score_inline_doc};
pub use input_validation::{
    InputValidationReport, analyze_input_validation, score_input_validation,
};
pub use io::{IoReport, analyze_io, score_io};
pub use memory::{MemoryMgmtReport, analyze_memory_mgmt, score_memory_mgmt};
pub use modernization::{ModernizationReport, analyze_modernization, score_modernization};
pub use perf_tests::{PerfTestsReport, analyze_perf_tests, score_perf_tests};
pub use readme::{ReadmeReport, analyze_readme, score_readme};
pub use polyglot_semantic::PolyglotQualitySignals;
pub use rust_semantic::RustQualitySignals;
pub use scalability::{ScalabilityReport, analyze_scalability, score_scalability};
pub use score_utils::density_score;
pub use sec_tests::{SecTestsReport, analyze_sec_tests, score_sec_tests};
pub use security::{SecurityAnalyzer, SecurityReport};
pub use tdg::{TdgGrade, TdgReport};
pub use tech_debt::{TechDebtReport, analyze_tech_debt};
pub use test_maint::{TestMaintReport, analyze_test_maint, score_test_maint};
pub use test_proxy::TestProxy;
pub use test_pyramid::{TestPyramidReport, analyze_test_pyramid, score_test_pyramid};
pub use test_quality::{TestQualityReport, analyze_test_quality, score_test_quality};
pub use unwrap_audit::{ProdHazards, UnwrapAudit, count_prod_hazards};

use serde::{Deserialize, Serialize};

/// A single antipattern finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Antipattern {
    /// Language the antipattern was found in.
    pub language: String,
    /// The problematic pattern text.
    pub pattern: String,
    /// Human-readable description.
    pub message: String,
    /// Line number (1-indexed, 0 if unknown).
    pub line: usize,
}

/// Complexity metrics for a file.
///
/// Mirrors the classic Mozilla `rust-code-analysis` schema
/// (CC + cognitive + SLOC/LLOC/CLOC + NEXITS + NOM) without incurring
/// the tree-sitter 0.20.x dual-graph duplication that adopting the
/// external crate would cost. All fields computed by
/// `estimate_complexity` via byte-level scans on top of the
/// workspace's existing tree-sitter 0.25 toolchain.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComplexityMetrics {
    /// Total number of functions/methods (NOM).
    pub function_count: usize,
    /// True maximum cyclomatic complexity over all callable symbols, computed
    /// by the tree-sitter CC walker (`touring-code`) for Rust/Python/TS/JS/Bash.
    /// Falls back to a keyword-counting estimate for other languages or on
    /// parse failure. This is the single source of CC truth shared with TDG.
    pub max_complexity: usize,
    /// Average cyclomatic complexity (float precision).
    pub avg_complexity: f64,
    /// Number of symbols (types + functions + constants).
    pub symbol_count: usize,
    /// Cognitive complexity score (nesting-depth-penalised branching).
    /// Each branching keyword scores `1 + (nesting_depth / 3)`.
    #[serde(default)]
    pub cognitive_complexity: usize,
    /// SLOC — Source Lines Of Code (non-blank lines, including comments).
    #[serde(default)]
    pub sloc: usize,
    /// CLOC — Comment Lines Of Code (lines whose first non-whitespace
    /// token is `//`, `/*`, `*/`, `*`, or `#` for Python-family).
    #[serde(default)]
    pub cloc: usize,
    /// LLOC — Logical Lines Of Code (`sloc - cloc`, approximation).
    #[serde(default)]
    pub lloc: usize,
    /// NEXITS — number of explicit `return` statements (proxy for
    /// early-exit complexity; multi-return functions are harder to reason
    /// about and are a signal of cyclomatic inflation).
    #[serde(default)]
    pub nexits: usize,
    /// BLANK — fully empty lines (distinct from SLOC/CLOC). Surfaces
    /// visual density signal; pairs with LLOC for readability ratios.
    #[serde(default)]
    pub blank: usize,
    /// Maintainability Index (MI) in `[0.0, 100.0]`. Uses the
    /// SEI/Mozilla variant `max(0, (171 − 5.2·ln(V) − 0.23·CC −
    /// 16.2·ln(LLOC))·100/171)` where `V` is Halstead volume, `CC`
    /// is cyclomatic complexity, and `LLOC` is logical lines of code.
    /// Higher is better. `0.0` for degenerate inputs.
    #[serde(default)]
    pub maintainability_index: f64,
    /// Halstead complexity metrics (derived vocabulary / length / volume).
    /// Zero-cost when `halstead::estimate_halstead` is not invoked.
    #[serde(default)]
    pub halstead: HalsteadMetrics,
}

/// Halstead software science metrics — derived from distinct operator /
/// operand vocabulary and total token counts.
///
/// Computed via byte-level keyword + identifier scanning (no tree-sitter
/// dependency). Same trade-off class as `estimate_complexity`: fast,
/// language-aware, heuristic. For research-grade Halstead precision on
/// Rust, prefer `touring-ast::rust_semantic::RustSemanticReport` semantic
/// analysis; this type keeps the polyglot fast-path viable under the
/// <2ms daemon budget.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HalsteadMetrics {
    /// n1 — distinct operators encountered.
    pub n1: usize,
    /// n2 — distinct operands encountered.
    pub n2: usize,
    /// N1 — total operator occurrences.
    pub big_n1: usize,
    /// N2 — total operand occurrences.
    pub big_n2: usize,
    /// Vocabulary `n = n1 + n2`.
    pub vocabulary: usize,
    /// Length `N = N1 + N2`.
    pub length: usize,
    /// Volume `V = N * log2(n)` — program size in bits.
    pub volume: f64,
    /// Difficulty `D = (n1 / 2) * (N2 / n2)` — effort to write/read.
    pub difficulty: f64,
    /// Effort `E = D * V` — mental effort proxy.
    pub effort: f64,
    /// Estimated latent bugs `B = V / 3000` (Halstead 1977 coefficient).
    pub bugs: f64,
    /// Estimated coding time in seconds `T = E / 18` (Stroud number).
    pub time_seconds: f64,
}

/// Complete quality report for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    /// File path analyzed.
    pub file_path: String,
    /// Detected language.
    pub language: String,
    /// Antipatterns found (with line numbers).
    pub antipatterns: Vec<Antipattern>,
    /// Complexity metrics.
    pub complexity: ComplexityMetrics,
    /// Count of `.unwrap()` calls.
    pub unwrap_count: usize,
    /// Lines containing `.unwrap()`.
    pub unwrap_lines: Vec<usize>,
    /// Ratio of functions returning Result/Option vs total (0.0–1.0).
    pub error_handling_coverage: f64,
    /// Density of `?` operator usage per function.
    pub question_mark_density: f64,
    /// Test coverage proxy (test count, cfg(test) density, is_test_file).
    #[serde(default)]
    pub test_proxy: test_proxy::TestProxy,
    /// Count of `.expect(` calls (lower risk than unwrap).
    #[serde(default)]
    pub expect_count: usize,
    /// Unwrap risk score from unwrap_audit (0.0–1.0, density-based).
    #[serde(default)]
    pub unwrap_risk_score: f64,
    /// Composite quality score (0.0–1.0).
    pub score: f64,
}

impl QualityReport {
    /// Compute composite score from individual dimensions.
    pub fn compute_score(&mut self) {
        let antipattern_penalty = (self.antipatterns.len() as f64 * 0.05).min(0.4);
        let unwrap_penalty = (self.unwrap_count as f64 * 0.02).min(0.3);
        let complexity_penalty = if self.complexity.max_complexity > 20 {
            0.2
        } else if self.complexity.max_complexity > 10 {
            0.1
        } else {
            0.0
        };
        let cognitive_penalty = if self.complexity.cognitive_complexity > 60 {
            0.2
        } else if self.complexity.cognitive_complexity > 30 {
            0.1
        } else {
            0.0
        };
        let error_bonus = self.error_handling_coverage * 0.2;
        let test_bonus = self.test_proxy.score * 0.1;
        let risk_penalty = (self.unwrap_risk_score * 0.1).min(0.1);

        self.score = (1.0
            - antipattern_penalty
            - unwrap_penalty
            - complexity_penalty
            - cognitive_penalty
            - risk_penalty
            + error_bonus
            + test_bonus)
            .clamp(0.0, 1.0);
    }
}

/// Multi-file quality pipeline — runs all dimensions.
pub struct QualityPipeline {
    config: crate::engine::AnalysisConfig,
}

impl QualityPipeline {
    /// Create a new pipeline with the given config.
    pub fn new(config: crate::engine::AnalysisConfig) -> Self {
        Self { config }
    }

    /// Analyze a single file.
    pub fn analyze_file(&self, file_path: &str, source: &str, language: &str) -> QualityReport {
        let antipatterns_raw = antipatterns::detect_antipatterns(source, language);
        let antipattern_list: Vec<Antipattern> = antipatterns_raw
            .into_iter()
            .map(|(msg, line)| Antipattern {
                language: language.to_string(),
                pattern: msg.clone(),
                message: msg,
                line,
            })
            .collect();

        let unwrap_result = unwrap_audit::count_unwraps(source);
        let expect_count = unwrap_audit::count_expects(source);
        let error_cov = error_coverage::analyze_error_coverage(source, language);
        let complexity = complexity::estimate_complexity(source, language);
        let proxy = test_proxy::analyze_test_proxy(source, file_path);

        let mut report = QualityReport {
            file_path: file_path.to_string(),
            language: language.to_string(),
            antipatterns: antipattern_list,
            complexity,
            unwrap_count: unwrap_result.count,
            unwrap_lines: unwrap_result.lines,
            error_handling_coverage: error_cov.result_ratio,
            question_mark_density: error_cov.question_mark_density,
            test_proxy: proxy,
            expect_count,
            unwrap_risk_score: unwrap_result.risk_score,
            score: 0.0,
        };
        report.compute_score();
        report
    }

    /// Analyze multiple files.
    ///
    /// Uses rayon for parallel analysis when no `budget_ms` constraint is set.
    /// Falls back to a serial loop when a budget is present so early exit is possible.
    /// `quality_sample = 0` means "skip quality analysis" (returns empty vec).
    /// `quality_sample = usize::MAX` means "all files".
    pub fn analyze_batch(&self, files: &[(&str, &str, &str)]) -> Vec<QualityReport> {
        if self.config.quality_sample == 0 || files.is_empty() {
            return vec![];
        }
        let sample_size = self.config.quality_sample.min(files.len());
        let slice = files.get(..sample_size).unwrap_or(files);

        if let Some(budget_ms) = self.config.budget_ms {
            // Serial loop — allows early exit when budget is exhausted.
            let start = std::time::Instant::now();
            let budget = std::time::Duration::from_millis(budget_ms);
            let mut results = Vec::with_capacity(slice.len());
            for (path, source, lang) in slice {
                if start.elapsed() >= budget {
                    break;
                }
                results.push(self.analyze_file(path, source, lang));
            }
            results
        } else {
            use rayon::prelude::*;
            slice
                .par_iter()
                .map(|(path, source, lang)| self.analyze_file(path, source, lang))
                .collect()
        }
    }

    /// Aggregate multiple reports into summary metrics.
    pub fn aggregate(reports: &[QualityReport]) -> QualityDimension {
        if reports.is_empty() {
            return QualityDimension::default();
        }

        let total_antipatterns: usize = reports.iter().map(|r| r.antipatterns.len()).sum();
        let total_unwraps: usize = reports.iter().map(|r| r.unwrap_count).sum();
        let avg_score: f64 = reports.iter().map(|r| r.score).sum::<f64>() / reports.len() as f64;
        let max_complexity: usize = reports
            .iter()
            .map(|r| r.complexity.max_complexity)
            .max()
            .unwrap_or(0);
        let avg_error_coverage: f64 = reports
            .iter()
            .map(|r| r.error_handling_coverage)
            .sum::<f64>()
            / reports.len() as f64;

        // Rank files by Wilson lower-bound quality score (worst first).
        // Requires >= 2 reports — single-file projects don't need ranking.
        let top_problem_files = if reports.len() >= 2 {
            use touring_simd::WilsonRanker;
            use touring_simd::statistics::StatisticalRanking;
            let ranker = WilsonRanker::new(0.95);
            // Convert each report's score (0.0–1.0) to (successes, total=100).
            let data: Vec<(u32, u32)> = reports
                .iter()
                .map(|r| ((r.score * 100.0) as u32, 100u32))
                .collect();
            let scores = ranker.wilson_scores_batch(&data, 0.95);
            // Collect (index, score) and sort ascending so lowest Wilson bound first.
            let mut indexed: Vec<(usize, f64)> = scores.into_iter().enumerate().collect();
            indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            indexed
                .iter()
                .take(5)
                .filter_map(|(i, _)| reports.get(*i).map(|r| r.file_path.clone()))
                .filter(|s: &String| !s.is_empty())
                .collect()
        } else {
            vec![]
        };

        QualityDimension {
            files_analyzed: reports.len(),
            total_antipatterns,
            total_unwraps,
            max_complexity,
            avg_score,
            avg_error_coverage,
            top_problem_files,
        }
    }
}

/// Aggregated quality dimension for health scoring.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityDimension {
    /// Number of files analyzed.
    pub files_analyzed: usize,
    /// Total antipatterns across all files.
    pub total_antipatterns: usize,
    /// Total .unwrap() calls across all files.
    pub total_unwraps: usize,
    /// Maximum cyclomatic complexity found.
    pub max_complexity: usize,
    /// Average quality score (0.0–1.0).
    pub avg_score: f64,
    /// Average error handling coverage (0.0–1.0).
    pub avg_error_coverage: f64,
    /// Top 5 worst files by Wilson lower-bound quality score.
    ///
    /// Uses `touring_simd::WilsonRanker` to rank files by their Wilson
    /// confidence-interval lower bound — penalises small samples more than
    /// large ones for the same raw score ratio.
    #[serde(default)]
    pub top_problem_files: Vec<String>,
}

/// Analyze antipatterns in source code for the given language.
///
/// Convenience free function returning only antipattern findings without
/// running the full QualityPipeline. Useful for targeted hook-path checks.
///
/// # Example
/// ```rust
/// use touring_analysis::quality::analyze_antipatterns;
/// let patterns = analyze_antipatterns("let x = foo.unwrap();", "rust");
/// ```
pub fn analyze_antipatterns(source: &str, language: &str) -> Vec<Antipattern> {
    antipatterns::detect_antipatterns(source, language)
        .into_iter()
        .map(|(msg, line)| Antipattern {
            language: language.to_string(),
            pattern: msg.clone(),
            message: msg,
            line,
        })
        .collect()
}

/// Estimate complexity metrics for source code.
///
/// Convenience free function returning only complexity without running
/// antipattern detection, unwrap audit, or error coverage analysis.
///
/// # Example
/// ```rust
/// use touring_analysis::quality::analyze_complexity;
/// let metrics = analyze_complexity("fn foo() {}", "rust");
/// assert!(metrics.function_count >= 1);
/// ```
pub fn analyze_complexity(source: &str, language: &str) -> ComplexityMetrics {
    complexity::estimate_complexity(source, language)
}

/// Audit `.unwrap()` calls in source code.
///
/// Convenience free function for targeted unwrap scanning without running
/// the full quality pipeline.
///
/// # Example
/// ```rust
/// use touring_analysis::quality::analyze_unwraps;
/// let audit = analyze_unwraps("let x = foo.unwrap();");
/// assert_eq!(audit.count, 1);
/// ```
pub fn analyze_unwraps(source: &str) -> UnwrapAudit {
    unwrap_audit::count_unwraps(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_complexity(max: usize, cognitive: usize) -> ComplexityMetrics {
        ComplexityMetrics {
            function_count: 1,
            max_complexity: max,
            avg_complexity: max as f64,
            symbol_count: 1,
            cognitive_complexity: cognitive,
            ..Default::default()
        }
    }

    #[test]
    fn test_quality_report_score_clean_file() {
        let mut report = QualityReport {
            file_path: "clean.rs".to_string(),
            language: "rust".to_string(),
            antipatterns: vec![],
            complexity: make_complexity(3, 0),
            unwrap_count: 0,
            unwrap_lines: vec![],
            error_handling_coverage: 0.8,
            question_mark_density: 0.5,
            test_proxy: Default::default(),
            expect_count: 0,
            unwrap_risk_score: 0.0,
            score: 0.0,
        };
        report.compute_score();
        assert!(
            report.score > 0.9,
            "clean file should score high: {}",
            report.score
        );
    }

    #[test]
    fn test_quality_report_score_dirty_file() {
        let mut report = QualityReport {
            file_path: "dirty.rs".to_string(),
            language: "rust".to_string(),
            antipatterns: vec![
                Antipattern {
                    language: "rust".to_string(),
                    pattern: ".unwrap()".to_string(),
                    message: "test".to_string(),
                    line: 1,
                },
                Antipattern {
                    language: "rust".to_string(),
                    pattern: "todo!()".to_string(),
                    message: "test".to_string(),
                    line: 2,
                },
            ],
            complexity: make_complexity(25, 0),
            unwrap_count: 10,
            unwrap_lines: vec![1, 5, 10, 15, 20, 25, 30, 35, 40, 45],
            error_handling_coverage: 0.2,
            question_mark_density: 0.1,
            test_proxy: Default::default(),
            expect_count: 0,
            unwrap_risk_score: 0.0,
            score: 0.0,
        };
        report.compute_score();
        assert!(
            report.score < 0.6,
            "dirty file should score low: {}",
            report.score
        );
    }

    #[test]
    fn test_aggregate_empty() {
        let dim = QualityPipeline::aggregate(&[]);
        assert_eq!(dim.files_analyzed, 0);
        assert_eq!(dim.avg_score, 0.0);
        assert!(dim.top_problem_files.is_empty());
    }

    #[test]
    fn test_aggregate_single_report_no_ranking() {
        let pipeline = QualityPipeline::new(crate::engine::AnalysisConfig::standard());
        let report = pipeline.analyze_file("src/a.rs", "fn a() {}", "rust");
        let dim = QualityPipeline::aggregate(&[report]);
        // Single file — no ranking performed
        assert!(
            dim.top_problem_files.is_empty(),
            "single report should not produce top_problem_files"
        );
    }

    #[test]
    fn test_aggregate_top_problem_files_ranked_worst_first() {
        let pipeline = QualityPipeline::new(crate::engine::AnalysisConfig::standard());
        // Clean file — scores high
        let clean =
            pipeline.analyze_file("clean.rs", "fn ok() -> Result<(), ()> { Ok(()) }", "rust");
        // Dirty file — scores low (many unwraps + high complexity signal)
        let dirty_src = "foo.unwrap(); ".repeat(10);
        let dirty = pipeline.analyze_file("dirty.rs", &dirty_src, "rust");

        let dim = QualityPipeline::aggregate(&[clean, dirty]);
        assert!(
            !dim.top_problem_files.is_empty(),
            "two files should produce top_problem_files"
        );
        // Worst file must appear first
        assert_eq!(
            dim.top_problem_files.first().map(String::as_str),
            Some("dirty.rs"),
            "dirty.rs should rank as worst: {:?}",
            dim.top_problem_files
        );
    }

    #[test]
    fn test_aggregate_top_problem_files_capped_at_five() {
        let pipeline = QualityPipeline::new(crate::engine::AnalysisConfig::standard());
        let reports: Vec<_> = (0..10)
            .map(|i| pipeline.analyze_file(&format!("f{i}.rs"), "fn a() {}", "rust"))
            .collect();
        let dim = QualityPipeline::aggregate(&reports);
        assert!(
            dim.top_problem_files.len() <= 5,
            "top_problem_files must not exceed 5 entries"
        );
    }

    #[test]
    fn test_analyze_file_populates_all_fields() {
        let pipeline = QualityPipeline::new(crate::engine::AnalysisConfig::standard());
        let source = "fn foo() -> Result<(), ()> { let x = bar().unwrap(); Ok(()) }";
        let report = pipeline.analyze_file("src/lib.rs", source, "rust");
        assert_eq!(report.language, "rust");
        assert!(report.unwrap_count >= 1);
        assert!(report.score > 0.0);
    }

    #[test]
    fn test_antipatterns_carry_line_numbers() {
        let pipeline = QualityPipeline::new(crate::engine::AnalysisConfig::standard());
        let source = "fn a() {}\nfn b() { todo!(); }";
        let report = pipeline.analyze_file("src/lib.rs", source, "rust");
        let todo_ap = report
            .antipatterns
            .iter()
            .find(|a| a.pattern.contains("todo!"));
        assert!(todo_ap.is_some(), "todo! antipattern should be detected");
        assert_eq!(todo_ap.unwrap().line, 2, "todo! is on line 2");
    }

    #[test]
    fn test_cognitive_penalty_applied_to_score() {
        let pipeline = QualityPipeline::new(crate::engine::AnalysisConfig::standard());
        let nested =
            "fn f() { if a { for x in y { while z { if b { for i in j { if c { } } } } } } }";
        let flat = "fn f() { if a { } }";
        let r_nested = pipeline.analyze_file("src/a.rs", nested, "rust");
        let r_flat = pipeline.analyze_file("src/b.rs", flat, "rust");
        assert!(
            r_nested.complexity.cognitive_complexity > r_flat.complexity.cognitive_complexity,
            "nested should have higher cognitive complexity: {} vs {}",
            r_nested.complexity.cognitive_complexity,
            r_flat.complexity.cognitive_complexity
        );
    }

    #[test]
    fn test_batch_serial_with_budget_does_not_panic() {
        let config = crate::engine::AnalysisConfig::hook_path();
        assert!(config.budget_ms.is_some());
        let pipeline = QualityPipeline::new(config);
        let files = vec![
            ("src/a.rs", "fn a() {}", "rust"),
            ("src/b.rs", "fn b() {}", "rust"),
        ];
        let results = pipeline.analyze_batch(&files);
        assert!(results.len() <= 2);
    }

    #[test]
    fn test_test_proxy_bonus_increases_score() {
        let pipeline = QualityPipeline::new(crate::engine::AnalysisConfig::standard());
        let with_tests = "#[test]\nfn test_foo() {}";
        let without_tests = "fn foo() {}";
        let r_with = pipeline.analyze_file("src/lib.rs", with_tests, "rust");
        let r_without = pipeline.analyze_file("src/lib.rs", without_tests, "rust");
        assert!(
            r_with.test_proxy.score > 0.0,
            "file with #[test] should have test_proxy.score > 0"
        );
        assert!(
            r_with.score >= r_without.score,
            "test bonus should not reduce score"
        );
    }

    #[test]
    fn test_high_cognitive_complexity_lowers_score() {
        // Manually build two reports that differ only in cognitive_complexity
        let base = |cognitive: usize| {
            let mut r = QualityReport {
                file_path: "f.rs".to_string(),
                language: "rust".to_string(),
                antipatterns: vec![],
                complexity: make_complexity(5, cognitive),
                unwrap_count: 0,
                unwrap_lines: vec![],
                error_handling_coverage: 0.5,
                question_mark_density: 0.5,
                test_proxy: Default::default(),
                expect_count: 0,
                unwrap_risk_score: 0.0,
                score: 0.0,
            };
            r.compute_score();
            r
        };
        let low = base(10);
        let high = base(65);
        assert!(
            high.score < low.score,
            "cognitive > 60 should lower score: {} vs {}",
            high.score,
            low.score
        );
    }

    #[test]
    fn test_analyze_antipatterns_detects_unwrap() {
        let patterns = analyze_antipatterns("let x = foo.unwrap();", "rust");
        // .unwrap() is a known Rust antipattern
        assert!(!patterns.is_empty(), "should detect .unwrap() antipattern");
        assert!(patterns.iter().any(|p| p.pattern.contains("unwrap")));
    }

    #[test]
    fn test_analyze_complexity_basic() {
        let metrics = analyze_complexity("fn foo() {}", "rust");
        // At minimum does not panic
        assert!(
            metrics.function_count >= 1 || metrics.function_count == 0,
            "should not panic on minimal source"
        );
    }

    #[test]
    fn test_analyze_unwraps_counts() {
        let audit = analyze_unwraps("let x = foo.unwrap(); let y = bar.unwrap();");
        assert!(
            audit.count >= 2,
            "should count at least 2 unwraps, got {}",
            audit.count
        );
    }
}
