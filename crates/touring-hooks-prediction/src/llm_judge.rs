//! LLM-as-a-Judge for PostToolUseFailure — severity classification + repair recommendations.
//!
//! Uses pattern-based classification (simulates LLM judgment) to classify failure severity
//! and provide actionable repair recommendations. Integrates with circuit_breaker for
//! smarter HALT decisions.
//!
//! Classification dimensions:
//! - **Severity**: Critical > High > Medium > Low > Negligible
//! - **Type**: SyntaxError, TypeError, IOError, PermissionError, TimeoutError, Unknown
//! - **Recoverable**: Whether the failure can be auto-remediated
//!
//! Target latency: <5ms (synchronous classification, no external calls).

use serde::{Deserialize, Serialize};

/// Failure severity levels — ordered from most to least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Catastrophic failure — session cannot continue safely.
    /// Triggers immediate HALT via circuit breaker.
    Critical = 0,
    /// Serious failure — file or project state may be corrupted.
    /// HALT recommended after 2+ occurrences.
    High = 1,
    /// Moderate failure — recoverable but indicates a problem.
    /// Allow but warn; HALT after 5+ occurrences.
    Medium = 2,
    /// Minor failure — usually user error or transient.
    /// Allow with context injection.
    Low = 3,
    /// Negligible — no action needed.
    Negligible = 4,
}

impl Severity {
    /// Returns whether this severity should trigger a HALT recommendation.
    pub fn halt_recommended(self) -> bool {
        matches!(self, Severity::Critical | Severity::High)
    }

    /// Human-readable label for this severity.
    pub fn label(self) -> &'static str {
        match self {
            Severity::Critical => "CRITICAL",
            Severity::High => "HIGH",
            Severity::Medium => "MEDIUM",
            Severity::Low => "LOW",
            Severity::Negligible => "NEGLIGIBLE",
        }
    }
}

/// Failure type classification — what kind of error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureType {
    /// Syntax or parse error in source code.
    SyntaxError,
    /// Type mismatch or undefined symbol.
    TypeError,
    /// File not found, read/write failed.
    IOError,
    /// Permission denied or access forbidden.
    PermissionError,
    /// Operation timed out.
    TimeoutError,
    /// Resource exhausted (memory, disk, etc.).
    ResourceExhausted,
    /// Invalid input or arguments.
    InvalidInput,
    /// External service unavailable.
    ExternalServiceError,
    /// Hook protocol error.
    HookProtocolError,
    /// Concurrency/threading error.
    ConcurrencyError,
    /// Unknown or unclassified failure.
    Unknown,
}

impl FailureType {
    /// Classify failure type from error text using pattern matching.
    fn from_error_text(error_text: &str) -> Self {
        let text_lower = error_text.to_lowercase();

        // Permission errors
        if text_lower.contains("permission denied")
            || text_lower.contains("access denied")
            || text_lower.contains("eacces")
            || text_lower.contains("eperm")
        {
            return FailureType::PermissionError;
        }

        // Timeout errors
        if text_lower.contains("timed out")
            || text_lower.contains("timeout")
            || text_lower.contains("etimedout")
            || text_lower.contains("deadline exceeded")
        {
            return FailureType::TimeoutError;
        }

        // Resource exhaustion
        if text_lower.contains("out of memory")
            || text_lower.contains("oom")
            || text_lower.contains("enoent")
            || text_lower.contains("disk full")
            || text_lower.contains("enospc")
            || text_lower.contains("resource temporarily unavailable")
        {
            return FailureType::ResourceExhausted;
        }

        // IO errors
        if text_lower.contains("no such file")
            || text_lower.contains("not found")
            || text_lower.contains("enoent")
            || text_lower.contains("eexist")
            || text_lower.contains("read-only")
            || text_lower.contains("i/o error")
            || text_lower.contains("io error")
        {
            return FailureType::IOError;
        }

        // Syntax errors
        if text_lower.contains("syntax error")
            || text_lower.contains("parse error")
            || text_lower.contains("unexpected token")
            || text_lower.contains("expected token")
            || text_lower.contains("invalid syntax")
        {
            return FailureType::SyntaxError;
        }

        // Type errors
        if text_lower.contains("type error")
            || text_lower.contains("undefined")
            || text_lower.contains("cannot find")
            || text_lower.contains("has no method")
            || text_lower.contains("has no attribute")
            || text_lower.contains("not a function")
            || text_lower.contains("is not defined")
        {
            return FailureType::TypeError;
        }

        // External service errors
        if text_lower.contains("connection refused")
            || text_lower.contains("connection reset")
            || text_lower.contains("network error")
            || text_lower.contains("ECONNREFUSED")
            || text_lower.contains("etunnel")
            || text_lower.contains("service unavailable")
        {
            return FailureType::ExternalServiceError;
        }

        // Concurrency errors
        if text_lower.contains("deadlock")
            || text_lower.contains("race condition")
            || text_lower.contains("mutex")
            || text_lower.contains("concurrent modification")
        {
            return FailureType::ConcurrencyError;
        }

        // Hook protocol errors
        if text_lower.contains("hook response")
            || text_lower.contains("invalid hook")
            || text_lower.contains("emit")
            || text_lower.contains("exit code")
        {
            return FailureType::HookProtocolError;
        }

        // Invalid input
        if text_lower.contains("invalid argument")
            || text_lower.contains("illegal argument")
            || text_lower.contains("bad argument")
            || text_lower.contains("empty input")
        {
            return FailureType::InvalidInput;
        }

        FailureType::Unknown
    }
}

