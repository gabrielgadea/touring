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

/// A Cargo workspace as the resolver sees it: its root and the crate map derived
/// from `<root>/crates/*/Cargo.toml` — crate names (underscored, as written in a
/// `use`) to their source directories, relative to `root`.
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
/// **One per workspace, never one per process (18/09/2026).** The map was a
/// process-wide `Lazy` built around the process's CURRENT DIRECTORY. The global
/// daemon is spawned by whichever session needs it first; that morning it was a
/// session in `~/Work`, the walk-up found no workspace, and the map stayed empty
/// for the daemon's whole life: an `index rebuild` of this repository filed 3.166
/// `use touring_…` imports in 615 files as `external`, zero as resolver debt, and
/// eleven symbols with live consumers read as NEW orphans in the judge. Cargo finds
/// the workspace of a FILE by walking up from it (The Cargo Book, "Workspaces");
/// [`workspace_for`] does the same.
///
/// [`record_consumer`]: https://docs.rs/touring-storage — `KnowledgeStore::record_consumer`
struct Workspace {
    /// Absolute path of the directory holding the `[workspace]` manifest.
    root: String,
    /// `(crate name, "crates/<dir>/src")`, longest name first.
    crates: Vec<(String, String)>,
    /// The entries of `crates` that are short aliases (`analysis` for
    /// `touring_analysis`), not package names: a local module may share one.
    aliases: std::collections::HashSet<String>,
}

impl Workspace {
    /// The source root of the crate a `use <name>::…` names, if it is one here.
    fn crate_src(&self, name: &str) -> Option<&str> {
        self.crates
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, src)| src.as_str())
    }

    /// Whether `name` is a real package name here — never a short alias.
    fn is_package(&self, name: &str) -> bool {
        !self.aliases.contains(name) && self.crates.iter().any(|(n, _)| n == name)
    }
}

/// Workspaces this process has resolved in, by root. The crate map costs a
/// `read_dir` plus one manifest read per crate, so it is built once per root.
static WORKSPACES: Lazy<
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<Workspace>>>,
> = Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// The workspace rooted at `root`, built on first use.
///
/// An empty crate map is said out loud, once per root: with it every import into
/// the workspace would read as a third-party crate, and the unresolved-import
/// report would call that "no resolver debt" (the 18/09/2026 failure).
fn workspace_at(root: &std::path::Path) -> std::sync::Arc<Workspace> {
    let key = root.to_string_lossy().into_owned();
    if let Ok(cache) = WORKSPACES.lock()
        && let Some(ws) = cache.get(&key)
    {
        return std::sync::Arc::clone(ws);
    }
    let (crates, aliases) = build_crate_map(root);
    if crates.is_empty() {
        tracing::warn!(
            root = %key,
            "Cargo workspace with no crates/<name>/src members: its Rust imports cannot be \
             resolved and are classified `unmeasured`"
        );
    }
    let ws = std::sync::Arc::new(Workspace {
        root: key.clone(),
        crates,
        aliases,
    });
    if let Ok(mut cache) = WORKSPACES.lock() {
        cache.insert(key, std::sync::Arc::clone(&ws));
    }
    ws
}

/// The workspace a resolution runs in.
///
/// 1. An ABSOLUTE `source_file` → the workspace that contains it, and nothing
///    else: a file outside every workspace has none, and borrowing the process's
///    would wire a foreign file into this repository's crates.
/// 2. Otherwise `source_file` is relative to the process's project:
///    `TOURING_PROJECT_ROOT` (pinned at spawn for every daemon), then the walk-up
///    from the current directory (tests, ad-hoc CLI runs).
fn workspace_for(source_file: Option<&str>) -> Option<std::sync::Arc<Workspace>> {
    let root = match source_file.map(std::path::Path::new) {
        Some(path) if path.is_absolute() => workspace_root_of(path)?,
        _ => process_workspace_root()?,
    };
    Some(workspace_at(&root))
}

/// The root [`workspace_for`] falls back to when it has no absolute path.
fn process_workspace_root() -> Option<std::path::PathBuf> {
    std::env::var_os("TOURING_PROJECT_ROOT")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .and_then(|p| workspace_root_of(&p))
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|d| workspace_root_of(&d))
        })
}

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
fn build_crate_map(
    root: &std::path::Path,
) -> (Vec<(String, String)>, std::collections::HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(root.join("crates")) else {
        return (Vec::new(), std::collections::HashSet::new());
    };
    let mut aliases = std::collections::HashSet::new();
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
            if let Some((a, _)) = &alias {
                aliases.insert(a.clone());
            }
            std::iter::once((name, src)).chain(alias)
        })
        .collect();
    // Longest name first so `touring_hooks_core::x` is never claimed by the
    // `touring_hooks` entry. The `::` in the prefix test already prevents that;
    // the ordering makes the invariant hold independently of that detail.
    map.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    (map, aliases)
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

/// The Cargo workspace `path` belongs to: the nearest ancestor (`path` itself
/// when it is a directory) whose `Cargo.toml` declares a `[workspace]` table.
/// Cargo's own rule — "inferred as the first Cargo.toml with `[workspace]`
/// upwards in the filesystem" (The Cargo Book, the `workspace` field).
///
/// Cached per starting directory: a rebuild asks once per file, and the walk-up
/// reads one manifest per level. Bounded by [`WORKSPACE_ROOTS_CAP`].
#[must_use]
fn workspace_root_of(path: &std::path::Path) -> Option<std::path::PathBuf> {
    type Roots = std::collections::HashMap<std::path::PathBuf, Option<std::path::PathBuf>>;
    static ROOTS: Lazy<std::sync::Mutex<Roots>> = Lazy::new(|| std::sync::Mutex::new(Roots::new()));
    let start = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    if let Ok(cache) = ROOTS.lock()
        && let Some(hit) = cache.get(&start)
    {
        return hit.clone();
    }
    let found = start
        .ancestors()
        .find(|dir| {
            std::fs::read_to_string(dir.join("Cargo.toml"))
                .is_ok_and(|manifest| declares_workspace(&manifest))
        })
        .map(std::path::Path::to_path_buf);
    if let Ok(mut cache) = ROOTS.lock() {
        if cache.len() >= WORKSPACE_ROOTS_CAP {
            cache.clear();
        }
        cache.insert(start, found.clone());
    }
    found
}

/// Directories [`workspace_root_of`] remembers before starting over.
const WORKSPACE_ROOTS_CAP: usize = 4096;

/// Whether a manifest declares a `[workspace]` table (or a `[workspace.*]` one,
/// which only a workspace root may carry). A table header, not a substring: the
/// old `contains("[workspace]")` also matched a comment that mentioned it.
fn declares_workspace(manifest: &str) -> bool {
    manifest.lines().map(str::trim).any(|line| {
        line == "[workspace]" || (line.starts_with("[workspace.") && line.ends_with(']'))
    })
}

/// Resolve `<dir>/<rel_path>` against both physical layouts Rust allows for a
/// module — `foo.rs` and `foo/mod.rs` — returning the workspace-relative path
/// of whichever exists.
///
/// `None` is a correct answer, not a failure: build-script modules generated
/// into `OUT_DIR` (e.g. `holon_core_capnp.rs` from a `.capnp` schema) have no
/// file under `src/`. Probing the filesystem instead of assuming file-style is
/// what keeps ~200 phantom paths out of the graph.
/// `p` on disk: joined under the workspace root when relative, taken as is when
/// absolute. The edit path anchors the importing file at the database root, so
/// paths derived from it arrive absolute; joining those under the root read
/// `<root>//<root>/…` and `resolve_reexport` followed no re-export on edit — one
/// of its two copies of this rule lacked the absolute case (18/09/2026).
fn under_workspace(ws_root: &str, p: &str) -> String {
    if ws_root.is_empty() || std::path::Path::new(p).is_absolute() {
        p.to_string()
    } else {
        format!("{ws_root}/{p}")
    }
}

