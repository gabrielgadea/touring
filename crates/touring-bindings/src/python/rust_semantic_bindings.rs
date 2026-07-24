//! Wave 5 (2026-04-18) — Python bindings for the Wave 4 deep Rust
//! semantic analyzer.
//!
//! Exposes three zero-copy helpers to Python consumers:
//!
//! - `rust_semantic_analyze(source)` → dict with the full
//!   `RustSemanticReport` serialized as JSON (lazy unmarshal on the
//!   Python side to avoid PyO3 ↔ serde double-allocation).
//! - `rust_format_source(source)` → rustfmt-clean string via prettyplease.
//! - `rust_public_api_surface(source)` → list[str] of stable surface
//!   identifiers (see `RustSemanticReport::public_api_surface`).
//! - `rust_workspace_info(manifest_dir)` → dict with packages, features,
//!   dependents per crate (cargo_metadata-backed).
//!
//! All four functions release the Python GIL during the Rust-side work
//! so concurrent callers from threaded workers (e.g. Python asyncio
//! with `run_in_executor`) do not serialize on the interpreter.
//!
//! # Why a separate module (instead of extending ast_bindings)
//!
//! `ast_bindings.rs` binds tree-sitter functionality — symbol extraction,
//! complexity counting, syntax validation across 13 languages. The syn
//! analyzer is Rust-only and has a distinct surface; splitting it here
//! keeps both modules focused and lets them evolve independently.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use touring_code::ast::rust_semantic::RustSemanticReport;
use touring_code::ast::{CodeGenWorkflow, WorkspaceInfo, format_rust_code};

/// Analyze a Rust source file and return a JSON-serializable report.
///
/// The returned dict mirrors `RustSemanticReport`:
/// - `generics`: list of {name, kind, bounds}
/// - `trait_impls`: list of impl-block summaries
/// - `lifetimes`: list of {name, occurrences}
/// - `derives`: dict of type-name → list[str]
/// - `where_clauses`: list of {context, predicates}
/// - `unsafe_blocks`, `async_fns`, `item_count`: ints
/// - `semantic_complexity`: float ∈ [0, 1]
/// - `total_trait_bounds`: int
#[pyfunction]
#[pyo3(signature = (source))]
fn rust_semantic_analyze(py: Python<'_>, source: &str) -> PyResult<Py<PyAny>> {
    // Release GIL for the (potentially expensive) syn parse.
    let (report, complexity, total_bounds) = py
        .detach(|| {
            let report = RustSemanticReport::from_source(source)
                .map_err(|e| format!("syn parse failed: {e}"))?;
            let complexity = report.semantic_complexity();
            let total_bounds = report.total_trait_bounds();
            Ok::<_, String>((report, complexity, total_bounds))
        })
        .map_err(PyValueError::new_err)?;

    // Serialize to JSON then re-parse on the Python side — this keeps
    // the FFI boundary simple (one String crossing) at the cost of one
    // extra allocation round-trip.
    let json = serde_json::to_string(&report)
        .map_err(|e| PyValueError::new_err(format!("json serialize failed: {e}")))?;

    let out = PyDict::new(py);
    out.set_item("report_json", json)?;
    out.set_item("semantic_complexity", complexity)?;
    out.set_item("total_trait_bounds", total_bounds)?;
    out.set_item("unsafe_blocks", report.unsafe_blocks)?;
    out.set_item("async_fns", report.async_fns)?;
    out.set_item("item_count", report.item_count)?;
    Ok(out.into_any().unbind())
}

/// Reformat Rust source via prettyplease — rustfmt-clean without
/// invoking the `rustfmt` binary.
#[pyfunction]
#[pyo3(signature = (source))]
fn rust_format_source(py: Python<'_>, source: &str) -> PyResult<String> {
    py.detach(|| format_rust_code(source).map_err(|e| format!("prettyplease failed: {e}")))
        .map_err(PyValueError::new_err)
}

/// Extract the stable public API surface of a Rust file as a sorted
/// list of `"kind name"` entries (see
/// `touring_code::ast::rust_semantic::RustSemanticReport::public_api_surface`).
#[pyfunction]
#[pyo3(signature = (source))]
fn rust_public_api_surface<'py>(py: Python<'py>, source: &str) -> PyResult<Bound<'py, PyList>> {
    let entries = py
        .detach(|| {
            RustSemanticReport::public_api_surface(source)
                .map_err(|e| format!("public_api_surface failed: {e}"))
        })
        .map_err(PyValueError::new_err)?;
    PyList::new(py, entries)
}

