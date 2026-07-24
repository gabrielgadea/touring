//! Shared regex patterns used across multiple hooks.
//!
//! Centralises file-path extraction regexes that were previously duplicated
//! in `pre_bash` (`FILE_CTX_RE`) and `post_bash` (`RE_FILE_CONTEXT`).

use once_cell::sync::Lazy;
use regex::Regex;

/// Regex to extract source-file paths from bash commands and output lines.
///
/// Matches common source-file extensions preceded by whitespace or start-of-string.
/// Used by `pre_bash::extract_file_context` and `post_bash`.
pub static FILE_PATH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:\s|^)([\w./\-]+\.(?:py|rs|ts|tsx|js|jsx|md|toml|json|yaml|yml|sh))\b")
        .expect("FILE_PATH_RE is a valid static regex")
});
