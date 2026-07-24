//! Validation middleware for hook payloads.
//!
//! Feature D (2026-04-24) — Schema Validation Layer
//!
//! Provides `validate_payload()` and `with_validation!` macro for
//! type-safe payload parsing with clear error messages at hook dispatch time.

use serde_json::Value;
use std::borrow::Cow;
use validator::{Validate, ValidationErrors};

use crate::HookResponse;

/// Validate a JSON payload into a typed struct and run validator derive checks.
///
/// Returns `Ok(typed)` on success.
/// Returns `Err(ValidationErrors)` with field-level error details on failure.
pub fn validate_payload<T: Validate + serde::de::DeserializeOwned>(
    payload: &Value,
) -> Result<T, ValidationErrors> {
    let typed: T = serde_json::from_value(payload.clone()).map_err(|e| {
        let mut errors = ValidationErrors::new();
        errors.add(
            "unknown",
            validator::ValidationError {
                code: Cow::Borrowed("parse_error"),
                message: Some(Cow::Owned(e.to_string())),
                params: std::collections::HashMap::new(),
            },
        );
        errors
    })?;

    // validate() returns Result<(), ValidationErrors> — convert to our return type
    typed.validate()?;
    Ok(typed)
}

/// Format validation errors as a human-readable string.
pub fn format_validation_errors(errors: &ValidationErrors) -> String {
    errors
        .field_errors()
        .iter()
        .map(|(field, field_errors)| {
            let msgs = field_errors
                .iter()
                .map(|e| {
                    e.message
                        .as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| format!("invalid value (code: {})", e.code))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {}", field, msgs)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Build a `HookResponse::Deny` from validation errors.
pub fn validation_deny(errors: &ValidationErrors, event_name: &str) -> HookResponse {
    let msg = format_validation_errors(errors);
    HookResponse::Deny {
        reason: format!("validation_failed: {}", msg),
        context: None,
        event_name: Some(event_name.to_string()),
    }
}

/// Macro to wrap a hook handler with payload validation.
///
/// Usage:
/// ```ignore
/// with_validation!(pre_edit_handler, PreEditPayload, rt, payload, |rt, validated| {
///     // actual handler body using `validated: PreEditPayload`
/// });
/// ```
#[macro_export]
macro_rules! with_validation {
    ($handler:ident, $payload_type:ident, $rt:ident, $payload:ident, $body:expr_2021) => {{
        fn actual_handler(
            $rt: &mut $crate::HookRuntime,
            validated: $payload_type,
        ) -> $crate::HookResponse {
            $body
        }

        match $crate::schemas::validation::validate_payload::<$payload_type>($payload) {
            Ok(validated) => actual_handler($rt, validated),
            Err(errors) => {
                $crate::schemas::validation::validation_deny(&errors, stringify!($handler))
            }
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::hook_payloads::PreReadPayload;

    #[test]
    fn validate_payload_success() {
        let json = serde_json::json!({
            "file_path": "src/main.rs",
            "offset": 10,
            "limit": 100
        });
        let result = validate_payload::<PreReadPayload>(&json);
        assert!(result.is_ok());
        let p = result.expect("validated payload");
        assert_eq!(p.file_path, "src/main.rs");
        assert_eq!(p.offset, Some(10));
        assert_eq!(p.limit, Some(100));
    }

    #[test]
    fn validate_payload_empty_file_path_fails() {
        let json = serde_json::json!({
            "file_path": "",
            "offset": 10
        });
        let result = validate_payload::<PreReadPayload>(&json);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(!errors.field_errors().is_empty());
        // field_errors keys are Cow<'_, str> (validator 0.20); compare via as_ref().
        assert!(
            errors
                .field_errors()
                .keys()
                .any(|k| k.as_ref() == "file_path")
        );
    }

    #[test]
    fn validate_payload_missing_field_fails() {
        let json = serde_json::json!({
            "offset": 10
        });
        let result = validate_payload::<PreReadPayload>(&json);
        assert!(result.is_err());
    }

    #[test]
    fn format_validation_errors_readable() {
        let json = serde_json::json!({"file_path": ""});
        let result = validate_payload::<PreReadPayload>(&json);
        assert!(result.is_err());
        let formatted = format_validation_errors(&result.unwrap_err());
        assert!(formatted.contains("file_path"));
    }
}
