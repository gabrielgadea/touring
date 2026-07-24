//! CLI bridge — spawns `touring` CLI commands and parses JSON output.

use std::process::Stdio;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Errors that can occur when invoking the touring CLI.
#[derive(Debug, Error, Clone)]
pub enum CliError {
    /// Failed to spawn the touring process.
    #[error("touring CLI spawn failed: {0}")]
    Spawn(String),

    /// Touring process exited with a non-zero exit code.
    #[error("touring CLI exited with non-zero: {exit_code}")]
    Exit {
        /// The exit code returned by the touring process.
        exit_code: i32,
    },

    /// Failed to read stdout/stderr from the touring process.
    #[error("read error: {0}")]
    Read(String),
}

/// Result type alias for CLI operations.
pub type CliResult<T> = std::result::Result<T, CliError>;

/// Run a touring CLI command and return its JSON stdout as a string.
///
/// # Errors
///
/// Returns `CliError` if the touring binary is not in PATH, the command
/// fails, or the output cannot be read.
pub async fn run_touring_command(args: &[&str]) -> CliResult<String> {
    let mut cmd = Command::new("touring");
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| CliError::Spawn(e.to_string()))?;

    let stdout = child.stdout.take()
        .ok_or_else(|| CliError::Read("stdout not piped".to_string()))?;
    let mut reader = BufReader::new(stdout).lines();
    let mut output = String::new();

    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|e| CliError::Read(e.to_string()))?
    {
        output.push_str(&line);
        output.push('\n');
    }

    let status = child.wait().await.map_err(|e| CliError::Spawn(e.to_string()))?;

    if !status.success() {
        return Err(CliError::Exit {
            exit_code: status.code().unwrap_or(-1),
        });
    }

    Ok(output)
}

/// Run `touring doctor -j` and deserialize the health report.
pub async fn fetch_health() -> CliResult<serde_json::Value> {
    let raw = run_touring_command(&["doctor", "-j"]).await?;
    serde_json::from_str(&raw).map_err(|e| CliError::Read(e.to_string()))
}

/// Run `touring status -j` and deserialize the status dashboard.
pub async fn fetch_status() -> CliResult<serde_json::Value> {
    let raw = run_touring_command(&["status", "-j"]).await?;
    serde_json::from_str(&raw).map_err(|e| CliError::Read(e.to_string()))
}

/// Run `touring wiring orphans -j` and deserialize the orphan list.
pub async fn fetch_orphans() -> CliResult<serde_json::Value> {
    let raw = run_touring_command(&["wiring", "orphans", "-j"]).await?;
    serde_json::from_str(&raw).map_err(|e| CliError::Read(e.to_string()))
}

/// Run `touring wiring modules -j` and deserialize the wiring modules report.
pub async fn fetch_wiring_modules() -> CliResult<serde_json::Value> {
    let raw = run_touring_command(&["wiring", "modules", "-j"]).await?;
    serde_json::from_str(&raw).map_err(|e| CliError::Read(e.to_string()))
}

/// Run `touring index find <query> -j` and deserialize symbol results.
pub async fn fetch_symbol_search(query: &str) -> CliResult<serde_json::Value> {
    let raw = run_touring_command(&["index", "find", query, "-j"]).await?;
    serde_json::from_str(&raw).map_err(|e| CliError::Read(e.to_string()))
}

/// Run `touring memory recall "<query>" -j` and deserialize memory entries.
pub async fn fetch_memory(query: &str) -> CliResult<serde_json::Value> {
    let raw = run_touring_command(&["memory", "recall", query, "-j"]).await?;
    serde_json::from_str(&raw).map_err(|e| CliError::Read(e.to_string()))
}

/// Run `touring viz wiring --format svg` and return the raw SVG string.
pub async fn fetch_viz_wiring_svg() -> CliResult<String> {
    let raw = run_touring_command(&["viz", "wiring", "--format", "svg"]).await?;
    Ok(raw)
}

/// Run `touring viz wiring --format json` and deserialize graph data.
pub async fn fetch_viz_wiring_json() -> CliResult<serde_json::Value> {
    let raw = run_touring_command(&["viz", "wiring", "--format", "json"]).await?;
    serde_json::from_str(&raw).map_err(|e| CliError::Read(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_touring_command_exit_zero() {
        let result = run_touring_command(&["doctor", "-j"]).await;
        assert!(result.is_ok());
        let json_str = result.unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn test_run_touring_command_invalid_command() {
        let result = run_touring_command(&["nonexistent-cmd-xyz-abcdefgh"]).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_error_display() {
        let spawn_err = CliError::Spawn("touring not found".to_string());
        assert!(spawn_err.to_string().contains("touring not found"));

        let exit_err = CliError::Exit { exit_code: 127 };
        assert!(exit_err.to_string().contains("127"));

        let read_err = CliError::Read("stream closed".to_string());
        assert!(read_err.to_string().contains("stream closed"));
    }

    #[test]
    fn test_cli_error_debug() {
        let spawn_err = CliError::Spawn("test spawn error".to_string());
        let debug_str = format!("{:?}", spawn_err);
        assert!(debug_str.contains("Spawn"));
        assert!(debug_str.contains("test spawn error"));

        let exit_err = CliError::Exit { exit_code: 42 };
        let debug_str = format!("{:?}", exit_err);
        assert!(debug_str.contains("Exit"));
    }

    #[tokio::test]
    async fn test_fetch_health_returns_valid_json() {
        let result = fetch_health().await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.is_array());
    }

    #[tokio::test]
    async fn test_fetch_status_returns_valid_json() {
        let result = fetch_status().await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.is_object());
    }
}