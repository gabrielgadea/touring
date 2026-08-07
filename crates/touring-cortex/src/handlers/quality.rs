//! Quality Gate Handlers — Lint enforcement, quality summaries, and compliance tracking.
//!
//! - `CodeStandardsEnforcerHandler`: PreToolUse[Write|Edit] (sync, CAN BLOCK) — diff-based ruff lint
//! - `PostQualityGateHandler`: PostToolUse[Write|Edit] (async) — format check + complexity + summary
//! - `ComplianceCollectorHandler`: PostToolUse (async) — metrics to compliance.jsonl + Wilson

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use moka::sync::Cache;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{Decision, HandlerResult, HookEvent};

use super::dspy::RustNativeValidator;

// W6 (master-plan-v3): bridge the cortex hook engine to the unified
// 50-dim quality harness. The handler gains an additional quality signal
// (50-dim composite) on top of its existing ruff-based diff lint, so that
// `touring-quality::score_target` blocks at composite < 0.80 OR when any
// P0 BLOCK dim fails — without duplicating per-dim logic in the cortex.
use touring_quality::{DimId, OutputFormat, QualityReport, score_target};

// ── H51: CodeStandardsEnforcerHandler ────────────────────────────────

/// Diff-based lint enforcement: only blocks on NEW violations (not pre-existing ones).
///
/// Flow:
/// 1. Hash new content → check cache → skip if already linted
/// 2. Read existing file → run ruff → get baseline violations
/// 3. Run ruff on new content → get new violations
/// 4. Diff: block only if new violations introduced
///
/// Uses moka W-TinyLFU cache for optimal performance under Zipfian workloads.
pub struct CodeStandardsEnforcerHandler {
    /// Cache: content_hash → (errors, warnings) to skip re-lint of identical content.
    /// Uses moka W-TinyLFU with 10_000 entry capacity.
    cache: Cache<u64, (u32, u32)>,
    /// Manual entry count since moka doesn't expose len().
    count: AtomicUsize,
}

impl Default for CodeStandardsEnforcerHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeStandardsEnforcerHandler {
    /// Creates a handler with a 10,000-entry result cache and zeroed counter.
    pub fn new() -> Self {
        Self {
            cache: Cache::new(10_000),
            count: AtomicUsize::new(0),
        }
    }

