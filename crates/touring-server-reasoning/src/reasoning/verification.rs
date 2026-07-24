//! Verification Gates for task execution quality assurance.
//!
//! Provides [`VerifyGate`] trait and implementations:
//! - [`TestGate`]: Runs `cargo test` for a package or workspace
//! - [`ClippyGate`]: Runs `cargo clippy` with deny-all policy
//!
//! Gates are executed by the orchestrator after task completion and return
//! a [`GateResult`] with pass/fail status and diagnostic output.

use std::path::Path;
use std::process::{Command, Output};
use std::time::Instant;

/// Result of a verification gate execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GateResult {
    /// Whether the gate passed.
    pub passed: bool,
    /// Gate name (e.g., "TestGate", "ClippyGate").
    pub gate: String,
    /// Human-readable summary.
    pub summary: String,
    /// Exit code from the underlying command.
    pub exit_code: i32,
    /// Duration of the gate execution.
    pub duration_ms: u64,
    /// Standard output (truncated to 4096 chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// Standard error (truncated to 4096 chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    /// Lines with errors/warnings (for display).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_lines: Option<Vec<String>>,
}

impl GateResult {
    /// Create a passing result.
    fn pass(
        gate: &str,
        summary: String,
        exit_code: i32,
        duration_ms: u64,
        output: &Output,
    ) -> Self {
        Self {
            passed: true,
            gate: gate.to_string(),
            summary,
            exit_code,
            duration_ms,
            stdout: Self::truncate_output(&output.stdout),
            stderr: Self::truncate_output(&output.stderr),
            error_lines: None,
        }
    }

    /// Create a failing result.
    fn fail(
        gate: &str,
        summary: String,
        exit_code: i32,
        duration_ms: u64,
        output: &Output,
    ) -> Self {
        let error_lines = Self::extract_error_lines(&output.stderr);
        Self {
            passed: false,
            gate: gate.to_string(),
            summary,
            exit_code,
            duration_ms,
            stdout: Self::truncate_output(&output.stdout),
            stderr: Self::truncate_output(&output.stderr),
            error_lines: Some(error_lines),
        }
    }

    fn truncate_output(output: &[u8]) -> Option<String> {
        let s = String::from_utf8_lossy(output).to_string();
        if s.len() > 4096 {
            Some(format!(
                "{}... [truncated {} chars]",
                &s[..4096],
                s.len() - 4096
            ))
        } else if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    fn extract_error_lines(stderr: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(stderr)
            .lines()
            .filter(|line| {
                line.contains("error")
                    || line.contains("warning:")
                    || line.contains("FAILED")
                    || line.contains("test result")
            })
            .take(20)
            .map(|s| s.to_string())
            .collect()
    }
}

/// Verification gate trait.
/// Gates run after task execution to verify quality criteria.
pub trait VerifyGate: Send + Sync {
    /// Run the verification gate.
    fn verify(&self, manifest_path: &Path) -> GateResult;

    /// Short name for the gate.
    fn name(&self) -> &'static str;
}

// ── Test Gate ────────────────────────────────────────────────────────────────

/// Runs `cargo test` for a package or workspace.
pub struct TestGate {
    /// Extra arguments to pass to `cargo test`.
    pub extra_args: Vec<String>,
    /// Package to test (None = whole workspace).
    pub package: Option<String>,
}

impl TestGate {
    /// Create a `TestGate` with no extra args, targeting the whole workspace.
    pub fn new() -> Self {
        Self {
            extra_args: Vec::new(),
            package: None,
        }
    }

    /// Set a specific package to test.
    pub fn package(mut self, pkg: &str) -> Self {
        self.package = Some(pkg.to_string());
        self
    }

    /// Add extra arguments to `cargo test`.
    pub fn extra_arg(mut self, arg: &str) -> Self {
        self.extra_args.push(arg.to_string());
        self
    }
}

impl Default for TestGate {
    fn default() -> Self {
        Self::new()
    }
}

impl VerifyGate for TestGate {
    fn name(&self) -> &'static str {
        "TestGate"
    }

    fn verify(&self, manifest_path: &Path) -> GateResult {
        let start = Instant::now();
        let mut cmd = Command::new("cargo");
        cmd.arg("test").arg("--message-format=json");

        if let Some(ref pkg) = self.package {
            cmd.arg("--package").arg(pkg);
        }

        for arg in &self.extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(manifest_path.parent().unwrap_or(manifest_path));

        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                return GateResult {
                    passed: false,
                    gate: "TestGate".to_string(),
                    summary: format!("Failed to run cargo test: {}", e),
                    exit_code: -1,
                    duration_ms,
                    stdout: None,
                    stderr: None,
                    error_lines: None,
                };
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let exit_code = output.status.code().unwrap_or(-1);

        // Parse JSON output lines for test results
        let test_summary = Self::parse_test_output(&output.stdout, exit_code);

        if output.status.success() {
            GateResult::pass("TestGate", test_summary, exit_code, duration_ms, &output)
        } else {
            GateResult::fail("TestGate", test_summary, exit_code, duration_ms, &output)
        }
    }
}

