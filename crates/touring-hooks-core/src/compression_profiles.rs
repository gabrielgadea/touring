//! NEW-1 — Per-Command Compression Profiles (RTK parity).
//!
//! Each profile applies filter/group/dedupe/truncate strategies tailored to
//! the structural conventions of a specific CLI tool's output. This drops
//! the bytes stored in `tool_outputs` AND returned by `ctx_retrieve` while
//! preserving the information the LLM needs to reason about the result.
//!
//! Profiles compose in `derive_summary`:
//! `compress_for(tool, args, raw)` → `redact_secrets(...)` → `snippet(...)` → store.
//!
//! Toggle via env `TOURING_COMPRESSION_PROFILES=0` (default ON).

use serde_json::Value;
use std::borrow::Cow;

/// Profile signature — applies_to is content/tool dispatch; compress is the
/// transformation. Pure functions only (no I/O, no globals) so testing and
/// composition stay deterministic.
pub trait Profile: Send + Sync {
    /// Stable identifier for this profile (used in telemetry and logs).
    fn name(&self) -> &'static str;
    /// Returns true when this profile should handle the given tool's output.
    fn applies_to(&self, tool_name: &str, args: &Value) -> bool;
    /// Compresses raw tool output, borrowing it unchanged when no transform applies.
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str>;
}

/// Single dispatch entry point with NEW-4 user-filter priority.
///
/// Lookup order:
/// 1. **NEW-4** user filter from `~/.config/touring/filters.toml`
/// 2. **NEW-1** built-in profile registry (15 profiles)
/// 3. raw passthrough
///
/// Counter `compression_profile_applied_count` increments for ANY match
/// (user or built-in) so observability is uniform.
///
/// **A2 (2026-08-08)** — every match also records the EXACT bytes in and out
/// (`record_compression_savings`). This is the site the savings claim was always
/// about: `ctx_roi` used to multiply the event counter above by an invented
/// 20_000 bytes, when the actual delta is right here as `raw.len() - out.len()`.
/// A match that compresses nothing now correctly contributes a zero saving
/// instead of a fictional 20 KB.
pub fn compress_for<'a>(tool_name: &str, args: &Value, raw: &'a str) -> Cow<'a, str> {
    if !crate::shared::feature_flags::compression_profiles_enabled() {
        return Cow::Borrowed(raw);
    }
    // NEW-4 priority: user filter wins
    let user_filters = crate::user_filters::load_user_filters();
    if let Some(uf) = crate::user_filters::find_matching_filter(&user_filters, tool_name, args) {
        crate::shared::gate_metrics::record_compression_profile_applied();
        let out = crate::user_filters::apply_user_filter(uf, raw);
        crate::shared::gate_metrics::record_compression_savings(raw, &out);
        return out;
    }
    // NEW-1 fallback: built-in registry
    for profile in registry().iter() {
        if profile.applies_to(tool_name, args) {
            crate::shared::gate_metrics::record_compression_profile_applied();
            let out = profile.compress(raw);
            crate::shared::gate_metrics::record_compression_savings(raw, &out);
            return out;
        }
    }
    Cow::Borrowed(raw)
}

/// Top-30 high-ROI profiles (extensible via NEW-4 user TOML DSL).
///
/// First match wins — order matters when patterns overlap (e.g.
/// `MakeProfile.applies_to` excludes `cargo` to avoid colliding with
/// CargoBuildProfile).
pub fn registry() -> Vec<Box<dyn Profile>> {
    vec![
        // Top-15 (Wave 2026-05-08).
        Box::new(CargoTestProfile),
        Box::new(CargoClippyProfile),
        Box::new(CargoBuildProfile),
        Box::new(GitLogProfile),
        Box::new(GitDiffProfile),
        Box::new(GitStatusProfile),
        Box::new(GhPrListProfile),
        Box::new(GhIssueListProfile),
        Box::new(KubectlGetProfile),
        Box::new(DockerPsProfile),
        Box::new(PytestProfile),
        Box::new(RuffProfile),
        Box::new(TscProfile),
        Box::new(NpmListProfile),
        Box::new(PipListProfile),
        // Next-15 (NEW-1 Wave post-RTK 2026-05-08 — closes 30/30 target).
        Box::new(EslintProfile),
        Box::new(MochaProfile),
        Box::new(JestProfile),
        Box::new(RustdocProfile),
        Box::new(TerraformPlanProfile),
        Box::new(AnsibleProfile),
        Box::new(MakeProfile),
        Box::new(JqProfile),
        Box::new(CurlVerboseProfile),
        Box::new(RsyncProfile),
        Box::new(TarProfile),
        Box::new(FfmpegProfile),
        Box::new(RipgrepProfile),
        Box::new(SystemctlStatusProfile),
        Box::new(JournalctlProfile),
    ]
}

