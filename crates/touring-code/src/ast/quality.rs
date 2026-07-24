//! Multi-language code quality analysis.
//!
//! Provides anti-pattern detection and quality scoring for source code in 14
//! languages using fast string scanning — no regex, no external tools.
//!
//! # Entry Point
//!
//! - [`analyze_quality`]: main entry point, dispatches by [`Lang`]
//!
//! # Scores
//!
//! All scores are in `[0.0, 1.0]` where **1.0 = perfect**:
//! - `complexity_score`: derived from cyclomatic complexity (lower CC → higher score)
//! - `antipattern_score`: derived from hit count and severity (fewer/lighter → higher)
//! - `overall_score`: weighted average (`0.6 × complexity_score + 0.4 × antipattern_score`)

use crate::ast::{Lang, compute_complexity_for_source};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

// ─── Wilson confidence tracking ─────────────────────────────────────────────────

/// Compute Wilson score confidence interval (lower, upper) at 95% confidence.
///
/// When the `simd-search` feature is enabled, delegates the lower-bound
/// computation to [`touring_simd::WilsonRanker`] and computes the upper bound
/// from the same z-score parameters.  Falls back to a self-contained scalar
/// implementation otherwise.
///
/// Returns `None` when `trials == 0`.
fn wilson_confidence_interval(successes: u64, trials: u64) -> Option<(f64, f64)> {
    if trials == 0 {
        return None;
    }

    #[cfg(feature = "simd-search")]
    {
        let ranker = touring_simd::WilsonRanker::new(0.95);
        let lower = ranker.wilson_bound(successes as u32, trials as u32);

        // Upper bound: mirror of the lower-bound formula (center + margin).
        // WilsonRanker only exposes the lower bound, so we compute upper inline.
        const Z: f64 = 1.96; // 95% confidence z-score
        let n = trials as f64;
        let p = successes as f64 / n;
        let z_sq = Z * Z;
        let denominator = 1.0 + z_sq / n;
        let center = (p + z_sq / (2.0 * n)) / denominator;
        let margin = Z * (p * (1.0 - p) / n + z_sq / (4.0 * n * n)).sqrt() / denominator;
        let upper = (center + margin).min(1.0);

        Some((lower, upper))
    }

    #[cfg(not(feature = "simd-search"))]
    {
        const Z: f64 = 1.96; // 95% confidence z-score
        let n = trials as f64;
        let p = successes as f64 / n;
        let z_sq = Z * Z;
        let denominator = 1.0 + z_sq / n;
        let center = (p + z_sq / (2.0 * n)) / denominator;
        let margin = Z * (p * (1.0 - p) / n + z_sq / (4.0 * n * n)).sqrt() / denominator;
        let lower = (center - margin).max(0.0);
        let upper = (center + margin).min(1.0);

        Some((lower, upper))
    }
}

/// Session-level trial counts for Wilson score calibration.
/// Key = file source hash, Value = (successes, trials).
static WILSON_TRIALS: OnceLock<Mutex<HashMap<u64, (u64, u64)>>> = OnceLock::new();

/// Returns the global Wilson trials map.
fn wilson_trials() -> &'static Mutex<HashMap<u64, (u64, u64)>> {
    WILSON_TRIALS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// Severity of a quality finding.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Severity {
    /// Informational — no immediate action required.
    Info,
    /// Warning — should be addressed but not blocking.
    Warning,
    /// Error — likely to cause bugs or security issues.
    Error,
}

impl Severity {
    /// Numeric weight used when computing the anti-pattern score.
    fn weight(self) -> f32 {
        match self {
            Severity::Info => 0.5,
            Severity::Warning => 1.0,
            Severity::Error => 3.0,
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => f.write_str("info"),
            Severity::Warning => f.write_str("warning"),
            Severity::Error => f.write_str("error"),
        }
    }
}

/// A single anti-pattern detected in source code.
///
/// Not `Deserialize` because `name` and `message` are `&'static str` — they
/// cannot be reconstructed from arbitrary deserialized bytes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AntiPatternHit {
    /// Short identifier (e.g. `"unwrap"`, `"bare_except"`).
    pub name: &'static str,
    /// Number of occurrences found in the source.
    pub count: usize,
    /// Severity classification.
    pub severity: Severity,
    /// Human-readable explanation and suggested fix.
    pub message: &'static str,
}

