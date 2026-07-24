//! CLI wrappers for invoking the touring CLI tool asynchronously.
//!
//! Provides ergonomic wrappers around tokio::process::Command for spawning
//! the touring CLI and parsing its JSON output.

use serde::de::DeserializeOwned;
use tokio::process::Command;

/// Binary name of the touring CLI tool.
const TOURING_BIN: &str = "touring";

/// Run an arbitrary touring CLI command and return the stdout as a string.
///
/// # Arguments
/// * `cmd` - The touring subcommand (e.g. "doctor", "wiring", "status")
/// * `args` - Additional arguments passed to the subcommand
///
/// # Returns
/// * `Ok(String)` - stdout from the touring CLI
/// * `Err(String)` - error message describing what went wrong
pub async fn run_touring_command(cmd: &str, args: &[&str]) -> Result<String, String> {
    let mut c = Command::new(TOURING_BIN);
    c.arg(cmd);
    for arg in args {
        c.arg(arg);
    }

    let output = c
        .output()
        .await
        .map_err(|e| format!("failed to spawn touring {}: {}", cmd, e))?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|e| format!("touring {} output is not UTF-8: {}", cmd, e))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "touring {} failed with exit code {:?}: {}",
            cmd,
            output.status.code(),
            stderr
        ))
    }
}

/// Run an arbitrary touring CLI command and parse its JSON stdout into type T.
///
/// # Type Parameters
/// * `T` - A serde deserializable type that matches the JSON output
///
/// # Arguments
/// * `cmd` - The touring subcommand (e.g. "doctor", "wiring", "status")
/// * `args` - Additional arguments passed to the subcommand (include "-j" for JSON)
///
/// # Returns
/// * `Ok(T)` - parsed JSON response
/// * `Err(String)` - error message describing what went wrong
pub async fn run_touring_json<T: DeserializeOwned>(cmd: &str, args: &[&str]) -> Result<T, String> {
    let mut full_args: Vec<&str> = args.to_vec();
    if !args.iter().any(|a| a.starts_with('-') && a.contains('j')) {
        // auto-append -j if not already present
        full_args.push("-j");
    }
    let stdout = run_touring_command(cmd, &full_args).await?;
    serde_json::from_str(&stdout)
        .map_err(|e| format!("failed to parse JSON from touring {}: {}", cmd, e))
}

/// Retrieve and parse `touring doctor -j` health summary.
pub async fn health_summary() -> Result<serde_json::Value, String> {
    run_touring_json("doctor", &["-j"]).await
}

/// Retrieve and parse `touring wiring orphans -j` orphan symbol list.
pub async fn orphans_list() -> Result<serde_json::Value, String> {
    run_touring_json("wiring", &["orphans", "-j"]).await
}

/// Retrieve and parse `touring wiring status -j` as a DOT-compatible export.
///
/// The touring CLI does not expose a raw DOT export subcommand;
/// this function fetches the wiring status JSON and returns it directly.
/// Consumers can transform the JSON into DOT using a library like
/// `petgraph` or `graphviz`.
pub async fn wiring_dot_export() -> Result<serde_json::Value, String> {
    run_touring_json("wiring", &["status", "-j"]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_touring_command_version() {
        // Test that run_touring_command itself succeeds (binary may not be in test PATH)
        let result = run_touring_command("--version", &[]).await;
        // The function should not panic; result reflects runtime availability
        if let Err(err) = result {
            // Binary not in PATH during test — this is expected in some environments
            assert!(
                err.contains("failed to spawn") || err.contains("not found"),
                "unexpected error: {}",
                err
            );
        }
    }

    #[tokio::test]
    async fn test_health_summary() {
        // touring doctor -j returns an Array of component objects, not a single Object
        let result = health_summary().await;
        match result {
            Ok(v) => {
                // Verify it's a JSON array (touring doctor -j format)
                assert!(
                    v.is_array(),
                    "touring doctor -j returns an array of components, got {:?}",
                    v
                );
            }
            Err(e) => {
                // Binary not in PATH during test — expected
                assert!(
                    e.contains("failed to spawn") || e.contains("not found"),
                    "unexpected error: {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_orphans_list() {
        let result = orphans_list().await;
        // touring wiring orphans -j returns {count, orphans: [...]}
        match result {
            Ok(v) => {
                assert!(
                    v.is_object(),
                    "touring wiring orphans -j returns an object, got {:?}",
                    v
                );
            }
            Err(e) => {
                assert!(
                    e.contains("failed to spawn") || e.contains("not found"),
                    "unexpected error: {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_wiring_dot_export() {
        let result = wiring_dot_export().await;
        match result {
            Ok(v) => {
                assert!(
                    v.is_object(),
                    "touring wiring status -j returns an object, got {:?}",
                    v
                );
            }
            Err(e) => {
                assert!(
                    e.contains("failed to spawn") || e.contains("not found"),
                    "unexpected error: {}",
                    e
                );
            }
        }
    }
}
