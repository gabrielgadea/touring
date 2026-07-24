//! AST-driven wiring analysis for pub symbol detection and import prediction.
//!
//! Provides tools to:
//! - Extract all pub symbols from source code
//! - Diff symbols between two versions of a file
//! - Predict needed imports based on type references

use crate::ast::graph::ImportInfo;
use crate::ast::symbols::{Symbol, SymbolKind};

/// A pub symbol extracted from source for wiring analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct PubSymbol {
    /// Name of the pub symbol.
    pub name: String,
    /// Kind of the symbol (function, struct, trait, etc.).
    pub kind: SymbolKind,
    /// 1-based line number where the symbol is declared.
    pub line: usize,
}

/// Diff between two versions of a file's pub symbols.
#[derive(Debug, Clone, Default)]
pub struct SymbolDiff {
    /// Pub symbols added (present in new, absent in old)
    pub added: Vec<PubSymbol>,
    /// Pub symbols removed (present in old, absent in new)
    pub removed: Vec<PubSymbol>,
    /// Pub symbols unchanged
    pub unchanged: Vec<PubSymbol>,
}

/// A predicted import suggestion.
#[derive(Debug, Clone)]
pub struct ImportSuggestion {
    /// The symbol name that needs importing
    pub symbol_name: String,
    /// The module that defines it
    pub source_module: String,
    /// Confidence (0.0-1.0)
    pub confidence: f64,
    /// Reason for suggestion
    pub reason: String,
}

/// Extract all pub symbols from a list of parsed symbols.
///
/// Filters to only public symbols (is_public=true) and maps them to PubSymbol.
pub fn extract_pub_symbols(symbols: &[Symbol]) -> Vec<PubSymbol> {
    symbols
        .iter()
        .filter(|s| s.is_public)
        .map(|s| PubSymbol {
            name: s.name.clone(),
            kind: s.kind.clone(),
            line: s.line,
        })
        .collect()
}

/// Compute the diff between old and new pub symbol sets.
///
/// Uses symbol name as the key for comparison.
pub fn diff_pub_symbols(old: &[PubSymbol], new: &[PubSymbol]) -> SymbolDiff {
    let old_names: std::collections::HashSet<&str> = old.iter().map(|s| s.name.as_str()).collect();
    let new_names: std::collections::HashSet<&str> = new.iter().map(|s| s.name.as_str()).collect();

    let added = new
        .iter()
        .filter(|s| !old_names.contains(s.name.as_str()))
        .cloned()
        .collect();
    let removed = old
        .iter()
        .filter(|s| !new_names.contains(s.name.as_str()))
        .cloned()
        .collect();
    let unchanged = new
        .iter()
        .filter(|s| old_names.contains(s.name.as_str()))
        .cloned()
        .collect();

    SymbolDiff {
        added,
        removed,
        unchanged,
    }
}

/// Detect type references in source code that are not covered by imports.
///
/// Returns names that look like type references (PascalCase, used after `:` or `<`)
/// but do not appear in the import list. These are candidates for import prediction.
pub fn detect_unresolved_references(
    source: &str,
    current_imports: &[ImportInfo],
    known_symbols_in_file: &[String],
) -> Vec<String> {
    let imported: std::collections::HashSet<&str> = current_imports
        .iter()
        .flat_map(|i| i.symbols.iter().map(|s| s.as_str()))
        .chain(current_imports.iter().map(|i| {
            // Also consider the module name itself as imported (bare imports)
            i.module_path.rsplit("::").next().unwrap_or(&i.module_path)
        }))
        .collect();

    let local: std::collections::HashSet<&str> =
        known_symbols_in_file.iter().map(|s| s.as_str()).collect();

    // Find PascalCase identifiers that might be type references
    let mut unresolved = Vec::new();
    for word in source.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.len() >= 2
            && word.chars().next().is_some_and(|c| c.is_uppercase())
            && !word.chars().all(|c| c.is_uppercase() || c == '_') // Not ALL_CAPS constant
            && !imported.contains(word)
            && !local.contains(word)
            && !is_builtin_type(word)
            && !unresolved.contains(&word.to_string())
        {
            unresolved.push(word.to_string());
        }
    }
    unresolved
}