    /// Hash content for cache key.
    fn hash_content(content: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    /// Run ruff on content, return (errors, warnings) count.
    fn run_ruff(content: &str, suffix: &str) -> Option<(u32, u32)> {
        let tmp_dir = std::env::temp_dir();
        let tmp_file = tmp_dir.join(format!("qgate_{}{suffix}.py", std::process::id()));
        if std::fs::write(&tmp_file, content).is_err() {
            return None;
        }

        let result = std::process::Command::new("ruff")
            .args(["check", "--output-format=json", "--no-fix"])
            .arg(&tmp_file)
            .output();

        let _ = std::fs::remove_file(&tmp_file);

        let output = result.ok()?;
        let diagnostics: Vec<serde_json::Value> =
            serde_json::from_slice(&output.stdout).unwrap_or_default();

        let mut errors = 0u32;
        let mut warnings = 0u32;
        for diag in &diagnostics {
            let code = diag.get("code").and_then(|v| v.as_str()).unwrap_or("");
            if code.starts_with('E') || code.starts_with('F') {
                errors += 1;
            } else {
                warnings += 1;
            }
        }

        Some((errors, warnings))
    }
}

impl Handler for CodeStandardsEnforcerHandler {
    fn name(&self) -> &str {
        "code_standards_enforcer"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Only lint Python files (Write only — Edit new_string is a fragment, not valid Python)
        let file_path = match &ctx.file_path {
            Some(fp) if fp.ends_with(".py") => fp.clone(),
            _ => return HandlerResult::skip(self.name()),
        };

        // Extract new content
        let new_content = ctx
            .tool_input
            .get("content")
            .or_else(|| ctx.tool_input.get("new_string"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if new_content.is_empty() || new_content.len() < 10 {
            return HandlerResult::skip(self.name());
        }

        // W6 (master-plan-v3): write the proposed content to a tmpfile so
        // `touring_quality::score_target` can read it. Cheap (a single
        // write + drop); avoids needing a separate "score buffer" path
        // in the engine. Best-effort: on any I/O error, fall through to
        // the existing ruff-based gate.
        let tmp = std::env::temp_dir().join(format!(
            "touring-cortex-quality-{}-{}.py",
            std::process::id(),
            content_hash_counter()
        ));
        if std::fs::write(&tmp, new_content).is_ok() {
            let tmp_str = tmp.to_string_lossy().to_string();
            if let Some(report) = score_python_quality(&tmp_str)
                && should_block_quality(&report)
            {
                let _ = std::fs::remove_file(&tmp);
                let reason = format!(
                    "CodeStandards BLOCK: composite={:.3} < 0.80 OR P0 BLOCK dim failed (blockers={:?}) in {}",
                    report.composite, report.blockers, file_path
                );
                return HandlerResult::block(self.name(), reason);
            }
            // Quality OK — fall through to existing ruff diff.
            let _ = std::fs::remove_file(&tmp);
        }

        // Evict cache if too large (prevent unbounded growth)
        if self.count.load(Ordering::Relaxed) > 1000 {
            self.cache.invalidate_all();
            self.count.store(0, Ordering::Relaxed);
        }

        // Check cache: if we already linted this exact content, skip
        let content_hash = Self::hash_content(new_content);
        if let Some(cached) = self.cache.get(&content_hash)
            && cached.0 == 0
            && cached.1 == 0
        {
            return HandlerResult::skip(self.name());
        }

        // Run ruff on new content
        let (new_errors, new_warnings) = match Self::run_ruff(new_content, "_new") {
            Some(counts) => counts,
            None => return HandlerResult::skip(self.name()), // ruff unavailable
        };

        // Cache the result
        self.cache.insert(content_hash, (new_errors, new_warnings));
        self.count.fetch_add(1, Ordering::Relaxed);

        // Get baseline violations from existing file (if it exists)
        let (base_errors, base_warnings) = if let Ok(existing) = std::fs::read_to_string(&file_path)
        {
            Self::run_ruff(&existing, "_base").unwrap_or((0, 0))
        } else {
            (0, 0) // New file — no baseline
        };

        // Diff: only count NEW violations
        let delta_errors = new_errors.saturating_sub(base_errors);
        let delta_warnings = new_warnings.saturating_sub(base_warnings);

        // Score: new_errors * 1.0 + new_warnings * 0.2
        let penalty = delta_errors as f64 * 1.0 + delta_warnings as f64 * 0.2;

        // Log to persistence
        if let Some(ref persistence) = ctx.persistence {
            let score = (1.0 - penalty * 0.1).clamp(0.0, 1.0);
            let _ = persistence.log_hook_event(
                "PreToolUse",
                "CodeStandardsEnforcer",
                &format!(
                    "penalty={penalty:.1} delta_e={delta_errors} delta_w={delta_warnings} file={file_path}"
                ),
                score,
            );
            let _ = persistence.drift_record("code_standards_score", score);
        }

        if penalty > 2.0 {
            let reason = format!(
                "CodeStandards BLOCK: +{delta_errors} new errors, +{delta_warnings} new warnings in {file_path}. Fix before writing."
            );
            HandlerResult::block(self.name(), reason)
        } else if delta_errors > 0 || delta_warnings > 2 {
            let warning =
                format!("CodeStandards WARN: +{delta_errors}E/+{delta_warnings}W in {file_path}");
            HandlerResult::allow(self.name(), Some(warning))
        } else {
            HandlerResult::skip(self.name())
        }
    }
}

// ── W6 (master-plan-v3) — bridge: 50-dim quality signal in handlers ───

/// Score a Python file using the unified `touring_quality::score_target`.
/// Returns `Some(QualityReport)` on success, `None` if the file cannot be
/// read (handler is best-effort — never panic on lint path).
fn score_python_quality(file_path: &str) -> Option<QualityReport> {
    let p = std::path::Path::new(file_path);
    let dims: &[DimId] = &[];
    score_target(p, dims, OutputFormat::Json).ok()
}

/// Decide whether a 50-dim quality report should BLOCK the write.
///
/// Block criteria (W6.T1.4 + W6.T1.5 of master-plan-v3):
/// - composite < 0.80 (Gold floor) → block
/// - any P0 BLOCK dim (`blockers` list) → block
fn should_block_quality(report: &QualityReport) -> bool {
    if report.composite < 0.80 {
        return true;
    }
    if !report.blockers.is_empty() {
        return true;
    }
    false
}

/// Monotonic counter for tmpfile names (avoids collisions across handler
/// invocations within the same process).
fn content_hash_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    CTR.fetch_add(1, Ordering::Relaxed)
}

// ── H52: PostQualityGateHandler ──────────────────────────────────────

/// Post-edit quality summary: format check + complexity estimate + summary injection.
pub struct PostQualityGateHandler;

impl Handler for PostQualityGateHandler {
    fn name(&self) -> &str {
        "post_quality_gate"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit")
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) if fp.ends_with(".py") || fp.ends_with(".rs") => fp.clone(),
            _ => return HandlerResult::skip(self.name()),
        };