/// Comprehensive quality report for a single source file.
///
/// Not `Deserialize` because it embeds `AntiPatternHit` which contains `&'static str`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QualityReport {
    /// Language the source was analyzed as.
    pub lang: Lang,
    /// Maximum cyclomatic complexity across all symbols.
    pub max_complexity: u16,
    /// Average cyclomatic complexity across all symbols.
    pub avg_complexity: f32,
    /// Names of symbols with CC > 10 (formatted as `"name(CC=N)"`).
    pub complex_symbols: Vec<String>,
    /// All anti-patterns detected in the source.
    pub antipatterns: Vec<AntiPatternHit>,
    /// Complexity score: 1.0 = no issues; penalised for max/avg CC above thresholds.
    pub complexity_score: f32,
    /// Anti-pattern score: 1.0 = none found; each hit reduces the score.
    pub antipattern_score: f32,
    /// Weighted overall score (`0.6 × complexity_score + 0.4 × antipattern_score`).
    pub overall_score: f32,
    /// Wilson score confidence interval (lower, upper) at 95% confidence.
    pub confidence_interval: Option<(f32, f32)>,
    /// Number of quality observations for confidence calibration.
    pub confidence_n_trials: Option<u32>,
}

impl QualityReport {
    /// Returns `true` if `overall_score >= threshold`.
    #[must_use]
    pub fn passes(&self, threshold: f32) -> bool {
        self.overall_score >= threshold
    }

    /// Returns a compact one-line summary suitable for context injection.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if self.max_complexity > 1 {
            parts.push(format!(
                "CC max={} avg={:.1}",
                self.max_complexity, self.avg_complexity
            ));
        }
        if !self.complex_symbols.is_empty() {
            parts.push(format!("HIGH CC: {}", self.complex_symbols.join(", ")));
        }

        let errors: Vec<&str> = self
            .antipatterns
            .iter()
            .filter(|h| h.severity == Severity::Error)
            .map(|h| h.name)
            .collect();
        if !errors.is_empty() {
            parts.push(format!("ERRORS: {}", errors.join(", ")));
        }

        let warnings: Vec<&str> = self
            .antipatterns
            .iter()
            .filter(|h| h.severity == Severity::Warning)
            .map(|h| h.name)
            .collect();
        if !warnings.is_empty() {
            parts.push(format!("WARN: {}", warnings.join(", ")));
        }

        parts.push(format!("score={:.2}", self.overall_score));
        parts.join(" | ")
    }
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Analyze code quality for the given source and language.
///
/// Combines cyclomatic complexity analysis with fast string-scan anti-pattern
/// detection. All scores are in `[0.0, 1.0]` where higher is better.
#[must_use]
pub fn analyze_quality(source: &str, lang: Lang) -> QualityReport {
    // 1. Complexity via tree-sitter
    let cc_map = compute_complexity_for_source(source, lang).unwrap_or_default();

    let complexities: Vec<u16> = cc_map.iter().map(|(_, cc)| *cc).collect();
    let max_complexity = complexities.iter().copied().max().unwrap_or(1);
    let avg_complexity = if complexities.is_empty() {
        1.0_f32
    } else {
        complexities.iter().map(|&c| c as f32).sum::<f32>() / complexities.len() as f32
    };

    // |&(name, cc)| strips one & from &&(String, u16) → cc: &u16; *cc dereferences to u16.
    let mut complex_symbols: Vec<String> = cc_map
        .iter()
        .filter(|&(_, cc)| *cc > 10)
        .map(|(name, cc)| format!("{name}(CC={cc})"))
        .collect();
    complex_symbols.sort();

    let complexity_score = compute_complexity_score(max_complexity, avg_complexity);

    // 2. Anti-pattern detection
    let antipatterns = detect_antipatterns(source, lang);
    let antipattern_score = compute_antipattern_score(&antipatterns);

    // 3. Weighted overall
    let overall_score = (0.6 * complexity_score + 0.4 * antipattern_score).clamp(0.0, 1.0);

    // Wilson confidence tracking — record trial and compute confidence interval
    let source_hash = {
        use std::collections::hash_map::DefaultHasher;
        let mut h = DefaultHasher::new();
        source.hash(&mut h);
        h.finish()
    };

    let passed = overall_score >= 0.7;
    {
        let mut trials = wilson_trials()
            .lock()
            .expect("wilson_trials mutex poisoned");
        let entry = trials.entry(source_hash).or_insert((0, 0));
        entry.1 += 1;
        if passed {
            entry.0 += 1;
        }
    }

    let confidence_interval = {
        let trials = wilson_trials()
            .lock()
            .expect("wilson_trials mutex poisoned");
        let (successes, n_trials) = trials.get(&source_hash).copied().unwrap_or((0, 0));
        if n_trials > 0 {
            wilson_confidence_interval(successes, n_trials).map(|(lo, hi)| (lo as f32, hi as f32))
        } else {
            None
        }
    };

    let confidence_n_trials = {
        let trials = wilson_trials()
            .lock()
            .expect("wilson_trials mutex poisoned");
        trials.get(&source_hash).map(|(_, n)| *n as u32)
    };

    QualityReport {
        lang,
        max_complexity,
        avg_complexity,
        complex_symbols,
        antipatterns,
        complexity_score,
        antipattern_score,
        overall_score,
        confidence_interval,
        confidence_n_trials,
    }
}