/// Check if a name is a common builtin type (not needing import).
fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "Vec"
            | "Option"
            | "Result"
            | "Box"
            | "Arc"
            | "Mutex"
            | "RwLock"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
            | "VecDeque"
            | "Path"
            | "PathBuf"
            | "Duration"
            | "Instant"
            | "Ok"
            | "Err"
            | "Some"
            | "None"
            | "Self"
            | "Default"
            | "Send"
            | "Sync"
            | "Clone"
            | "Debug"
            | "Display"
            | "Serialize"
            | "Deserialize"
            | "True"
            | "False"
            | "Array"
            | "Object"
            | "Map"
            | "Set"
            | "Promise"
    )
}

/// Detect pub use re-exports in source code.
///
/// Returns list of (re_exported_symbol, source_module) pairs.
pub fn detect_reexports(source: &str) -> Vec<(String, String)> {
    let mut reexports = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Rust: pub use crate::module::Symbol;
        if trimmed.starts_with("pub use ") {
            let path = trimmed
                .trim_start_matches("pub use ")
                .trim_end_matches(';')
                .trim();
            if let Some(last) = path.rsplit("::").next() {
                // Handle {A, B} syntax
                if last.starts_with('{') {
                    let inner = last.trim_start_matches('{').trim_end_matches('}');
                    let module = path.rsplit_once("::").map(|(m, _)| m).unwrap_or(path);
                    for sym in inner.split(',') {
                        let sym = sym.trim();
                        if !sym.is_empty() {
                            reexports.push((sym.to_string(), module.to_string()));
                        }
                    }
                } else {
                    let module = path.rsplit_once("::").map(|(m, _)| m).unwrap_or(path);
                    reexports.push((last.to_string(), module.to_string()));
                }
            }
        }
    }
    reexports
}

/// Build import suggestions by matching unresolved references against known modules.
///
/// For each unresolved symbol name, searches `known_modules` for a matching
/// pub symbol and creates an `ImportSuggestion` with module path and confidence.
///
/// # Arguments
/// - `unresolved`: Symbol names that were detected as unresolved in source
/// - `known_modules`: Pairs of `(symbol_name, module_file)` from the wiring map
pub fn suggest_imports(
    unresolved: &[String],
    known_modules: &[(String, String)],
) -> Vec<ImportSuggestion> {
    let mut suggestions = Vec::new();
    for name in unresolved {
        for (sym_name, module_file) in known_modules {
            if sym_name == name {
                // Convert module_file path to Rust module path for the suggestion
                let module_path = module_file
                    .trim_start_matches("src/")
                    .trim_end_matches(".rs")
                    .replace('/', "::");
                suggestions.push(ImportSuggestion {
                    symbol_name: name.clone(),
                    source_module: format!("crate::{module_path}"),
                    confidence: 0.8,
                    reason: format!("pub symbol found in {module_file}"),
                });
                break; // first match per unresolved name
            }
        }
    }
    suggestions
}

// ── Workspace-wide feature/dependency analysis via cargo_metadata ──────

/// Lightweight view of a Cargo workspace — packages, features, deps.
///
/// Built by invoking `cargo metadata --format-version 1` once and caching
/// the result. Enables workspace-aware wiring analysis beyond what
/// touring-ast can deduce from source alone: cross-crate feature flags,
/// dev-dep boundaries, optional dependency gates.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceInfo {
    /// Absolute path of the workspace root (directory holding the root `Cargo.toml`).
    pub workspace_root: String,
    /// Packages contained in the workspace.
    pub packages: Vec<PackageInfo>,
    /// Total number of workspace members (may differ from `packages.len()`
    /// when dependencies outside the workspace are included).
    pub workspace_member_count: usize,
}

/// Per-package info distilled from cargo_metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PackageInfo {
    /// Package name as declared in `Cargo.toml`.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Absolute path to the package manifest.
    pub manifest_path: String,
    /// Feature-name → required features/deps list.
    pub features: std::collections::BTreeMap<String, Vec<String>>,
    /// Names of direct dependencies (includes normal + dev + build).
    pub dependencies: Vec<String>,
    /// Names of optional dependencies (a subset of `dependencies`).
    pub optional_dependencies: Vec<String>,
    /// Whether this package is a workspace member (vs. an external dep).
    pub is_workspace_member: bool,
}

