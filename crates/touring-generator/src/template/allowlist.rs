//! `VariableAllowlist` — regex-enforced template variable name validation.

use crate::error::GenerateError;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Compiled allowlist regex — initialized once at first use.
static NAME_REGEX: OnceLock<Regex> = OnceLock::new();

fn name_regex() -> &'static Regex {
    NAME_REGEX.get_or_init(|| {
        // SAFETY: hardcoded pattern — compile failure is a programmer error, not a runtime condition.
        #[allow(clippy::expect_used)]
        Regex::new(r"^[a-zA-Z][a-zA-Z0-9_]{0,63}$")
            .expect("VariableAllowlist: static regex must compile")
    })
}

/// Validates template variable keys against a safe name pattern.
///
/// Prevents injection via malformed variable names. Pattern:
/// `^[a-zA-Z][a-zA-Z0-9_]{0,63}$`
pub struct VariableAllowlist;

impl VariableAllowlist {
    /// Construct with the default safe-name pattern.
    ///
    /// # Panics
    ///
    /// Panics if the static regex pattern fails to compile (impossible for
    /// the hardcoded pattern — would be a compile-time regression).
    #[must_use]
    pub fn new() -> Self {
        // Eagerly initialize the OnceLock so the first call has no cold start.
        let _ = name_regex();
        Self
    }

    /// Validate all keys in `vars` against the allowlist pattern.
    ///
    /// # Errors
    ///
    /// Returns [`GenerateError::TemplateVariableRejected`] if any key does not
    /// match `^[a-zA-Z][a-zA-Z0-9_]{0,63}$`.
    pub fn validate(&self, vars: &HashMap<String, serde_json::Value>) -> Result<(), GenerateError> {
        let re = name_regex();
        for key in vars.keys() {
            if !re.is_match(key) {
                return Err(GenerateError::TemplateVariableRejected {
                    key: key.clone(),
                    reason: "must match ^[a-zA-Z][a-zA-Z0-9_]{0,63}$".into(),
                });
            }
        }
        Ok(())
    }
}

impl Default for VariableAllowlist {
    fn default() -> Self {
        Self::new()
    }
}