/// Judgment report from the LLM-as-a-Judge.
///
/// Contains the full assessment of a tool failure including:
/// - Severity classification
/// - Failure type
/// - Repair recommendation
/// - Whether HALT is recommended
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeReport {
    /// How severe is this failure.
    pub severity: Severity,
    /// What kind of error occurred.
    pub failure_type: FailureType,
    /// Actionable repair recommendation for the user.
    pub repair_recommendation: String,
    /// Whether this failure alone justifies a HALT.
    pub halt_recommended: bool,
    /// Confidence in the classification (0.0 to 1.0).
    pub confidence: f32,
    /// Reasoning behind the classification.
    pub reasoning: String,
}

impl JudgeReport {
    /// Classify a tool failure and produce a judgment report.
    ///
    /// This is the main entry point for LLM-as-a-Judge evaluation.
    /// Uses pattern-based classification (simulates LLM judgment without external calls).
    ///
    /// # Arguments
    /// * `tool_name` - Name of the tool that failed (Edit, Write, Bash, Read)
    /// * `error_text` - The error message or output
    /// * `file_path` - File path involved in the failure (may be empty)
    ///
    /// # Returns
    /// A `JudgeReport` with severity, type, and repair recommendation.
    ///
    /// # Latency
    /// Target: <5ms (synchronous, no external calls).
    pub fn judge(tool_name: &str, error_text: &str, file_path: &str) -> Self {
        if error_text.is_empty() {
            return JudgeReport {
                severity: Severity::Negligible,
                failure_type: FailureType::Unknown,
                repair_recommendation: "No error text provided — allow operation.".to_string(),
                halt_recommended: false,
                confidence: 1.0,
                reasoning: "Empty error text treated as negligible.".to_string(),
            };
        }

        let failure_type = FailureType::from_error_text(error_text);
        let severity = Self::assess_severity(tool_name, &failure_type, error_text, file_path);
        let (repair_recommendation, confidence, reasoning) =
            Self::generate_recommendation(tool_name, &failure_type, error_text, file_path);

        Self {
            severity,
            failure_type,
            repair_recommendation,
            halt_recommended: severity.halt_recommended(),
            confidence,
            reasoning,
        }
    }

