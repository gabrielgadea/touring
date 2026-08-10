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

/// `pub use <crate>::<…>;` — the facade re-export form that
/// [`resolve_reexport`] follows back to the crate that really owns the module.
static REEXPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*pub\s+use\s+([A-Za-z_][A-Za-z0-9_]*)::([^;]+);").expect("static regex")
});

/// Mapping from touring crate names (underscored, as written in a `use`) to
/// their source directory paths, used for cross-crate import resolution.
///
/// **Derived from the workspace, never hand-maintained.** It used to be a
/// literal list, and it rotted exactly the way a hand-maintained mirror of the
/// filesystem does: of its 11 entries, 5 pointed at directories that no longer
/// exist (`touring-ast`, `touring-index`, `touring-learning`, `touring-core`,
/// `touring-wasm` — all since renamed or merged) and it named only 6 of the
/// workspace's 41 live crates. Every import of an unmapped crate resolved to
/// `None`, so [`record_consumer`] was never called and the producer row kept
/// `consumer_file IS NULL` — i.e. the symbol was reported as an orphan while
/// having real consumers. Measured 2026-08-07: 5031 orphans over 12052
/// producers (42%), and 301 of 1711 distinct `module_file` values pointing at
/// files absent from disk.
///
/// Deriving the map makes that drift class unrepresentable: a renamed crate is
/// picked up on the next process start, and a deleted one disappears.
///
/// [`record_consumer`]: https://docs.rs/touring-storage — `KnowledgeStore::record_consumer`
static TOURING_CRATE_MAP: Lazy<Vec<(String, String)>> = Lazy::new(build_crate_map);

/// Names that must never become a bare alias: they are real crates in the
/// language or the build graph, so aliasing them would resolve `use core::mem`
/// into some workspace file and fabricate an edge.
const ALIAS_DENY: &[&str] = &["core", "std", "alloc", "test", "proc_macro", "macros"];

/// Walk `<workspace>/crates/*/Cargo.toml` and pair each package name with its
/// `src` root, plus the `touring_`-less bare alias the literal map offered for
/// ergonomics (`touring_analysis` ⇒ also `analysis`). The alias set GREW with
/// this change — 11 hand-listed entries became every live crate — except for
/// [`ALIAS_DENY`], which drops `core`: the old map aliased it to
/// `crates/touring-core/src`, so a plain `use core::…` was one existing file
/// away from being wired into an unrelated crate.
fn build_crate_map() -> Vec<(String, String)> {
    let Some(root) = find_workspace_root() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(std::path::Path::new(&root).join("crates")) else {
        return Vec::new();
    };
    let mut map: Vec<(String, String)> = entries
        .flatten()
        .filter(|e| e.path().join("src").is_dir())
        .filter_map(|e| {
            let manifest = std::fs::read_to_string(e.path().join("Cargo.toml")).ok()?;
            let name = package_name(&manifest)?;
            let dir = e.path().file_name()?.to_str()?.to_string();
            Some((name.replace('-', "_"), format!("crates/{dir}/src")))
        })
        .flat_map(|(name, src)| {
            let alias = name
                .strip_prefix("touring_")
                .filter(|a| !ALIAS_DENY.contains(a))
                .map(|a| (a.to_string(), src.clone()));
            std::iter::once((name, src)).chain(alias)
        })
        .collect();
    // Longest name first so `touring_hooks_core::x` is never claimed by the
    // `touring_hooks` entry. The `::` in the prefix test already prevents that;
    // the ordering makes the invariant hold independently of that detail.
    map.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    map
}

/// The `name` field of a manifest's `[package]` table.
///
/// Scoped to that table on purpose: `[[bin]]`, `[lib]` and `[dependencies]`
/// entries also carry a `name`, and taking the first one in the file would key
/// the crate under a binary's name.
fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package && let Some(rest) = line.strip_prefix("name") {
            return rest
                .trim_start()
                .strip_prefix('=')
                .map(|v| v.trim().trim_matches('"').to_string())
                .filter(|v| !v.is_empty());
        }
    }
    None
}

/// The workspace root — the nearest ancestor whose `Cargo.toml` declares
/// `[workspace]`.
///
/// Cached: the root cannot change while the process lives, and this is called
/// once per module resolution — i.e. several times per import, for every file of
/// a rebuild. Uncached it re-ran `current_dir()` plus a walk-up that reads every
/// `Cargo.toml` on the way, which is pure syscall churn on a 3117-file index.
fn find_workspace_root() -> Option<String> {
    static ROOT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        // Walk up from current directory looking for a Cargo.toml with [workspace]
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let toml_path = dir.join("Cargo.toml");
            if toml_path.is_file()
                && let Ok(content) = std::fs::read_to_string(&toml_path)
                && content.contains("[workspace]")
            {
                return Some(dir.display().to_string());
            }
            if !dir.pop() {
                break;
            }
        }
        None
    })
    .clone()
}

/// Resolve `<dir>/<rel_path>` against both physical layouts Rust allows for a
/// module — `foo.rs` and `foo/mod.rs` — returning the workspace-relative path
/// of whichever exists.
///
/// `None` is a correct answer, not a failure: build-script modules generated
/// into `OUT_DIR` (e.g. `holon_core_capnp.rs` from a `.capnp` schema) have no
/// file under `src/`. Probing the filesystem instead of assuming file-style is
/// what keeps ~200 phantom paths out of the graph.
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

