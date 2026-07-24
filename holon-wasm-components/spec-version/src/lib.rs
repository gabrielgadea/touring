//! THSF Fase 4 proof-of-life WASM component.
//!
//! Exports exactly one capability — `spec-version` — which returns the
//! holon-core WIT package version (currently `"0.1.0"`) as JSON bytes
//! in the response's `stdout` field.
//!
//! The purpose of this component is *not* functional value but pipeline
//! validation: it proves that `wit-bindgen` + `wasm32-wasip2` + `wasmtime
//! component run` all work end-to-end before we port real capabilities
//! (symbol-index, blast-radius, quality-gate in waves 4C).
//!
//! Target `wasm32-wasip2` ships a full `std` — no need for `no_std`.

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

wit_bindgen::generate!({
    path: "../../crates/touring-wasm/wit/holon-core.wit",
    world: "holon-component",
});

// Pull the generated types into scope.
use exports::holon::core::capabilities::{Guest, InvokeError, InvokeRequest, InvokeResponse};

/// Canonical identifier of the only capability this component exports.
const CAPABILITY: &str = "spec-version";

/// WIT package version mirrored at build time. Keep in sync with
/// `holon-core.wit`.
const SPEC_VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

struct Component;

impl Guest for Component {
    fn list_capabilities() -> Vec<String> {
        vec![CAPABILITY.to_string()]
    }

    fn invoke(request: InvokeRequest) -> Result<InvokeResponse, InvokeError> {
        if request.capability != CAPABILITY {
            return Err(InvokeError::UnknownCapability(request.capability));
        }

        let body = format!(r#"{{"spec_version":"{SPEC_VERSION}"}}"#);
        Ok(InvokeResponse {
            exit_code: 0,
            stdout: body.into_bytes(),
            stderr: Vec::new(),
            duration_ms: 0,
            logged: false,
        })
    }
}

export!(Component);