impl TestGate {
    fn parse_test_output(stdout: &[u8], exit_code: i32) -> String {
        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut ignored = 0usize;

        for line in String::from_utf8_lossy(stdout).lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json.get("reason").and_then(|r| r.as_str()) == Some("test") {
                    if let Some(Some(true)) = json.get("success").map(|v| v.as_bool()) {
                        passed += 1;
                    } else if let Some(Some(false)) = json.get("success").map(|v| v.as_bool()) {
                        failed += 1;
                    }
                }
                if let Some("ignored") = json.get("reason").and_then(|r| r.as_str()) {
                    ignored += 1;
                }
            }
        }

        if passed > 0 || failed > 0 || ignored > 0 {
            format!("{} passed; {} failed; {} ignored", passed, failed, ignored)
        } else if exit_code == 0 {
            "All tests passed".to_string()
        } else {
            "Tests failed".to_string()
        }
    }
}

// ── Clippy Gate ─────────────────────────────────────────────────────────────

/// Runs `cargo clippy` with deny-all policy.
pub struct ClippyGate {
    /// Extra arguments to pass to `cargo clippy`.
    pub extra_args: Vec<String>,
    /// Package to clippy (None = whole workspace).
    pub package: Option<String>,
}

impl ClippyGate {
    /// Create a `ClippyGate` with no extra args, targeting the whole workspace.
    pub fn new() -> Self {
        Self {
            extra_args: Vec::new(),
            package: None,
        }
    }

    /// Set a specific package to clippy.
    pub fn package(mut self, pkg: &str) -> Self {
        self.package = Some(pkg.to_string());
        self
    }

    /// Add extra arguments to `cargo clippy`.
    pub fn extra_arg(mut self, arg: &str) -> Self {
        self.extra_args.push(arg.to_string());
        self
    }
}

impl Default for ClippyGate {
    fn default() -> Self {
        Self::new()
    }
}

impl VerifyGate for ClippyGate {
    fn name(&self) -> &'static str {
        "ClippyGate"
    }

    fn verify(&self, manifest_path: &Path) -> GateResult {
        let start = Instant::now();
        let mut cmd = Command::new("cargo");
        cmd.arg("clippy").arg("--").arg("--deny").arg("warnings");

        if let Some(ref pkg) = self.package {
            cmd.arg("--package").arg(pkg);
        }

        for arg in &self.extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(manifest_path.parent().unwrap_or(manifest_path));

        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                return GateResult {
                    passed: false,
                    gate: "ClippyGate".to_string(),
                    summary: format!("Failed to run cargo clippy: {}", e),
                    exit_code: -1,
                    duration_ms,
                    stdout: None,
                    stderr: None,
                    error_lines: None,
                };
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let exit_code = output.status.code().unwrap_or(-1);

        let summary = if output.status.success() {
            "Clippy: no warnings or errors".to_string()
        } else {
            Self::count_warnings(&output.stderr)
        };

        if output.status.success() {
            GateResult::pass("ClippyGate", summary, exit_code, duration_ms, &output)
        } else {
            GateResult::fail("ClippyGate", summary, exit_code, duration_ms, &output)
        }
    }
}

impl ClippyGate {
    fn count_warnings(stderr: &[u8]) -> String {
        let stderr_str = String::from_utf8_lossy(stderr);
        let error_count = stderr_str.matches("error:").count();
        let warning_count = stderr_str.matches("warning:").count();

        if error_count > 0 {
            format!("{} error(s), {} warning(s)", error_count, warning_count)
        } else if warning_count > 0 {
            format!("{} warning(s)", warning_count)
        } else {
            "Clippy check failed".to_string()
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_result_pass() {
        let output = Command::new("echo").arg("test output").output().unwrap();
        let result = GateResult::pass("TestGate", "All passed".to_string(), 0, 100, &output);
        assert!(result.passed);
        assert_eq!(result.gate, "TestGate");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_gate_result_fail() {
        let output = Command::new("echo")
            .arg("error: something failed")
            .output()
            .unwrap();
        let result = GateResult::fail("TestGate", "Tests failed".to_string(), 1, 100, &output);
        assert!(!result.passed);
        assert!(result.error_lines.is_some());
    }

    #[test]
    fn test_test_gate_new() {
        let gate = TestGate::new();
        assert_eq!(gate.name(), "TestGate");
        assert!(gate.package.is_none());
    }

    #[test]
    fn test_test_gate_builder() {
        let gate = TestGate::new().package("my-crate").extra_arg("--lib");
        assert_eq!(gate.package.as_deref(), Some("my-crate"));
        assert_eq!(gate.extra_args, vec!["--lib"]);
    }

    #[test]
    fn test_clippy_gate_new() {
        let gate = ClippyGate::new();
        assert_eq!(gate.name(), "ClippyGate");
    }

    #[test]
    fn test_truncate_output() {
        let long = "x".repeat(5000);
        let result = GateResult::truncate_output(long.as_bytes()).unwrap();
        assert!(result.contains("... [truncated"));
        assert!(result.len() < 5000);
    }
}