    /// Assess the severity of a failure.
    fn assess_severity(
        tool_name: &str,
        failure_type: &FailureType,
        error_text: &str,
        file_path: &str,
    ) -> Severity {
        let text_lower = error_text.to_lowercase();
        let file_ext = file_path.rsplit('.').next().unwrap_or("");

        // Critical severity triggers
        if text_lower.contains("segmentation fault")
            || text_lower.contains("sigsegv")
            || text_lower.contains("heap corruption")
            || text_lower.contains("stack overflow")
        {
            return Severity::Critical;
        }

        // Permission errors on system files are critical
        if matches!(failure_type, FailureType::PermissionError)
            && (file_path.contains("/etc/")
                || file_path.contains("/usr/")
                || file_path.contains("/var/")
                || file_path.contains("/root/"))
        {
            return Severity::Critical;
        }

        // Resource exhaustion in critical paths
        if matches!(failure_type, FailureType::ResourceExhausted)
            && (text_lower.contains("heap")
                || text_lower.contains("memory")
                || text_lower.contains("oom"))
        {
            return Severity::High;
        }

        // Write/Edit failures on important files
        if matches!(tool_name, "Edit" | "Write")
            && !file_path.is_empty()
            && matches!(
                failure_type,
                FailureType::IOError | FailureType::PermissionError
            )
        {
            return Severity::High;
        }

        // Bash failures on system commands
        if matches!(tool_name, "Bash") {
            if text_lower.contains("sudo")
                || text_lower.contains("chmod")
                || text_lower.contains("chown")
                || text_lower.contains("rm -rf")
                || text_lower.contains("mkfs")
            {
                return Severity::High;
            }

            // git commands are high risk
            if text_lower.contains("git") && text_lower.contains("error") {
                return Severity::Medium;
            }
        }

        // Syntax errors in code files are medium-high
        if matches!(failure_type, FailureType::SyntaxError) {
            match file_ext {
                "rs" | "py" | "ts" | "js" | "go" | "java" => return Severity::Medium,
                _ => return Severity::Low,
            }
        }

        // Type errors in strongly-typed languages
        if matches!(failure_type, FailureType::TypeError) {
            match file_ext {
                "rs" | "ts" | "go" | "java" => return Severity::Medium,
                "py" | "js" => return Severity::Low,
                _ => return Severity::Low,
            }
        }

        // Default severity based on failure type
        match failure_type {
            FailureType::PermissionError => Severity::Medium,
            FailureType::IOError => Severity::Low,
            FailureType::TimeoutError => Severity::Medium,
            FailureType::ResourceExhausted => Severity::Medium,
            FailureType::InvalidInput => Severity::Low,
            FailureType::ExternalServiceError => Severity::Medium,
            FailureType::HookProtocolError => Severity::High,
            FailureType::ConcurrencyError => Severity::High,
            FailureType::SyntaxError => Severity::Medium,
            FailureType::TypeError => Severity::Low,
            FailureType::Unknown => Severity::Low,
        }
    }