/// Follow a facade crate's `pub use <other_crate>::<module>;` back to the crate
/// that physically owns the module.
///
/// `use touring_hooks::tantivy_index::TantivyIndex` cannot resolve inside
/// `crates/touring-hooks/src/` — no such file exists there. `touring-hooks`
/// only re-exports it (`pub use touring_hooks_core::tantivy_index;`), and the
/// real file is `crates/touring-hooks-core/src/tantivy_index.rs`. Without
/// following the re-export the consumer edge is dropped and that producer row
/// keeps `consumer_file IS NULL`, i.e. `TantivyIndex` is reported an orphan
/// while 39 files reference it. Five crates re-export that one module.
///
/// The resolved target is still filesystem-probed by [`resolve_module_layout`],
/// so a mis-read `pub use` yields `None` rather than a phantom path.
fn resolve_reexport(facade_src_root: &str, rel: &str, depth: u8) -> Option<String> {
    // Facade chains are shallow (crate → core). The cap makes a `pub use` cycle
    // between two crates terminate instead of recursing until the stack dies.
    const MAX_DEPTH: u8 = 4;
    if depth >= MAX_DEPTH {
        return None;
    }
    let ws_root = find_workspace_root().unwrap_or_default();
    let abs = |p: &str| {
        if ws_root.is_empty() {
            p.to_string()
        } else {
            format!("{ws_root}/{p}")
        }
    };

    // Walk the path progressively, longest real prefix first.
    //
    // Until 2026-08-08 this only read the crate's `lib.rs` and only matched the
    // FIRST segment — so it found `crate::tantivy_index::X` (re-exported at the
    // root) and missed `crate::shared::feature_flags::X`, whose `pub use` lives
    // in `src/shared/mod.rs`, one level down. A re-export can be declared in
    // ANY module file, at any depth, so the search has to follow the path
    // instead of assuming the root.
    //
    // Longest prefix first because the deepest real module is the most
    // specific place the next segment could be re-exported from; falling back
    // to shorter prefixes preserves the old root-level behaviour as the i == 0
    // case (prefix empty → `lib.rs`).
    let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    for split in (0..parts.len()).rev() {
        let (prefix, next) = (&parts[..split], *parts.get(split)?);
        let holder = if prefix.is_empty() {
            format!("{facade_src_root}/lib.rs")
        } else {
            match resolve_module_layout(facade_src_root, &prefix.join("/")) {
                Some(f) => f,
                None => continue,
            }
        };
        let Ok(content) = std::fs::read_to_string(abs(&holder)) else {
            continue;
        };
        // What remains to resolve inside the origin crate: the re-exported
        // segment plus everything after it.
        let remaining = parts.get(split..).map(|t| t.join("/")).unwrap_or_default();
        for origin in reexport_origins(&content, next) {
            let Some((_, target_root)) = TOURING_CRATE_MAP.iter().find(|(n, _)| *n == origin)
            else {
                continue;
            };
            if let Some(hit) = resolve_module_layout(target_root, &remaining)
                .or_else(|| resolve_reexport(target_root, &remaining, depth + 1))
            {
                return Some(hit);
            }
        }
    }
    None
}

/// Crate names this file re-exports `module` from, in declaration order.
///
/// Matches both the named form (`pub use c::m;`, `pub use c::{m, n};`,
/// `pub use c::m::Item;`) and the glob form (`pub use c::*;`) — under a glob
/// any module of `c` is in scope, so `c` is a candidate the caller then probes.
fn reexport_origins(content: &str, module: &str) -> Vec<String> {
    REEXPORT_RE
        .captures_iter(content)
        .filter_map(|c| {
            let origin = c.get(1)?.as_str();
            let tail = c.get(2)?.as_str();
            let names = tail.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'));
            let glob = tail
                .trim_end_matches(|ch: char| ch.is_whitespace())
                .ends_with('*');
            (glob || names.into_iter().any(|n| n == module)).then(|| origin.to_string())
        })
        .collect()
}

/// Item definitions — one compiled pass instead of a regex per symbol.
static ITEM_DEF_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:async\s+)?(?:unsafe\s+)?(?:fn|struct|enum|trait|type|union|const|static|mod|macro_rules!)\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .expect("static regex")
});

/// What a re-export hop needs to know about one module file.
///
/// Scanned once and cached: [`definer_module`] is called per (module, symbol)
/// pair while a rebuild walks the workspace — 78.143 wiring rows on 08/08/2026 —
/// so re-reading and re-scanning the same `mod.rs` for every symbol it
/// re-exports is the difference between a bounded cost and a per-edge one.
struct ModuleFacts {
    defined: std::collections::HashSet<Box<str>>,
    reexports: Vec<(Box<str>, Box<str>)>,
}

impl ModuleFacts {
    fn parse(content: &str) -> Self {
        Self {
            defined: ITEM_DEF_RE
                .captures_iter(content)
                .filter_map(|c| Some(Box::from(c.get(1)?.as_str())))
                .collect(),
            reexports: REEXPORT_RE
                .captures_iter(content)
                .filter_map(|c| Some((Box::from(c.get(1)?.as_str()), Box::from(c.get(2)?.as_str()))))
                .collect(),
        }
    }
}