// ─── Helper utilities (pure) ────────────────────────────────────────────────

fn detect_in_command(args: &Value, needles: &[&str]) -> bool {
    let cmd = args.get("command").and_then(Value::as_str).unwrap_or("");
    needles.iter().any(|n| cmd.contains(n))
}

fn keep_lines_matching(raw: &str, predicates: &[&str]) -> String {
    raw.lines()
        .filter(|line| predicates.iter().any(|p| line.contains(p)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_lines(raw: &str, max_lines: usize) -> String {
    let total = raw.lines().count();
    if total <= max_lines {
        return raw.to_string();
    }
    let kept: Vec<&str> = raw.lines().take(max_lines).collect();
    format!(
        "{}\n... (+{} more lines truncated by compression profile)",
        kept.join("\n"),
        total - max_lines
    )
}

fn dedupe_consecutive(raw: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut last: Option<String> = None;
    let mut count = 0u32;
    for line in raw.lines() {
        match &last {
            Some(prev) if prev == line => count += 1,
            _ => {
                if count > 1 {
                    let last_idx = out.len() - 1;
                    out[last_idx] = format!("{} (×{count})", out[last_idx]);
                }
                out.push(line.to_string());
                last = Some(line.to_string());
                count = 1;
            }
        }
    }
    if count > 1 {
        let last_idx = out.len() - 1;
        out[last_idx] = format!("{} (×{count})", out[last_idx]);
    }
    out.join("\n")
}

// ─── Profile implementations ────────────────────────────────────────────────

struct CargoTestProfile;
impl Profile for CargoTestProfile {
    fn name(&self) -> &'static str {
        "cargo_test"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["cargo test"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Keep failures + summary; drop "running N tests" + ".... ok" noise
        let keep = keep_lines_matching(
            raw,
            &[
                "FAILED",
                "panicked",
                "test result:",
                "error[E",
                "error:",
                "thread '",
                "left:",
                "right:",
            ],
        );
        if keep.is_empty() {
            // No failures — keep summary only
            Cow::Owned(keep_lines_matching(raw, &["test result:"]))
        } else {
            Cow::Owned(truncate_lines(&keep, 80))
        }
    }
}

struct CargoClippyProfile;
impl Profile for CargoClippyProfile {
    fn name(&self) -> &'static str {
        "cargo_clippy"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["cargo clippy"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(raw, &["warning:", "error[", "error:", "= help:", "-->"]);
        Cow::Owned(truncate_lines(&dedupe_consecutive(&keep), 100))
    }
}

struct CargoBuildProfile;
impl Profile for CargoBuildProfile {
    fn name(&self) -> &'static str {
        "cargo_build"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["cargo build", "cargo check"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(
            raw,
            &["error[", "error:", "warning:", "Finished", "Compiling"],
        );
        Cow::Owned(truncate_lines(&keep, 60))
    }
}

struct GitLogProfile;
impl Profile for GitLogProfile {
    fn name(&self) -> &'static str {
        "git_log"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["git log"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Collapse multi-line log entries to one-line: <abbrev_sha> <subject>
        let lines: Vec<String> = raw
            .split("\ncommit ")
            .filter(|chunk| !chunk.trim().is_empty())
            .filter_map(|chunk| {
                let mut it = chunk.lines();
                let header = it.next()?;
                let sha = header
                    .trim_start_matches("commit ")
                    .chars()
                    .take(7)
                    .collect::<String>();
                let subject = it
                    .find(|l| !l.starts_with("Author:") && !l.starts_with("Date:") && !l.is_empty())
                    .unwrap_or("(no subject)")
                    .trim();
                Some(format!("{sha} {subject}"))
            })
            .collect();
        Cow::Owned(truncate_lines(&lines.join("\n"), 50))
    }
}

struct GitDiffProfile;
impl Profile for GitDiffProfile {
    fn name(&self) -> &'static str {
        "git_diff"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["git diff", "git show"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Keep file headers + first @@ hunk per file, drop subsequent hunk bodies
        let keep = keep_lines_matching(raw, &["diff --git", "+++ ", "--- ", "@@", "Binary files"]);
        Cow::Owned(truncate_lines(&keep, 80))
    }
}

struct GitStatusProfile;
impl Profile for GitStatusProfile {
    fn name(&self) -> &'static str {
        "git_status"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["git status"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Drop the help blurb between sections
        let keep: Vec<&str> = raw
            .lines()
            .filter(|l| {
                !l.starts_with("  (")
                    && !l.contains("git checkout --")
                    && !l.contains("git restore")
            })
            .collect();
        Cow::Owned(truncate_lines(&keep.join("\n"), 80))
    }
}

struct GhPrListProfile;
impl Profile for GhPrListProfile {
    fn name(&self) -> &'static str {
        "gh_pr_list"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["gh pr list", "gh pr ls"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        Cow::Owned(truncate_lines(raw, 30))
    }
}

struct GhIssueListProfile;
impl Profile for GhIssueListProfile {
    fn name(&self) -> &'static str {
        "gh_issue_list"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["gh issue list", "gh issue ls"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        Cow::Owned(truncate_lines(raw, 30))
    }
}

struct KubectlGetProfile;
impl Profile for KubectlGetProfile {
    fn name(&self) -> &'static str {
        "kubectl_get"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["kubectl get"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        Cow::Owned(truncate_lines(raw, 60))
    }
}

struct DockerPsProfile;
impl Profile for DockerPsProfile {
    fn name(&self) -> &'static str {
        "docker_ps"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["docker ps", "docker container ls"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        Cow::Owned(truncate_lines(raw, 40))
    }
}

struct PytestProfile;
impl Profile for PytestProfile {
    fn name(&self) -> &'static str {
        "pytest"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["pytest", "py.test"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(
            raw,
            &[
                "FAILED", "PASSED", "ERROR", "assert", "passed", "failed", "==",
            ],
        );
        Cow::Owned(truncate_lines(&keep, 80))
    }
}

struct RuffProfile;
impl Profile for RuffProfile {
    fn name(&self) -> &'static str {
        "ruff"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["ruff check", "ruff format"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        Cow::Owned(truncate_lines(&dedupe_consecutive(raw), 60))
    }
}

struct TscProfile;
impl Profile for TscProfile {
    fn name(&self) -> &'static str {
        "tsc"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["tsc", "tsc --noEmit"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(raw, &["error TS", "Found ", " errors", " error."]);
        Cow::Owned(truncate_lines(&keep, 60))
    }
}

struct NpmListProfile;
impl Profile for NpmListProfile {
    fn name(&self) -> &'static str {
        "npm_list"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["npm list", "npm ls"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        Cow::Owned(truncate_lines(raw, 40))
    }
}

struct PipListProfile;
impl Profile for PipListProfile {
    fn name(&self) -> &'static str {
        "pip_list"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["pip list", "pip3 list"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        Cow::Owned(truncate_lines(raw, 40))
    }
}

// ─── 15 additional profiles (NEW-1 30-tool target, post-Wave 2026-05-08) ───
// All keep their `applies_to` conservative (well-known patterns) and fall
// back to raw passthrough on parse uncertainty (REGRA #0 — preserves info).

struct EslintProfile;
impl Profile for EslintProfile {
    fn name(&self) -> &'static str {
        "eslint"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["eslint", "npm run lint"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(raw, &["error", "warning", "✖", "problem", "/", ":"]);
        Cow::Owned(truncate_lines(&dedupe_consecutive(&keep), 80))
    }
}

struct MochaProfile;
impl Profile for MochaProfile {
    fn name(&self) -> &'static str {
        "mocha"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["mocha", "npm test"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(
            raw,
            &[
                "passing",
                "failing",
                "pending",
                "✗",
                "✘",
                "Error",
                "AssertionError",
            ],
        );
        Cow::Owned(truncate_lines(&keep, 80))
    }
}

struct JestProfile;
impl Profile for JestProfile {
    fn name(&self) -> &'static str {
        "jest"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["jest", "npx jest"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(
            raw,
            &["PASS", "FAIL", "Tests:", "Snapshots:", "Suites:", "✕", "●"],
        );
        Cow::Owned(truncate_lines(&keep, 80))
    }
}

struct RustdocProfile;
impl Profile for RustdocProfile {
    fn name(&self) -> &'static str {
        "rustdoc"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["cargo doc", "rustdoc"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(
            raw,
            &["warning:", "error[", "error:", "Documenting", "Finished"],
        );
        Cow::Owned(truncate_lines(&keep, 50))
    }
}

struct TerraformPlanProfile;
impl Profile for TerraformPlanProfile {
    fn name(&self) -> &'static str {
        "terraform_plan"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["terraform plan", "tofu plan"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(
            raw,
            &[
                "Plan:",
                "+ ",
                "- ",
                "~ ",
                "will be created",
                "will be destroyed",
                "will be updated",
                "Error:",
                "Warning:",
            ],
        );
        Cow::Owned(truncate_lines(&keep, 100))
    }
}

struct AnsibleProfile;
impl Profile for AnsibleProfile {
    fn name(&self) -> &'static str {
        "ansible_play"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["ansible-playbook", "ansible "])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(
            raw,
            &[
                "ok:",
                "changed:",
                "failed:",
                "skipped:",
                "PLAY ",
                "TASK ",
                "PLAY RECAP",
                "fatal:",
            ],
        );
        Cow::Owned(truncate_lines(&keep, 80))
    }
}

struct MakeProfile;
impl Profile for MakeProfile {
    fn name(&self) -> &'static str {
        "make"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash"
            && detect_in_command(args, &["make ", "make-"])
            && !detect_in_command(args, &["cargo "])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(
            raw,
            &[
                "Error",
                "error:",
                "warning:",
                "make:",
                "make[",
                "***",
                "undefined reference",
            ],
        );
        Cow::Owned(truncate_lines(&keep, 60))
    }
}

