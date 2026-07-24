//! Schema registry + wiring gate adapters.
//!
//! Extracted from `core/context.rs` (F-9 modularization): `SchemaRegistry`
//! (plan-version migration), `SynWiringGateAdapter` (syntax-level orphan /
//! forbidden-`allow` gate via `syn`), and `CompositeWiringGate` (stacked
//! Syn + Analysis gate). Re-exported from `core::context` so the public API
//! (`crate::SchemaRegistry`, `crate::SynWiringGateAdapter`,
//! `crate::CompositeWiringGate`, …) is preserved verbatim. `WiringGateFn` and
//! the `AnalysisGateAdapter` / `WiringGateError` types stay in `context.rs`
//! and are referenced by full path / cfg-gated `use`.

use crate::error::GenerateError;
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "analysis-gate")]
use crate::core::context::{AnalysisGateAdapter, WiringGateError};

// ── SchemaRegistry (PLN2 section 8.1) ────────────────────────────────────────

/// Plan schema migration registry — supports v1→v2 plan deserialization.
///
/// Holds the current engine schema version and registered migration adapters.
/// Future waves register adapters for older plan formats.
#[derive(Debug, Default)]
pub struct SchemaRegistry {
    /// Current engine schema version (semver string).
    pub engine_version: String,
    /// Registered plan version migrations: `"1.0.0" → "2.0.0"`.
    pub migrations: HashMap<String, String>,
}

impl SchemaRegistry {
    /// Construct with the given engine version.
    pub fn new(engine_version: impl Into<String>) -> Self {
        Self {
            engine_version: engine_version.into(),
            migrations: HashMap::new(),
        }
    }

    /// Check if a plan version `v` is compatible with the current engine.
    #[must_use]
    pub fn is_compatible(&self, v: &str) -> bool {
        v == self.engine_version || self.migrations.contains_key(v)
    }

    /// Register a migration: plans of version `from` can be migrated to `to`.
    pub fn register_migration(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.migrations.insert(from.into(), to.into());
    }
}

// ── SynWiringGateAdapter (PLN2 section 8.1 — feature `syn-quote`) ────────────

/// Wiring gate adapter backed by `syn::parse_file`.
///
/// Parses each rendered Rust file with `syn` and enforces wiring invariants:
///
/// 1. **Syntax validity** — any unparseable file is rejected immediately
/// 2. **Orphan budget** — rejects files whose `pub` item count exceeds
///    `max_pub_items_per_file` (default 50) because an unbounded dump of
///    public symbols is almost always an orphan risk
/// 3. **Forbidden attributes** — rejects `#[allow(dead_code)]` and
///    `#[allow(unused)]` at item level, which is the exact anti-pattern
///    REGRA #0 POTENCIALIZAR forbids
///
/// Non-Rust files (detected by the `.rs` suffix) are skipped — the gate
/// only applies to Rust artifacts. This keeps the gate safe for mixed-kind
/// plans (YAML, Markdown, Dockerfile, etc.).
///
/// # Usage
///
/// ```rust,ignore
/// use std::sync::Arc;
/// use touring_generator::{SynWiringGateAdapter, WiringGateFn};
///
/// let adapter = Arc::new(SynWiringGateAdapter::new());
/// let adapter_clone = Arc::clone(&adapter);
/// let fn_box: WiringGateFn = Arc::new(move |files| adapter_clone.check(files));
/// ```
#[derive(Debug, Clone)]
pub struct SynWiringGateAdapter {
    /// Maximum number of top-level `pub` items permitted per Rust file.
    /// Files exceeding this threshold are rejected as likely orphan risks.
    pub max_pub_items_per_file: usize,
    /// Whether to reject files containing forbidden `#[allow(...)]` attributes.
    pub reject_forbidden_allows: bool,
}