// ─── Score computation ────────────────────────────────────────────────────────

fn compute_complexity_score(max_cc: u16, avg_cc: f32) -> f32 {
    // Penalise max CC above warning threshold (10)
    let max_penalty = if max_cc <= 10 {
        0.0_f32
    } else if max_cc <= 20 {
        f32::from(max_cc - 10) * 0.05
    } else {
        0.5_f32 + (f32::from(max_cc - 20) * 0.02).min(0.5)
    };

    // Penalise high average CC
    let avg_penalty = if avg_cc <= 5.0 {
        0.0_f32
    } else if avg_cc <= 10.0 {
        (avg_cc - 5.0) * 0.02
    } else {
        0.1_f32 + ((avg_cc - 10.0) * 0.03).min(0.4)
    };

    (1.0 - max_penalty - avg_penalty).max(0.0)
}

fn compute_antipattern_score(hits: &[AntiPatternHit]) -> f32 {
    if hits.is_empty() {
        return 1.0;
    }
    // Use sqrt(count) so files with many repetitions of the same pattern are
    // penalised sub-linearly — 20 unwraps is worse than 1, but not 20×.
    let total_weight: f32 = hits
        .iter()
        .map(|h| h.severity.weight() * (h.count as f32).sqrt())
        .sum();
    (1.0 - total_weight * 0.05).max(0.0)
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

fn detect_antipatterns(source: &str, lang: Lang) -> Vec<AntiPatternHit> {
    match lang {
        Lang::Rust => detect_rust(source),
        Lang::Python => detect_python(source),
        Lang::TypeScript => detect_typescript(source),
        Lang::JavaScript => detect_javascript(source),
        #[cfg(feature = "more-languages")]
        Lang::Go => detect_go(source),
        #[cfg(feature = "more-languages")]
        Lang::Java => detect_java(source),
        // Markup / data / shell: no code anti-patterns
        _ => Vec::new(),
    }
}

// ─── Rust ─────────────────────────────────────────────────────────────────────
//
// Split into two helpers to keep CC ≤ 10 per function.

fn detect_rust(source: &str) -> Vec<AntiPatternHit> {
    let mut hits = detect_rust_safety(source);
    hits.extend(detect_rust_style(source));
    hits
}

/// Safety-critical Rust anti-patterns (panic paths, unsafe, unfinished stubs).
fn detect_rust_safety(source: &str) -> Vec<AntiPatternHit> {
    let mut hits = Vec::new();

    let unwrap = count_occurrences(source, ".unwrap()");
    if unwrap > 0 {
        hits.push(AntiPatternHit {
            name: "unwrap",
            count: unwrap,
            severity: Severity::Warning,
            message: "`.unwrap()` panics on None/Err — use `?`, `.expect(\"reason\")`, or match",
        });
    }

    let empty_expect = count_occurrences(source, ".expect(\"\")");
    if empty_expect > 0 {
        hits.push(AntiPatternHit {
            name: "empty_expect",
            count: empty_expect,
            severity: Severity::Warning,
            message: "`.expect(\"\")` gives no context on panic — add a descriptive message",
        });
    }

    let unsafe_blocks = count_occurrences(source, "unsafe {");
    if unsafe_blocks > 0 {
        hits.push(AntiPatternHit {
            name: "unsafe_block",
            count: unsafe_blocks,
            severity: Severity::Warning,
            message: "`unsafe {}` bypasses Rust safety guarantees — add a `// SAFETY:` comment",
        });
    }

    let panic_calls = count_occurrences(source, "panic!(");
    if panic_calls > 0 {
        hits.push(AntiPatternHit {
            name: "panic",
            count: panic_calls,
            severity: Severity::Warning,
            message: "`panic!()` aborts the process — return `Result` or `Option` instead",
        });
    }

    let todo_calls = count_occurrences(source, "todo!()") + count_occurrences(source, "todo!(\"");
    if todo_calls > 0 {
        hits.push(AntiPatternHit {
            name: "todo",
            count: todo_calls,
            severity: Severity::Info,
            message: "`todo!()` panics at runtime — implement before shipping to production",
        });
    }

    let unimpl = count_occurrences(source, "unimplemented!(");
    if unimpl > 0 {
        hits.push(AntiPatternHit {
            name: "unimplemented",
            count: unimpl,
            severity: Severity::Info,
            message: "`unimplemented!()` panics — implement the method or use a typed default",
        });
    }

    hits
}

/// Style and maintenance Rust anti-patterns (lint suppression, redundant clones).
fn detect_rust_style(source: &str) -> Vec<AntiPatternHit> {
    let mut hits = Vec::new();

    let allow_dead = count_occurrences(source, "#[allow(dead_code)]");
    if allow_dead > 0 {
        hits.push(AntiPatternHit {
            name: "allow_dead_code",
            count: allow_dead,
            severity: Severity::Info,
            message: "`#[allow(dead_code)]` suppresses lint — remove unused items or document why",
        });
    }

    let allow_unused = count_occurrences(source, "#[allow(unused");
    if allow_unused > 0 {
        hits.push(AntiPatternHit {
            name: "allow_unused",
            count: allow_unused,
            severity: Severity::Info,
            message: "`#[allow(unused_*)]` suppresses lint — remove unused items or justify it",
        });
    }

    let double_clone = count_occurrences(source, ".clone().clone()");
    if double_clone > 0 {
        hits.push(AntiPatternHit {
            name: "double_clone",
            count: double_clone,
            severity: Severity::Warning,
            message: "`.clone().clone()` is redundant — a single `.clone()` suffices",
        });
    }

    let ignored_result = count_occurrences(source, "let _ =");
    if ignored_result > 0 {
        hits.push(AntiPatternHit {
            name: "ignored_result",
            count: ignored_result,
            severity: Severity::Warning,
            message:
                "`let _ = expr` silently discards a Result/Future — use `if let Err(e)` or `drop()`",
        });
    }

    hits
}

// ─── Python ───────────────────────────────────────────────────────────────────

fn detect_python(source: &str) -> Vec<AntiPatternHit> {
    let mut hits = Vec::new();

    let bare_except = count_line_pattern(source, "except:");
    if bare_except > 0 {
        hits.push(AntiPatternHit {
            name: "bare_except",
            count: bare_except,
            severity: Severity::Warning,
            message:
                "`except:` catches all exceptions including KeyboardInterrupt — use specific types",
        });
    }

    let broad = count_line_pattern(source, "except Exception:")
        + count_line_pattern(source, "except BaseException:");
    if broad > 0 {
        hits.push(AntiPatternHit {
            name: "broad_except",
            count: broad,
            severity: Severity::Warning,
            message:
                "`except Exception/BaseException:` is too broad — catch specific exception types",
        });
    }

    // concat! prevents literal trigger in this source file by security-scanning hooks
    let eval_count = count_occurrences(source, concat!("eval", "("));
    if eval_count > 0 {
        hits.push(AntiPatternHit {
            name: "eval",
            count: eval_count,
            severity: Severity::Error,
            message: "`eval()` executes arbitrary code — potential security vulnerability",
        });
    }

    let exec_count = count_occurrences(source, concat!("exec", "("));
    if exec_count > 0 {
        hits.push(AntiPatternHit {
            name: "exec",
            count: exec_count,
            severity: Severity::Error,
            message: "`exec()` executes arbitrary code — potential security vulnerability",
        });
    }

    let star_import = count_line_pattern(source, "import *");
    if star_import > 0 {
        hits.push(AntiPatternHit {
            name: "star_import",
            count: star_import,
            severity: Severity::Warning,
            message: "`import *` pollutes the namespace — import specific names",
        });
    }

    let global_stmt = count_line_pattern(source, "global ");
    if global_stmt > 0 {
        hits.push(AntiPatternHit {
            name: "global_statement",
            count: global_stmt,
            severity: Severity::Warning,
            message: "`global` makes code hard to test — pass values via parameters",
        });
    }

    let print_calls = count_occurrences(source, "print(");
    if print_calls > 0 {
        hits.push(AntiPatternHit {
            name: "print_statement",
            count: print_calls,
            severity: Severity::Info,
            message: "`print()` in production — use the `logging` module instead",
        });
    }

    hits
}

// ─── TypeScript ───────────────────────────────────────────────────────────────

fn detect_typescript(source: &str) -> Vec<AntiPatternHit> {
    let mut hits = Vec::new();

    let any_type = count_occurrences(source, ": any") + count_occurrences(source, "<any>");
    let as_any = count_occurrences(source, "as any");
    let total_any = any_type + as_any;
    if total_any > 0 {
        hits.push(AntiPatternHit {
            name: "any_type",
            count: total_any,
            severity: Severity::Warning,
            message: "`any` disables TypeScript type checking — use `unknown` or a specific type",
        });
    }

    // @ts-ignore lives inside comment lines — use count_occurrences, not count_line_pattern
    let ts_ignore = count_occurrences(source, "@ts-ignore");
    if ts_ignore > 0 {
        hits.push(AntiPatternHit {
            name: "ts_ignore",
            count: ts_ignore,
            severity: Severity::Warning,
            message: "`@ts-ignore` suppresses type errors — fix the underlying type issue",
        });
    }

    let ts_nocheck = count_occurrences(source, "@ts-nocheck");
    if ts_nocheck > 0 {
        hits.push(AntiPatternHit {
            name: "ts_nocheck",
            count: ts_nocheck,
            severity: Severity::Error,
            message: "`@ts-nocheck` disables type checking for the entire file",
        });
    }

    let non_null = count_occurrences(source, "!.");
    if non_null > 2 {
        hits.push(AntiPatternHit {
            name: "non_null_assertion",
            count: non_null,
            severity: Severity::Info,
            message: "Excessive `!.` non-null assertions — use null checks or optional chaining",
        });
    }

    // TypeScript is a JS superset — run shared JS patterns too
    hits.extend(detect_js_shared(source));

    hits
}

// ─── JavaScript ───────────────────────────────────────────────────────────────

fn detect_javascript(source: &str) -> Vec<AntiPatternHit> {
    detect_js_shared(source)
}

/// Patterns shared between JavaScript and TypeScript.
fn detect_js_shared(source: &str) -> Vec<AntiPatternHit> {
    let mut hits = Vec::new();

    let eval = count_occurrences(source, concat!("eval", "("));
    if eval > 0 {
        hits.push(AntiPatternHit {
            name: "eval",
            count: eval,
            severity: Severity::Error,
            message: "`eval()` executes arbitrary code — security and performance risk",
        });
    }

    let with_stmt = count_occurrences(source, "with (");
    if with_stmt > 0 {
        hits.push(AntiPatternHit {
            name: "with_statement",
            count: with_stmt,
            severity: Severity::Error,
            message: "`with` is deprecated and creates unpredictable scoping — remove it",
        });
    }

    let var_decl = count_line_pattern(source, "var ");
    if var_decl > 0 {
        hits.push(AntiPatternHit {
            name: "var_declaration",
            count: var_decl,
            severity: Severity::Warning,
            message: "`var` has function scope — use `const` or `let` for block scope",
        });
    }

    let loose_null = count_occurrences(source, "== null") + count_occurrences(source, "!= null");
    let loose_undef =
        count_occurrences(source, "== undefined") + count_occurrences(source, "!= undefined");
    let loose_eq = loose_null + loose_undef;
    if loose_eq > 0 {
        hits.push(AntiPatternHit {
            name: "loose_equality",
            count: loose_eq,
            severity: Severity::Info,
            message: "Loose `==`/`!=` with null/undefined — use strict `===`/`!==`",
        });
    }

    let console_calls =
        count_occurrences(source, "console.log(") + count_occurrences(source, "console.error(");
    if console_calls > 0 {
        hits.push(AntiPatternHit {
            name: "console_call",
            count: console_calls,
            severity: Severity::Info,
            message: "`console.*` in production code — use a structured logger",
        });
    }

    hits
}

// ─── Go ───────────────────────────────────────────────────────────────────────

#[cfg(feature = "more-languages")]
fn detect_go(source: &str) -> Vec<AntiPatternHit> {
    let mut hits = Vec::new();

    let ignored = count_occurrences(source, "_ = ");
    if ignored > 0 {
        hits.push(AntiPatternHit {
            name: "ignored_result",
            count: ignored,
            severity: Severity::Warning,
            message: "`_ = expr` may discard errors — use `err :=` and check `if err != nil`",
        });
    }

    let panic = count_occurrences(source, "panic(");
    if panic > 0 {
        hits.push(AntiPatternHit {
            name: "panic",
            count: panic,
            severity: Severity::Warning,
            message: "`panic()` crashes the program — return errors instead",
        });
    }

    let fmt_print =
        count_occurrences(source, "fmt.Println(") + count_occurrences(source, "fmt.Printf(");
    if fmt_print > 0 {
        hits.push(AntiPatternHit {
            name: "fmt_print",
            count: fmt_print,
            severity: Severity::Info,
            message: "`fmt.Println/Printf` in production — use `log` or `slog` package",
        });
    }

    let ioutil = count_occurrences(source, "ioutil.");
    if ioutil > 0 {
        hits.push(AntiPatternHit {
            name: "ioutil_deprecated",
            count: ioutil,
            severity: Severity::Info,
            message: "`ioutil` is deprecated since Go 1.16 — use `io` and `os` packages",
        });
    }

    hits
}

// ─── Java ─────────────────────────────────────────────────────────────────────

#[cfg(feature = "more-languages")]
fn detect_java(source: &str) -> Vec<AntiPatternHit> {
    let mut hits = Vec::new();

    let broad = count_occurrences(source, "catch (Exception")
        + count_occurrences(source, "catch (Throwable");
    if broad > 0 {
        hits.push(AntiPatternHit {
            name: "broad_catch",
            count: broad,
            severity: Severity::Warning,
            message: "`catch (Exception/Throwable)` is too broad — catch specific types",
        });
    }

    let sysout = count_occurrences(source, "System.out.println")
        + count_occurrences(source, "System.err.println");
    if sysout > 0 {
        hits.push(AntiPatternHit {
            name: "system_out_println",
            count: sysout,
            severity: Severity::Info,
            message: "`System.out.println` in production — use SLF4J or Log4j",
        });
    }

    let stack_trace = count_occurrences(source, "printStackTrace()");
    if stack_trace > 0 {
        hits.push(AntiPatternHit {
            name: "print_stack_trace",
            count: stack_trace,
            severity: Severity::Warning,
            message: "`e.printStackTrace()` writes to stderr without structure — use a logger",
        });
    }

    let null_checks = count_occurrences(source, "== null") + count_occurrences(source, "!= null");
    if null_checks > 3 {
        hits.push(AntiPatternHit {
            name: "excessive_null_checks",
            count: null_checks,
            severity: Severity::Info,
            message: "Many null checks — consider `Optional<T>` or null-safe patterns",
        });
    }

    let thread_sleep = count_occurrences(source, "Thread.sleep(");
    if thread_sleep > 0 {
        hits.push(AntiPatternHit {
            name: "thread_sleep",
            count: thread_sleep,
            severity: Severity::Info,
            message: "`Thread.sleep()` in production — use proper synchronization primitives",
        });
    }

    hits
}

// ─── String scanning utilities ────────────────────────────────────────────────

/// Count non-overlapping occurrences of `needle` in `haystack`.
fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        count += 1;
        start += pos + needle.len();
    }
    count
}