/// Load cargo workspace metadata from a directory containing Cargo.toml
/// (or any ancestor) and return a dict: `{packages, features, members}`.
///
/// `packages`  — list of {name, version, manifest_path}
/// `features`  — dict of package-name → list[str] of feature names
/// `members`   — list of workspace-member package names
#[pyfunction]
#[pyo3(signature = (manifest_dir))]
fn rust_workspace_info(py: Python<'_>, manifest_dir: &str) -> PyResult<Py<PyAny>> {
    let ws = py
        .detach(|| {
            WorkspaceInfo::load(manifest_dir).map_err(|e| format!("cargo_metadata failed: {e}"))
        })
        .map_err(PyValueError::new_err)?;

    let out = PyDict::new(py);

    // packages: list of {name, version, manifest_path}. `WorkspaceInfo`
    // exposes `packages` as a public `Vec<PackageInfo>` field.
    let packages = PyList::empty(py);
    for pkg in &ws.packages {
        let d = PyDict::new(py);
        d.set_item("name", pkg.name.as_str())?;
        d.set_item("version", pkg.version.as_str())?;
        d.set_item("manifest_path", pkg.manifest_path.as_str())?;
        d.set_item("is_workspace_member", pkg.is_workspace_member)?;
        packages.append(d)?;
    }
    out.set_item("packages", packages)?;

    // features: dict of package-name → list[str]
    let features = PyDict::new(py);
    for pkg in &ws.packages {
        let names: Vec<String> = pkg.features.keys().cloned().collect();
        features.set_item(pkg.name.as_str(), names)?;
    }
    out.set_item("features", features)?;

    // members: list of names of the workspace-member packages.
    let members: Vec<String> = ws
        .workspace_members()
        .into_iter()
        .map(|p| p.name.clone())
        .collect();
    out.set_item("members", members)?;

    Ok(out.into_any().unbind())
}

/// Wave 5 synergy entry point — one-shot code-gen workflow.
///
/// Runs `CodeGenWorkflow::analyze(source)` server-side. Returns a dict:
/// - `semantic_json`: serialized RustSemanticReport
/// - `public_api`: list[str]
/// - `formatted_source`: Optional[str]
/// - `semantic_complexity`: float ∈ [0, 1]
/// - `total_trait_bounds`: int
/// - `complexity_band`: str ("simple"|"moderate"|"complex"|"very_complex")
/// - `has_public_surface`: bool
///
/// Replaces 4 separate PyO3 round trips with a single call — critical
/// for latency-sensitive post-edit hooks where every cross-FFI call
/// adds ~50 µs. `py.allow_threads` releases the GIL during the syn
/// parse so concurrent Python workers do not serialize on the analyzer.
#[pyfunction]
#[pyo3(signature = (source, format = true))]
fn rust_code_gen_workflow(py: Python<'_>, source: &str, format: bool) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(|| {
            if format {
                CodeGenWorkflow::analyze(source)
            } else {
                CodeGenWorkflow::analyze_no_format(source)
            }
            .map_err(|e| format!("workflow failed: {e}"))
        })
        .map_err(PyValueError::new_err)?;

    let out = PyDict::new(py);
    let semantic_json = serde_json::to_string(&report.semantic)
        .map_err(|e| PyValueError::new_err(format!("serialize semantic: {e}")))?;
    out.set_item("semantic_json", semantic_json)?;
    out.set_item("public_api", &report.public_api)?;
    out.set_item("formatted_source", &report.formatted_source)?;
    out.set_item("semantic_complexity", report.semantic_complexity)?;
    out.set_item("total_trait_bounds", report.total_trait_bounds)?;
    out.set_item("complexity_band", report.complexity_band())?;
    out.set_item("has_public_surface", report.has_public_surface())?;
    Ok(out.into_any().unbind())
}

/// Register the Wave 5 semantic bindings into the parent `claude_learning_kernel` module.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(rust_semantic_analyze, m)?)?;
    m.add_function(wrap_pyfunction!(rust_format_source, m)?)?;
    m.add_function(wrap_pyfunction!(rust_public_api_surface, m)?)?;
    m.add_function(wrap_pyfunction!(rust_workspace_info, m)?)?;
    m.add_function(wrap_pyfunction!(rust_code_gen_workflow, m)?)?;
    Ok(())
}