        if ctx.context_budget_remaining < 100 {
            return HandlerResult::skip(self.name());
        }

        let mut stages: Vec<String> = Vec::new();

        // Stage 2: Format check (ruff format --check for Python)
        if file_path.ends_with(".py") {
            let fmt_result = std::process::Command::new("ruff")
                .args(["format", "--check", "--quiet"])
                .arg(&file_path)
                .output();

            match fmt_result {
                Ok(o) if o.status.success() => stages.push("fmt:OK".into()),
                Ok(_) => stages.push("fmt:NEEDS_FORMAT".into()),
                Err(_) => {} // ruff unavailable, skip
            }
        }

        // Stage 4: Complexity estimate (line count heuristic)
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            let total_lines = content.lines().count();
            let fn_count = content
                .lines()
                .filter(|l| {
                    let trimmed = l.trim();
                    trimmed.starts_with("def ")
                        || trimmed.starts_with("async def ")
                        || trimmed.starts_with("fn ")
                        || trimmed.starts_with("pub fn ")
                        || trimmed.starts_with("pub fn ")
                })
                .count();

            let avg_fn_len = total_lines.checked_div(fn_count).unwrap_or(total_lines);

            let complexity_note = if avg_fn_len > 50 {
                format!("complexity:HIGH(avg {avg_fn_len} lines/fn)")
            } else if avg_fn_len > 25 {
                format!("complexity:MODERATE(avg {avg_fn_len} lines/fn)")
            } else {
                format!("complexity:OK(avg {avg_fn_len} lines/fn)")
            };
            stages.push(complexity_note);

            // Log metrics
            if let Some(ref persistence) = ctx.persistence {
                let score = if avg_fn_len > 50 {
                    0.5
                } else if avg_fn_len > 25 {
                    0.75
                } else {
                    1.0
                };
                let _ = persistence.log_hook_event(
                    "PostToolUse",
                    "PostQualityGate",
                    &format!(
                        "lines={total_lines} fns={fn_count} avg={avg_fn_len} file={file_path}"
                    ),
                    score,
                );
            }
        }

        if stages.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Stage 6: Summary
        let filename = file_path.rsplit('/').next().unwrap_or(&file_path);
        let summary = format!("QGate[{filename}]: {}", stages.join(" | "));
        HandlerResult::allow(self.name(), Some(summary))
    }
}

// ── H53: ComplianceCollectorHandler ──────────────────────────────────

/// Collect tool usage metrics and append to compliance.jsonl for tracking.
pub struct ComplianceCollectorHandler;

impl Handler for ComplianceCollectorHandler {
    fn name(&self) -> &str {
        "compliance_collector"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let tool_name = ctx.tool_name.as_deref().unwrap_or("unknown");
        let file_ext = ctx
            .file_path
            .as_deref()
            .and_then(|p| p.rsplit('.').next())
            .unwrap_or("none");

        let success = !matches!(ctx.decision, Decision::Block(_));

        // Build JSONL entry
        let entry = serde_json::json!({
            "ts": chrono_timestamp(),
            "tool": tool_name,
            "file_type": file_ext,
            "success": success,
            "session": &ctx.session_id,
        });

        // Append to compliance.jsonl
        let metrics_dir = dirs_home().join(".claude/metrics");
        let _ = std::fs::create_dir_all(&metrics_dir);
        let metrics_file = metrics_dir.join("compliance.jsonl");

        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&metrics_file)
        {
            let _ = writeln!(f, "{}", entry);
        }

        // Update Wilson score for compliance tracking
        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.wilson_update(&format!("compliance:{tool_name}"), success);
        }

        HandlerResult::skip(self.name()) // Never injects context
    }
}

// ── H54: DspyQualityBridgeHandler ────────────────────────────────────