/// Facts per absolute path, revalidated by mtime.
///
/// `mtime` is what keeps this honest: a daemon lives across edits, and a cache
/// that never expires would keep asserting yesterday's module layout — the same
/// staleness class the hook's portfolio cache hit on 08/08/2026 (finding F4).
/// Bounded by [`FACTS_CACHE_CAP`]: past it the map is dropped whole, because a
/// cold re-read costs one `read_to_string` and unbounded growth costs a daemon.
type CachedFacts = (Option<std::time::SystemTime>, std::sync::Arc<ModuleFacts>);

static MODULE_FACTS: Lazy<std::sync::Mutex<std::collections::HashMap<String, CachedFacts>>> =
    Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

const FACTS_CACHE_CAP: usize = 8192;

fn module_facts(abs_path: &str) -> Option<std::sync::Arc<ModuleFacts>> {
    let mtime = std::fs::metadata(abs_path)
        .ok()
        .and_then(|m| m.modified().ok());
    if let Ok(cache) = MODULE_FACTS.lock()
        && let Some((cached_mtime, facts)) = cache.get(abs_path)
        && *cached_mtime == mtime
    {
        return Some(std::sync::Arc::clone(facts));
    }
    let facts = std::sync::Arc::new(ModuleFacts::parse(&std::fs::read_to_string(abs_path).ok()?));
    if let Ok(mut cache) = MODULE_FACTS.lock() {
        if cache.len() >= FACTS_CACHE_CAP {
            cache.clear();
        }
        cache.insert(abs_path.to_string(), (mtime, std::sync::Arc::clone(&facts)));
    }
    Some(facts)
}

/// Does this file DEFINE `symbol` (as opposed to re-exporting it)?
///
/// Covers item kinds rather than parsing Rust: a false negative only means the
/// re-export chain is followed one hop further, which terminates at the depth
/// cap, so the failure mode is "no improvement", never a wrong attribution.
///
/// Production reads the same answer off the cached [`ModuleFacts`]; this states
/// the contract at content level, which is what the tests assert against. Pure
/// delegation, so the two can never drift apart.
#[cfg(test)]
fn defines_symbol(content: &str, symbol: &str) -> bool {
    ModuleFacts::parse(content).defined.contains(symbol)
}

/// Module path of the intra-crate `pub use` that re-exports `symbol`, if any.
///
/// `pub use hybrid::pipeline::{A, Symbol};` → `Some("hybrid/pipeline")`
/// `pub use hybrid::Symbol;`               → `Some("hybrid")`
/// `pub use hybrid::*;`                    → `Some("hybrid")` (probe candidate)
/// Pure delegation to [`reexport_path_from`] — see [`defines_symbol`].
#[cfg(test)]
fn intra_crate_reexport_path(content: &str, symbol: &str) -> Option<String> {
    reexport_path_from(&ModuleFacts::parse(content), symbol)
}

/// The module tail of one `pub use` clause, if it carries `symbol`.
fn reexport_tail_for(tail: &str, symbol: &str) -> Option<String> {
    let is_glob = tail.trim_end().ends_with('*');
    let mut names = tail.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'));
    if !is_glob && !names.any(|n| n == symbol) {
        return None;
    }
    // Everything before the braced list, or before the final `::Item`.
    if let Some((before, _)) = tail.split_once("::{") {
        Some(before.to_string())
    } else if let Some((before, _)) = tail.rsplit_once("::") {
        Some(before.to_string())
    } else if tail.trim().starts_with('{') || is_glob || tail.trim() == symbol {
        Some(String::new())
    } else {
        None
    }
}

fn reexport_path_from(facts: &ModuleFacts, symbol: &str) -> Option<String> {
    facts.reexports.iter().find_map(|(head, tail)| {
        let module_tail = reexport_tail_for(tail, symbol)?;
        let joined = if module_tail.is_empty() {
            head.to_string()
        } else {
            format!("{head}::{module_tail}")
        };
        Some(joined.replace("::", "/").trim_matches('/').to_string())
    })
}

/// Follow intra-crate `pub use` chains from `module_file` to the file that
/// actually defines `symbol`.
///
/// Returns `None` when `module_file` already defines the symbol (nothing to
/// follow) or when no chain reaches a definition — in both cases the caller
/// keeps the module it already resolved, so this can only improve attribution,
/// never lose it.
/// Private on purpose: [`definer_module`] is the only way in, so no call site
/// can bypass the single entry point the guard test enforces.
#[must_use]
fn follow_intra_crate_reexport(module_file: &str, symbol: &str, depth: u8) -> Option<String> {
    const MAX_DEPTH: u8 = 3;
    if depth >= MAX_DEPTH || symbol.is_empty() {
        return None;
    }
    let ws_root = find_workspace_root().unwrap_or_default();
    let abs = |p: &str| {
        if ws_root.is_empty() || std::path::Path::new(p).is_absolute() {
            p.to_string()
        } else {
            format!("{ws_root}/{p}")
        }
    };
    let facts = module_facts(&abs(module_file))?;
    if facts.defined.contains(symbol) {
        return None;
    }
    let rel = reexport_path_from(&facts, symbol)?;
    // Sibling modules resolve against the directory holding `module_file`.
    let dir = std::path::Path::new(module_file)
        .parent()?
        .to_str()?
        .to_string();
    let target = resolve_module_layout(&dir, &rel)?;
    if module_facts(&abs(&target)).is_some_and(|t| t.defined.contains(symbol)) {
        return Some(target);
    }
    follow_intra_crate_reexport(&target, symbol, depth + 1)
}

