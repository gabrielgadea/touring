//! WasmPluginRunner -- thin adapter over `touring-wasm` for the server plugin system.
//!
//! All sandboxing guarantees (fuel metering, 1 MB stack limit, import
//! whitelist, no host filesystem/network access) are provided by
//! `touring_bindings::wasm::{WasmRunner, WasmModule}`.  This module is a zero-cost
//! re-export layer so that the rest of `touring-server` keeps the same
//! `plugins::runner::WasmPluginRunner` surface it always had.

#[cfg(feature = "wasm-plugins")]
use touring_bindings::wasm::WasmRunner;

/// Re-export the canonical fuel / success result from touring-wasm.
#[cfg(feature = "wasm-plugins")]
pub(crate) use touring_bindings::wasm::PluginResult;

/// Re-export PluginContext so callers in this crate don't import touring-wasm directly.
#[cfg(feature = "wasm-plugins")]
pub(crate) use touring_bindings::wasm::PluginContext;

/// Re-export the maximum fuel constant for parity with the old inline definition.
#[cfg(feature = "wasm-plugins")]
pub(crate) use touring_bindings::wasm::MAX_FUEL;

/// WASM plugin runner — delegates entirely to [`touring_bindings::wasm::WasmRunner`].
///
/// All security properties are inherited from `touring-wasm`:
/// - Fuel limit: [`MAX_FUEL`] instructions per execution.
/// - Stack limit: 1 MB per instance.
/// - Import allowlist: only `env::log` and `env::get_config` by default.
/// - Sandbox: no host filesystem / network / syscall access.
#[cfg(feature = "wasm-plugins")]
pub struct WasmPluginRunner {
    inner: WasmRunner,
}

#[cfg(feature = "wasm-plugins")]
impl WasmPluginRunner {
    /// Create a new runner with security defaults.
    ///
    /// Returns [`crate::ServerError::WasmInit`] if the WASM engine fails to initialize.
    pub fn new() -> Result<Self, crate::ServerError> {
        Ok(Self {
            inner: WasmRunner::new().map_err(|e| crate::ServerError::WasmInit(e.to_string()))?,
        })
    }

    /// Run a WASM plugin from raw bytes with the given input text.
    #[allow(dead_code)]
    /// False positive: called from tools_infra.rs:1040 via #[cfg(feature = "wasm-plugins")] gate.
    pub(crate) fn run_plugin(
        &self,
        wasm_bytes: &[u8],
        input: &str,
    ) -> Result<PluginResult, String> {
        let module = self.inner.load_module(wasm_bytes)?;
        let ctx = PluginContext::new(input);
        module.call_evaluate(&ctx).map_err(Into::into)
    }

    /// Run a WASM plugin from WAT (WebAssembly Text format) source.
    pub(crate) fn run_wat(&self, wat_source: &str, input: &str) -> Result<PluginResult, String> {
        let module = self.inner.load_wat(wat_source)?;
        let ctx = PluginContext::new(input);
        module.call_evaluate(&ctx).map_err(Into::into)
    }

    /// Add an import to the allowlist (format: `"module::function"`).
    pub(crate) fn allow_import(&mut self, import_name: &str) {
        self.inner.allow_import(import_name);
    }

    /// Return the current import allowlist.
    pub(crate) fn allowed_imports(&self) -> &std::collections::HashSet<String> {
        self.inner.allowed_imports()
    }

    /// Return the maximum fuel limit per execution.
    pub(crate) fn max_fuel(&self) -> u64 {
        MAX_FUEL
    }

    /// Run a WAT plugin with an extended import allowlist.
    ///
    /// Convenience entry point that wires together `allow_import`, `allowed_imports`,
    /// `run_wat`, and `max_fuel` into a single call:
    /// 1. Adds each entry in `extra_imports` to the allowlist.
    /// 2. Validates the allowlist size against the fuel budget (1 import ≈ 1 000 fuel).
    /// 3. Executes the WAT source with the given input.
    ///
    /// Returns `Err` if fuel would be exceeded or execution fails.
    pub fn run_wat_with_imports(
        &mut self,
        wat_source: &str,
        input: &str,
        extra_imports: &[&str],
    ) -> Result<PluginResult, String> {
        for import in extra_imports {
            self.allow_import(import);
        }
        let import_count = self.allowed_imports().len() as u64;
        let fuel_budget = self.max_fuel();
        if import_count * 1_000 > fuel_budget {
            return Err(format!(
                "Too many imports ({import_count}): estimated fuel cost exceeds budget ({fuel_budget})"
            ));
        }
        self.run_wat(wat_source, input)
    }
}

#[cfg(all(test, feature = "wasm-plugins"))]
mod tests {
    use super::*;

    #[test]
    fn test_runner_creation() {
        let runner = WasmPluginRunner::new();
        assert!(runner.is_ok());
        assert_eq!(runner.unwrap().max_fuel(), 10_000_000);
    }

    #[test]
    fn test_default_whitelist() {
        let runner = WasmPluginRunner::new().unwrap();
        assert!(runner.allowed_imports().contains("env::log"));
        assert!(runner.allowed_imports().contains("env::get_config"));
        assert!(!runner.allowed_imports().contains("evil::hack"));
    }

    #[test]
    fn test_add_import_to_whitelist() {
        let mut runner = WasmPluginRunner::new().unwrap();
        assert!(!runner.allowed_imports().contains("custom::func"));
        runner.allow_import("custom::func");
        assert!(runner.allowed_imports().contains("custom::func"));
    }

    #[test]
    fn test_simple_wasm_execution() {
        let runner = WasmPluginRunner::new().unwrap();
        let result = runner.run_wat(r#"(module)"#, "test input");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().fuel_consumed, 0);
    }

    #[test]
    fn test_import_whitelist_rejection() {
        let runner = WasmPluginRunner::new().unwrap();
        let wat = r#"(module (import "evil" "hack" (func)))"#;
        let result = runner.run_wat(wat, "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not in allowlist"));
    }

    #[test]
    fn test_invalid_wasm_bytes() {
        let runner = WasmPluginRunner::new().unwrap();
        let result = runner.run_plugin(b"not valid wasm", "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid WASM module"));
    }
}
