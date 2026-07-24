//! touring_desktop_ui — Desktop UI crate for Touring with egui 0.34, tokio, serde, graphviz. Light/dark theme support via Theme enum..
//!
//! Module structure:
//!   - [`error`]: typed errors via `thiserror`.
//!   - [`types`]: public data types.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::process::Stdio;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Errors specific to command execution in the desktop UI module.
#[derive(Debug, Error)]
pub enum CommandError {
    /// I/O failure while spawning or communicating with the `touring` CLI
    /// subprocess (executable missing, permission denied, broken pipe).
    #[error("touring CLI command failed: {0}")]
    CommandFailed(#[from] std::io::Error),

    /// The `touring` CLI subprocess exited with a non-zero status code,
    /// indicating the underlying operation failed (see `exit_code` for the
    /// raw value reported by the OS).
    #[error("touring CLI exited with non-zero status: {exit_code}")]
    CommandExit {
        /// Raw exit code reported by the OS for the failed `touring`
        /// subprocess (typically 1..=127; 0 means success and never reaches
        /// this branch).
        exit_code: i32,
    },

    /// Could not decode stdout/stderr of the `touring` subprocess as UTF-8
    /// or the buffered read failed midway through the byte stream.
    #[error("touring CLI output read error: {0}")]
    OutputRead(String),
}

/// Result type alias for command execution results.
pub type CommandResult<T> = std::result::Result<T, CommandError>;

/// Theme variant for the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    /// Dark theme.
    Dark,
    /// Light theme.
    Light,
}

/// Builder for constructing app instances.
#[derive(Debug, Default)]
pub struct AppBuilder {
    theme: Option<Theme>,
}

impl AppBuilder {
    /// Sets the theme for the app being built.
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Builds the app instance.
    #[must_use]
    pub fn build(self) -> Theme {
        self.theme.unwrap_or(Theme::Dark)
    }
}

/// Spawns a touring CLI command asynchronously and streams its output.
///
/// # Errors
///
/// Returns an error if the touring CLI cannot be spawned or if it exits with a non-zero status.
pub async fn spawn_touring_command(args: &[&str]) -> CommandResult<String> {
    let mut cmd = Command::new("touring");
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(CommandError::CommandFailed)?;

    let stdout = child.stdout.take().expect("stdout piped");
    let mut reader = BufReader::new(stdout).lines();
    let mut output = String::new();

    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|e| CommandError::OutputRead(e.to_string()))?
    {
        output.push_str(&line);
        output.push('\n');
    }

    let status = child.wait().await.map_err(CommandError::CommandFailed)?;

    if !status.success() {
        return Err(CommandError::CommandExit {
            exit_code: status.code().unwrap_or(-1),
        });
    }

    Ok(output)
}

pub mod app;
pub mod cli;
pub mod components;
pub mod error;
pub mod types;

pub use error::{Error, Result};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_re_exports_compile() {
        let _err: Result<()> = Ok(());
    }
}