impl Default for SynWiringGateAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SynWiringGateAdapter {
    /// Construct with `POTENCIALIZAR` defaults (`max_pub=50`, `reject_allows=true`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_pub_items_per_file: 50,
            reject_forbidden_allows: true,
        }
    }

    /// Construct with custom thresholds.
    #[must_use]
    pub fn with_config(max_pub_items_per_file: usize, reject_forbidden_allows: bool) -> Self {
        Self {
            max_pub_items_per_file,
            reject_forbidden_allows,
        }
    }

    /// Run the gate against a batch of rendered files.
    ///
    /// Returns `Ok(())` if all files pass, `Err(GenerateError)` on the first
    /// violation encountered. The error carries the file path so the generator
    /// can feed it back to the planner for replan.
    ///
    /// # Errors
    ///
    /// - `GenerateError::Internal` — file is not parseable as valid Rust
    /// - `GenerateError::Internal` — file has too many top-level `pub` items
    /// - `GenerateError::Internal` — file contains a forbidden `#[allow(...)]`
    pub fn check(&self, files: &[crate::plan::result::RenderedFile]) -> Result<(), GenerateError> {
        for rendered in files {
            // Case-insensitive .rs extension check via std::path::Path.
            // Skips non-Rust artifacts (Markdown, YAML, Dockerfile, etc.) safely.
            let is_rust = std::path::Path::new(&rendered.path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"));
            if !is_rust {
                continue;
            }
            self.check_rust_file(rendered)?;
        }
        Ok(())
    }

    /// Check a single Rust file — used internally by `check` and tests.
    fn check_rust_file(
        &self,
        rendered: &crate::plan::result::RenderedFile,
    ) -> Result<(), GenerateError> {
        let parsed = syn::parse_file(&rendered.content).map_err(|e| {
            GenerateError::Internal(format!(
                "wiring gate: file `{}` is not valid Rust: {e}",
                rendered.path
            ))
        })?;

        let mut pub_count: usize = 0;
        for item in &parsed.items {
            if Self::is_public_item(item) {
                pub_count += 1;
            }
            if self.reject_forbidden_allows && Self::has_forbidden_allow(item) {
                return Err(GenerateError::Internal(format!(
                    "wiring gate: file `{}` contains forbidden `#[allow(dead_code|unused)]` \
                     (REGRA #0 POTENCIALIZAR violation)",
                    rendered.path
                )));
            }
        }

        if pub_count > self.max_pub_items_per_file {
            return Err(GenerateError::Internal(format!(
                "wiring gate: file `{}` has {} pub items (max {}); likely orphan risk",
                rendered.path, pub_count, self.max_pub_items_per_file
            )));
        }

        Ok(())
    }

    /// Returns `true` if the item is declared with `pub` visibility.
    fn is_public_item(item: &syn::Item) -> bool {
        match item {
            syn::Item::Const(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Enum(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Fn(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Mod(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Static(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Struct(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Trait(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::TraitAlias(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Type(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Union(x) => matches!(x.vis, syn::Visibility::Public(_)),
            syn::Item::Use(x) => matches!(x.vis, syn::Visibility::Public(_)),
            _ => false,
        }
    }

    /// Returns `true` if the item carries a forbidden `#[allow(dead_code)]`
    /// or `#[allow(unused)]` attribute — REGRA #0 POTENCIALIZAR violation.
    fn has_forbidden_allow(item: &syn::Item) -> bool {
        let attrs: &[syn::Attribute] = match item {
            syn::Item::Const(x) => &x.attrs,
            syn::Item::Enum(x) => &x.attrs,
            syn::Item::Fn(x) => &x.attrs,
            syn::Item::Mod(x) => &x.attrs,
            syn::Item::Static(x) => &x.attrs,
            syn::Item::Struct(x) => &x.attrs,
            syn::Item::Trait(x) => &x.attrs,
            syn::Item::TraitAlias(x) => &x.attrs,
            syn::Item::Type(x) => &x.attrs,
            syn::Item::Union(x) => &x.attrs,
            syn::Item::Use(x) => &x.attrs,
            _ => return false,
        };
        attrs.iter().any(Self::attribute_is_forbidden_allow)
    }

    /// Check a single attribute for the forbidden `allow` pattern.
    fn attribute_is_forbidden_allow(attr: &syn::Attribute) -> bool {
        if !attr.path().is_ident("allow") {
            return false;
        }
        let mut found = false;
        let parse_result = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("dead_code") || meta.path.is_ident("unused") {
                found = true;
            }
            Ok(())
        });
        parse_result.is_ok() && found
    }

    /// Build a `WiringGateFn` closure that invokes this adapter.
    ///
    /// The returned closure takes ownership of the adapter via `Arc` so it
    /// can be stored inside `GeneratorContext::wiring_gate_fn`.
    #[must_use]
    pub fn into_closure(self) -> crate::core::context::WiringGateFn {
        let adapter = Arc::new(self);
        Arc::new(
            move |files: &[crate::plan::result::RenderedFile], _plan_id: &str| adapter.check(files),
        )
    }

    /// Emit a synergistic consumer stub that wires an orphan pub symbol into
    /// active use — REGRA #0 POTENCIALIZAR.
    ///
    /// Returns a `proc_macro2::TokenStream` built via `quote!` that a caller
    /// (e.g. `ConsumerGenerator` kind of `touring-generator`) can stream to
    /// a template or `format_rust_code`-pipe straight into a `RenderedFile`.
    ///
    /// This is the deliberate wire point for the three proc-macro crates:
    /// `syn` parses the orphan declaration, `quote!` builds the consumer
    /// body, `proc_macro2::TokenStream` carries the result across the
    /// boundary. The adapter therefore exercises all three dependencies
    /// on every invocation.
    ///
    /// # Arguments
    ///
    /// - `orphan_name` — the orphan pub symbol identifier (e.g. `"MyWidget"`).
    ///   Must be a valid Rust identifier; otherwise an empty `TokenStream`
    ///   is returned and the caller can fall back to a free-form template.
    /// - `orphan_crate` — optional crate that exports the orphan. When
    ///   `Some`, the stub references `#orphan_crate::#orphan_name`; when
    ///   `None`, the stub references `crate::#orphan_name`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use touring_generator::SynWiringGateAdapter;
    /// let adapter = SynWiringGateAdapter::new();
    /// let stub = adapter.suggest_consumer_stub("FooWidget", Some("touring_ast"));
    /// assert!(stub.to_string().contains("consume_foo_widget"));
    /// ```
    #[must_use]
    pub fn suggest_consumer_stub(
        &self,
        orphan_name: &str,
        orphan_crate: Option<&str>,
    ) -> proc_macro2::TokenStream {
        let Ok(ident) = syn::parse_str::<syn::Ident>(orphan_name) else {
            return proc_macro2::TokenStream::new();
        };
        let consumer_name = format!("consume_{}", snake_case(orphan_name));
        let Ok(consumer_ident) = syn::parse_str::<syn::Ident>(&consumer_name) else {
            return proc_macro2::TokenStream::new();
        };
        if let Some(crate_ident) =
            orphan_crate.and_then(|c| syn::parse_str::<syn::Ident>(&c.replace('-', "_")).ok())
        {
            quote::quote! {
                /// Auto-generated consumer stub (REGRA #0 POTENCIALIZAR).
                ///
                /// Wires orphan pub symbol `#crate_ident::#ident` into active use.
                fn #consumer_ident() {
                    let _ = #crate_ident::#ident;
                }
            }
        } else {
            quote::quote! {
                /// Auto-generated consumer stub (REGRA #0 POTENCIALIZAR).
                ///
                /// Wires orphan pub symbol `crate::#ident` into active use.
                fn #consumer_ident() {
                    let _ = crate::#ident;
                }
            }
        }
    }
}

/// Convert an `UpperCamelCase` identifier to `snake_case`.
/// Used by `suggest_consumer_stub` to produce readable consumer function names.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

// ── CompositeWiringGate (PLN2 section 8.1 — feature `syn-quote+analysis-gate`)

/// Stacked pre-commit wiring gate running **both** the syntax-level and
/// DB-backed gates in sequence.
///
/// The order is optimized for fail-fast:
/// 1. `SynWiringGateAdapter::check` — pure syntax parsing, no I/O (< 1 ms)
/// 2. `AnalysisGateAdapter::check` — opens `Mutex<rusqlite::Connection>` and
///    cross-references symbols against `wiring_map` (1–10 ms typical)
///
/// If either gate rejects, the commit is aborted with the specific error
/// from the first failing gate. The gates are independent — passing Syn
/// does NOT imply Analysis will pass, and vice versa. Defense-in-depth.
///
/// # Wiring
///
/// `CompositeWiringGate::into_closure()` returns a `WiringGateFn` that can
/// be injected directly into `GeneratorContext::wiring_gate_fn`. Callers
/// who already hold the two underlying adapters can pass them as refs;
/// callers who want a one-shot construct use `open()` to build both.
///
/// # POTENCIALIZAR
///
/// Combines two orthogonal validation strategies into a single closure,
/// letting `touring-generator` provide production-grade wiring enforcement
/// without the caller having to wire two separate closures.
#[cfg(feature = "analysis-gate")]
pub struct CompositeWiringGate {
    syn_gate: SynWiringGateAdapter,
    analysis_gate: AnalysisGateAdapter,
}

#[cfg(feature = "analysis-gate")]
impl std::fmt::Debug for CompositeWiringGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeWiringGate")
            .field("syn_gate", &self.syn_gate)
            .field("analysis_gate", &self.analysis_gate)
            .finish()
    }
}

#[cfg(feature = "analysis-gate")]
impl CompositeWiringGate {
    /// Open the knowledge DB at `db_path` and build both gates with default thresholds.
    ///
    /// # Errors
    ///
    /// Returns [`WiringGateError`] when the database cannot be opened or the
    /// `AnalysisGateAdapter` constructor fails. The Syn gate construction
    /// is infallible (no I/O).
    pub fn open(db_path: &std::path::Path) -> Result<Self, WiringGateError> {
        let analysis_gate = AnalysisGateAdapter::open(db_path)?;
        Ok(Self {
            syn_gate: SynWiringGateAdapter::new(),
            analysis_gate,
        })
    }

    /// Open with thresholds + bypass driven by env vars (see
    /// `AnalysisGateAdapter::open_with_env`). The Syn gate stays at default —
    /// it only catches syntax errors and forbidden `#[allow(dead_code)]`
    /// attributes, which is REGRA #0 hygiene that is never bypassed.
    ///
    /// # Errors
    ///
    /// Returns [`WiringGateError`] when opening the database fails — same as `open()`.
    pub fn open_with_env(db_path: &std::path::Path) -> Result<Self, WiringGateError> {
        let analysis_gate = AnalysisGateAdapter::open_with_env(db_path)?;
        Ok(Self {
            syn_gate: SynWiringGateAdapter::new(),
            analysis_gate,
        })
    }

    /// Compose with externally-built adapters.
    ///
    /// Useful when the caller already holds tuned `SynWiringGateAdapter`
    /// and `AnalysisGateAdapter` instances and wants to reuse them.
    #[must_use]
    pub fn compose(syn_gate: SynWiringGateAdapter, analysis_gate: AnalysisGateAdapter) -> Self {
        Self {
            syn_gate,
            analysis_gate,
        }
    }

    /// Run both gates against a batch of rendered files.
    ///
    /// Fail-fast order: Syn first (fast), Analysis second (DB I/O).
    ///
    /// # Errors
    ///
    /// Returns `GenerateError::Internal` from whichever gate rejects first.
    /// The error message identifies which gate fired the rejection so
    /// callers can distinguish syntax vs. wiring-DB rejections.
    pub fn check(
        &self,
        files: &[crate::plan::result::RenderedFile],
        plan_id: &str,
    ) -> Result<(), GenerateError> {
        self.syn_gate.check(files)?;
        self.analysis_gate.check(files, plan_id)?;
        Ok(())
    }

    /// Build a `WiringGateFn` closure that invokes both gates.
    #[must_use]
    pub fn into_closure(self) -> crate::core::context::WiringGateFn {
        let adapter = Arc::new(self);
        Arc::new(
            move |files: &[crate::plan::result::RenderedFile], plan_id: &str| {
                adapter.check(files, plan_id)
            },
        )
    }

    /// Returns a reference to the underlying syntax gate for inspection.
    #[must_use]
    pub fn syn_gate(&self) -> &SynWiringGateAdapter {
        &self.syn_gate
    }

    /// Returns a reference to the underlying analysis gate for inspection.
    #[must_use]
    pub fn analysis_gate(&self) -> &AnalysisGateAdapter {
        &self.analysis_gate
    }
}