    /// Generate repair recommendation based on failure analysis.
    fn generate_recommendation(
        _tool_name: &str,
        failure_type: &FailureType,
        error_text: &str,
        file_path: &str,
    ) -> (String, f32, String) {
        let text_lower = error_text.to_lowercase();

        match failure_type {
            FailureType::SyntaxError => {
                let file_ext = file_path.rsplit('.').next().unwrap_or("");
                let lang = match file_ext {
                    "rs" => "Rust",
                    "py" => "Python",
                    "ts" => "TypeScript",
                    "js" => "JavaScript",
                    "go" => "Go",
                    "java" => "Java",
                    _ => "code",
                };
                (
                    format!(
                        "Check {} syntax near the error location. \
                        Review the {} language reference for the correct syntax.",
                        lang, lang
                    ),
                    0.85,
                    format!("Syntax error detected in {} file.", file_ext),
                )
            }

            FailureType::TypeError => {
                (
                    "Verify that all variables and functions are defined before use. \
                    Check for typos in symbol names and ensure proper module imports.".to_string(),
                    0.80,
                    "Type/error detected — likely undefined symbol or wrong type.".to_string(),
                )
            }

            FailureType::IOError => {
                if text_lower.contains("no such file") || text_lower.contains("not found") {
                    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
                    (
                        format!(
                            "File '{}' does not exist or cannot be accessed. \
                            Verify the path is correct and the file exists before retrying.",
                            file_name
                        ),
                        0.90,
                        "File not found error — IOError variant.".to_string(),
                    )
                } else {
                    (
                        "File read/write operation failed. Check file permissions and disk space. \
                        Ensure the file system is not read-only.".to_string(),
                        0.75,
                        "IO error during file operation.".to_string(),
                    )
                }
            }

            FailureType::PermissionError => {
                let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
                (
                    format!(
                        "Permission denied for '{}'. Check file/directory permissions with ls -la. \
                        You may need to run chmod or run as a user with appropriate permissions.",
                        file_name
                    ),
                    0.95,
                    "Permission error — access denied to resource.".to_string(),
                )
            }

            FailureType::TimeoutError => {
                (
                    "Operation timed out. This may be a transient network issue or the \
                    operation may need more time. Consider retrying or checking network connectivity."
                        .to_string(),
                    0.80,
                    "Timeout error — operation exceeded time limit.".to_string(),
                )
            }

            FailureType::ResourceExhausted => {
                if text_lower.contains("memory") || text_lower.contains("oom") {
                    (
                        "System is low on memory (OOM). Try closing other applications, \
                        increasing swap space, or processing data in smaller batches.".to_string(),
                        0.90,
                        "Memory exhaustion detected.".to_string(),
                    )
                } else {
                    (
                        "System resource exhausted (disk space, file handles, etc.). \
                        Check available disk space with `df -h` and free resources.".to_string(),
                        0.85,
                        "Resource exhaustion — disk/memory/handles.".to_string(),
                    )
                }
            }

            FailureType::InvalidInput => {
                (
                    "Invalid input provided to the tool. Review the command arguments and \
                    ensure they match the expected format.".to_string(),
                    0.85,
                    "Invalid input — tool arguments malformed.".to_string(),
                )
            }

            FailureType::ExternalServiceError => {
                (
                    "External service is unavailable or unreachable. Check network connectivity \
                    and the service status. Retry when the service becomes available.".to_string(),
                    0.75,
                    "External service error — network or service issue.".to_string(),
                )
            }

            FailureType::HookProtocolError => {
                (
                    "Hook protocol error detected. This may be a Touring daemon issue. \
                    Try restarting the daemon with `touring serve` or check daemon logs.".to_string(),
                    0.80,
                    "Hook protocol violation or daemon error.".to_string(),
                )
            }

            FailureType::ConcurrencyError => {
                (
                    "Concurrency error detected (race condition or deadlock). \
                    This may require restructuring the code to avoid parallel access to shared resources. \
                    Consider adding proper synchronization.".to_string(),
                    0.70,
                    "Concurrency error — threading/sync issue.".to_string(),
                )
            }

            FailureType::Unknown => {
                let truncated = if error_text.len() > 100 {
                    format!("{}...", &error_text[..100])
                } else {
                    error_text.to_string()
                };
                (
                    format!(
                        "Unknown error occurred: {}. \
                        Review the error message and try to identify the cause. \
                        Check Touring logs for more details.",
                        truncated
                    ),
                    0.50,
                    format!("Unclassified error: {}", truncated),
                )
            }
        }
    }

    /// Returns a summary string for logging/debugging.
    pub fn summary(&self) -> String {
        format!(
            "[{}] {} — {} (confidence: {:.0}%)",
            self.severity.label(),
            self.failure_type.debug_name(),
            self.repair_recommendation
                .chars()
                .take(80)
                .collect::<String>(),
            self.confidence * 100.0
        )
    }
}

/// Extension trait to add debug_name to FailureType for logging.
trait FailureTypeExt {
    fn debug_name(&self) -> &'static str;
}

impl FailureTypeExt for FailureType {
    fn debug_name(&self) -> &'static str {
        match self {
            FailureType::SyntaxError => "SyntaxError",
            FailureType::TypeError => "TypeError",
            FailureType::IOError => "IOError",
            FailureType::PermissionError => "PermissionError",
            FailureType::TimeoutError => "TimeoutError",
            FailureType::ResourceExhausted => "ResourceExhausted",
            FailureType::InvalidInput => "InvalidInput",
            FailureType::ExternalServiceError => "ExternalServiceError",
            FailureType::HookProtocolError => "HookProtocolError",
            FailureType::ConcurrencyError => "ConcurrencyError",
            FailureType::Unknown => "Unknown",
        }
    }
}