struct JqProfile;
impl Profile for JqProfile {
    fn name(&self) -> &'static str {
        "jq"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &[" jq ", "| jq"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // jq output is typically JSON; truncate at 40 lines (preserves structure
        // headers); raw passthrough preferred when output is small.
        if raw.len() < 4_000 {
            Cow::Borrowed(raw)
        } else {
            Cow::Owned(truncate_lines(raw, 40))
        }
    }
}

struct CurlVerboseProfile;
impl Profile for CurlVerboseProfile {
    fn name(&self) -> &'static str {
        "curl_verbose"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash"
            && detect_in_command(args, &["curl"])
            && (detect_in_command(args, &[" -v", " --verbose", " -vv"])
                || detect_in_command(args, &[" -i", " --include"]))
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Keep status line + relevant headers + body errors; drop SSL handshake noise.
        let keep = keep_lines_matching(
            raw,
            &[
                "HTTP/",
                "Content-Type:",
                "Content-Length:",
                "Location:",
                "< HTTP",
                "> POST",
                "> GET",
                "> PUT",
                "> DELETE",
                "Error",
            ],
        );
        Cow::Owned(truncate_lines(&keep, 50))
    }
}

struct RsyncProfile;
impl Profile for RsyncProfile {
    fn name(&self) -> &'static str {
        "rsync"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["rsync"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Keep stats summary + errors; drop per-file transfer log noise.
        let keep = keep_lines_matching(
            raw,
            &[
                "sent ",
                "received ",
                "total size",
                "speedup is",
                "rsync error",
                "rsync warning",
                "deleting",
                "skipping",
            ],
        );
        if keep.is_empty() {
            Cow::Owned(truncate_lines(raw, 30))
        } else {
            Cow::Owned(keep)
        }
    }
}

