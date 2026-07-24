//! Fast, dependency-free symbol/import extractors and import-path resolution.
//!
//! Wave R+C I2 (2026-06-10): extracted from the dispatch layer's
//! `hooks/post_read.rs` — these are pure code-analysis engines (zero
//! HookRuntime, zero I/O) consumed by post_read, `shared::reindex` and the
//! cli index handlers. `post_read` re-exports them, so every
//! `crate::post_read::extract_*` / `resolve_import_path*` path is unchanged.

use once_cell::sync::Lazy;
use regex::Regex;

static PYTHON_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:from\s+(\S+)\s+import|import\s+(\S+))").expect("static regex")
});

static RUST_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^use\s+([\w:]+)").expect("static regex"));

static TS_JS_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)(?:from\s+['"]([^'"]+)['"]|require\s*\(\s*['"]([^'"]+)['"])"#)
        .expect("static regex")
});

static PYTHON_SYMBOL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^(?:class|def|async\s+def)\s+(\w+)").expect("static regex"));

static RUST_SYMBOL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:pub\s+)?(?:fn|struct|enum|trait|impl|type|const|static|mod)\s+(\w+)")
        .expect("static regex")
});

static TS_JS_SYMBOL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:export\s+)?(?:function|class|interface|type|enum|const|let|var)\s+(\w+)")
        .expect("static regex")
});

/// Mapping from touring crate names to their source directory paths.
/// Used for cross-crate import resolution.
static TOURING_CRATE_MAP: &[(&str, &str)] = &[
    ("touring_analysis", "crates/touring-analysis/src"),
    ("touring_ast", "crates/touring-ast/src"),
    ("touring_cortex", "crates/touring-cortex/src"),
    ("touring_hooks", "crates/touring-hooks/src"),
    ("touring_index", "crates/touring-index/src"),
    ("touring_learning", "crates/touring-learning/src"),
    ("touring_python", "crates/touring-python/src"),
    ("touring_server", "crates/touring-server/src"),
    ("touring_simd", "crates/touring-simd/src"),
    ("touring_foundation", "crates/touring-core/src"),
    ("touring_wasm", "crates/touring-wasm/src"),
    (
        "touring_integration_tests",
        "crates/touring-integration-tests/src",
    ),
    // Aliases without touring_ prefix for ergonomics
    ("analysis", "crates/touring-analysis/src"),
    ("ast", "crates/touring-ast/src"),
    ("cortex", "crates/touring-cortex/src"),
    ("hooks", "crates/touring-hooks/src"),
    ("index", "crates/touring-index/src"),
    ("learning", "crates/touring-learning/src"),
    ("python", "crates/touring-python/src"),
    ("server", "crates/touring-server/src"),
    ("simd", "crates/touring-simd/src"),
    ("core", "crates/touring-core/src"),
    ("wasm", "crates/touring-wasm/src"),
    ("integration_tests", "crates/touring-integration-tests/src"),
];