/// Bridge to Python DSPy quality validation via subprocess.
/// Fail-open: if DSPy or script unavailable, silently skips.
#[derive(Default)]
pub struct DspyQualityBridgeHandler {
    /// Cached result of `python3 -c "import dspy"` check (true=available, false=unavailable).
    /// Once checked, never re-checked in the same process.
    dspy_available: std::sync::OnceLock<bool>,
}

impl DspyQualityBridgeHandler {
    /// Creates a handler whose DSPy-availability check is lazily resolved on first use.
    pub fn new() -> Self {
        Self {
            dspy_available: std::sync::OnceLock::new(),
        }
    }

    fn is_dspy_available(&self) -> bool {
        *self.dspy_available.get_or_init(|| {
            std::process::Command::new("python3")
                .args(["-c", "import dspy"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
    }
}

impl Handler for DspyQualityBridgeHandler {
    fn name(&self) -> &str {
        "dspy_quality_bridge"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit")
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) if fp.ends_with(".py") => fp.clone(),
            _ => return HandlerResult::skip(self.name()),
        };

        if ctx.context_budget_remaining < 80 {
            return HandlerResult::skip(self.name());
        }

        // E5-S8: Try DSPy first; on failure/unavailable, fall back to RustNativeValidator
        let dspy_available = self.is_dspy_available();
        let script = dirs_home().join(".claude/scripts/dspy_quality_bridge.py");

        if dspy_available
            && script.exists()
            && let Ok(output) = std::process::Command::new("python3")
                .arg(&script)
                .arg(&file_path)
                .output()
            && output.status.success()
            && let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        {
            let e = parsed.get("errors").and_then(|v| v.as_u64()).unwrap_or(0);
            let w = parsed.get("warnings").and_then(|v| v.as_u64()).unwrap_or(0);
            if parsed.get("status").and_then(|v| v.as_str()) != Some("skip") {
                ctx.needs_cache_invalidation = true;
                let context = format!("DSPy[{e}E/{w}W]: {file_path}");
                return HandlerResult::allow(self.name(), Some(context));
            }
        }

        // E5-S8: Rust-native fallback — only runs if DSPy failed or was unavailable
        let validator = RustNativeValidator::new();
        let code = std::fs::read_to_string(&file_path).unwrap_or_default();
        let result = validator.validate(&file_path, &code);

        // score < 0.5 with errors = block; 0.5-0.8 = warn; > 0.8 = skip
        if result.score < 0.5 && result.errors > 0 {
            let context = format!(
                "RustValidator[block: score={:.2}]: {} — {}",
                result.score, file_path, result.details
            );
            return HandlerResult::block(self.name(), context);
        }

        if result.score < 0.8 {
            ctx.needs_cache_invalidation = true;
            let context = format!(
                "RustValidator[warn: score={:.2}]: {} — {}",
                result.score, file_path, result.details
            );
            return HandlerResult::allow(self.name(), Some(context));
        }

        HandlerResult::skip(self.name())
    }
}

// ── H55: DspySessionOptimizerHandler ─────────────────────────────────

/// Run session optimization on SessionEnd via Python subprocess.
pub struct DspySessionOptimizerHandler;

impl Handler for DspySessionOptimizerHandler {
    fn name(&self) -> &str {
        "dspy_session_optimizer"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let script = dirs_home().join(".claude/scripts/dspy_session_optimizer.py");
        if !script.exists() {
            return HandlerResult::skip(self.name());
        }

        let result = std::process::Command::new("python3").arg(&script).output();

        let output = match result {
            Ok(o) if o.status.success() => o,
            _ => return HandlerResult::skip(self.name()),
        };

        // Parse and store suggestions in RlmMemory
        if let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            && let Some(suggestions) = parsed.get("suggestions").and_then(|v| v.as_array())
            && !suggestions.is_empty()
        {
            let summary: Vec<String> = suggestions
                .iter()
                .filter_map(|s| s.get("message").and_then(|m| m.as_str()))
                .map(String::from)
                .collect();

            if let Some(ref rlm) = ctx.rlm {
                use touring_intelligence::rl::memory::rlm::MemoryTier;
                let _ = rlm.store(
                    "session:dspy_suggestions",
                    MemoryTier::Core,
                    &summary.join(" | "),
                    Some("suggestion"),
                    None,
                );
            }

            if let Some(ref persistence) = ctx.persistence {
                let _ = persistence.log_hook_event(
                    "SessionEnd",
                    "DspySessionOptimizer",
                    &format!("suggestions={}", suggestions.len()),
                    0.8,
                );
            }
        }

        HandlerResult::skip(self.name()) // Never injects context on SessionEnd
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Get home directory path.
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// Unix epoch timestamp as string — precise and unambiguous for JSONL metrics.
fn chrono_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    secs.to_string()
}

// ── H98: Analysis Health Handler (touring-analysis v0.2.0) ──────────

/// H98 — Injects touring-analysis health summary after Edit/Write operations.
///
/// Runs `analyze_knowledge()` on the knowledge DB and injects a one-line
/// summary with knowledge score, hot files, and active gotchas into
/// Claude's context. Lightweight: ~1ms (DB-only, no file reads).
pub struct AnalysisHealthHandler;

impl Handler for AnalysisHealthHandler {
    fn name(&self) -> &str {
        "h98_analysis_health"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Edit|Write|MultiEdit")
    }

    fn priority(&self) -> u8 {
        200 // Low priority — runs after core handlers
    }

    fn timeout_ms(&self) -> u64 {
        10 // Fast — DB queries only
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let report = touring_analysis::knowledge::analyze_knowledge(ctx.knowledge.conn_ref());

        let mut parts = Vec::new();
        parts.push(format!("knowledge:{:.2}", report.score));
        if report.hot_files > 0 {
            parts.push(format!("hot_files:{}", report.hot_files));
        }
        if report.active_gotchas > 0 {
            parts.push(format!("gotchas:{}", report.active_gotchas));
        }
        parts.push(format!("files:{}", report.total_files));

        let summary = format!("H98 [{}]", parts.join(" "));
        HandlerResult::allow(self.name(), Some(summary))
    }
}

// ── Registration ─────────────────────────────────────────────────────

// ── H99: Code Health Gate (Cortex Reactive Health Guardian) ─────────

/// Block writes when knowledge score is below this level (subcritical).
const HEALTH_GATE_CRITICAL: f64 = 0.20;
/// Warn (allow) when knowledge score is below this level (degraded).
const HEALTH_GATE_DEGRADED: f64 = 0.40;

/// H99 — Critical health gate that BLOCKS writes when codebase health is subcritical.
///
/// Intercepts `PreToolUse[Write|Edit|MultiEdit]` and calls `analyze_knowledge()` for
/// a fast DB-only health check. If the composite health score drops below
/// `HEALTH_GATE_CRITICAL`, emits `Decision::Block` to prevent further degradation.
///
/// Wilson lower bound is computed as: `score - 1.96 * sqrt(score*(1-score) / n)`,
/// giving an early-fire signal when data points are few.
///
/// - `priority = 20` — runs before most handlers to fail fast
/// - `is_critical = true` — never skipped on budget exhaustion
/// - `timeout_ms = 5` — DB queries only, no file I/O
pub struct CodeHealthGateHandler;

impl Handler for CodeHealthGateHandler {
    fn name(&self) -> &str {
        "h99_code_health_gate"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit")
    }

    fn priority(&self) -> u8 {
        20 // Critical priority — runs early to fail fast
    }

    fn is_critical(&self) -> bool {
        true // Health gate must always run regardless of budget
    }

    fn timeout_ms(&self) -> u64 {
        5 // Fast — DB queries only
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let report = touring_analysis::knowledge::analyze_knowledge(ctx.knowledge.conn_ref());
        let score = report.score;

        if score < HEALTH_GATE_CRITICAL {
            // Wilson lower bound: score - 1.96 * sqrt(score*(1-score) / n)
            let n = report.total_files.max(1) as f64;
            let wilson_lower = (score - 1.96 * (score * (1.0 - score) / n).sqrt()).max(0.0);
            return HandlerResult::block(
                self.name(),
                format!(
                    "H99 [CODE HEALTH GATE] BLOCKED — health score {:.2} \
                     (wilson_lower={:.2}) is below critical threshold {:.2}. \
                     hot_files={} active_gotchas={} total_files={}. \
                     Run `touring e2e -j` to diagnose and \
                     `touring evolution drift` to identify regressions.",
                    score,
                    wilson_lower,
                    HEALTH_GATE_CRITICAL,
                    report.hot_files,
                    report.active_gotchas,
                    report.total_files
                ),
            );
        }

        if score < HEALTH_GATE_DEGRADED {
            let context = format!(
                "H99 [CODE HEALTH GATE] WARNING — health score {:.2} is degraded \
                 (threshold={:.2}). Recommend `touring evolution drift` before \
                 committing further changes. hot_files={} active_gotchas={}",
                score, HEALTH_GATE_DEGRADED, report.hot_files, report.active_gotchas,
            );
            return HandlerResult::allow(self.name(), Some(context));
        }

        HandlerResult::allow(self.name(), None)
    }
}

/// Register all quality gate handlers.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(CodeHealthGateHandler)); // H99 first — critical gate
    pipeline.register(Box::new(CodeStandardsEnforcerHandler::new()));
    pipeline.register(Box::new(PostQualityGateHandler));
    pipeline.register(Box::new(ComplianceCollectorHandler));
    pipeline.register(Box::new(DspyQualityBridgeHandler::new()));
    pipeline.register(Box::new(DspySessionOptimizerHandler));
    pipeline.register(Box::new(AnalysisHealthHandler));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_content_deterministic() {
        let h1 = CodeStandardsEnforcerHandler::hash_content("def foo(): pass");
        let h2 = CodeStandardsEnforcerHandler::hash_content("def foo(): pass");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_content_different() {
        let h1 = CodeStandardsEnforcerHandler::hash_content("def foo(): pass");
        let h2 = CodeStandardsEnforcerHandler::hash_content("def bar(): pass");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_run_ruff_valid_python() {
        // Only run if ruff is available
        if std::process::Command::new("ruff")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let result = CodeStandardsEnforcerHandler::run_ruff("x = 1\n", "_test");
        assert!(result.is_some());
        let (errors, warnings) = result.unwrap();
        assert_eq!(errors, 0);
        assert_eq!(warnings, 0);
    }

    #[test]
    fn test_run_ruff_with_error() {
        if std::process::Command::new("ruff")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let result = CodeStandardsEnforcerHandler::run_ruff("import os\nimport os\n", "_test_err");
        assert!(result.is_some());
        let (errors, _warnings) = result.unwrap();
        // Redefined import should trigger F811
        assert!(errors > 0 || _warnings > 0);
    }

    #[test]
    fn test_chrono_timestamp_format() {
        let ts = chrono_timestamp();
        // Unix epoch seconds — all digits, reasonable range
        let secs: u64 = ts.parse().expect("should be numeric");
        assert!(secs > 1_700_000_000); // after ~2023
        assert!(secs < 2_000_000_000); // before ~2033
    }

    #[test]
    fn test_compliance_jsonl_entry_shape() {
        let entry = serde_json::json!({
            "ts": chrono_timestamp(),
            "tool": "Write",
            "file_type": "py",
            "success": true,
            "session": "test-session",
        });
        assert!(entry.get("tool").is_some());
        assert!(entry.get("ts").is_some());
    }

    // ── H99 CodeHealthGateHandler tests ────────────────────────────────

    #[test]
    fn h99_health_gate_is_critical_and_high_priority() {
        let h = CodeHealthGateHandler;
        assert!(
            h.is_critical(),
            "H99 must be critical (never skipped on budget)"
        );
        assert!(
            h.priority() < 32,
            "H99 priority must be in the critical-infrastructure band (0-31)"
        );
    }

    #[test]
    fn h99_health_gate_thresholds_ordering() {
        // CRITICAL < DEGRADED < 1.0 (logical invariant)
        assert!(HEALTH_GATE_CRITICAL < HEALTH_GATE_DEGRADED);
        assert!(HEALTH_GATE_DEGRADED < 1.0);
        assert!(HEALTH_GATE_CRITICAL >= 0.0);
    }

    #[test]
    fn h99_wilson_lower_bound_stays_non_negative() {
        // Wilson lower bound must not go negative (clamped to 0.0)
        let score = 0.05_f64;
        let n = 1_f64;
        let wilson_lower = (score - 1.96 * (score * (1.0 - score) / n).sqrt()).max(0.0);
        assert!(wilson_lower >= 0.0, "Wilson lower bound must be >= 0.0");
    }

    #[test]
    fn h99_health_gate_events_and_matcher() {
        use crate::types::HookEvent;
        let h = CodeHealthGateHandler;
        assert!(h.events().contains(&HookEvent::PreToolUse));
        assert_eq!(h.tool_matcher(), Some("Write|Edit|MultiEdit"));
    }
}