struct TarProfile;
impl Profile for TarProfile {
    fn name(&self) -> &'static str {
        "tar"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["tar -", "tar c", "tar x"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        let keep = keep_lines_matching(raw, &["tar: ", "Error", "Cannot ", "Exiting", "Removing"]);
        if keep.is_empty() {
            Cow::Owned(truncate_lines(raw, 20))
        } else {
            Cow::Owned(keep)
        }
    }
}

struct FfmpegProfile;
impl Profile for FfmpegProfile {
    fn name(&self) -> &'static str {
        "ffmpeg"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["ffmpeg"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Keep encoding summary + errors; drop frame-by-frame progress.
        let keep = keep_lines_matching(
            raw,
            &[
                "Input #",
                "Output #",
                "Stream #",
                "Error",
                "Conversion failed",
                "video:",
                "audio:",
                "muxing overhead",
            ],
        );
        Cow::Owned(truncate_lines(&keep, 40))
    }
}

struct RipgrepProfile;
impl Profile for RipgrepProfile {
    fn name(&self) -> &'static str {
        "ripgrep"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["rg ", "ripgrep "])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // ripgrep output is already concise (file:line:match). Truncate hard
        // when output exceeds 100 hits.
        Cow::Owned(truncate_lines(raw, 100))
    }
}