/// The module that DEFINES `symbol`, given the module an import resolved to.
///
/// **Every** write of a consumer edge goes through here. Measured 08/08/2026:
/// applying the hop only at the hook-runtime call site left the `index rebuild`
/// site — which writes the bulk of the rows — attributing re-exported symbols to
/// the facade, so `hybrid/pipeline.rs::KeywordSearch` stayed a **false orphan**
/// while its real consumer was credited to `hybrid_search/mod.rs`. Two call
/// sites resolving the same question differently is the C08 asymmetry the
/// decision matrix names; one shared entry point is what removes it, and
/// `record_consumer_sites_resolve_the_definer` keeps a third site from drifting.
///
/// Falls back to `module_file` unchanged whenever no chain reaches a definition,
/// so this can only improve attribution, never lose it.
#[must_use]
pub fn definer_module(module_file: &str, symbol: &str) -> String {
    follow_intra_crate_reexport(module_file, symbol, 0)
        .unwrap_or_else(|| module_file.to_string())
}

/// Why an import failed to resolve — S1 classification (2026-08-07).
///
/// The raw unresolved count is honest about "we could not look", but it still
/// merges three different facts, and only one of them is anybody's debt. The
/// first live measurement made that obvious: of 7.197 unresolved call sites,
/// the top entries were `super` (1.298), `serde` (531) and `std::path` (403) —
/// a scope keyword and two external crates, none of which the resolver is
/// supposed to map to a workspace file.
///
/// Publishing 7.197 as "resolver debt" would repeat, in a new field, exactly
/// the collapse this work removes from `contract_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedClass {
    /// `use super::*`, `self::`, `crate` bare — relative to the module
    /// hierarchy, which this resolver deliberately does not model. Expected,
    /// not debt.
    ScopeKeyword,
    /// A third-party or std crate: outside the workspace by definition, so
    /// there is no producer row to find. Expected, not debt.
    External,
    /// The first segment names a workspace crate, yet the path did not resolve.
    /// **This is the only class that is a resolver defect** — and the only one
    /// worth ranking for repair.
    WorkspaceUnresolved,
    /// The segment matches a workspace crate only through its SHORT alias, and
    /// that alias is also a plausible third-party name.
    ///
    /// `touring-rkyv` is aliased `rkyv`, which collides with the real `rkyv`
    /// crate — the homonymy VP-Scout chain 4 exists to catch. Calling it debt
    /// would inflate the defect list with imports of a dependency; calling it
    /// external would hide a genuine miss. Neither claim is supported, so it
    /// gets its own bucket instead of a guess.
    AmbiguousAlias,
}

impl UnresolvedClass {
    /// Stable string persisted alongside the row.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScopeKeyword => "scope_keyword",
            Self::External => "external",
            Self::WorkspaceUnresolved => "workspace_unresolved",
            Self::AmbiguousAlias => "ambiguous_alias",
        }
    }

    /// Whether this class represents a defect worth fixing.
    #[must_use]
    pub const fn is_debt(self) -> bool {
        matches!(self, Self::WorkspaceUnresolved)
    }
}