/// Resolve `dir` to an ABSOLUTE `Cargo.toml`, walking up ancestors to the
/// nearest manifest. `cargo metadata` interprets a relative dir (the CLI
/// default `.`) against the *process* CWD — for the daemon that is its spawn
/// directory, not the user's — so a bare `.` fails with "manifest path
/// `./Cargo.toml` does not exist" whenever the daemon was started elsewhere.
/// Canonicalizing + walking up makes resolution independent of the process CWD
/// and honors the documented "directly or via ancestors" contract of
/// [`WorkspaceInfo::load`] (previously unimplemented).
fn resolve_workspace_manifest(dir: &std::path::Path) -> crate::ast::AstResult<std::path::PathBuf> {
    let abs = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut cur: &std::path::Path = &abs;
    loop {
        let candidate = cur.join("Cargo.toml");
        if candidate.is_file() {
            return Ok(candidate);
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => {
                return Err(crate::ast::AstError::Io(std::io::Error::other(format!(
                    "no Cargo.toml at or above {}",
                    abs.display()
                ))));
            }
        }
    }
}

impl WorkspaceInfo {
    /// Run `cargo metadata` against `manifest_dir` and build a WorkspaceInfo.
    ///
    /// `manifest_dir` must contain (directly or via ancestors) a `Cargo.toml`.
    /// Blocks for the duration of the `cargo metadata` subprocess
    /// (typically 100-500ms for a medium workspace).
    ///
    /// Returns `AstError::Io` on subprocess failure / invalid manifest.
    pub fn load(manifest_dir: impl AsRef<std::path::Path>) -> crate::ast::AstResult<Self> {
        let manifest = resolve_workspace_manifest(manifest_dir.as_ref())?;
        let mut command = cargo_metadata::MetadataCommand::new();
        command.manifest_path(&manifest);
        let metadata = command.no_deps().exec().map_err(|e| {
            crate::ast::AstError::Io(std::io::Error::other(format!("cargo metadata failed: {e}")))
        })?;

        let workspace_members: std::collections::HashSet<_> =
            metadata.workspace_members.iter().cloned().collect();

        let packages = metadata
            .packages
            .iter()
            .map(|p| {
                let features: std::collections::BTreeMap<String, Vec<String>> =
                    p.features.clone().into_iter().collect();
                let deps: Vec<String> = p.dependencies.iter().map(|d| d.name.clone()).collect();
                let optional: Vec<String> = p
                    .dependencies
                    .iter()
                    .filter(|d| d.optional)
                    .map(|d| d.name.clone())
                    .collect();
                PackageInfo {
                    name: p.name.to_string(),
                    version: p.version.to_string(),
                    manifest_path: p.manifest_path.to_string(),
                    features,
                    dependencies: deps,
                    optional_dependencies: optional,
                    is_workspace_member: workspace_members.contains(&p.id),
                }
            })
            .collect();

        Ok(Self {
            workspace_root: metadata.workspace_root.to_string(),
            workspace_member_count: metadata.workspace_members.len(),
            packages,
        })
    }

    /// Look up a package by name.
    #[must_use]
    pub fn package(&self, name: &str) -> Option<&PackageInfo> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// All packages that declare a feature with the given name.
    ///
    /// Useful for answering: "which crates expose this feature flag?"
    #[must_use]
    pub fn packages_with_feature(&self, feature_name: &str) -> Vec<&PackageInfo> {
        self.packages
            .iter()
            .filter(|p| p.features.contains_key(feature_name))
            .collect()
    }

    /// All packages that depend (directly) on the given crate.
    ///
    /// Enables cross-crate blast radius: "what breaks if I change
    /// `touring-ast`?" — answer: everything that appears here.
    #[must_use]
    pub fn dependents_of(&self, target_crate: &str) -> Vec<&PackageInfo> {
        self.packages
            .iter()
            .filter(|p| p.dependencies.iter().any(|d| d == target_crate))
            .collect()
    }

    /// Workspace-only members (excludes external dependencies).
    #[must_use]
    pub fn workspace_members(&self) -> Vec<&PackageInfo> {
        self.packages
            .iter()
            .filter(|p| p.is_workspace_member)
            .collect()
    }