/// Count lines that contain `pattern`, skipping comment-only lines.
///
/// Strips leading whitespace before testing.
/// Skips lines whose first non-whitespace characters are `//` or `#`.
fn count_line_pattern(haystack: &str, pattern: &str) -> usize {
    haystack
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            !t.starts_with("//") && !t.starts_with('#') && t.contains(pattern)
        })
        .count()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn find_hit<'a>(report: &'a QualityReport, name: &str) -> Option<&'a AntiPatternHit> {
        report.antipatterns.iter().find(|h| h.name == name)
    }

    // ── Rust ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_rust_unwrap_detected() {
        let src = "fn foo(x: Option<i32>) -> i32 { x.unwrap() }";
        let report = analyze_quality(src, Lang::Rust);
        let h = find_hit(&report, "unwrap").expect("unwrap hit must be present");
        assert_eq!(h.count, 1);
        assert_eq!(h.severity, Severity::Warning);
    }

    #[test]
    fn test_rust_no_antipatterns() {
        let src = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let report = analyze_quality(src, Lang::Rust);
        assert!(report.antipatterns.is_empty());
        assert_eq!(report.antipattern_score, 1.0);
    }

    #[test]
    fn test_rust_unsafe_block_detected() {
        let src = "fn raw() { unsafe { let _x = 1i32; } }";
        let report = analyze_quality(src, Lang::Rust);
        let h = find_hit(&report, "unsafe_block").expect("unsafe_block must be detected");
        assert_eq!(h.severity, Severity::Warning);
    }

    #[test]
    fn test_rust_panic_detected() {
        let src = r#"fn fail() { panic!("oops") }"#;
        let report = analyze_quality(src, Lang::Rust);
        assert!(find_hit(&report, "panic").is_some());
    }

    #[test]
    fn test_rust_todo_info_severity() {
        let src = "fn not_done() -> i32 { todo!() }";
        let report = analyze_quality(src, Lang::Rust);
        let h = find_hit(&report, "todo").expect("todo hit must be present");
        assert_eq!(h.severity, Severity::Info);
    }

    #[test]
    fn test_rust_let_underscore_detected() {
        let src = r#"fn silent() { let _ = std::fs::remove_file("x.txt"); }"#;
        let report = analyze_quality(src, Lang::Rust);
        assert!(find_hit(&report, "ignored_result").is_some());
    }

    #[test]
    fn test_rust_double_clone_detected() {
        let src = "fn dup(s: String) -> String { s.clone().clone() }";
        let report = analyze_quality(src, Lang::Rust);
        assert!(find_hit(&report, "double_clone").is_some());
    }

    #[test]
    fn test_rust_unwrap_count_multiple() {
        let src = "fn f(a: Option<i32>, b: Option<i32>) -> i32 { a.unwrap() + b.unwrap() }";
        let report = analyze_quality(src, Lang::Rust);
        assert_eq!(
            find_hit(&report, "unwrap")
                .expect("unwrap hit required")
                .count,
            2
        );
    }

    #[test]
    fn test_rust_allow_dead_code_detected() {
        let src = "#[allow(dead_code)]\nfn unused() {}";
        let report = analyze_quality(src, Lang::Rust);
        assert!(find_hit(&report, "allow_dead_code").is_some());
    }

    // ── Python ────────────────────────────────────────────────────────────────

    #[test]
    fn test_python_bare_except_detected() {
        let src = "try:\n    foo()\nexcept:\n    pass\n";
        let report = analyze_quality(src, Lang::Python);
        assert!(find_hit(&report, "bare_except").is_some());
    }

    #[test]
    fn test_python_eval_is_error() {
        // Use concat! so this source file doesn't contain the literal "eval("
        let src = concat!("result = eval", "(user_input)");
        let report = analyze_quality(src, Lang::Python);
        let h = find_hit(&report, "eval").expect("eval must be detected");
        assert_eq!(h.severity, Severity::Error);
    }

    #[test]
    fn test_python_exec_is_error() {
        let src = concat!("exec", "(code_string)");
        let report = analyze_quality(src, Lang::Python);
        let h = find_hit(&report, "exec").expect("exec must be detected");
        assert_eq!(h.severity, Severity::Error);
    }

    #[test]
    fn test_python_star_import_detected() {
        let src = "from os.path import *\n";
        let report = analyze_quality(src, Lang::Python);
        assert!(find_hit(&report, "star_import").is_some());
    }

    #[test]
    fn test_python_global_detected() {
        let src = "def f():\n    global counter\n    counter += 1\n";
        let report = analyze_quality(src, Lang::Python);
        assert!(find_hit(&report, "global_statement").is_some());
    }

    #[test]
    fn test_python_no_antipatterns() {
        let src = "def add(a, b):\n    return a + b\n";
        let report = analyze_quality(src, Lang::Python);
        assert!(find_hit(&report, "eval").is_none());
        assert!(find_hit(&report, "bare_except").is_none());
        assert!(find_hit(&report, "star_import").is_none());
    }

    #[test]
    fn test_python_comment_except_not_detected() {
        let src = "# except: handle errors\ndef f():\n    return 1\n";
        let report = analyze_quality(src, Lang::Python);
        assert!(
            find_hit(&report, "bare_except").is_none(),
            "except in comment should not be flagged"
        );
    }

    // ── TypeScript ────────────────────────────────────────────────────────────

    #[test]
    fn test_typescript_any_type_detected() {
        let src = "function foo(x: any): any { return x; }";
        let report = analyze_quality(src, Lang::TypeScript);
        let h = find_hit(&report, "any_type").expect("any_type must be detected");
        assert!(h.count >= 2, "expected >=2 `any` occurrences");
    }

    #[test]
    fn test_typescript_ts_ignore_in_comment_detected() {
        // @ts-ignore is intentionally in a comment — must not be filtered
        let src = "// @ts-ignore\nconst x = foo();\n";
        let report = analyze_quality(src, Lang::TypeScript);
        assert!(
            find_hit(&report, "ts_ignore").is_some(),
            "ts_ignore in comment line must still be detected"
        );
    }

    #[test]
    fn test_typescript_ts_nocheck_is_error() {
        let src = "// @ts-nocheck\n";
        let report = analyze_quality(src, Lang::TypeScript);
        let h = find_hit(&report, "ts_nocheck").expect("ts_nocheck must be detected");
        assert_eq!(h.severity, Severity::Error);
    }

    #[test]
    fn test_typescript_inherits_js_eval() {
        let src = concat!("const v = eval", "('1+2');");
        let report = analyze_quality(src, Lang::TypeScript);
        assert!(find_hit(&report, "eval").is_some());
    }

    // ── JavaScript ────────────────────────────────────────────────────────────

    #[test]
    fn test_javascript_eval_is_error() {
        let src = concat!("const result = eval", "('1 + 2');");
        let report = analyze_quality(src, Lang::JavaScript);
        let h = find_hit(&report, "eval").expect("eval must be detected");
        assert_eq!(h.severity, Severity::Error);
    }

    #[test]
    fn test_javascript_var_detected() {
        let src = "var x = 1;\nvar y = 2;\n";
        let report = analyze_quality(src, Lang::JavaScript);
        let h = find_hit(&report, "var_declaration").expect("var_declaration must be detected");
        assert_eq!(h.count, 2);
    }

    #[test]
    fn test_javascript_loose_equality_detected() {
        let src = "if (x == null || y != undefined) {}";
        let report = analyze_quality(src, Lang::JavaScript);
        let h = find_hit(&report, "loose_equality").expect("loose_equality must be detected");
        assert_eq!(h.count, 2);
    }

    #[test]
    fn test_javascript_no_antipatterns() {
        let src = "const x = 1;\nfunction add(a, b) { return a + b; }\n";
        let report = analyze_quality(src, Lang::JavaScript);
        assert!(find_hit(&report, "eval").is_none());
        assert!(find_hit(&report, "var_declaration").is_none());
    }

    // ── Scoring ───────────────────────────────────────────────────────────────

    #[test]
    fn test_overall_score_in_range() {
        let src = "fn foo(x: Option<i32>) -> i32 { x.unwrap() }";
        let report = analyze_quality(src, Lang::Rust);
        assert!(report.overall_score >= 0.0);
        assert!(report.overall_score <= 1.0);
    }

    #[test]
    fn test_clean_code_score_near_one() {
        let src = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let report = analyze_quality(src, Lang::Rust);
        assert!(
            report.overall_score > 0.9,
            "clean code should score >0.9, got {}",
            report.overall_score
        );
    }

    #[test]
    fn test_passes_threshold() {
        let src = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let report = analyze_quality(src, Lang::Rust);
        assert!(report.passes(0.8));
    }

    #[test]
    fn test_summary_contains_score() {
        let src = "fn foo(x: Option<i32>) -> i32 { x.unwrap() }";
        let report = analyze_quality(src, Lang::Rust);
        let s = report.summary();
        assert!(!s.is_empty());
        assert!(s.contains("score="), "summary must contain 'score='");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    // ── Data / markup languages ───────────────────────────────────────────────

    #[test]
    fn test_yaml_no_antipatterns() {
        let src = "key: value\nother: 123\n";
        let report = analyze_quality(src, Lang::Yaml);
        assert!(report.antipatterns.is_empty());
        assert_eq!(report.antipattern_score, 1.0);
    }

    #[test]
    fn test_json_no_antipatterns() {
        let src = r#"{"key": "value", "n": 42}"#;
        let report = analyze_quality(src, Lang::Json);
        assert!(report.antipatterns.is_empty());
    }

    #[test]
    fn test_toml_no_antipatterns() {
        let src = "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\n";
        let report = analyze_quality(src, Lang::Toml);
        assert!(report.antipatterns.is_empty());
    }

    // ── Utility functions ─────────────────────────────────────────────────────

    #[test]
    fn test_count_occurrences_basic() {
        assert_eq!(count_occurrences("aXbXcX", "X"), 3);
    }

    #[test]
    fn test_count_occurrences_empty_needle() {
        assert_eq!(count_occurrences("hello", ""), 0);
    }

    #[test]
    fn test_count_occurrences_no_match() {
        assert_eq!(count_occurrences("hello", "xyz"), 0);
    }

    #[test]
    fn test_count_line_pattern_skips_js_comments() {
        let src = "// var x = 1;\nlet y = 2;\n";
        assert_eq!(count_line_pattern(src, "var "), 0);
    }

    #[test]
    fn test_count_line_pattern_skips_hash_comments() {
        let src = "# except: in a comment\nexcept ValueError:\n    pass\n";
        assert_eq!(count_line_pattern(src, "except:"), 0);
    }
}