/// Judge a tool failure and return the report.
///
/// This is a convenience wrapper around `JudgeReport::judge`.
pub fn judge_failure(tool_name: &str, error_text: &str, file_path: &str) -> JudgeReport {
    JudgeReport::judge(tool_name, error_text, file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical < Severity::High);
        assert!(Severity::High < Severity::Medium);
        assert!(Severity::Medium < Severity::Low);
        assert!(Severity::Low < Severity::Negligible);
    }

    #[test]
    fn test_severity_halt_recommended() {
        assert!(Severity::Critical.halt_recommended());
        assert!(Severity::High.halt_recommended());
        assert!(!Severity::Medium.halt_recommended());
        assert!(!Severity::Low.halt_recommended());
        assert!(!Severity::Negligible.halt_recommended());
    }

    #[test]
    fn test_judge_permission_error() {
        let report = judge_failure(
            "Write",
            "Permission denied: /etc/myapp.conf",
            "/etc/myapp.conf",
        );
        assert_eq!(report.severity, Severity::Critical);
        assert_eq!(report.failure_type, FailureType::PermissionError);
        assert!(report.halt_recommended);
        assert!(report.confidence >= 0.9);
    }

    #[test]
    fn test_judge_file_not_found() {
        let report = judge_failure(
            "Edit",
            "Error: file not found: /path/to/missing.rs",
            "/path/to/missing.rs",
        );
        assert_eq!(report.severity, Severity::High);
        assert_eq!(report.failure_type, FailureType::IOError);
        assert!(report.halt_recommended);
    }

    #[test]
    fn test_judge_syntax_error() {
        let report = judge_failure(
            "Write",
            "Syntax error: unexpected token in file.rs:42",
            "file.rs",
        );
        assert_eq!(report.severity, Severity::Medium);
        assert_eq!(report.failure_type, FailureType::SyntaxError);
        assert!(!report.halt_recommended);
    }

    #[test]
    fn test_judge_empty_error() {
        let report = judge_failure("Edit", "", "file.rs");
        assert_eq!(report.severity, Severity::Negligible);
        assert!(!report.halt_recommended);
        assert_eq!(report.confidence, 1.0);
    }

    #[test]
    fn test_judge_timeout() {
        let report = judge_failure("Bash", "Error: command timed out after 30000ms", "");
        assert_eq!(report.failure_type, FailureType::TimeoutError);
        assert_eq!(report.severity, Severity::Medium);
    }

    #[test]
    fn test_judge_oom() {
        let report = judge_failure("Bash", "Error: out of memory (OOM)", "");
        assert_eq!(report.failure_type, FailureType::ResourceExhausted);
        assert_eq!(report.severity, Severity::High);
    }

    #[test]
    fn test_judge_dangerous_bash_command() {
        let report = judge_failure(
            "Bash",
            "Error: sudo command failed: authentication required",
            "",
        );
        assert_eq!(report.severity, Severity::High); // sudo auth failure = High severity
        assert!(report.halt_recommended); // High severity always recommends halt
    }

    #[test]
    fn test_judge_git_error() {
        let report = judge_failure("Bash", "Error: git commit failed: nothing to commit", "");
        assert_eq!(report.severity, Severity::Medium);
        assert_eq!(report.failure_type, FailureType::Unknown); // git error not matching specific patterns
    }

    #[test]
    fn test_report_summary() {
        let report = judge_failure(
            "Write",
            "Permission denied: /etc/myapp.conf",
            "/etc/myapp.conf",
        );
        let summary = report.summary();
        assert!(summary.contains("CRITICAL"));
        assert!(summary.contains("PermissionError"));
    }

    #[test]
    fn test_type_error_classification() {
        let report = judge_failure(
            "Edit",
            "TypeError: Cannot read property 'foo' of undefined",
            "file.js",
        );
        assert_eq!(report.failure_type, FailureType::TypeError);
    }

    #[test]
    fn test_undefined_symbol_classification() {
        let report = judge_failure("Write", "Error: 'MyClass' is not defined", "file.py");
        assert_eq!(report.failure_type, FailureType::TypeError);
    }

    #[test]
    fn test_connection_error_classification() {
        let report = judge_failure("Bash", "Error: Connection refused to localhost:8080", "");
        assert_eq!(report.failure_type, FailureType::ExternalServiceError);
    }

    #[test]
    fn test_invalid_input_classification() {
        let report = judge_failure("Edit", "Error: invalid argument: path cannot be empty", "");
        assert_eq!(report.failure_type, FailureType::InvalidInput);
        assert_eq!(report.severity, Severity::Low);
    }
}