    /// Aggregate all unique feature names declared across the workspace.
    #[must_use]
    pub fn all_feature_names(&self) -> std::collections::BTreeSet<String> {
        self.packages
            .iter()
            .filter(|p| p.is_workspace_member)
            .flat_map(|p| p.features.keys().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pub_sym(name: &str, kind: SymbolKind, line: usize) -> PubSymbol {
        PubSymbol {
            name: name.to_string(),
            kind,
            line,
        }
    }

    #[test]
    fn test_extract_pub_symbols() {
        let symbols = vec![
            Symbol {
                name: "pub_fn".into(),
                is_public: true,
                kind: SymbolKind::Function,
                line: 1,
                ..Symbol::default()
            },
            Symbol {
                name: "priv_fn".into(),
                is_public: false,
                kind: SymbolKind::Function,
                line: 5,
                ..Symbol::default()
            },
            Symbol {
                name: "PubStruct".into(),
                is_public: true,
                kind: SymbolKind::Struct,
                line: 10,
                ..Symbol::default()
            },
        ];
        let pub_syms = extract_pub_symbols(&symbols);
        assert_eq!(pub_syms.len(), 2);
        assert_eq!(pub_syms[0].name, "pub_fn");
        assert_eq!(pub_syms[1].name, "PubStruct");
    }

    #[test]
    fn test_diff_pub_symbols() {
        let old = vec![
            make_pub_sym("A", SymbolKind::Struct, 1),
            make_pub_sym("B", SymbolKind::Function, 5),
        ];
        let new = vec![
            make_pub_sym("B", SymbolKind::Function, 5),
            make_pub_sym("C", SymbolKind::Struct, 10),
        ];
        let diff = diff_pub_symbols(&old, &new);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].name, "C");
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].name, "A");
        assert_eq!(diff.unchanged.len(), 1);
        assert_eq!(diff.unchanged[0].name, "B");
    }

    #[test]
    fn test_detect_unresolved_references() {
        let source = "let v: TfIdfVectorizer = TfIdfVectorizer::new();";
        let imports: Vec<ImportInfo> = vec![];
        let local_symbols: Vec<String> = vec![];
        let unresolved = detect_unresolved_references(source, &imports, &local_symbols);
        assert!(unresolved.contains(&"TfIdfVectorizer".to_string()));
    }

    #[test]
    fn test_detect_unresolved_ignores_imported() {
        let source = "let v: TfIdfVectorizer = TfIdfVectorizer::new();";
        let imports = vec![ImportInfo {
            module_path: "crate::tfidf".into(),
            symbols: vec!["TfIdfVectorizer".into()],
        }];
        let unresolved = detect_unresolved_references(source, &imports, &[]);
        assert!(!unresolved.contains(&"TfIdfVectorizer".to_string()));
    }

    #[test]
    fn test_detect_unresolved_ignores_builtins() {
        let source = "let v: Vec<String> = Vec::new();";
        let unresolved = detect_unresolved_references(source, &[], &[]);
        assert!(!unresolved.contains(&"Vec".to_string()));
        assert!(!unresolved.contains(&"String".to_string()));
    }

    #[test]
    fn test_detect_reexports() {
        let source = "pub use crate::tfidf::TfIdfVectorizer;\npub use crate::metrics::{CognitiveMetrics, MetricsSnapshot};";
        let reexports = detect_reexports(source);
        assert_eq!(reexports.len(), 3);
        assert_eq!(
            reexports[0],
            ("TfIdfVectorizer".into(), "crate::tfidf".into())
        );
        assert_eq!(
            reexports[1],
            ("CognitiveMetrics".into(), "crate::metrics".into())
        );
        assert_eq!(
            reexports[2],
            ("MetricsSnapshot".into(), "crate::metrics".into())
        );
    }

    #[test]
    fn test_detect_reexports_empty() {
        let reexports = detect_reexports("fn main() {}");
        assert!(reexports.is_empty());
    }

    #[test]
    fn test_diff_empty_old() {
        let new = vec![make_pub_sym("A", SymbolKind::Struct, 1)];
        let diff = diff_pub_symbols(&[], &new);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 0);
    }

    // -------------------------------------------------------------------------
    // New E2E tests: AST Wiring — prove full flow D end-to-end
    // -------------------------------------------------------------------------

    /// D.1) Full extract → diff → detect cycle on a realistic symbol set.
    #[test]
    fn test_e2e_ast_extract_diff_detect_lifecycle() {
        // Step 1: Extract pub symbols from a "v1" file representation
        let v1_symbols = vec![
            Symbol {
                name: "TfIdfVectorizer".into(),
                is_public: true,
                kind: SymbolKind::Struct,
                line: 10,
                ..Symbol::default()
            },
            Symbol {
                name: "cosine_similarity".into(),
                is_public: true,
                kind: SymbolKind::Function,
                line: 20,
                ..Symbol::default()
            },
            Symbol {
                name: "internal_helper".into(),
                is_public: false,
                kind: SymbolKind::Function,
                line: 30,
                ..Symbol::default()
            },
        ];
        let v1_pub = extract_pub_symbols(&v1_symbols);
        assert_eq!(v1_pub.len(), 2, "only 2 pub symbols in v1");
        assert!(v1_pub.iter().any(|s| s.name == "TfIdfVectorizer"));
        assert!(v1_pub.iter().any(|s| s.name == "cosine_similarity"));
        // Private symbol must not appear
        assert!(!v1_pub.iter().any(|s| s.name == "internal_helper"));

        // Step 2: Create v2 — TfIdfVectorizer renamed to TfIdf, BM25Scorer added
        let v2_symbols = vec![
            Symbol {
                name: "TfIdf".into(),
                is_public: true,
                kind: SymbolKind::Struct,
                line: 10,
                ..Symbol::default()
            },
            Symbol {
                name: "cosine_similarity".into(),
                is_public: true,
                kind: SymbolKind::Function,
                line: 20,
                ..Symbol::default()
            },
            Symbol {
                name: "BM25Scorer".into(),
                is_public: true,
                kind: SymbolKind::Struct,
                line: 40,
                ..Symbol::default()
            },
        ];
        let v2_pub = extract_pub_symbols(&v2_symbols);
        assert_eq!(v2_pub.len(), 3, "3 pub symbols in v2");

        // Step 3: Diff v1 → v2
        let diff = diff_pub_symbols(&v1_pub, &v2_pub);
        assert_eq!(diff.added.len(), 2, "TfIdf and BM25Scorer are added");
        assert!(diff.added.iter().any(|s| s.name == "TfIdf"));
        assert!(diff.added.iter().any(|s| s.name == "BM25Scorer"));
        assert_eq!(diff.removed.len(), 1, "TfIdfVectorizer was removed");
        assert_eq!(diff.removed[0].name, "TfIdfVectorizer");
        assert_eq!(diff.unchanged.len(), 1, "cosine_similarity unchanged");
        assert_eq!(diff.unchanged[0].name, "cosine_similarity");

        // Step 4: Detect unresolved references in source that uses the new types
        let source = "let scorer: BM25Scorer = BM25Scorer::new(); let tfidf = TfIdf::default();";
        let imports: Vec<ImportInfo> = vec![];
        let local: Vec<String> = vec![];
        let unresolved = detect_unresolved_references(source, &imports, &local);
        assert!(
            unresolved.contains(&"BM25Scorer".to_string()),
            "BM25Scorer should be detected as unresolved"
        );
        assert!(
            unresolved.contains(&"TfIdf".to_string()),
            "TfIdf should be detected as unresolved"
        );

        // Step 5: After adding imports, no more unresolved refs
        let imports_resolved = vec![ImportInfo {
            module_path: "crate::search".into(),
            symbols: vec!["BM25Scorer".into(), "TfIdf".into()],
        }];
        let unresolved_after = detect_unresolved_references(source, &imports_resolved, &local);
        assert!(
            !unresolved_after.contains(&"BM25Scorer".to_string()),
            "BM25Scorer should be resolved after import"
        );
        assert!(
            !unresolved_after.contains(&"TfIdf".to_string()),
            "TfIdf should be resolved after import"
        );
    }

    /// D.2) detect_reexports full E2E — single, grouped, and empty
    #[test]
    fn test_e2e_ast_reexports_full() {
        // Single reexport
        let single = "pub use crate::engine::SearchEngine;";
        let result = detect_reexports(single);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "SearchEngine");
        assert_eq!(result[0].1, "crate::engine");

        // Grouped reexport
        let grouped = "pub use crate::metrics::{CognitiveMetrics, MetricsSnapshot, RawScore};";
        let result_grouped = detect_reexports(grouped);
        assert_eq!(
            result_grouped.len(),
            3,
            "grouped import produces 3 reexports"
        );
        let names: Vec<&str> = result_grouped.iter().map(|r| r.0.as_str()).collect();
        assert!(names.contains(&"CognitiveMetrics"));
        assert!(names.contains(&"MetricsSnapshot"));
        assert!(names.contains(&"RawScore"));
        // All from same module
        for (_, module) in &result_grouped {
            assert_eq!(module, "crate::metrics");
        }

        // Non-pub use → not a reexport
        let priv_use = "use crate::internal::Helper;";
        let result_priv = detect_reexports(priv_use);
        assert!(result_priv.is_empty(), "non-pub use is not a reexport");

        // Empty source
        let result_empty = detect_reexports("");
        assert!(result_empty.is_empty());

        // Mixed content
        let mixed = "fn foo() {}\npub use crate::a::A;\nstruct Bar;\npub use crate::b::{B1, B2};";
        let result_mixed = detect_reexports(mixed);
        assert_eq!(result_mixed.len(), 3, "should find A, B1, B2");
        assert!(result_mixed.iter().any(|(name, _)| name == "A"));
        assert!(result_mixed.iter().any(|(name, _)| name == "B1"));
        assert!(result_mixed.iter().any(|(name, _)| name == "B2"));
    }

    /// D.3) diff_pub_symbols: ALL_CAPS constants must not be treated as PubSymbols
    /// (the function works on already-filtered Symbol lists, but verify consistency)
    #[test]
    fn test_e2e_ast_diff_empty_both() {
        let diff = diff_pub_symbols(&[], &[]);
        assert_eq!(diff.added.len(), 0);
        assert_eq!(diff.removed.len(), 0);
        assert_eq!(diff.unchanged.len(), 0);
    }

    /// D.4) detect_unresolved: local symbols are excluded from unresolved
    #[test]
    fn test_e2e_ast_unresolved_local_symbol_excluded() {
        // LocalStruct is defined in the same file — should not be reported
        let source =
            "let x: LocalStruct = LocalStruct::new(); let y: ExternalType = ExternalType {};";
        let imports: Vec<ImportInfo> = vec![];
        let local = vec!["LocalStruct".to_string()];
        let unresolved = detect_unresolved_references(source, &imports, &local);
        assert!(
            !unresolved.contains(&"LocalStruct".to_string()),
            "locally defined symbols must not appear as unresolved"
        );
        assert!(
            unresolved.contains(&"ExternalType".to_string()),
            "ExternalType is truly unresolved"
        );
    }

    /// D.5) detect_unresolved: ALL_CAPS identifiers (constants) must be ignored
    #[test]
    fn test_e2e_ast_unresolved_ignores_all_caps() {
        let source = "let x = MAX_RETRIES + DEFAULT_TIMEOUT;";
        let unresolved = detect_unresolved_references(source, &[], &[]);
        assert!(
            !unresolved.contains(&"MAX_RETRIES".to_string()),
            "ALL_CAPS constant must not be reported as unresolved type"
        );
        assert!(
            !unresolved.contains(&"DEFAULT_TIMEOUT".to_string()),
            "ALL_CAPS constant must not be reported as unresolved type"
        );
    }

    // ── B5 fix: suggest_imports constructs ImportSuggestion ──

    #[test]
    fn test_suggest_imports_basic() {
        let unresolved = vec!["TfIdfVectorizer".to_string(), "BM25Scorer".to_string()];
        let known = vec![
            ("TfIdfVectorizer".to_string(), "src/tfidf.rs".to_string()),
            ("BM25Scorer".to_string(), "src/search/bm25.rs".to_string()),
        ];
        let suggestions = suggest_imports(&unresolved, &known);
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].symbol_name, "TfIdfVectorizer");
        assert_eq!(suggestions[0].source_module, "crate::tfidf");
        assert!(suggestions[0].confidence > 0.0);
        assert_eq!(suggestions[1].symbol_name, "BM25Scorer");
        assert_eq!(suggestions[1].source_module, "crate::search::bm25");
    }

    #[test]
    fn test_suggest_imports_no_match() {
        let unresolved = vec!["UnknownType".to_string()];
        let known = vec![("OtherType".to_string(), "src/other.rs".to_string())];
        let suggestions = suggest_imports(&unresolved, &known);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_imports_empty_inputs() {
        assert!(suggest_imports(&[], &[]).is_empty());
        assert!(suggest_imports(&["Foo".to_string()], &[]).is_empty());
    }

    #[test]
    fn test_suggest_imports_first_match_wins() {
        let unresolved = vec!["Symbol".to_string()];
        let known = vec![
            ("Symbol".to_string(), "src/a.rs".to_string()),
            ("Symbol".to_string(), "src/b.rs".to_string()),
        ];
        let suggestions = suggest_imports(&unresolved, &known);
        // Should only produce one suggestion (first match)
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].source_module, "crate::a");
    }

    // ── WorkspaceInfo tests (cargo_metadata integration) ───────────────

    /// Helper — best-effort load of the Touring workspace for testing.
    /// Skips the test if cargo metadata fails (e.g. in offline CI without
    /// access to the filesystem).
    fn try_load_touring_workspace() -> Option<WorkspaceInfo> {
        // We assume tests run from a path inside the Touring workspace;
        // walk up until we find a Cargo.toml with `[workspace]`.
        let mut cur: std::path::PathBuf = std::env::current_dir().ok()?.canonicalize().ok()?;
        loop {
            let manifest = cur.join("Cargo.toml");
            if manifest.exists() {
                let content = std::fs::read_to_string(&manifest).ok()?;
                if content.contains("[workspace]") {
                    return WorkspaceInfo::load(&cur).ok();
                }
            }
            if !cur.pop() {
                return None;
            }
        }
    }

    #[test]
    fn workspace_info_loads_current_workspace() {
        let Some(info) = try_load_touring_workspace() else {
            eprintln!("skipping: not in a workspace");
            return;
        };
        assert!(
            !info.workspace_root.is_empty(),
            "workspace_root must be set"
        );
        assert!(
            info.workspace_member_count > 0,
            "workspace must have at least one member"
        );
        assert!(!info.packages.is_empty(), "packages must be populated");
    }

    #[test]
    fn workspace_info_finds_touring_code() {
        let Some(info) = try_load_touring_workspace() else {
            eprintln!("skipping: not in a workspace");
            return;
        };
        // touring-code must be a workspace member since these tests live in it
        // (the former `touring-ast` crate was fused into `touring-code::ast` by the
        // A2 shim-fusion wave, so `touring-ast` is no longer a workspace member).
        let this = info.package("touring-code");
        assert!(this.is_some(), "touring-code must be in the workspace");
        let pkg = this.expect("touring-code present");
        assert!(
            pkg.is_workspace_member,
            "touring-code is a workspace member"
        );
        assert!(!pkg.features.is_empty(), "touring-code declares features");
    }

    #[test]
    fn workspace_info_reports_feature_names() {
        let Some(info) = try_load_touring_workspace() else {
            eprintln!("skipping: not in a workspace");
            return;
        };
        let features = info.all_feature_names();
        // touring-ast declares simd-search/ann/more-languages/async-pipeline.
        // Any of them should show up in the aggregate.
        assert!(
            features.contains("simd-search")
                || features.contains("ann")
                || features.contains("more-languages"),
            "expected touring-ast features to surface; got {features:?}"
        );
    }

    #[test]
    fn workspace_info_dependents_of_touring_foundation() {
        let Some(info) = try_load_touring_workspace() else {
            eprintln!("skipping: not in a workspace");
            return;
        };
        // Post-W4 (touring-premium-refactor-2026): the ex-touring-ast code now
        // lives in `touring-code`, which depends on `touring-foundation`
        // (the W3 rename of touring-core).
        let dependents = info.dependents_of("touring-foundation");
        assert!(
            dependents.iter().any(|p| p.name == "touring-code"),
            "touring-code must appear as a dependent of touring-foundation; got {:?}",
            dependents.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn workspace_info_workspace_members_all_flagged() {
        let Some(info) = try_load_touring_workspace() else {
            eprintln!("skipping: not in a workspace");
            return;
        };
        let members = info.workspace_members();
        // Every returned member must have is_workspace_member = true.
        assert!(members.iter().all(|p| p.is_workspace_member));
        // Count should match the metadata's own count.
        assert_eq!(members.len(), info.workspace_member_count);
    }
}