/// Classify an unresolved Rust import path.
///
/// Uses the same `TOURING_CRATE_MAP` the resolver itself uses, so the verdict
/// cannot drift from the resolution attempt that produced it.
#[must_use]
pub fn classify_unresolved(module_path: &str) -> UnresolvedClass {
    let head = module_path
        .split("::")
        .next()
        .unwrap_or(module_path)
        .trim();
    if matches!(head, "super" | "self" | "Self" | "crate" | "") {
        return UnresolvedClass::ScopeKeyword;
    }
    // The map is keyed by the crate's snake_case name; `use touring_foo::…`
    // (unambiguous) and `use foo::…` (the short alias) both appear in real
    // code — but only the first PROVES the import targets this workspace.
    let prefixed = TOURING_CRATE_MAP
        .iter()
        .any(|(name, _)| format!("touring_{name}") == head);
    if prefixed {
        return UnresolvedClass::WorkspaceUnresolved;
    }
    let short_alias = TOURING_CRATE_MAP.iter().any(|(name, _)| name == head);
    if short_alias {
        UnresolvedClass::AmbiguousAlias
    } else {
        UnresolvedClass::External
    }
}

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
    if let Some(crates_pos) = source_file.find("crates/")
        && let Some(src_pos) = source_file[crates_pos..].find("/src")
    {
        let end = crates_pos + src_pos + 4; // include "/src"
        return Some(source_file[..end].to_string());
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
            // (`find_workspace_root` / `resolve_module_layout` live at module
            // scope — see below — so the derived crate map and the re-export
            // follower can share them.)

            // First, check for cross-crate imports (e.g., touring_analysis::pipeline::Builder)
            for (crate_name, crate_path) in TOURING_CRATE_MAP.iter() {
                if let Some(rest) = import.strip_prefix(&format!("{}::", crate_name)) {
                    // `rest` is a MODULE path: `extract_file_imports` returns
                    // `(module_path, symbols)` with the symbols already split
                    // off (`use a::b::{C, D}` → `("a::b", [C, D])`).
                    //
                    // This branch used to `rsplit_once("::")` and discard the
                    // last segment as if it were the symbol — the contract its
                    // author documented, but not the one the caller supplies.
                    // For a two-segment import the two happen to agree, which
                    // is why it survived; for `touring_hooks_core::knowledge::
                    // models` it resolved `knowledge` and lost a module level.
                    // Try the honest reading first and keep the stripped form
                    // as a fallback, so a caller that DOES pass a symbol still
                    // resolves. Both candidates are filesystem-probed, so a
                    // wrong guess yields `None` rather than a phantom path.
                    let rel = rest.replace("::", "/");
                    return resolve_module_layout(crate_path, &rel)
                        .or_else(|| {
                            rest.rsplit_once("::").and_then(|(module_path, _symbol)| {
                                resolve_module_layout(crate_path, &module_path.replace("::", "/"))
                            })
                        })
                        .or_else(|| resolve_reexport(crate_path, &rel, 0));
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

            if let Some(src) = source_file
                && (import.starts_with("crate::") || import.starts_with("super::"))
                && let Some(crate_src_root) = detect_crate_src_root(src)
            {
                let relative = import
                    .strip_prefix("crate::")
                    .or_else(|| import.strip_prefix("super::"))
                    .unwrap_or(import);
                let rel = relative.replace("::", "/");
                // Try file-style and directory-style layouts, then this crate's
                // own re-exports: `use crate::tantivy_index::TantivyIndex` in
                // touring-cli names a module the crate does not contain —
                // `touring-cli/src/lib.rs` re-exports it from touring-hooks-core
                // (`pub use touring_hooks_core::tantivy_index;`). Same defect as
                // the cross-crate arm above, reached by a different path.
                //
                // Either None (neither layout exists nor any re-export, e.g. an
                // OUT_DIR-generated module) or the filename keyword sentinel
                // rejects the candidate.
                let candidate = resolve_module_layout(&crate_src_root, &rel)
                    .or_else(|| resolve_reexport(&crate_src_root, &rel, 0))?;
                if is_keyword_filename(&candidate) {
                    return None;
                }
                return Some(candidate);
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

/// The three defects that made `wiring orphans` unusable, each pinned by the
/// case that failed before the fix (measured 2026-08-07: 5031 orphans over
/// 12052 producers — 42% — of which the great majority were resolution
/// failures, not unwired code).
#[cfg(test)]
mod crate_map_and_reexport_tests {
    use super::{
        ALIAS_DENY, TOURING_CRATE_MAP, defines_symbol, find_workspace_root,
        follow_intra_crate_reexport, intra_crate_reexport_path, package_name, reexport_origins,
        resolve_import_path_with_source,
    };

    /// Defect 1 — the literal map named 6 live crates out of 41, so an import
    /// of any other crate resolved to `None` and its producers looked orphan.
    #[test]
    fn derived_map_covers_crates_the_literal_list_never_named() {
        for name in [
            "touring_storage",
            "touring_code",
            "touring_dispatch",
            "touring_hooks_core",
            "touring_rkyv",
            "touring_quality",
            "touring_intelligence",
            "touring_foundation",
        ] {
            assert!(
                TOURING_CRATE_MAP.iter().any(|(n, _)| n == name),
                "{name} is a live workspace crate but is absent from the derived map"
            );
        }
    }

    /// Defect 1, other half — 5 of the literal map's 11 entries pointed at
    /// directories deleted by past renames (`touring-core` → `touring-foundation`
    /// among them), so even a "mapped" crate resolved into nothing.
    #[test]
    fn every_mapped_source_root_exists_on_disk() {
        let root = find_workspace_root().expect("tests run inside the workspace");
        for (name, src) in TOURING_CRATE_MAP.iter() {
            assert!(
                std::path::Path::new(&root).join(src).is_dir(),
                "{name} maps to {src}, which does not exist"
            );
        }
    }

    /// `touring_foundation` must reach the crate that currently owns the code,
    /// not the pre-rename directory the literal map froze.
    #[test]
    fn renamed_crate_maps_to_its_current_directory() {
        let src = TOURING_CRATE_MAP
            .iter()
            .find(|(n, _)| n == "touring_foundation")
            .map(|(_, s)| s.as_str());
        assert_eq!(src, Some("crates/touring-foundation/src"));
    }

    /// The bare-alias ergonomics are kept and widened to every crate — but not
    /// for names that shadow a real crate. The literal map aliased `core` to
    /// `crates/touring-core/src`; one matching filename away, `use core::mem`
    /// would have been wired into an unrelated crate.
    #[test]
    fn bare_aliases_cover_every_crate_except_the_shadowing_ones() {
        assert!(TOURING_CRATE_MAP.iter().any(|(n, _)| n == "analysis"));
        assert!(TOURING_CRATE_MAP.iter().any(|(n, _)| n == "storage"));
        for denied in ALIAS_DENY {
            assert!(
                !TOURING_CRATE_MAP.iter().any(|(n, _)| n == denied),
                "{denied} shadows a real crate and must never be aliased"
            );
        }
    }

    /// Defect 2 — the cross-crate arm discarded the last path segment as if it
    /// were the symbol, but `extract_file_imports` already splits symbols off.
    /// `knowledge.rs` AND `knowledge/models.rs` both exist here, so the bug did
    /// not merely fail: it attributed the edge to a real, wrong file.
    #[test]
    fn deep_module_path_keeps_every_segment() {
        assert_eq!(
            resolve_import_path_with_source("touring_hooks_core::knowledge::models", "rust", None),
            Some("crates/touring-hooks-core/src/knowledge/models.rs".to_string())
        );
    }

    /// …while an import that really does carry a trailing symbol still resolves,
    /// via the fallback (the form the older callers and tests use).
    #[test]
    fn trailing_symbol_still_resolves_through_the_fallback() {
        assert_eq!(
            resolve_import_path_with_source(
                "touring_analysis::pipeline::AnalysisPipelineBuilder",
                "rust",
                None
            ),
            Some("crates/touring-analysis/src/pipeline.rs".to_string())
        );
    }

    /// Defect 3 — a module reached through a facade. `touring-hooks` re-exports
    /// `touring_dispatch::*`, which re-exports `touring_hooks_core::tantivy_index`;
    /// the file only exists in the third crate. Before the fix the edge was
    /// dropped and `TantivyIndex` was reported orphan while 39 files use it.
    #[test]
    fn reexport_chain_reaches_the_crate_that_owns_the_module() {
        assert_eq!(
            resolve_import_path_with_source("touring_hooks::tantivy_index", "rust", None),
            Some("crates/touring-hooks-core/src/tantivy_index.rs".to_string())
        );
    }

    /// Same defect through the `crate::` arm: touring-cli imports
    /// `crate::tantivy_index::…`, a module it re-exports rather than contains.
    #[test]
    fn crate_relative_import_follows_the_crates_own_reexport() {
        assert_eq!(
            resolve_import_path_with_source(
                "crate::tantivy_index",
                "rust",
                Some("crates/touring-cli/src/cli/handlers/mcp.rs")
            ),
            Some("crates/touring-hooks-core/src/tantivy_index.rs".to_string())
        );
    }

    /// A re-export that leads nowhere must stay `None` — the whole point of the
    /// filesystem probe is that a guess never becomes a phantom path.
    #[test]
    fn unresolvable_module_stays_none_rather_than_becoming_a_phantom() {
        assert_eq!(
            resolve_import_path_with_source("touring_hooks::no_such_module", "rust", None),
            None
        );
    }

    /// Every production write of a consumer edge resolves the DEFINER first.
    ///
    /// A per-call-site guard, not a per-fix one: the 08/08/2026 finding was that
    /// the hop existed at one of two sites, so the rebuild kept crediting
    /// facades. Whoever adds the third site either routes it through
    /// [`definer_module`] or states here why its module is already a definer.
    #[test]
    fn record_consumer_sites_resolve_the_definer() {
        /// First argument → why it is already a definer (or not a module file).
        const EXEMPT: &[(&str, &str)] = &[
            (
                "&edge.package_key",
                "Go package key `go:<path>`, not a Rust module file — no `pub use` chain exists",
            ),
            (
                "module_file",
                "F9 method-dispatch pass: comes from `find_producer_modules_for_methods`, \
                 i.e. already a producer row",
            ),
            (
                "&entry.module_file",
                "wiring repair: comes from an orphan PRODUCER row, a definer by construction",
            ),
        ];
        let Some(root) = find_workspace_root() else {
            return;
        };
        let mut sources = Vec::new();
        collect_rust_sources(std::path::Path::new(&root).join("crates"), &mut sources);
        assert!(
            sources.len() > 100,
            "the walk found {} files — it is not reaching the workspace",
            sources.len()
        );
        let mut offenders = Vec::new();
        for path in &sources {
            if path.ends_with("knowledge_wiring.rs") {
                continue; // the definition site itself
            }
            let Ok(src) = std::fs::read_to_string(path) else {
                continue;
            };
            // Test modules re-create rows by hand; only production writes matter.
            let production = src.split("#[cfg(test)]").next().unwrap_or("");
            for call in production.match_indices(".record_consumer") {
                let Some(arg) = first_argument(&production[call.0..]) else {
                    continue;
                };
                if arg.contains("definer_module")
                    || binds_from_definer(production, arg)
                    || EXEMPT.iter().any(|(a, _)| *a == arg)
                {
                    continue;
                }
                offenders.push(format!("{path}: record_consumer({arg}, …)"));
            }
        }
        assert!(
            offenders.is_empty(),
            "these consumer writes skip definer_module and are not documented as exempt:\n  {}",
            offenders.join("\n  ")
        );
    }

    /// Was `arg` bound from [`definer_module`]? Covers the idiomatic
    /// `let definer = definer_module(…);` (and the shadowing form) that reads
    /// better at the call site than inlining the whole path into the argument.
    fn binds_from_definer(src: &str, arg: &str) -> bool {
        let name = arg.trim_start_matches('&');
        src.match_indices(&format!("let {name} ="))
            .any(|(i, _)| {
                src[i..]
                    .split_once(';')
                    .is_some_and(|(binding, _)| binding.contains("definer_module"))
            })
    }

    /// The first argument of the call starting at `from`, trimmed.
    fn first_argument(from: &str) -> Option<&str> {
        let open = from.find('(')?;
        let mut depth = 0i32;
        for (i, ch) in from[open..].char_indices() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 1 => return Some(from[open + 1..open + i].trim()),
                _ => {}
            }
            if depth == 0 && i > 0 {
                return None; // call closed before any comma
            }
        }
        None
    }

    fn collect_rust_sources(dir: std::path::PathBuf, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // `src/` only: `tests/` and `benches/` build fixtures by hand.
                if path.file_name().is_some_and(|n| n == "target" || n == "tests") {
                    continue;
                }
                collect_rust_sources(path, out);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !path.to_string_lossy().ends_with("_tests.rs")
                && let Some(p) = path.to_str()
            {
                out.push(p.to_string());
            }
        }
    }

    #[test]
    fn defines_symbol_recognizes_the_item_kinds() {
        assert!(defines_symbol("pub trait KeywordSearch: Send {}", "KeywordSearch"));
        assert!(defines_symbol("pub struct Foo;", "Foo"));
        assert!(defines_symbol("    pub(crate) fn helper() {}", "helper"));
        assert!(defines_symbol("pub async fn go() {}", "go"));
        assert!(defines_symbol("enum Private {}", "Private"));
        assert!(!defines_symbol("pub use hybrid::pipeline::KeywordSearch;", "KeywordSearch"));
        assert!(!defines_symbol("let KeywordSearchLike = 1;", "KeywordSearch"));
    }

    #[test]
    fn intra_crate_reexport_path_reads_every_form() {
        assert_eq!(
            intra_crate_reexport_path("pub use hybrid::pipeline::{A, KeywordSearch};", "KeywordSearch")
                .as_deref(),
            Some("hybrid/pipeline")
        );
        assert_eq!(
            intra_crate_reexport_path("pub use hybrid::KeywordSearch;", "KeywordSearch").as_deref(),
            Some("hybrid")
        );
        assert_eq!(
            intra_crate_reexport_path("pub use hybrid::*;", "Anything").as_deref(),
            Some("hybrid")
        );
        assert_eq!(
            intra_crate_reexport_path("pub use other::Thing;", "KeywordSearch"),
            None
        );
    }

    #[test]
    fn follows_a_real_two_segment_reexport_to_its_definer() {
        // The exact row that degraded `touring doctor` on 2026-08-08.
        let Some(root) = find_workspace_root() else { return };
        let holder = "crates/touring-storage/src/hybrid_search/mod.rs";
        if !std::path::Path::new(&format!("{root}/{holder}")).exists() {
            return; // not in this checkout — do not fail the suite on layout
        }
        let hit = follow_intra_crate_reexport(holder, "KeywordSearch", 0);
        assert_eq!(
            hit.as_deref(),
            Some("crates/touring-storage/src/hybrid_search/hybrid/pipeline.rs"),
            "re-export must resolve to the defining module, not the facade"
        );
    }

    #[test]
    fn a_module_that_defines_the_symbol_is_left_alone() {
        let Some(root) = find_workspace_root() else { return };
        let definer = "crates/touring-storage/src/hybrid_search/hybrid/pipeline.rs";
        if !std::path::Path::new(&format!("{root}/{definer}")).exists() {
            return;
        }
        assert_eq!(
            follow_intra_crate_reexport(definer, "KeywordSearch", 0),
            None,
            "no hop needed when the file already defines it"
        );
    }

    #[test]
    fn an_unresolvable_chain_yields_none_never_a_phantom() {
        assert_eq!(follow_intra_crate_reexport("does/not/exist.rs", "Whatever", 0), None);
        assert_eq!(follow_intra_crate_reexport("crates/touring-storage/src/lib.rs", "", 0), None);
    }

    #[test]
    fn reexport_origins_reads_named_brace_and_glob_forms() {
        let src = "pub use touring_dispatch::*;\n\
                   pub use touring_hooks_core::tantivy_index;\n\
                   pub use touring_storage::{knowledge, wiring};\n";
        assert_eq!(
            reexport_origins(src, "tantivy_index"),
            ["touring_dispatch", "touring_hooks_core"]
        );
        assert_eq!(
            reexport_origins(src, "wiring"),
            ["touring_dispatch", "touring_storage"]
        );
    }

    /// `[[bin]]`, `[lib]` and dependency tables carry a `name` too — reading the
    /// first one in the file would key a crate under its binary's name.
    #[test]
    fn package_name_reads_only_the_package_table() {
        let manifest = "[workspace]\n\
                        [package]\nname = \"touring-cli\"\nversion = \"1.0\"\n\
                        [[bin]]\nname = \"touring\"\n";
        assert_eq!(package_name(manifest).as_deref(), Some("touring-cli"));
        assert_eq!(package_name("[[bin]]\nname = \"touring\"\n"), None);
    }

    // ── resolve_reexport: qualquer módulo, qualquer profundidade ────────────

    /// Re-export declarado em `src/shared/mod.rs`, não no `lib.rs`.
    ///
    /// A versão anterior lia só o `lib.rs` do crate e casava só o PRIMEIRO
    /// segmento, então achava `crate::tantivy_index::X` (re-exportado na raiz)
    /// e perdia este. Consequência medida: o produtor real em
    /// touring-hooks-shared ficava com zero consumidores e era contado órfão
    /// toda vez que um consumidor era re-indexado.
    #[test]
    fn a_reexport_declared_inside_a_submodule_resolves() {
        let hit = resolve_import_path_with_source(
            "crate::shared::feature_flags",
            "rust",
            Some("crates/touring-hooks-core/src/compression_profiles.rs"),
        );
        assert_eq!(
            hit.as_deref(),
            Some("crates/touring-hooks-shared/src/feature_flags.rs"),
            "tem de seguir o `pub use` de shared/mod.rs, não parar no lib.rs"
        );
    }

    /// O caso raiz que já funcionava continua funcionando (não-regressão).
    #[test]
    fn a_root_level_reexport_still_resolves() {
        let hit = resolve_import_path_with_source(
            "crate::tantivy_index",
            "rust",
            Some("crates/touring-cli/src/cli/handlers/index.rs"),
        );
        assert!(
            hit.as_deref().is_some_and(|p| p.ends_with("tantivy_index.rs")),
            "re-export de raiz regrediu: {hit:?}"
        );
    }

    /// Módulo local real tem precedência: nada de sair procurando re-export
    /// quando o arquivo existe no próprio crate.
    #[test]
    fn a_local_module_wins_over_any_reexport_search() {
        let hit = resolve_import_path_with_source(
            "crate::compression_profiles",
            "rust",
            Some("crates/touring-hooks-core/src/lib.rs"),
        );
        assert_eq!(
            hit.as_deref(),
            Some("crates/touring-hooks-core/src/compression_profiles.rs")
        );
    }

    /// Caminho que não existe em lugar nenhum devolve None — nunca um palpite.
    #[test]
    fn an_unresolvable_module_yields_none_never_a_phantom_path() {
        assert_eq!(
            resolve_import_path_with_source(
                "crate::modulo_que_nao_existe_em_lugar_nenhum",
                "rust",
                Some("crates/touring-hooks-core/src/lib.rs"),
            ),
            None
        );
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
        assert_eq!(
            resolved.as_deref(),
            Some(expected.to_string_lossy().as_ref())
        );
        // Homonimia guard: the resolved path must carry no `.`/`..` segments,
        // else the consumer row would never JOIN the producer row.
        let s = resolved.expect("resolved");
        assert!(
            !s.contains("/./") && !s.contains("/../"),
            "path must be lexically normal: {s}"
        );
    }
}

/// S1 classification — the guard against a new number repeating the old sin.
#[cfg(test)]
mod unresolved_class_tests {
    use super::{UnresolvedClass, classify_unresolved};

    #[test]
    fn scope_keywords_are_not_debt() {
        // `super` alone accounted for 1.298 of the first 7.197 unresolved call
        // sites. The resolver declines these by design (no module hierarchy),
        // so counting them as defects would send a reader chasing nothing.
        for kw in ["super", "self", "Self", "crate", "super::foo::Bar", "crate::x"] {
            let c = classify_unresolved(kw);
            assert_eq!(c, UnresolvedClass::ScopeKeyword, "{kw}");
            assert!(!c.is_debt(), "{kw} must not read as debt");
        }
    }

    #[test]
    fn third_party_and_std_are_not_debt() {
        for ext in [
            "serde",
            "std::path",
            "std::collections::HashMap",
            "tempfile",
            "criterion",
            "clap::Parser",
            "tokio::sync::Mutex",
        ] {
            let c = classify_unresolved(ext);
            assert_eq!(c, UnresolvedClass::External, "{ext}");
            assert!(!c.is_debt(), "{ext} has no producer row to find");
        }
    }

    #[test]
    fn an_unresolved_workspace_path_is_the_only_debt() {
        // Uses the live crate map, so this asserts against the same source the
        // resolver consults. Any workspace crate name works; pick one that must
        // exist for the workspace to build at all.
        let c = classify_unresolved("touring_storage::no_such_module::Thing");
        assert_eq!(c, UnresolvedClass::WorkspaceUnresolved);
        assert!(c.is_debt(), "a workspace path that did not resolve IS a defect");
        // The short alias alone does NOT prove workspace membership — the
        // `touring-rkyv` / `rkyv` collision is real, so it gets its own bucket
        // rather than being asserted into either side.
        assert_eq!(
            classify_unresolved("storage::no_such_module"),
            UnresolvedClass::AmbiguousAlias
        );
        assert!(
            !UnresolvedClass::AmbiguousAlias.is_debt(),
            "an unproven claim must not inflate the defect list"
        );
    }

    #[test]
    fn the_class_strings_round_trip_into_sql() {
        assert_eq!(UnresolvedClass::ScopeKeyword.as_str(), "scope_keyword");
        assert_eq!(UnresolvedClass::External.as_str(), "external");
        assert_eq!(
            UnresolvedClass::WorkspaceUnresolved.as_str(),
            "workspace_unresolved"
        );
        assert_eq!(UnresolvedClass::AmbiguousAlias.as_str(), "ambiguous_alias");
    }

    #[test]
    fn an_empty_path_degrades_to_keyword_never_to_debt() {
        // Defensive: a malformed import must not inflate the defect count.
        assert_eq!(classify_unresolved(""), UnresolvedClass::ScopeKeyword);
        assert_eq!(classify_unresolved("   "), UnresolvedClass::ScopeKeyword);
    }
}