struct SystemctlStatusProfile;
impl Profile for SystemctlStatusProfile {
    fn name(&self) -> &'static str {
        "systemctl_status"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["systemctl status"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Keep service summary + last log lines; strip cgroup/memory tree.
        let keep = keep_lines_matching(
            raw,
            &[
                "●",
                "Active:",
                "Loaded:",
                "Main PID:",
                "Memory:",
                "Tasks:",
                "Status:",
                "error",
                "Failed",
            ],
        );
        Cow::Owned(truncate_lines(&keep, 30))
    }
}

struct JournalctlProfile;
impl Profile for JournalctlProfile {
    fn name(&self) -> &'static str {
        "journalctl"
    }
    fn applies_to(&self, tool: &str, args: &Value) -> bool {
        tool == "Bash" && detect_in_command(args, &["journalctl"])
    }
    fn compress<'a>(&self, raw: &'a str) -> Cow<'a, str> {
        // Keep error/warning lines + tail; drop info/debug.
        let keep = keep_lines_matching(
            raw,
            &[
                "ERROR", "WARN", "FATAL", "PANIC", "error", "warning", "Failed", "failed", "panic",
            ],
        );
        if keep.is_empty() {
            // No errors — keep last 30 lines as tail.
            let tail: Vec<&str> = raw.lines().rev().take(30).collect();
            let mut rev = tail.clone();
            rev.reverse();
            Cow::Owned(rev.join("\n"))
        } else {
            Cow::Owned(truncate_lines(&dedupe_consecutive(&keep), 80))
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cargo_test_keeps_failures_drops_running_chatter() {
        let raw = "running 100 tests\n\
            test foo ... ok\n\
            test bar ... FAILED\n\
            failures:\n    bar\n\
            test result: FAILED. 99 passed; 1 failed\n";
        let p = CargoTestProfile;
        let out = p.compress(raw);
        assert!(out.contains("FAILED"));
        assert!(out.contains("test result:"));
        assert!(!out.contains("test foo ... ok"));
    }

    #[test]
    fn test_git_log_one_line_format() {
        let raw = "commit abc123def456\n\
            Author: foo\n\
            Date: 2026-05-08\n\
            \n    fix bug\n\n\
            commit deadbeef0000\n\
            Author: bar\n\
            Date: 2026-05-08\n\
            \n    feat: x\n";
        let p = GitLogProfile;
        let out = p.compress(raw);
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.iter().any(|l| l.contains("fix bug")));
        assert!(lines.iter().any(|l| l.contains("feat: x")));
        assert!(!out.contains("Author:"));
    }

    #[test]
    fn test_git_diff_keeps_headers_drops_bodies() {
        let raw = "diff --git a/foo.rs b/foo.rs\n\
            --- a/foo.rs\n+++ b/foo.rs\n\
            @@ -1,3 +1,5 @@\n line a\n line b\n+new line\n line c\n";
        let p = GitDiffProfile;
        let out = p.compress(raw);
        assert!(out.contains("diff --git"));
        assert!(out.contains("@@"));
        assert!(!out.contains("line a"));
    }

    #[test]
    fn test_dedupe_collapses_consecutive() {
        let raw = "X\nX\nX\nY\n";
        let out = dedupe_consecutive(raw);
        assert!(out.contains("(×3)"));
        assert!(out.contains("Y"));
    }

    #[test]
    fn test_truncate_lines_adds_marker() {
        let raw = (0..100)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = truncate_lines(&raw, 10);
        assert!(out.contains("(+90 more lines"));
    }

    #[test]
    // Cross-audit fix (2026-06-09): these 3 tests are the only ones that go through
    // `compress_for`'s `TOURING_COMPRESSION_PROFILES` env gate + the global counter;
    // serialize them so the "disabled" test's env mutation cannot leak into the
    // increment assertion under parallel execution. The other 15 tests call the
    // compression fns directly and are immune.
    #[serial_test::serial]
    fn test_compress_for_returns_borrowed_when_disabled() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_COMPRESSION_PROFILES", "0") };
        let raw = "any output";
        let out = compress_for("Bash", &json!({"command": "cargo test"}), raw);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(&*out, raw);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_COMPRESSION_PROFILES") };
    }

    #[test]
    // Cross-audit fix (2026-06-09): these 3 tests are the only ones that go through
    // `compress_for`'s `TOURING_COMPRESSION_PROFILES` env gate + the global counter;
    // serialize them so the "disabled" test's env mutation cannot leak into the
    // increment assertion under parallel execution. The other 15 tests call the
    // compression fns directly and are immune.
    #[serial_test::serial]
    fn test_compress_for_unknown_tool_falls_through() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_COMPRESSION_PROFILES") };
        let raw = "some output";
        let out = compress_for(
            "UnknownTool",
            &json!({"command": "rare_internal_cli --foo"}),
            raw,
        );
        assert_eq!(&*out, raw, "no matching profile → passthrough");
    }

    #[test]
    // Cross-audit fix (2026-06-09): these 3 tests are the only ones that go through
    // `compress_for`'s `TOURING_COMPRESSION_PROFILES` env gate + the global counter;
    // serialize them so the "disabled" test's env mutation cannot leak into the
    // increment assertion under parallel execution. The other 15 tests call the
    // compression fns directly and are immune.
    #[serial_test::serial]
    fn test_compress_for_increments_counter_on_match() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_COMPRESSION_PROFILES") };
        let before = crate::shared::gate_metrics::global()
            .compression_profile_applied_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let _ = compress_for(
            "Bash",
            &json!({"command": "cargo test"}),
            "test result: ok\n",
        );
        let after = crate::shared::gate_metrics::global()
            .compression_profile_applied_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(after > before);
    }

    #[test]
    fn test_registry_has_30_profiles() {
        assert_eq!(registry().len(), 30);
    }

    #[test]
    fn test_eslint_keeps_errors() {
        let raw = "/foo.js:10:5  error  Missing semicolon\n3 problems (3 errors)\n";
        let p = EslintProfile;
        let out = p.compress(raw);
        assert!(out.contains("error"));
    }

    #[test]
    fn test_jest_keeps_pass_fail() {
        let raw = "PASS  src/foo.test.js\nFAIL  src/bar.test.js\n  ✕ test should work\nTests: 1 failed, 1 passed\n";
        let p = JestProfile;
        let out = p.compress(raw);
        assert!(out.contains("FAIL"));
        assert!(out.contains("PASS"));
    }

    #[test]
    fn test_terraform_plan_keeps_changes() {
        let raw = "Plan: 3 to add, 1 to change, 0 to destroy.\n  + resource \"aws_s3_bucket\" \"x\"\n  - resource \"aws_iam_user\" \"y\"\n";
        let p = TerraformPlanProfile;
        let out = p.compress(raw);
        assert!(out.contains("Plan:"));
        assert!(out.contains("+ "));
    }

    #[test]
    fn test_ansible_keeps_play_recap() {
        let raw = "PLAY [webservers] *\nTASK [install nginx] *\nok: [server1]\nfailed: [server2]\nPLAY RECAP *\n";
        let p = AnsibleProfile;
        let out = p.compress(raw);
        assert!(out.contains("PLAY RECAP"));
        assert!(out.contains("failed:"));
    }

    #[test]
    fn test_jq_passthrough_when_small() {
        let raw = r#"{"a": 1, "b": 2}"#;
        let p = JqProfile;
        let out = p.compress(raw);
        assert_eq!(&*out, raw);
    }

    #[test]
    fn test_journalctl_keeps_errors_or_tails() {
        let raw = "info: starting up\nERROR: connection refused\nFATAL: out of memory\n";
        let p = JournalctlProfile;
        let out = p.compress(raw);
        assert!(out.contains("ERROR") || out.contains("FATAL"));
    }

    #[test]
    fn test_systemctl_status_keeps_summary() {
        let raw = "● foo.service - Foo\nLoaded: loaded\nActive: active (running)\nMain PID: 1234\nMemory: 12M\nTasks: 5\n";
        let p = SystemctlStatusProfile;
        let out = p.compress(raw);
        assert!(out.contains("Active:"));
        assert!(out.contains("Memory:"));
    }

    #[test]
    fn test_make_excludes_cargo() {
        let p = MakeProfile;
        let cargo_args = json!({"command": "cargo build --release"});
        assert!(!p.applies_to("Bash", &cargo_args));
        let make_args = json!({"command": "make all"});
        assert!(p.applies_to("Bash", &make_args));
    }

    #[test]
    fn test_pytest_keeps_failures_and_summary() {
        let raw = "test_foo PASSED\ntest_bar FAILED\nassertion error\n=== 1 failed, 1 passed ===\n";
        let p = PytestProfile;
        let out = p.compress(raw);
        assert!(out.contains("FAILED"));
        assert!(out.contains("passed"));
    }
}