fn resolve_module_layout(ws_root: &str, dir: &str, rel_path: &str) -> Option<String> {
    // A relative `dir` is relative to the WORKSPACE root, never to the cwd.
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
fn resolve_reexport(ws: &Workspace, facade_src_root: &str, rel: &str, depth: u8) -> Option<String> {
    // Facade chains are shallow (crate → core). The cap makes a `pub use` cycle
    // between two crates terminate instead of recursing until the stack dies.
    const MAX_DEPTH: u8 = 4;
    if depth >= MAX_DEPTH {
        return None;
    }
    let ws_root = ws.root.as_str();
    let abs = |p: &str| under_workspace(ws_root, p);

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
            match resolve_module_layout(ws_root, facade_src_root, &prefix.join("/")) {
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
            let Some(target_root) = ws.crate_src(&origin) else {
                continue;
            };
            if let Some(hit) = resolve_module_layout(ws_root, target_root, &remaining)
                .or_else(|| resolve_reexport(ws, target_root, &remaining, depth + 1))
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
    /// Python only: each `from <module> import <names>` of the file, the
    /// shape a façade forwards a symbol through (`from .modelo import No`).
    python_imports: Vec<(Box<str>, Vec<Box<str>>)>,
}

impl ModuleFacts {
    /// Facts of `content`, read as the language of `path` (Python by `.py`,
    /// Rust otherwise — the two languages whose re-exports are followed).
    fn parse(content: &str, path: &str) -> Self {
        if path.ends_with(".py") {
            return Self::parse_python(content, path);
        }
        Self {
            defined: ITEM_DEF_RE
                .captures_iter(content)
                .filter_map(|c| Some(Box::from(c.get(1)?.as_str())))
                .collect(),
            reexports: REEXPORT_RE
                .captures_iter(content)
                .filter_map(|c| {
                    Some((Box::from(c.get(1)?.as_str()), Box::from(c.get(2)?.as_str())))
                })
                .collect(),
            python_imports: Vec::new(),
        }
    }

    /// A Python module's definitions (the tree-sitter extractor the index
    /// uses) and its `from … import …` statements.
    fn parse_python(content: &str, path: &str) -> Self {
        Self {
            defined: crate::ast_bridge::extract_enriched_symbols(content, path)
                .unwrap_or_default()
                .into_iter()
                .map(|symbol| Box::from(symbol.name.as_str()))
                .collect(),
            reexports: Vec::new(),
            python_imports: crate::ast_bridge::extract_file_imports(content, path)
                .into_iter()
                .map(|(module, names)| {
                    (
                        Box::from(module.as_str()),
                        names.iter().map(|name| Box::from(name.as_str())).collect(),
                    )
                })
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
    let facts = std::sync::Arc::new(ModuleFacts::parse(
        &std::fs::read_to_string(abs_path).ok()?,
        abs_path,
    ));
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
    ModuleFacts::parse(content, "mod.rs")
        .defined
        .contains(symbol)
}

/// Module path of the intra-crate `pub use` that re-exports `symbol`, if any.
///
/// `pub use hybrid::pipeline::{A, Symbol};` → `Some("hybrid/pipeline")`
/// `pub use hybrid::Symbol;`               → `Some("hybrid")`
/// `pub use hybrid::*;`                    → `Some("hybrid")` (probe candidate)
/// Pure delegation to [`reexport_path_from`] — see [`defines_symbol`].
#[cfg(test)]
fn intra_crate_reexport_path(content: &str, symbol: &str) -> Option<String> {
    reexport_path_from(&ModuleFacts::parse(content, "mod.rs"), symbol)
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
fn follow_intra_crate_reexport(
    ws_root: &str,
    module_file: &str,
    symbol: &str,
    depth: u8,
) -> Option<String> {
    const MAX_DEPTH: u8 = 3;
    if depth >= MAX_DEPTH || symbol.is_empty() {
        return None;
    }
    let abs = |p: &str| under_workspace(ws_root, p);
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
    let target = resolve_module_layout(ws_root, &dir, &rel)?;
    if module_facts(&abs(&target)).is_some_and(|t| t.defined.contains(symbol)) {
        return Some(target);
    }
    follow_intra_crate_reexport(ws_root, &target, symbol, depth + 1)
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
///
/// `consumer` is the file whose import resolved to `module_file`: an absolute
/// path pins the workspace (see [`workspace_for`]), so the chain is followed in
/// the repository the consumer lives in, not in the one around the process.
#[must_use]
pub fn definer_module(module_file: &str, symbol: &str, consumer: Option<&str>) -> String {
    if module_file.ends_with(".py") {
        return follow_python_reexport(module_file, symbol, 0)
            .unwrap_or_else(|| module_file.to_string());
    }
    let ws_root = workspace_for(consumer).map_or_else(String::new, |ws| ws.root.clone());
    follow_intra_crate_reexport(&ws_root, module_file, symbol, 0)
        .unwrap_or_else(|| module_file.to_string())
}

/// The resolution-aware sibling of [`definer_module`]: `Some(file)` ONLY when
/// the symbol is genuinely defined at `module_file` or a re-export chain
/// reaches a definition; `None` when the only answer would be the fallback —
/// which is a guess, never a resolution. 24/09/2026 (touring-36): the fallback
/// was being recorded as an `ast_resolved` consumer edge to a phantom producer
/// no repair could ever clear — the doctor's `wiring_diagnostic` measured
/// `kind_unknown=8` of them, permanent by construction.
pub fn definer_module_opt(
    module_file: &str,
    symbol: &str,
    consumer: Option<&str>,
) -> Option<String> {
    if module_file.ends_with(".py") {
        return follow_python_reexport(module_file, symbol, 0);
    }
    let ws_root = workspace_for(consumer).map_or_else(String::new, |ws| ws.root.clone());
    let facts = module_facts(&under_workspace(&ws_root, module_file))?;
    if facts.defined.contains(symbol) {
        return Some(module_file.to_string());
    }
    follow_intra_crate_reexport(&ws_root, module_file, symbol, 0)
}

/// The Python module that DEFINES `symbol`, from the module an import named.
///
/// A façade that imports the symbol (`from .modelo import No`, usually with the
/// name in `__all__`) forwards it; the chain is followed through the SAME
/// resolver an import uses. 18/09/2026 (analise): after `grafo_memoria.py` was
/// split into four siblings behind a façade, every consumer importing `No` from
/// the façade was credited to it, `impact No` counted those edges by name and
/// `orphans` found `grafo_modelo.py::No` with none — the two disagreed. Only a
/// name forwarded unchanged is followed (an alias renames the symbol, and the
/// edge is keyed by name). `module_file` is absolute, as the Python resolver
/// returns it; `None` when no hop reaches a definition.
fn follow_python_reexport(module_file: &str, symbol: &str, depth: u8) -> Option<String> {
    // The same bound as the Rust chain: façades nest one or two levels deep.
    const MAX_DEPTH: u8 = 3;
    if depth >= MAX_DEPTH || symbol.is_empty() {
        return None;
    }
    let facts = module_facts(module_file)?;
    if facts.defined.contains(symbol) {
        return Some(module_file.to_string());
    }
    let (module, _) = facts.python_imports.iter().find(|(_, names)| {
        names.iter().any(|name| {
            let (original, alias) = name
                .split_once(" as ")
                .map_or((&**name, None), |(o, a)| (o, Some(a)));
            original.trim() == symbol && alias.is_none_or(|a| a.trim() == symbol)
        })
    })?;
    let target = resolve_python_import(module, Some(module_file))?;
    follow_python_reexport(&target, symbol, depth + 1)
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
    /// No crate map to judge against: the file is outside every Cargo workspace,
    /// or its workspace has no `crates/<name>/src` member. Not debt, and not
    /// "external" either — calling it external is how 3.166 imports into this
    /// very workspace read as third-party on 18/09/2026, and the debt count read
    /// zero while the resolver was blind.
    Unmeasured,
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
            Self::Unmeasured => "unmeasured",
        }
    }

    /// Whether this class represents a defect worth fixing.
    #[must_use]
    pub const fn is_debt(self) -> bool {
        matches!(self, Self::WorkspaceUnresolved)
    }
}

/// Classify an unresolved Rust import path, judged in the workspace of
/// `source_file` — the same one [`resolve_import_path_with_source`] tried, so the
/// verdict cannot drift from the attempt that produced it.
#[must_use]
pub fn classify_unresolved(module_path: &str, source_file: Option<&str>) -> UnresolvedClass {
    let head = module_path.split("::").next().unwrap_or(module_path).trim();
    if matches!(head, "super" | "self" | "Self" | "crate" | "") {
        return UnresolvedClass::ScopeKeyword;
    }
    let Some(ws) = workspace_for(source_file).filter(|ws| !ws.crates.is_empty()) else {
        return UnresolvedClass::Unmeasured;
    };
    // A package name PROVES the import targets this workspace; a short alias
    // (`use storage::…` for `touring_storage`) only suggests it.
    if ws.is_package(head) {
        UnresolvedClass::WorkspaceUnresolved
    } else if ws.crate_src(head).is_some() {
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

/// Resolve a `super::…` / `self::…` import (or a bare `super`/`self`) relative
/// to the importing file's place in the module tree (W, 2026-09-02).
///
/// Rust module semantics: `a/b.rs` is module `a::b`, whose children live in
/// `a/b/`; `a/mod.rs` (and `lib.rs`/`main.rs`) IS module `a`, whose children
/// live in `a/`. `self` names the module's own child directory, each `super`
/// climbs one module, and the remainder is probed with
/// [`resolve_module_layout`] — so a guess never becomes a phantom path. A bare
/// `super`/`self` names the module itself: `<dir>.rs` or `<dir>/mod.rs`, or
/// `lib.rs`/`main.rs` once the climb reached the crate root.
fn resolve_scope_relative(ws_root: &str, import: &str, source_file: &str) -> Option<String> {
    let path = std::path::Path::new(source_file);
    let dir = path.parent()?;
    let stem = path.file_stem()?.to_str()?;
    let mut module_dir = if matches!(stem, "mod" | "lib" | "main") {
        dir.to_path_buf()
    } else {
        dir.join(stem)
    };
    let mut segments = import.split("::").peekable();
    while let Some(seg) = segments.peek().copied() {
        match seg {
            "self" => {
                segments.next();
            }
            "super" => {
                segments.next();
                module_dir = module_dir.parent()?.to_path_buf();
            }
            _ => break,
        }
    }
    let rest: Vec<&str> = segments.collect();
    if rest.is_empty() {
        let name = module_dir.file_name()?.to_str()?;
        let parent = module_dir.parent()?.to_str()?;
        return resolve_module_layout(ws_root, parent, name).or_else(|| {
            let root = module_dir.to_str()?;
            resolve_module_layout(ws_root, root, "lib")
                .or_else(|| resolve_module_layout(ws_root, root, "main"))
        });
    }
    resolve_module_layout(ws_root, module_dir.to_str()?, &rest.join("/"))
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

/// How far up the directory chain a source-root search may walk. Bounded so a
/// resolution can never wander out to `/` and match an unrelated file on the
/// host (a Maven layout needs at most `src/main/java/<a>/<b>/<c>`).
const MAX_SOURCE_ROOT_WALK: usize = 8;

/// Candidate roots an ABSOLUTE Python import is resolved against, nearest-first.
///
/// Python resolves `import app.config` through `sys.path` — the source ROOT —
/// not through the importing file's directory (that is Python 2 implicit
/// relative import, removed in Python 3). So walk up out of the package: every
/// directory holding an `__init__.py` is still inside the package, and the
/// first one without it is the source root. The working directory is appended
/// last because in a production daemon run it is the project root, the same
/// contract the Rust arm's relative `resolve_module_layout` probes rely on.
///
/// 18/09/2026: after the package root, the PROJECT root is tried — the first
/// ancestor of the importing file holding a project marker — then its `src/`,
/// then the file's ancestors inside the project, the order Pyright documents
/// for absolute imports (workspace root, local `src`, parents of the importing
/// file). Until then the only other root was the process cwd, the Rust arm's
/// 30.4.55 defect in its Python form: `from memoria.grafo import No` in
/// `artefato/uso.py` resolved only in a daemon born at the project root.
fn python_source_roots(source_file: Option<&str>) -> Vec<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    let mut push = |dir: std::path::PathBuf| {
        if !roots.contains(&dir) {
            roots.push(dir);
        }
    };
    if let Some(src) = source_file
        && let Some(file_dir) = std::path::Path::new(src)
            .parent()
            .map(std::path::Path::to_path_buf)
    {
        let mut dir = file_dir.clone();
        for _ in 0..MAX_SOURCE_ROOT_WALK {
            if !dir.join("__init__.py").is_file() {
                break;
            }
            match dir.parent() {
                Some(parent) => dir = parent.to_path_buf(),
                None => break,
            }
        }
        push(dir);
        if let Some(project) = python_project_root(&file_dir) {
            push(project.clone());
            push(project.join("src"));
            for ancestor in file_dir.ancestors() {
                if !ancestor.starts_with(&project) || ancestor == project {
                    break;
                }
                push(ancestor.to_path_buf());
            }
        }
    }
    push(std::path::PathBuf::from("."));
    roots
}

/// The project a Python file belongs to: its nearest ancestor holding a
/// project marker. Bounded like every other upward walk here.
fn python_project_root(start: &std::path::Path) -> Option<std::path::PathBuf> {
    const MARKERS: [&str; 5] = [
        ".touring",
        ".git",
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
    ];
    start
        .ancestors()
        .take(4 * MAX_SOURCE_ROOT_WALK)
        .find(|dir| MARKERS.iter().any(|marker| dir.join(marker).exists()))
        .map(std::path::Path::to_path_buf)
}

/// Resolve a dotted Python module to a file that EXISTS, or `None`.
///
/// Two physical layouts, mirroring the Rust arm's file-style/`mod.rs` pair:
///   1. module-style:  `<root>/<a>/<b>.py`
///   2. package-style: `<root>/<a>/<b>/__init__.py`
///
/// `None` is the correct answer for stdlib and site-packages imports
/// (`pathlib`, `PIL`, `concurrent.futures`): they name no first-party file, so
/// there is no producer row to key. The previous version skipped the probe and
/// returned `Some("pathlib.py")` — a producer for a file that does not exist.
/// Measured 2026-08-19 in `analise`: 88 phantom `module_file` values, 76 of
/// them carrying `symbol_kind='unknown'`, all of them unreachable by any JOIN.
/// This is the identical defect the Rust arm fixed twice (phantom `super.rs`,
/// then `blast_radius.rs` vs `blast_radius/mod.rs`) and the TS/JS arm was born
/// with; the Python and Java arms never received it.
fn resolve_python_import(import: &str, source_file: Option<&str>) -> Option<String> {
    if import.starts_with('.') {
        return resolve_relative_python_import(import, source_file);
    }
    let rel = import.replace('.', "/");
    for root in python_source_roots(source_file) {
        let module = root.join(format!("{rel}.py"));
        if module.is_file() {
            return Some(module.to_string_lossy().into_owned());
        }
        let package = root.join(&rel).join("__init__.py");
        if package.is_file() {
            return Some(package.to_string_lossy().into_owned());
        }
    }
    None
}

/// Resolve `from .modulo import X`, `from ..pkg.modulo import X` and
/// `from . import X` against the importing file's PACKAGE.
///
/// The dots are the anchor: one dot is the importing file's own directory, each
/// extra dot climbs one package — never the source root, which is what an
/// absolute import uses. Until 16/09/2026 the query dropped the dots and this
/// function probed the root, so a relative import resolved to nothing and its
/// producer stayed an orphan: 136 of the 231 residual false orphans measured in
/// the analise (`from .c11_c12 import c11_reconciliacao_das_camadas`).
///
/// `from . import X` names the package itself, and the package probe below
/// answers it without a special case: with an empty tail, `dir.join("")` is
/// `dir`, so the probe is `<dir>/__init__.py`. The explicit branch that used to
/// sit here was dead code — mutation testing found it by surviving.
fn resolve_relative_python_import(import: &str, source_file: Option<&str>) -> Option<String> {
    let rest = import.trim_start_matches('.');
    let dots = import.len() - rest.len();
    let mut dir = std::path::Path::new(source_file?).parent()?.to_path_buf();
    // One dot is "here"; every extra dot is one package up.
    for _ in 1..dots {
        dir = dir.parent()?.to_path_buf();
    }
    let rel = rest.replace('.', "/");
    let module = dir.join(format!("{rel}.py"));
    if module.is_file() {
        return Some(module.to_string_lossy().into_owned());
    }
    let package = dir.join(&rel).join("__init__.py");
    package
        .is_file()
        .then(|| package.to_string_lossy().into_owned())
}

/// Resolve a Java FQN to a file that EXISTS, or `None`.
///
/// A fully-qualified name is rooted at a source root, which Maven and Gradle
/// nest under `src/main/java` (or `src/test/java`) rather than at the
/// repository root — the "known limitation" the previous comment described.
/// Probing every ancestor of the importing file finds that root wherever it
/// sits, and answering `None` keeps a JDK import (`java.util.List`) from
/// becoming a producer row for `java/util/List.java`.
fn resolve_java_import(import: &str, source_file: Option<&str>) -> Option<String> {
    let rel = format!("{}.java", import.replace('.', "/"));
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    if let Some(src) = source_file
        && let Some(dir) = std::path::Path::new(src).parent()
    {
        roots.extend(
            dir.ancestors()
                .take(MAX_SOURCE_ROOT_WALK)
                .map(std::path::Path::to_path_buf),
        );
    }
    roots.push(std::path::PathBuf::from("."));
    roots.push(std::path::PathBuf::from("src/main/java"));
    roots.push(std::path::PathBuf::from("src/test/java"));
    roots
        .into_iter()
        .map(|root| root.join(&rel))
        .find(|candidate| candidate.is_file())
        .map(|candidate| candidate.to_string_lossy().into_owned())
}

/// The file that DECLARES a Rust module, given the module's own file.
///
/// Cargo's layout, read backwards: `src/a/b.rs` and `src/a/b/mod.rs` are both
/// declared by `src/a.rs` or `src/a/mod.rs`; a module directly under `src/` is
/// declared by the crate root (`lib.rs`, else `main.rs`). Option B of 19/09/2026
/// credits that file, because the producer row of `pub mod b;` lives there.
///
/// `root` anchors the probe; the answer keeps the shape of `module_file`
/// (relative when it is relative). `None` when no candidate exists on disk.
#[must_use]
pub fn declaring_file_for_module(module_file: &str, root: &std::path::Path) -> Option<String> {
    let path = std::path::Path::new(module_file);
    let dir = if path.file_name().is_some_and(|n| n == "mod.rs") {
        path.parent()?.parent()?
    } else {
        path.parent()?
    };
    let exists = |candidate: &std::path::Path| {
        let absolute = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            root.join(candidate)
        };
        absolute.is_file()
    };
    let candidates = if dir.file_name().is_some_and(|n| n == "src") {
        vec![dir.join("lib.rs"), dir.join("main.rs")]
    } else {
        let mut with_rs = dir.to_path_buf();
        with_rs.set_extension("rs");
        vec![with_rs, dir.join("mod.rs")]
    };
    candidates
        .into_iter()
        .find(|candidate| exists(candidate))
        .map(|candidate| candidate.to_string_lossy().into_owned())
}

/// The file that declares the module a `crate::…` path names, walked FORWARD
/// from the crate root, plus that module's name.
///
/// [`declaring_file_for_module`] reads Cargo's layout backwards, so it needs the
/// module's own file — and two shapes have none the layout names:
/// `#[path = "cli/handlers/dispatch.rs"] pub mod cli_handlers;` (12 modules of
/// this workspace, all of touring-cli's `cli_handlers_*`) and an inline
/// `mod shared { … }` (3 more). Both stayed orphans with a `crate::…::` call one
/// line away, measured 19/09/2026. Walking forward, each segment is looked up as
/// a `mod` item of the file reached so far, so the declarer is known by
/// construction and no filesystem guess is involved.
///
/// `None` when a segment is not declared where the walk stands, or when the walk
/// would have to descend into an inline body — a guess never becomes an edge.
#[must_use]
pub fn declarer_of_crate_path(
    path: &str,
    source_file: &str,
    root: &std::path::Path,
) -> Option<(String, String)> {
    use touring_code::ast::graph::ModuleDeclaration;

    let rest = path.strip_prefix("crate::")?;
    // `join` keeps an absolute crate root as it is, and anchors a relative one.
    let src = root.join(detect_crate_src_root(source_file)?);
    let mut current = ["lib.rs", "main.rs"]
        .into_iter()
        .map(|name| src.join(name))
        .find(|candidate| candidate.is_file())?;
    let mut segments = rest.split("::").peekable();
    while let Some(segment) = segments.next() {
        let declaration = module_declarations_of(&current)?.get(segment)?.clone();
        if segments.peek().is_none() {
            let relative = current.strip_prefix(root).unwrap_or(&current);
            return Some((relative.to_string_lossy().into_owned(), segment.to_string()));
        }
        let directory = child_module_dir(&current);
        current = match declaration {
            // The items live in this very file; the walk has no file to open.
            ModuleDeclaration::Inline => return None,
            ModuleDeclaration::AtPath(attribute) => directory.join(attribute),
            ModuleDeclaration::Elsewhere => [
                directory.join(format!("{segment}.rs")),
                directory.join(segment).join("mod.rs"),
            ]
            .into_iter()
            .find(|candidate| candidate.is_file())?,
        };
    }
    None
}

/// A file's module declarations, memoised by path and modification time.
///
/// The forward walk reads the crate root for EVERY `crate::…` path of every file
/// — tens of thousands of reads and parses of a handful of `lib.rs` files in one
/// rebuild, which is why the walk could only afford to be a fallback. The mtime
/// is part of the key because the daemon lives for days: a file edited between
/// two rebuilds must be re-read, or the map credits a module it no longer
/// declares. A file that cannot be read or stat'd is simply not cached.
fn module_declarations_of(
    file: &std::path::Path,
) -> Option<
    std::sync::Arc<std::collections::BTreeMap<String, touring_code::ast::graph::ModuleDeclaration>>,
> {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::{Arc, Mutex, OnceLock};

    type Cache = HashMap<
        std::path::PathBuf,
        (
            std::time::SystemTime,
            Arc<BTreeMap<String, touring_code::ast::graph::ModuleDeclaration>>,
        ),
    >;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

    let modified = std::fs::metadata(file).ok()?.modified().ok()?;
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = cache.lock()
        && let Some((cached_at, declarations)) = map.get(file)
        && *cached_at == modified
    {
        return Some(Arc::clone(declarations));
    }
    let source = std::fs::read_to_string(file).ok()?;
    let declarations = Arc::new(touring_code::ast::graph::rust_module_declarations(&source));
    if let Ok(mut map) = cache.lock() {
        map.insert(file.to_path_buf(), (modified, Arc::clone(&declarations)));
    }
    Some(declarations)
}

/// The directory a file's child modules live in: its own when the file IS a
/// module root (`lib.rs`, `main.rs`, `mod.rs`), else the directory named after
/// it. This is also what a `#[path = "…"]` on one of those children is relative
/// to.
fn child_module_dir(file: &std::path::Path) -> std::path::PathBuf {
    let directory = file.parent().unwrap_or_else(|| std::path::Path::new(""));
    match file.file_stem().and_then(|stem| stem.to_str()) {
        Some("lib" | "main" | "mod") => directory.to_path_buf(),
        Some(stem) => directory.join(stem),
        None => directory.to_path_buf(),
    }
}

/// Resolve an import string, optionally using the source file path to resolve
/// `crate::` imports relative to the correct workspace crate.
pub fn resolve_import_path_with_source(
    import: &str,
    language: &str,
    source_file: Option<&str>,
) -> Option<String> {
    match language {
        "python" => resolve_python_import(import, source_file),
        "rust" => {
            // The workspace of the importing file (Cargo's rule), never the cwd's.
            let ws = workspace_for(source_file);
            let ws_root = ws.as_deref().map_or("", |ws| ws.root.as_str());
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
            // W (2026-09-02): `super::` / `self::` (and bare `super`/`self`) are
            // relative to the MODULE HIERARCHY of the importing file, not to the
            // crate root. The `crate::`/`super::` branch below strips the keyword
            // and probes `<crate>/src/<rest>`, which is right only for a
            // top-level module; measured on the live map: 4.519 imports filed as
            // "scope_keyword" while the producers they name read orphan.
            if let Some(src) = source_file
                && (import == "super"
                    || import == "self"
                    || import.starts_with("super::")
                    || import.starts_with("self::"))
                && let Some(candidate) = resolve_scope_relative(ws_root, import, src)
            {
                if is_keyword_filename(&candidate) {
                    return None;
                }
                return Some(candidate);
            }
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
            // A relative `dir` (cross-crate workspace map, e.g.
            // "crates/touring-analysis/src") is probed under the WORKSPACE root of
            // the importing file. It used to be "relative to the current working
            // directory, which equals the workspace root in production daemon
            // runs" — true until a daemon was spawned from `~/Work` (18/09/2026).

            // First, check for cross-crate imports (e.g., touring_analysis::pipeline::Builder)
            let crates = ws.as_deref().map_or(&[][..], |ws| ws.crates.as_slice());
            for (crate_name, crate_path) in crates {
                // `use touring_foo::{A, B}` reaches here as the module path
                // `touring_foo` alone: the symbols live at the crate root, defined
                // in `lib.rs` or re-exported there. Only `{crate}::…` was matched,
                // so every crate-root import went to `wiring_unresolved` (307 in
                // the workspace, 14/09/2026) and the producers behind them read as
                // orphans once the name-inference pass stopped covering imported
                // types (cross-audit R2-5). The full `touring_` name only: a bare
                // short alias (`storage`) can be a local module.
                if import == crate_name.as_str()
                    && ws.as_deref().is_some_and(|ws| ws.is_package(crate_name))
                {
                    return resolve_module_layout(ws_root, crate_path, "lib");
                }
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
                    return resolve_module_layout(ws_root, crate_path, &rel)
                        .or_else(|| {
                            rest.rsplit_once("::").and_then(|(module_path, _symbol)| {
                                resolve_module_layout(
                                    ws_root,
                                    crate_path,
                                    &module_path.replace("::", "/"),
                                )
                            })
                        })
                        .or_else(|| {
                            ws.as_deref()
                                .and_then(|ws| resolve_reexport(ws, crate_path, &rel, 0))
                        });
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
                let candidate =
                    resolve_module_layout(ws_root, &crate_src_root, &rel).or_else(|| {
                        ws.as_deref()
                            .and_then(|ws| resolve_reexport(ws, &crate_src_root, &rel, 0))
                    })?;
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
        "java" => resolve_java_import(import, source_file),
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
        ALIAS_DENY, defines_symbol, intra_crate_reexport_path, package_name, reexport_origins,
        resolve_import_path_with_source,
    };

    /// The crate map of the workspace these tests run in (no absolute source, so
    /// the process fallback — the crate's own directory under `cargo test`).
    fn crate_map() -> Vec<(String, String)> {
        super::workspace_for(None)
            .expect("tests run inside the workspace")
            .crates
            .clone()
    }

    fn find_workspace_root() -> Option<String> {
        super::process_workspace_root().map(|p| p.to_string_lossy().into_owned())
    }

    fn follow_intra_crate_reexport(module_file: &str, symbol: &str, depth: u8) -> Option<String> {
        super::follow_intra_crate_reexport(
            &find_workspace_root().unwrap_or_default(),
            module_file,
            symbol,
            depth,
        )
    }

    /// Cross-audit 14/09/2026 (R2-5): `use touring_assists::{ALL_HANDLERS, …}`
    /// hands the resolver the bare crate name. It resolves to the crate root, and
    /// the root's `pub use handlers::*;` leads to the file that defines the symbol.
    #[test]
    fn a_crate_root_import_resolves_through_the_root_to_the_definer() {
        let consumer = Some("crates/touring-server/src/cli/assist.rs");
        let root = resolve_import_path_with_source("touring_assists", "rust", consumer);
        assert_eq!(root.as_deref(), Some("crates/touring-assists/src/lib.rs"));
        assert_eq!(
            super::definer_module("crates/touring-assists/src/lib.rs", "ALL_HANDLERS", None),
            "crates/touring-assists/src/handlers/mod.rs"
        );
        assert_eq!(
            resolve_import_path_with_source("storage", "rust", consumer),
            None,
            "a bare short alias may be a local module and is never taken for a crate root"
        );
    }

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
                crate_map().iter().any(|(n, _)| n == name),
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
        for (name, src) in &crate_map() {
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
        let map = crate_map();
        let src = map
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
        let map = crate_map();
        assert!(map.iter().any(|(n, _)| n == "analysis"));
        assert!(map.iter().any(|(n, _)| n == "storage"));
        for denied in ALIAS_DENY {
            assert!(
                !map.iter().any(|(n, _)| n == denied),
                "{denied} shadows a real crate and must never be aliased"
            );
        }
    }

    /// Defect 2 — the cross-crate arm discarded the last path segment as if it
    /// were the symbol, but `extract_file_imports` already splits symbols off.
    ///
    /// Post-W72 (2026-08-12): the dead fork files under
    /// `touring-hooks-core/src/knowledge/` were deleted — the crate's
    /// `pub use touring_storage::knowledge;` re-export is the only definition.
    /// This assertion now pins the stronger property: the resolver FOLLOWS the
    /// re-export and lands on the canonical storage file, never on a stale
    /// fork copy (which is what made the original bug invisible).
    #[test]
    fn deep_module_path_keeps_every_segment() {
        assert_eq!(
            resolve_import_path_with_source("touring_hooks_core::knowledge::models", "rust", None),
            Some("crates/touring-storage/src/knowledge/models.rs".to_string())
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

    /// W (2026-09-02) fixture: a crate whose `capability` module has three files
    /// plus a child module dir, so every scope keyword has a distinct target.
    fn scope_fixture() -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "touring-scope-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let cap = base.join("crates/demo/src/capability");
        std::fs::create_dir_all(cap.join("limits")).expect("fixture dirs");
        for f in ["mod.rs", "limits.rs", "enforce_linux.rs"] {
            std::fs::write(cap.join(f), "").expect("fixture file");
        }
        std::fs::write(cap.join("limits/helper.rs"), "").expect("fixture child");
        std::fs::write(base.join("crates/demo/src/lib.rs"), "").expect("fixture lib");
        std::fs::write(base.join("crates/demo/src/util.rs"), "").expect("fixture util");
        base
    }

    /// `super::x` names the PARENT module's child, i.e. a sibling file of the
    /// importing module — not a file at the crate root. Before W the resolver
    /// stripped `super::` and probed `<crate>/src/enforce_linux.rs`, found
    /// nothing, and filed 4.519 such imports as "scope_keyword, expected, not
    /// debt" — while the producers they name read as orphans.
    #[test]
    fn super_import_resolves_to_a_sibling_of_the_importing_module() {
        let base = scope_fixture();
        let src = base.join("crates/demo/src/capability/limits.rs");
        let got = resolve_import_path_with_source(
            "super::enforce_linux",
            "rust",
            Some(src.to_str().expect("utf8")),
        );
        let _ = std::fs::remove_dir_all(&base);
        assert!(
            got.as_deref()
                .is_some_and(|p| p.ends_with("crates/demo/src/capability/enforce_linux.rs")),
            "got {got:?}"
        );
    }

    /// `self::x` names the importing module's OWN child (`limits/helper.rs`).
    #[test]
    fn self_import_resolves_into_the_modules_child_directory() {
        let base = scope_fixture();
        let src = base.join("crates/demo/src/capability/limits.rs");
        let got = resolve_import_path_with_source(
            "self::helper",
            "rust",
            Some(src.to_str().expect("utf8")),
        );
        let _ = std::fs::remove_dir_all(&base);
        assert!(
            got.as_deref()
                .is_some_and(|p| p.ends_with("crates/demo/src/capability/limits/helper.rs")),
            "got {got:?}"
        );
    }

    /// From `capability/mod.rs` the module IS `capability`, so `super` is the
    /// crate root; `super::super::util` from `capability/limits.rs` climbs twice
    /// to the same place. Both land on `src/util.rs`.
    #[test]
    fn super_climbs_from_mod_rs_and_double_super_climbs_twice() {
        let base = scope_fixture();
        let from_mod = base.join("crates/demo/src/capability/mod.rs");
        let from_leaf = base.join("crates/demo/src/capability/limits.rs");
        let a = resolve_import_path_with_source(
            "super::util",
            "rust",
            Some(from_mod.to_str().expect("utf8")),
        );
        let b = resolve_import_path_with_source(
            "super::super::util",
            "rust",
            Some(from_leaf.to_str().expect("utf8")),
        );
        let _ = std::fs::remove_dir_all(&base);
        assert!(
            a.as_deref()
                .is_some_and(|p| p.ends_with("crates/demo/src/util.rs")),
            "mod.rs → {a:?}"
        );
        assert!(
            b.as_deref()
                .is_some_and(|p| p.ends_with("crates/demo/src/util.rs")),
            "double super → {b:?}"
        );
    }

    /// `use super::Foo;` arrives as module path "super" — the parent module
    /// ITSELF (`capability/mod.rs`), which the old guard returned `None` for.
    #[test]
    fn bare_super_import_resolves_to_the_parent_modules_own_file() {
        let base = scope_fixture();
        let src = base.join("crates/demo/src/capability/limits.rs");
        let got =
            resolve_import_path_with_source("super", "rust", Some(src.to_str().expect("utf8")));
        let _ = std::fs::remove_dir_all(&base);
        assert!(
            got.as_deref()
                .is_some_and(|p| p.ends_with("crates/demo/src/capability/mod.rs")),
            "got {got:?}"
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
            (
                "declaring_file",
                "self-reference (D9): the symbols are the file's own declarations, \
                 so the file is their definer by construction",
            ),
            (
                "&declarer",
                "module traversal (option B, 19/09/2026): `declaring_file_for_module` \
                 derives the file holding `pub mod <name>;` from Cargo's layout, and \
                 that declaration IS the definition of a module — no `pub use` chain \
                 renames it",
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
        src.match_indices(&format!("let {name} =")).any(|(i, _)| {
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
                if path
                    .file_name()
                    .is_some_and(|n| n == "target" || n == "tests")
                {
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
        assert!(defines_symbol(
            "pub trait KeywordSearch: Send {}",
            "KeywordSearch"
        ));
        assert!(defines_symbol("pub struct Foo;", "Foo"));
        assert!(defines_symbol("    pub(crate) fn helper() {}", "helper"));
        assert!(defines_symbol("pub async fn go() {}", "go"));
        assert!(defines_symbol("enum Private {}", "Private"));
        assert!(!defines_symbol(
            "pub use hybrid::pipeline::KeywordSearch;",
            "KeywordSearch"
        ));
        assert!(!defines_symbol(
            "let KeywordSearchLike = 1;",
            "KeywordSearch"
        ));
    }

    #[test]
    fn intra_crate_reexport_path_reads_every_form() {
        assert_eq!(
            intra_crate_reexport_path(
                "pub use hybrid::pipeline::{A, KeywordSearch};",
                "KeywordSearch"
            )
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
        let Some(root) = find_workspace_root() else {
            return;
        };
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
        let Some(root) = find_workspace_root() else {
            return;
        };
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
        assert_eq!(
            follow_intra_crate_reexport("does/not/exist.rs", "Whatever", 0),
            None
        );
        assert_eq!(
            follow_intra_crate_reexport("crates/touring-storage/src/lib.rs", "", 0),
            None
        );
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
            hit.as_deref()
                .is_some_and(|p| p.ends_with("tantivy_index.rs")),
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
mod crate_path_walk_tests {
    use super::declarer_of_crate_path;
    use std::fs;
    use tempfile::TempDir;

    /// Walking forward reaches the two shapes Cargo's layout cannot name.
    /// `#[path]`: all 12 of them in this workspace are touring-cli's
    /// `cli_handlers_*`, each with a `crate::cli_handlers_*::` call one line away
    /// and each an orphan until 19/09/2026.
    #[test]
    fn the_walk_finds_the_declarer_of_every_module_shape() {
        let tmp = TempDir::new().expect("tempdir");
        let src = tmp.path().join("crates/demo/src");
        fs::create_dir_all(src.join("cli/handlers")).expect("mkdir handlers");
        fs::create_dir_all(src.join("plain")).expect("mkdir plain");
        fs::write(
            src.join("lib.rs"),
            "#[path = \"cli/handlers/dispatch.rs\"]\npub mod cli_handlers;\npub mod plain;\npub mod inline_mod { pub fn f() {} }\n",
        )
        .expect("lib.rs");
        fs::write(src.join("cli/handlers/dispatch.rs"), "pub fn run() {}\n").expect("dispatch.rs");
        fs::write(src.join("plain.rs"), "pub mod deep;\n").expect("plain.rs");
        fs::write(src.join("plain/deep.rs"), "pub fn f() {}\n").expect("deep.rs");
        let consumer = src.join("cli_e2e.rs");
        fs::write(&consumer, "fn f() { crate::cli_handlers::run(); }\n").expect("consumer");
        let consumer = consumer.to_str().expect("utf8");
        let walk = |path: &str| declarer_of_crate_path(path, consumer, tmp.path());

        assert_eq!(
            walk("crate::cli_handlers"),
            Some(("crates/demo/src/lib.rs".into(), "cli_handlers".into())),
            "`#[path]` names the file, and the crate root still declares the module"
        );
        assert_eq!(
            walk("crate::plain::deep"),
            Some(("crates/demo/src/plain.rs".into(), "deep".into())),
            "the walk descends one level and credits the file that declares the leaf"
        );
        assert_eq!(
            walk("crate::inline_mod"),
            Some(("crates/demo/src/lib.rs".into(), "inline_mod".into())),
            "an inline `mod x {{ … }}` is declared by the file holding its body"
        );
        // Negative controls: a guess never becomes an edge.
        assert_eq!(walk("crate::zz_inexistente"), None);
        assert_eq!(
            walk("crate::inline_mod::deeper"),
            None,
            "the walk has no file to open inside an inline body"
        );
        assert_eq!(
            walk("touring_demo::cli_handlers"),
            None,
            "not a `crate::` path"
        );
    }
}

#[cfg(test)]
mod python_java_resolver_tests {
    use super::resolve_import_path_with_source;
    use std::fs;
    use tempfile::TempDir;

    /// A stdlib or site-packages import names no first-party file. The Rust and
    /// TS/JS arms already answer `None` for these (crate map / bare specifier);
    /// the Python arm answered `Some("pathlib.py")` — a producer row for a file
    /// that does not exist. Measured 2026-08-19 in `analise`: 88 phantom
    /// `module_file` values, 76 of them carrying `symbol_kind='unknown'`.
    #[test]
    fn stdlib_import_is_not_a_project_file() {
        let tmp = TempDir::new().expect("tempdir");
        let app = tmp.path().join("app.py");
        fs::write(&app, "import pathlib\n").expect("write app.py");

        for module in ["pathlib", "collections", "concurrent.futures", "PIL"] {
            assert_eq!(
                resolve_import_path_with_source(
                    module,
                    "python",
                    Some(app.to_str().expect("utf8"))
                ),
                None,
                "`{module}` has no file in the project tree — it must not become a producer row"
            );
        }
    }

    /// B5 (16/09/2026): a relative import is anchored at the importing file's
    /// PACKAGE, never at the source root. Resolving it from the root is how 136
    /// of the 231 residual false orphans in the analise were produced
    /// (`from .c11_c12 import c11_reconciliacao_das_camadas`).
    #[test]
    fn a_relative_python_import_resolves_against_its_own_package() {
        let tmp = TempDir::new().expect("tempdir");
        let pkg = tmp.path().join("pacote");
        let sub = pkg.join("sub");
        fs::create_dir_all(&sub).expect("mkdir sub");
        fs::write(pkg.join("__init__.py"), "").expect("pkg init");
        fs::write(sub.join("__init__.py"), "").expect("sub init");
        fs::write(pkg.join("formato.py"), "PUB = 1\n").expect("formato.py");
        fs::write(sub.join("svg.py"), "from ..formato import PUB\n").expect("svg.py");
        // A same-name module at the ROOT: resolving from the root would find this
        // one, so the test tells the two apart instead of just asserting "some file".
        fs::write(tmp.path().join("formato.py"), "ERRADO = 1\n").expect("root formato.py");
        let svg = sub.join("svg.py");
        let svg = svg.to_str().expect("utf8");

        assert_eq!(
            resolve_import_path_with_source("..formato", "python", Some(svg)),
            Some(pkg.join("formato.py").to_string_lossy().into_owned()),
            "two dots climb to the package, not to the source root"
        );
        assert_eq!(
            resolve_import_path_with_source(".", "python", Some(svg)),
            Some(sub.join("__init__.py").to_string_lossy().into_owned()),
            "`from . import X` names the package itself"
        );
        assert_eq!(
            resolve_import_path_with_source(".svg", "python", Some(svg)),
            Some(svg.to_string()),
            "one dot is the importing file's own directory"
        );
        assert_eq!(
            resolve_import_path_with_source("...nada", "python", Some(svg)),
            None,
            "a climb past the tree resolves to nothing, never to a guess"
        );
    }

    #[test]
    fn python_module_resolves_when_the_file_exists() {
        let tmp = TempDir::new().expect("tempdir");
        let pkg = tmp.path().join("packages").join("kazuba_core");
        fs::create_dir_all(&pkg).expect("mkdir pkg");
        fs::write(pkg.join("models.py"), "class User:\n    pass\n").expect("write models.py");
        let app = tmp.path().join("app.py");
        fs::write(&app, "from packages.kazuba_core.models import User\n").expect("write app.py");

        let resolved = resolve_import_path_with_source(
            "packages.kazuba_core.models",
            "python",
            Some(app.to_str().expect("utf8")),
        );
        assert_eq!(
            resolved.as_deref(),
            Some(pkg.join("models.py").to_string_lossy().as_ref()),
            "a dotted module that exists on disk must resolve to its .py file"
        );
    }

    /// `import pkg` where `pkg/` is a package directory resolves to its
    /// `__init__.py` — the Python analogue of the Rust arm's `mod.rs`
    /// directory-style layout, which that arm learned to probe after ~200
    /// phantoms.
    #[test]
    fn python_package_resolves_to_init_file() {
        let tmp = TempDir::new().expect("tempdir");
        let pkg = tmp.path().join("converter");
        fs::create_dir_all(&pkg).expect("mkdir converter");
        fs::write(pkg.join("__init__.py"), "").expect("write __init__.py");
        let app = tmp.path().join("main.py");
        fs::write(&app, "import converter\n").expect("write main.py");

        let resolved = resolve_import_path_with_source(
            "converter",
            "python",
            Some(app.to_str().expect("utf8")),
        );
        assert_eq!(
            resolved.as_deref(),
            Some(pkg.join("__init__.py").to_string_lossy().as_ref()),
            "a package directory must resolve to its __init__.py"
        );
    }

    /// The importer sits inside a package, so the module it names is rooted at
    /// the package's PARENT (the source root) — walking up past every
    /// `__init__.py` is how Python itself resolves an absolute import.
    #[test]
    fn absolute_import_resolves_from_the_source_root_not_the_importer_dir() {
        let tmp = TempDir::new().expect("tempdir");
        let app_pkg = tmp.path().join("app");
        fs::create_dir_all(&app_pkg).expect("mkdir app");
        fs::write(app_pkg.join("__init__.py"), "").expect("write app/__init__.py");
        fs::write(app_pkg.join("config.py"), "SETTINGS = {}\n").expect("write config.py");
        let main = app_pkg.join("main.py");
        fs::write(&main, "from app.config import SETTINGS\n").expect("write main.py");

        let resolved = resolve_import_path_with_source(
            "app.config",
            "python",
            Some(main.to_str().expect("utf8")),
        );
        assert_eq!(
            resolved.as_deref(),
            Some(app_pkg.join("config.py").to_string_lossy().as_ref()),
            "`app.config` from inside `app/` resolves at the source root, not `app/app/config.py`"
        );
    }

    /// Java carried the same unprobed dotted→path scheme as Python, and its own
    /// comment said so. `java.util.List` is not a file in anyone's repository.
    #[test]
    fn java_external_import_is_not_a_project_file() {
        let tmp = TempDir::new().expect("tempdir");
        let src = tmp.path().join("Main.java");
        fs::write(&src, "import java.util.List;\n").expect("write Main.java");

        assert_eq!(
            resolve_import_path_with_source(
                "java.util.List",
                "java",
                Some(src.to_str().expect("utf8"))
            ),
            None,
            "a JDK import has no file in the project tree"
        );
    }

    #[test]
    fn java_import_resolves_when_the_file_exists() {
        let tmp = TempDir::new().expect("tempdir");
        let pkg = tmp.path().join("com").join("foo");
        fs::create_dir_all(&pkg).expect("mkdir com/foo");
        fs::write(
            pkg.join("Bar.java"),
            "package com.foo;\npublic class Bar {}\n",
        )
        .expect("write Bar.java");
        let main = tmp.path().join("Main.java");
        fs::write(&main, "import com.foo.Bar;\n").expect("write Main.java");

        assert_eq!(
            resolve_import_path_with_source(
                "com.foo.Bar",
                "java",
                Some(main.to_str().expect("utf8"))
            )
            .as_deref(),
            Some(pkg.join("Bar.java").to_string_lossy().as_ref()),
            "a FQN whose source file exists resolves to it"
        );
    }

    /// Maven/Gradle nest sources under `src/main/java`; the FQN is rooted there,
    /// not at the repository root. The old comment called this a "known
    /// limitation shared with Python" — probing makes it work instead.
    #[test]
    fn java_resolves_under_a_maven_source_root() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("src").join("main").join("java");
        let pkg = root.join("com").join("acme");
        fs::create_dir_all(&pkg).expect("mkdir pkg");
        fs::write(
            pkg.join("Service.java"),
            "package com.acme;\npublic class Service {}\n",
        )
        .expect("write Service.java");
        let main = root.join("com").join("acme").join("Main.java");
        fs::write(&main, "import com.acme.Service;\n").expect("write Main.java");

        assert_eq!(
            resolve_import_path_with_source(
                "com.acme.Service",
                "java",
                Some(main.to_str().expect("utf8"))
            )
            .as_deref(),
            Some(pkg.join("Service.java").to_string_lossy().as_ref()),
            "the FQN is rooted at src/main/java, not at the repository root"
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
        for kw in [
            "super",
            "self",
            "Self",
            "crate",
            "super::foo::Bar",
            "crate::x",
        ] {
            let c = classify_unresolved(kw, None);
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
            let c = classify_unresolved(ext, None);
            assert_eq!(c, UnresolvedClass::External, "{ext}");
            assert!(!c.is_debt(), "{ext} has no producer row to find");
        }
    }

    #[test]
    fn an_unresolved_workspace_path_is_the_only_debt() {
        // Uses the live crate map, so this asserts against the same source the
        // resolver consults. Any workspace crate name works; pick one that must
        // exist for the workspace to build at all.
        let c = classify_unresolved("touring_storage::no_such_module::Thing", None);
        assert_eq!(c, UnresolvedClass::WorkspaceUnresolved);
        assert!(
            c.is_debt(),
            "a workspace path that did not resolve IS a defect"
        );
        // The short alias alone does NOT prove workspace membership — the
        // `touring-rkyv` / `rkyv` collision is real, so it gets its own bucket
        // rather than being asserted into either side.
        assert_eq!(
            classify_unresolved("storage::no_such_module", None),
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
        assert_eq!(classify_unresolved("", None), UnresolvedClass::ScopeKeyword);
        assert_eq!(
            classify_unresolved("   ", None),
            UnresolvedClass::ScopeKeyword
        );
    }
}

/// 18/09/2026 — the workspace of a resolution is the workspace of the FILE
/// (Cargo's rule), never the process's current directory. `cargo test` runs in
/// this repository; every fixture below is ANOTHER workspace, so a resolver that
/// still read the process's directory would not know the fixture's crates.
#[cfg(test)]
mod workspace_scope_tests {
    use super::{
        UnresolvedClass, classify_unresolved, declares_workspace, definer_module,
        resolve_import_path_with_source, workspace_root_of,
    };
    use std::path::{Path, PathBuf};

    /// A throwaway tree under the temp dir, removed on drop.
    struct Tree(PathBuf);

    impl Tree {
        fn new(tag: &str, files: &[(&str, &str)]) -> Self {
            let root =
                std::env::temp_dir().join(format!("touring-ws-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for (rel, body) in files {
                let path = root.join(rel);
                std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
                std::fs::write(path, body).expect("write");
            }
            Self(root)
        }

        fn file(&self, rel: &str) -> String {
            self.0.join(rel).to_string_lossy().into_owned()
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// `touring-alpha` defines `widgets::Gadget` and re-exports it from its root;
    /// `touring-beta` imports it both ways.
    fn two_crate_workspace(tag: &str) -> Tree {
        Tree::new(
            tag,
            &[
                (
                    "Cargo.toml",
                    "# a comment that names [workspace] is not the table\n[workspace]\nmembers = [\"crates/*\"]\n",
                ),
                (
                    "crates/alpha/Cargo.toml",
                    "[package]\nname = \"touring-alpha\"\n",
                ),
                (
                    "crates/alpha/src/lib.rs",
                    "pub mod widgets;\npub use widgets::*;\n",
                ),
                ("crates/alpha/src/widgets.rs", "pub struct Gadget;\n"),
                (
                    "crates/beta/Cargo.toml",
                    "[package]\nname = \"touring-beta\"\n",
                ),
                (
                    "crates/beta/src/lib.rs",
                    "use touring_alpha::widgets::Gadget;\nuse touring_alpha::Gadget as G;\n",
                ),
            ],
        )
    }

    #[test]
    fn a_file_resolves_in_its_own_workspace_whatever_the_process_directory_is() {
        let ws = two_crate_workspace("resolve");
        let consumer = ws.file("crates/beta/src/lib.rs");
        assert_eq!(
            workspace_root_of(Path::new(&consumer)).as_deref(),
            Some(ws.0.as_path())
        );
        assert_eq!(
            resolve_import_path_with_source("touring_alpha::widgets", "rust", Some(&consumer))
                .as_deref(),
            Some("crates/alpha/src/widgets.rs")
        );
        // The crate-root form lands on `lib.rs`, and the definer hop follows the
        // root's `pub use widgets::*` to the file that defines the symbol.
        assert_eq!(
            resolve_import_path_with_source("touring_alpha", "rust", Some(&consumer)).as_deref(),
            Some("crates/alpha/src/lib.rs")
        );
        assert_eq!(
            definer_module("crates/alpha/src/lib.rs", "Gadget", Some(&consumer)),
            "crates/alpha/src/widgets.rs"
        );
    }

    #[test]
    fn unresolved_imports_are_judged_by_the_consumers_workspace() {
        let ws = two_crate_workspace("classify");
        let consumer = ws.file("crates/beta/src/lib.rs");
        let class = |path: &str| classify_unresolved(path, Some(&consumer));
        assert_eq!(
            class("touring_alpha::no_such"),
            UnresolvedClass::WorkspaceUnresolved
        );
        assert_eq!(class("alpha::no_such"), UnresolvedClass::AmbiguousAlias);
        assert_eq!(class("serde::de"), UnresolvedClass::External);
        // This repository's crates are third-party from inside the fixture.
        assert_eq!(
            class("touring_storage::knowledge"),
            UnresolvedClass::External
        );
    }

    #[test]
    fn a_file_outside_every_workspace_is_unmeasured_and_borrows_nothing() {
        let tree = Tree::new(
            "none",
            &[("lonely.rs", "use touring_storage::knowledge;\n")],
        );
        let file = tree.file("lonely.rs");
        assert_eq!(workspace_root_of(Path::new(&file)), None);
        assert_eq!(
            classify_unresolved("touring_storage::knowledge", Some(&file)),
            UnresolvedClass::Unmeasured,
            "no crate map is 'not measured', never 'external' — the 18/09 blind spot"
        );
        assert!(!UnresolvedClass::Unmeasured.is_debt());
        assert_eq!(
            resolve_import_path_with_source("touring_storage::knowledge", "rust", Some(&file)),
            None,
            "an absolute path outside every workspace never borrows the process's crates"
        );
    }

    #[test]
    fn a_workspace_without_crate_members_is_unmeasured() {
        let tree = Tree::new(
            "empty",
            &[
                ("Cargo.toml", "[workspace]\nmembers = []\n"),
                ("src/main.rs", "fn main() {}\n"),
            ],
        );
        assert_eq!(
            classify_unresolved("touring_x::y", Some(&tree.file("src/main.rs"))),
            UnresolvedClass::Unmeasured
        );
    }

    #[test]
    fn a_workspace_is_a_table_header_never_a_substring() {
        assert!(declares_workspace("[workspace]\nmembers = []\n"));
        assert!(declares_workspace("  [workspace.lints.rust]\n"));
        assert!(!declares_workspace(
            "# see [workspace] in the root\n[package]\nname = \"x\"\n"
        ));
        assert!(!declares_workspace("[package]\nedition.workspace = true\n"));
    }
}