/// Extract imports via fast regex (fallback for non-AST languages).
pub fn extract_imports_fast(content: &str, language: &str) -> Vec<String> {
    // Only process first 500 lines (imports are at the top)
    let head: String = content.lines().take(500).collect::<Vec<_>>().join("\n");

    match language {
        "python" => PYTHON_IMPORT_RE
            .captures_iter(&head)
            .filter_map(|c| {
                c.get(1)
                    .or(c.get(2))
                    .map(|m| m.as_str().trim_end_matches(',').to_string())
            })
            .collect(),
        "rust" => RUST_IMPORT_RE
            .captures_iter(&head)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect(),
        "typescript" | "javascript" => TS_JS_IMPORT_RE
            .captures_iter(&head)
            .filter_map(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Extract top-level symbol names via fast regex (fallback for non-AST languages).
pub fn extract_symbols_fast(content: &str, language: &str) -> Vec<String> {
    match language {
        "python" => PYTHON_SYMBOL_RE
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect(),
        "rust" => RUST_SYMBOL_RE
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect(),
        "typescript" | "javascript" => TS_JS_SYMBOL_RE
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Attempt to resolve an import string to a project file path.
pub fn resolve_import_path(import: &str, language: &str) -> Option<String> {
    resolve_import_path_with_source(import, language, None)
}

/// Extract the crate src root from a source file path.
/// "crates/touring-hooks/src/foo/bar.rs" → "crates/touring-hooks/src"
/// "crates/touring-server/src/server/main.rs" → "crates/touring-server/src"
fn detect_crate_src_root(source_file: &str) -> Option<String> {
    if let Some(crates_pos) = source_file.find("crates/") {
        if let Some(src_pos) = source_file[crates_pos..].find("/src") {
            let end = crates_pos + src_pos + 4; // include "/src"
            return Some(source_file[..end].to_string());
        }
    }
    None
}

/// Lexically normalize a path — collapse `.` components and resolve `..`
/// WITHOUT touching the filesystem (no symlink resolution). Used by the TS/JS
/// resolver so a specifier like `./models` joined onto `src/app.ts` yields the
/// clean `src/models` rather than `src/./models` — the latter would key a
/// consumer row under a path that never JOINs the producer row (`src/models.ts`),
/// the path-homonimia bug class that produced phantom nodes historically.
fn normalize_lexical(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolve an import string, optionally using the source file path to resolve
/// `crate::` imports relative to the correct workspace crate.
pub fn resolve_import_path_with_source(
    import: &str,
    language: &str,
    source_file: Option<&str>,
) -> Option<String> {
    match language {
        "python" => {
            // Convert dot-notation to path: "packages.kazuba_core.models" → "packages/kazuba_core/models.py"
            let path = import.replace('.', "/");
            Some(format!("{path}.py"))
        }
        "rust" => {
            // ─── Rust scope-keyword guard (regression: phantom super.rs) ───
            // `use super::*`, `use self::Foo`, etc. resolve relative to the
            // module hierarchy, not to literal files. Without proper hierarchy
            // analysis (which this resolver does not do), keyword imports
            // cannot be mapped to a real path. The previous fallback below
            // (`import.replace("crate::", "src/").replace("::", "/")`) silently
            // turned bare `"super"` into the phantom file `"super.rs"`, which
            // then surfaced in /api/viz/workspace as 7 pseudo-nodes with 708
            // outgoing edges and 0 incoming (the "vortex" signature of a
            // resolver bug). Returning None here makes the consumer-recording
            // step skip these imports gracefully.
            //
            // Note: `crate::*` is intentionally NOT in this guard because the
            // existing fallback to `src/<rest>.rs` is a reasonable
            // project-root-relative resolution (legacy behaviour preserved).
            const RUST_SCOPE_KEYWORDS: &[&str] = &["super", "self", "Self"];
            if RUST_SCOPE_KEYWORDS.contains(&import) {
                return None;
            }
            for kw in RUST_SCOPE_KEYWORDS {
                let prefix = format!("{kw}::");
                if import.starts_with(&prefix) {
                    // With source_file context the branch below resolves these
                    // via crate_src_root; without context we cannot.
                    source_file?;
                    break;
                }
            }
            // Filesystem-aware module resolution helper.
            //
            // Rust supports two physical layouts for `mod foo`:
            //   1. file-style:      `<dir>/foo.rs`
            //   2. directory-style: `<dir>/foo/mod.rs`
            //
            // Without checking the filesystem, the resolver previously assumed
            // file-style and emitted phantom nodes whenever the real layout was
            // directory-style (~200 phantoms in the touring workspace, e.g.
            // `crates/touring-analysis/src/blast_radius.rs` → real file is
            // `crates/touring-analysis/src/blast_radius/mod.rs`).
            //
            // This helper tries both layouts and returns the first that exists,
            // or `None` if neither does. The `None` case is the correct outcome
            // for build-script generated modules whose source lives in `OUT_DIR`
            // (e.g. `holon_core_capnp.rs` produced by `capnpc` from a `.capnp`
            // schema) — those modules have no physical file in `src/`, so they
            // legitimately have no resolvable target in the project tree.
            //
            // When called with a relative `dir` (cross-crate workspace map,
            // e.g. "crates/touring-analysis/src"), existence is checked
            // relative to the current working directory, which equals the
            // workspace root in production daemon runs.
            fn find_workspace_root() -> Option<String> {
                // Walk up from current directory looking for a Cargo.toml with [workspace]
                let mut dir = std::env::current_dir().ok()?;
                loop {
                    let toml_path = dir.join("Cargo.toml");
                    if toml_path.is_file() {
                        if let Ok(content) = std::fs::read_to_string(&toml_path) {
                            if content.contains("[workspace]") {
                                return Some(dir.display().to_string());
                            }
                        }
                    }
                    if !dir.pop() {
                        break;
                    }
                }
                None
            }

            fn resolve_module_layout(dir: &str, rel_path: &str) -> Option<String> {
                // Resolve dir relative to workspace root, not cwd
                let ws_root = find_workspace_root().unwrap_or_default();
                let abs_dir = if std::path::Path::new(dir).is_absolute() {
                    dir.to_string()
                } else if !ws_root.is_empty() {
                    format!("{}/{}", ws_root, dir)
                } else {
                    dir.to_string()
                };
                let file_style = format!("{abs_dir}/{rel_path}.rs");
                if std::path::Path::new(&file_style).exists() {
                    // Return workspace-relative path for backwards compatibility
                    let rel = format!("{}/{}", dir, rel_path);
                    return Some(format!("{}.rs", rel));
                }
                let dir_style = format!("{abs_dir}/{rel_path}/mod.rs");
                if std::path::Path::new(&dir_style).exists() {
                    return Some(format!("{}/{}/mod.rs", dir, rel_path));
                }
                None
            }

            // First, check for cross-crate imports (e.g., touring_analysis::pipeline::Builder)
            for (crate_name, crate_path) in TOURING_CRATE_MAP {
                if let Some(rest) = import.strip_prefix(&format!("{}::", crate_name)) {
                    // cross-crate: touring_analysis::pipeline::Builder → crates/touring-analysis/src/pipeline.rs
                    // rest = "pipeline::Builder" — rsplit_once gives ("pipeline", "Builder")
                    let (module_path, _symbol) = rest.rsplit_once("::").unwrap_or((rest, ""));
                    let rel = module_path.replace("::", "/");
                    return resolve_module_layout(crate_path, &rel);
                }
            }
            // Resolve crate-relative imports using the source file's crate root.
            // "crate::foo::Bar" from "crates/touring-hooks/src/lib.rs"
            //   → "crates/touring-hooks/src/foo/Bar.rs"
            // Helper: reject any candidate whose final path segment is a
            // Rust scope keyword. Catches phantom variants that bypass the
            // input-side guard above (e.g. `import = "crate::super"` → bug
            // path `crates/X/src/super.rs`, `import = "super::super"` → ditto).
            fn is_keyword_filename(p: &str) -> bool {
                std::path::Path::new(p)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|stem| matches!(stem, "super" | "self" | "Self"))
            }

            if let Some(src) = source_file {
                if import.starts_with("crate::") || import.starts_with("super::") {
                    if let Some(crate_src_root) = detect_crate_src_root(src) {
                        let relative = import
                            .strip_prefix("crate::")
                            .or_else(|| import.strip_prefix("super::"))
                            .unwrap_or(import);
                        let rel = relative.replace("::", "/");
                        // Try file-style and directory-style layouts; either
                        // None (neither exists, e.g. OUT_DIR-generated module)
                        // or filename keyword sentinel rejects the candidate.
                        if let Some(candidate) = resolve_module_layout(&crate_src_root, &rel) {
                            if is_keyword_filename(&candidate) {
                                return None;
                            }
                            return Some(candidate);
                        }
                        return None;
                    }
                }
            }
            // External-import guard (regression: phantom std/serde/tokio/etc.):
            // If the import didn't match a workspace crate or `crate::` prefix,
            // it is an external dependency (`std::*`, `tokio::*`, `serde::*`,
            // `tempfile::*`, `anyhow::*`, third-party crate, …). The naive
            // fallback below previously turned `"std::collections"` into the
            // phantom `"std/collections.rs"`, `"tempfile"` into `"tempfile.rs"`,
            // `"serde"` into `"serde.rs"`, etc. Those paths do not exist in
            // the workspace and surfaced as phantom nodes in viz workspace.
            // External dependencies have no project file to resolve to.
            if !import.starts_with("crate::") {
                return None;
            }
            // Fallback for crate-relative paths when no source file given.
            // "crate::hooks::classifier" → "src/hooks/classifier.rs"
            let path = import.replace("crate::", "src/").replace("::", "/");
            let candidate = format!("{path}.rs");
            if is_keyword_filename(&candidate) {
                return None;
            }
            Some(candidate)
        }
        "typescript" | "javascript" => {
            // Bare specifiers ("react", "@scope/pkg") are external packages
            // (node_modules), not first-party project files — no producer row
            // to wire to.
            if !import.starts_with('.') {
                return None;
            }
            // Resolve the relative specifier against the importing file's
            // directory, then apply Node/TS module resolution: an explicit file,
            // else each source extension, else a directory `index.<ext>`.
            // Filesystem probing mirrors the Rust arm; the returned path is
            // canonicalized to workspace-relative form by `record_consumer`, so
            // it JOINs the producer row keyed by the same file.
            let base_dir = std::path::Path::new(source_file?).parent()?;
            let joined = normalize_lexical(&base_dir.join(import));
            // Specifier already carries an extension (`import "./x.js"`).
            if joined.is_file() {
                return Some(joined.to_string_lossy().into_owned());
            }
            const TS_JS_EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs"];
            for ext in TS_JS_EXTS {
                let candidate = std::path::PathBuf::from(format!("{}.{ext}", joined.display()));
                if candidate.is_file() {
                    return Some(candidate.to_string_lossy().into_owned());
                }
            }
            for ext in TS_JS_EXTS {
                let candidate = joined.join(format!("index.{ext}"));
                if candidate.is_file() {
                    return Some(candidate.to_string_lossy().into_owned());
                }
            }
            None
        }
        "java" => {
            // `import com.foo.Bar;` → `com/foo/Bar.java`. Java is file-based
            // (one public type per file), so the fully-qualified name maps
            // directly to a path — the same pure dotted→path scheme as the
            // Python arm (no filesystem probe). It matches when the Java source
            // root is the workspace root; nested Maven/Gradle `src/main/java/`
            // roots are a known limitation shared with Python (external imports
            // like `java.util.List` resolve to a no-producer row, marked
            // `extern` by the backfill pass). docs/2026-07-03-polyglot-parity-plan.md §6.
            Some(format!("{}.java", import.replace('.', "/")))
        }
        "go" => {
            // A Go import path denotes a PACKAGE (a directory of files), not a
            // single source file, and carries no symbol — usage is `pkg.Foo()`,
            // wired via method-dispatch (`find_producer_modules_for_methods`),
            // not import resolution. File-keyed resolution is a semantic
            // mismatch, so it is intentionally None here; a package-aware wiring
            // model is deferred (docs/2026-07-03-polyglot-parity-plan.md §6).
            // Extraction is still wired (go_imports.scm) for dependency listing.
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod ts_js_resolver_tests {
    use super::resolve_import_path_with_source;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn typescript_relative_import_resolves_to_file_with_extension() {
        let tmp = TempDir::new().expect("tempdir");
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).expect("mkdir src");
        fs::write(src.join("models.ts"), "export class User {}").expect("write models.ts");
        let app = src.join("app.ts");
        fs::write(&app, "import { User } from './models';").expect("write app.ts");

        let resolved = resolve_import_path_with_source(
            "./models",
            "typescript",
            Some(app.to_str().expect("utf8")),
        );
        assert_eq!(
            resolved.as_deref(),
            Some(src.join("models.ts").to_string_lossy().as_ref()),
            "relative TS import must resolve to the .ts file (extension probed)"
        );
    }

    #[test]
    fn javascript_directory_index_resolution() {
        let tmp = TempDir::new().expect("tempdir");
        let widgets = tmp.path().join("lib").join("widgets");
        fs::create_dir_all(&widgets).expect("mkdir widgets");
        fs::write(widgets.join("index.js"), "module.exports = {};").expect("write index.js");
        let app = tmp.path().join("lib").join("main.js");
        fs::write(&app, "const w = require('./widgets');").expect("write main.js");

        let resolved = resolve_import_path_with_source(
            "./widgets",
            "javascript",
            Some(app.to_str().expect("utf8")),
        );
        assert_eq!(
            resolved.as_deref(),
            Some(widgets.join("index.js").to_string_lossy().as_ref()),
            "directory import must resolve to index.js"
        );
    }

    #[test]
    fn external_package_specifier_is_not_a_project_file() {
        assert_eq!(
            resolve_import_path_with_source("react", "typescript", Some("src/app.ts")),
            None,
            "bare specifiers are external packages, not first-party files"
        );
        assert_eq!(
            resolve_import_path_with_source("@scope/pkg", "javascript", Some("src/app.js")),
            None
        );
    }

    #[test]
    fn java_import_maps_dotted_name_to_source_path() {
        assert_eq!(
            resolve_import_path_with_source("com.foo.Bar", "java", None).as_deref(),
            Some("com/foo/Bar.java"),
            "Java FQN maps directly to a source path (pure dotted→path)"
        );
    }

    #[test]
    fn go_import_is_not_file_resolvable() {
        // A Go import path denotes a package (directory), not a single file —
        // wiring flows via method-dispatch, not import resolution.
        assert_eq!(
            resolve_import_path_with_source("mymod/internal/pkg", "go", None),
            None
        );
    }

    #[test]
    fn parent_dir_specifier_is_lexically_normalized() {
        let tmp = TempDir::new().expect("tempdir");
        let shared = tmp.path().join("shared");
        fs::create_dir_all(&shared).expect("mkdir shared");
        fs::write(shared.join("types.ts"), "export type T = number;").expect("write types.ts");
        let feature = tmp.path().join("feature");
        fs::create_dir_all(&feature).expect("mkdir feature");
        let comp = feature.join("comp.ts");
        fs::write(&comp, "import { T } from '../shared/types';").expect("write comp.ts");

        let resolved = resolve_import_path_with_source(
            "../shared/types",
            "typescript",
            Some(comp.to_str().expect("utf8")),
        );
        let expected = shared.join("types.ts");
        assert_eq!(resolved.as_deref(), Some(expected.to_string_lossy().as_ref()));
        // Homonimia guard: the resolved path must carry no `.`/`..` segments,
        // else the consumer row would never JOIN the producer row.
        let s = resolved.expect("resolved");
        assert!(
            !s.contains("/./") && !s.contains("/../"),
            "path must be lexically normal: {s}"
        );
    }
}
