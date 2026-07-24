//! Intra-crate module import-cycle detection (hermetic).
//!
//! Builds the module→module import graph directly from a crate's own source
//! tree (`use crate::<top-level>` edges) and runs petgraph's Kosaraju SCC — the
//! same algorithm as [`crate::wiring::cycle_detection::detect_import_cycles`],
//! but sourced *hermetically from the target's files* rather than the daemon's
//! `wiring_map`. This makes F1.8 correct for ANY target crate (no daemon, no
//! cross-project staleness): a reported SCC means two top-level module subtrees
//! genuinely `use crate::` each other.
//!
//! Scope note: cargo forbids cycles *between* crates, so intra-crate module
//! cycles are the only meaningful F1.8 signal. Edges are collapsed to the
//! top-level module so an SCC is a real architectural coupling cycle (no false
//! positives — self-references collapse to a self-loop and are dropped).
//!
//! Backs the F1.8 dimension. Hermetic: pure file parse + graph, no process spawn.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

/// Report of intra-crate module import cycles.
#[derive(Debug, Clone, Default)]
pub struct ModuleCycleReport {
    /// Resolved `src/` root of the crate, or `None` when the target is not part
    /// of a crate (caller then falls back to a local hygiene proxy).
    pub crate_src: Option<PathBuf>,
    /// Number of top-level modules in the graph.
    pub modules_analyzed: usize,
    /// Detected cycles — each a list of mutually-dependent top-level modules.
    pub cycles: Vec<Vec<String>>,
}

impl ModuleCycleReport {
    /// Number of distinct module-import cycles (SCCs with > 1 node).
    #[must_use]
    pub fn cycle_count(&self) -> usize {
        self.cycles.len()
    }

    /// Whether the target resolved to a crate (so the result is meaningful).
    #[must_use]
    pub fn is_crate_scoped(&self) -> bool {
        self.crate_src.is_some()
    }
}

/// Bound on the directory walk-up when resolving the crate root.
const MAX_WALK_UP: usize = 12;
/// Bound on source files enumerated (a crate's `src/` is small).
const MAX_SOURCE_FILES: usize = 4000;

/// Hermetic intra-crate module cycle analyzer.
pub struct ModuleCycleAnalyzer;

impl ModuleCycleAnalyzer {
    /// Construct the analyzer (stateless).
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Detect module import cycles for the crate containing `target`.
    #[must_use]
    pub fn analyze(&self, target: &Path) -> ModuleCycleReport {
        let Some(src) = find_crate_src(target) else {
            return ModuleCycleReport::default();
        };

        // 1. Enumerate source files → module path per file.
        let mut files = Vec::new();
        collect_rs(&src, &mut files);

        // 2. Map each file to its FULL module path; collect the known set.
        //    W4 (2026-07-02): was top-level-only, which collapsed a submodule
        //    cycle `a::b ↔ a::c` into a self-loop on `a` and discarded it, and
        //    ignored `use super::` / `use self::`. Full-path nodes fix both.
        let mut file_modules: Vec<(String, PathBuf)> = Vec::new();
        let mut known_full: HashSet<String> = HashSet::new();
        for f in &files {
            if let Ok(rel) = f.strip_prefix(&src)
                && let Some(mp) = module_path(rel)
            {
                if !mp.is_empty() {
                    known_full.insert(mp.clone());
                }
                file_modules.push((mp, f.clone()));
            }
        }

        // 3. Build the edge set (module → used module), resolving `crate::`,
        //    `super::`, and `self::` imports to the longest KNOWN module prefix.
        //    FP-safe: an edge is only added to a confirmed module (an item path
        //    resolves to its enclosing module), so module-vs-item ambiguity can
        //    never fabricate a false cycle.
        let mut edges: HashSet<(String, String)> = HashSet::new();
        for (mp, path) in &file_modules {
            if mp.is_empty() {
                continue; // crate root (lib.rs/main.rs) is not a cycle participant
            }
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };
            for tgt in resolve_use_targets(&content, mp, &known_full) {
                // A parent module using a child's types (or a child using its
                // parent's) is idiomatic Rust composition — a containment
                // relationship, NOT an architectural coupling cycle. Never let a
                // hierarchical edge seed an SCC (F1.8 fidelity); sibling coupling
                // (`gate_metrics` ↔ `gate_metrics_snapshot`) is unaffected.
                if tgt != *mp && !is_hierarchical(mp, &tgt) {
                    edges.insert((mp.clone(), tgt));
                }
            }
        }

        // 4. Kosaraju SCC → cycles (SCC with > 1 node).
        let cycles = extract_cycles(&edges);
        ModuleCycleReport {
            crate_src: Some(src),
            modules_analyzed: known_full.len(),
            cycles,
        }
    }
}

impl Default for ModuleCycleAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Ascend from `target` to the nearest `Cargo.toml`; return its `src/` dir.
fn find_crate_src(target: &Path) -> Option<PathBuf> {
    let mut dir: &Path = if target.is_dir() {
        target
    } else {
        target.parent()?
    };
    for _ in 0..MAX_WALK_UP {
        if dir.join("Cargo.toml").is_file() {
            let src = dir.join("src");
            return src.is_dir().then_some(src);
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
    None
}

/// Stack-based recursive walk collecting `.rs` files under `root` (bounded).
fn collect_rs(root: &Path, out: &mut Vec<PathBuf>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if out.len() >= MAX_SOURCE_FILES {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(p);
                if out.len() >= MAX_SOURCE_FILES {
                    return;
                }
            }
        }
    }
}

/// Derive a `::`-joined module path from a file path relative to `src/`.
/// `lib.rs`/`main.rs` at the root → crate root (empty string).
fn module_path(rel: &Path) -> Option<String> {
    let mut comps: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let last = comps.pop()?;
    let stem = last.strip_suffix(".rs")?;
    if comps.is_empty() {
        if stem == "lib" || stem == "main" {
            return Some(String::new());
        }
        return Some(stem.to_string());
    }
    if stem == "mod" {
        return Some(comps.join("::"));
    }
    comps.push(stem.to_string());
    Some(comps.join("::"))
}

/// Parent module of `m` (drops the last `::` segment); `""` for a top-level
/// module or the crate root. Used to resolve `super::` relative imports.
fn parent_module(m: &str) -> String {
    match m.rfind("::") {
        Some(i) => m[..i].to_string(),
        None => String::new(),
    }
}

/// Whether module paths `a` and `b` are in an ancestor/descendant (containment)
/// relationship — one is a strict `::`-delimited prefix of the other, e.g.
/// `conflict` and `conflict::sla`. A parent using a child (or vice versa) is
/// idiomatic Rust composition, so such an edge must never seed a cycle SCC. The
/// `::` delimiter is load-bearing: `gate_metrics` is **not** an ancestor of the
/// sibling `gate_metrics_snapshot` (no `gate_metrics::` prefix), so a genuine
/// cycle between siblings is still detected.
fn is_hierarchical(a: &str, b: &str) -> bool {
    b.starts_with(&format!("{a}::")) || a.starts_with(&format!("{b}::"))
}

/// The `::`-separated leading identifiers of `s`, stopping at the first
/// non-path token (whitespace, `;`, `,`, `{`, ` as `) or a relative keyword.
fn path_segments(s: &str) -> Vec<String> {
    let mut segs = Vec::new();
    for raw in s.trim().split("::") {
        let id: String = raw
            .trim()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if id.is_empty() || id == "self" || id == "super" || id == "crate" {
            break;
        }
        segs.push(id);
    }
    segs
}

/// Longest joined prefix of `segs` that is a KNOWN module path. This maps an
/// item path (`a::b::Foo`) to its enclosing module (`a::b`) and a submodule
/// path (`a::b::c`) to itself, without ever guessing.
fn longest_known_prefix(segs: &[String], known: &HashSet<String>) -> Option<String> {
    for len in (1..=segs.len()).rev() {
        let cand = segs[..len].join("::");
        if known.contains(&cand) {
            return Some(cand);
        }
    }
    None
}

/// Resolve one import path fragment `after` (the text following a `crate::` /
/// `self::` / `super::` marker) against `base` (the resolved module prefix for
/// that marker), inserting each confirmed target module into `out`.
fn add_resolved(after: &str, base: &[String], known: &HashSet<String>, out: &mut HashSet<String>) {
    let mut segs = base.to_vec();
    segs.extend(path_segments(after));
    if let Some(m) = longest_known_prefix(&segs, known) {
        out.insert(m);
    }
}

/// Per-line mask marking every line that lives inside a `#[cfg(test)]`-gated
/// module (or is a `#[cfg(test)]`-gated single `use`). A test-only import is
/// scaffolding, NOT a production architectural coupling, so F1.8 must not count
/// its edge — otherwise a `#[cfg(test)] mod tests { use crate::… }` fabricates a
/// spurious cycle that no production build has (2026-07-02 calibration).
///
/// Brace-depth is tracked from the gated module's opening `{` to its matching
/// `}`. Test modules are conventionally the last item in a file, so an early
/// exit on a brace inside a string can only *keep* an extra edge (never drop a
/// production one), and the mask is a strict superset of the true test region in
/// the common case — the conservative direction for a cycle detector.
fn cfg_test_mask(content: &str) -> Vec<bool> {
    let lines: Vec<&str> = content.lines().collect();
    let mut mask = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().starts_with("#[cfg(test)]") {
            // Skip blank lines to find what the attribute gates.
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < lines.len() {
                let next = lines[j].trim();
                if next.starts_with("mod ") || next.starts_with("pub mod ") {
                    // Gated module: mask the attribute + body until braces balance.
                    mask[i] = true;
                    let mut depth: i32 = 0;
                    let mut opened = false;
                    let mut k = j;
                    while k < lines.len() {
                        mask[k] = true;
                        for c in lines[k].chars() {
                            if c == '{' {
                                depth += 1;
                                opened = true;
                            } else if c == '}' {
                                depth -= 1;
                            }
                        }
                        if opened && depth <= 0 {
                            break;
                        }
                        k += 1;
                    }
                    i = k + 1;
                    continue;
                } else if next.starts_with("use ") || next.starts_with("pub use ") {
                    // Gated single import.
                    mask[i] = true;
                    mask[j] = true;
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    mask
}

/// Resolve every `use crate::… / super::… / self::…` import in `content` to the
/// set of KNOWN modules it references, relative to the importing file's module
/// path `current`. Handles grouped `use crate::{A, B::c}` and multiple markers
/// per line. Only confirmed modules are returned (FP-safe — see step 3).
fn resolve_use_targets(content: &str, current: &str, known: &HashSet<String>) -> HashSet<String> {
    let mut out = HashSet::new();
    let markers: [(&str, Vec<String>); 3] = [
        ("crate::", Vec::new()),
        (
            "self::",
            if current.is_empty() {
                Vec::new()
            } else {
                current.split("::").map(str::to_string).collect()
            },
        ),
        ("super::", {
            let p = parent_module(current);
            if p.is_empty() {
                Vec::new()
            } else {
                p.split("::").map(str::to_string).collect()
            }
        }),
    ];
    let cfg_test = cfg_test_mask(content);
    for (idx, line) in content.lines().enumerate() {
        if cfg_test.get(idx).copied().unwrap_or(false) {
            continue; // #[cfg(test)]-gated import → not a production coupling edge
        }
        let l = line.trim_start();
        if !(l.starts_with("use ") || l.starts_with("pub use ")) {
            continue;
        }
        for (marker, base) in &markers {
            let mut rest = l;
            while let Some(pos) = rest.find(marker) {
                let after = &rest[pos + marker.len()..];
                if let Some(group) = after.strip_prefix('{') {
                    let end = group.find('}').unwrap_or(group.len());
                    for part in group[..end].split(',') {
                        add_resolved(part, base, known, &mut out);
                    }
                } else {
                    add_resolved(after, base, known, &mut out);
                }
                rest = &rest[pos + marker.len()..];
            }
        }
    }
    out
}

/// Build a DiGraph from `(src, dst)` module edges and return SCCs with > 1 node.
fn extract_cycles(edges: &HashSet<(String, String)>) -> Vec<Vec<String>> {
    let mut graph: DiGraph<String, ()> = DiGraph::new();
    let mut node_map: HashMap<String, NodeIndex> = HashMap::new();
    for (src, dst) in edges {
        let s = *node_map
            .entry(src.clone())
            .or_insert_with(|| graph.add_node(src.clone()));
        let d = *node_map
            .entry(dst.clone())
            .or_insert_with(|| graph.add_node(dst.clone()));
        graph.add_edge(s, d, ());
    }
    let mut cycles = Vec::new();
    for scc in kosaraju_scc(&graph) {
        if scc.len() > 1 {
            let mut path: Vec<String> = scc.iter().map(|&i| graph[i].clone()).collect();
            path.sort();
            cycles.push(path);
        }
    }
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a temp crate: `Cargo.toml` + `src/<name>.rs` files from
    /// `(module_file, content)` pairs.
    fn make_crate(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmpdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"c\"\nversion=\"0.1.0\"\n",
        )
        .expect("manifest");
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).expect("src");
        for (name, content) in files {
            let p = src.join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).expect("subdir");
            }
            std::fs::write(p, content).expect("file");
        }
        dir
    }

    #[test]
    fn non_crate_target_is_not_crate_scoped() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let loose = dir.path().join("loose.rs");
        std::fs::write(&loose, "fn x() {}\n").expect("write");
        let report = ModuleCycleAnalyzer::new().analyze(&loose);
        assert!(!report.is_crate_scoped(), "no Cargo.toml ancestor");
        assert_eq!(report.cycle_count(), 0);
    }

    #[test]
    fn acyclic_chain_has_no_cycles() {
        // lib → a → b → c (a uses crate::b, b uses crate::c)
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\npub mod c;\n"),
            ("a.rs", "use crate::b;\npub fn fa() {}\n"),
            ("b.rs", "use crate::c;\npub fn fb() {}\n"),
            ("c.rs", "pub fn fc() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert!(report.is_crate_scoped());
        assert_eq!(
            report.cycle_count(),
            0,
            "linear chain a→b→c must be acyclic, got {:?}",
            report.cycles
        );
    }

    #[test]
    fn mutual_dependency_is_one_cycle() {
        // a ↔ b (a uses crate::b, b uses crate::a)
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\n"),
            ("a.rs", "use crate::b;\npub fn fa() {}\n"),
            ("b.rs", "use crate::a;\npub fn fb() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(
            report.cycle_count(),
            1,
            "a↔b is one cycle, got {:?}",
            report.cycles
        );
        let scc = report.cycles.first().expect("one scc");
        assert!(scc.contains(&"a".to_string()) && scc.contains(&"b".to_string()));
    }

    #[test]
    fn submodule_cycle_via_super_is_detected() {
        // W4 (2026-07-02) regression guard: a cycle between two SUBMODULES of the
        // same top-level module (a::b ↔ a::c) that the old top-level-collapse
        // model discarded as a self-loop on `a`. Uses `super::` relative import.
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\n"),
            ("a/mod.rs", "pub mod b;\npub mod c;\n"),
            ("a/b.rs", "use super::c;\npub fn fb() {}\n"),
            ("a/c.rs", "use super::b;\npub fn fc() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(
            report.cycle_count(),
            1,
            "a::b ↔ a::c (via super::) is one submodule cycle, got {:?}",
            report.cycles
        );
        let scc = report.cycles.first().expect("one scc");
        assert!(
            scc.contains(&"a::b".to_string()) && scc.contains(&"a::c".to_string()),
            "SCC must be the two submodules, got {scc:?}"
        );
    }

    #[test]
    fn item_import_does_not_fabricate_cycle() {
        // FP guard: `use crate::a::Thing` (an ITEM in module `a`) from `b`, and
        // `a` does not import `b` → no cycle. Item-vs-module resolution must not
        // invent an edge to a non-module path.
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\n"),
            ("a.rs", "pub struct Thing;\npub fn fa() {}\n"),
            ("b.rs", "use crate::a::Thing;\npub fn fb() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(
            report.cycle_count(),
            0,
            "one-way item import is not a cycle, got {:?}",
            report.cycles
        );
    }

    #[test]
    fn three_node_cycle_is_one_scc() {
        // a → b → c → a
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\npub mod c;\n"),
            ("a.rs", "use crate::b;\n"),
            ("b.rs", "use crate::c;\n"),
            ("c.rs", "use crate::a;\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(report.cycle_count(), 1, "got {:?}", report.cycles);
        assert_eq!(report.cycles[0].len(), 3, "all 3 modules in the SCC");
    }

    #[test]
    fn self_reference_is_not_a_cycle() {
        // a/mod.rs uses crate::a::helper — same top-level → self-loop dropped.
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\n"),
            ("a/mod.rs", "pub mod helper;\nuse crate::a::helper;\n"),
            ("a/helper.rs", "pub fn h() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(report.cycle_count(), 0, "self-reference is no cycle");
    }

    #[test]
    fn grouped_use_is_parsed() {
        // a uses crate::{b, c}; b uses crate::a → a↔b cycle (c is acyclic).
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\npub mod c;\n"),
            ("a.rs", "use crate::{b, c};\n"),
            ("b.rs", "use crate::a;\n"),
            ("c.rs", "pub fn fc() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(report.cycle_count(), 1, "a↔b only, got {:?}", report.cycles);
    }

    #[test]
    fn module_path_derivation() {
        assert_eq!(module_path(Path::new("lib.rs")), Some(String::new()));
        assert_eq!(module_path(Path::new("main.rs")), Some(String::new()));
        assert_eq!(module_path(Path::new("foo.rs")), Some("foo".to_string()));
        assert_eq!(
            module_path(Path::new("foo/mod.rs")),
            Some("foo".to_string())
        );
        assert_eq!(
            module_path(Path::new("foo/bar.rs")),
            Some("foo::bar".to_string())
        );
    }

    #[test]
    fn resolve_use_targets_maps_items_to_known_modules() {
        // W4 (2026-07-02): the resolver maps item paths to their enclosing
        // KNOWN module and drops unknown paths (FP-safe).
        let known: std::collections::HashSet<String> = ["foo", "baz", "a", "b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let c = "use crate::foo::Bar;\npub use crate::baz;\nuse crate::{a, b::C};\n";
        let got = resolve_use_targets(c, "somewhere", &known);
        assert!(
            got.contains("foo"),
            "item crate::foo::Bar → module foo: {got:?}"
        );
        assert!(got.contains("baz"));
        assert!(got.contains("a"));
        assert!(got.contains("b"), "grouped b::C → module b: {got:?}");
        // An unknown path resolves to no module → no edge fabricated.
        let none = resolve_use_targets("use crate::unknown::X;\n", "x", &known);
        assert!(
            none.is_empty(),
            "unknown path must not create an edge: {none:?}"
        );
    }

    #[test]
    fn cfg_test_module_import_does_not_fabricate_cycle() {
        // 2026-07-02 calibration: `a` imports `crate::b` ONLY inside a
        // `#[cfg(test)] mod tests` block; `b` imports `crate::a` in production.
        // The test-only edge is scaffolding, not architectural coupling, so a↔b
        // must NOT be reported as a production cycle.
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\n"),
            (
                "a.rs",
                "pub fn fa() {}\n#[cfg(test)]\nmod tests {\n    use crate::b;\n    #[test]\n    fn t() { let _ = b::fb(); }\n}\n",
            ),
            ("b.rs", "use crate::a;\npub fn fb() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(
            report.cycle_count(),
            0,
            "test-only import must not fabricate a cycle, got {:?}",
            report.cycles
        );
    }

    #[test]
    fn cfg_test_gated_single_use_is_dropped() {
        // A `#[cfg(test)] use crate::b;` single item (not a mod) is test-only too.
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\n"),
            ("a.rs", "#[cfg(test)]\nuse crate::b;\npub fn fa() {}\n"),
            ("b.rs", "use crate::a;\npub fn fb() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(
            report.cycle_count(),
            0,
            "gated single use must be dropped, got {:?}",
            report.cycles
        );
    }

    #[test]
    fn production_cycle_still_detected_after_cfg_test_masking() {
        // Guard the guard: a genuine PRODUCTION a↔b cycle must still be found —
        // the cfg(test) mask must not suppress real couplings.
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\n"),
            (
                "a.rs",
                "use crate::b;\npub fn fa() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n",
            ),
            ("b.rs", "use crate::a;\npub fn fb() {}\n"),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(
            report.cycle_count(),
            1,
            "production a↔b cycle must survive masking, got {:?}",
            report.cycles
        );
    }

    #[test]
    fn is_hierarchical_distinguishes_containment_from_siblings() {
        // Parent ↔ child (containment): hierarchical.
        assert!(is_hierarchical("conflict", "conflict::sla"));
        assert!(is_hierarchical("conflict::sla", "conflict"));
        assert!(is_hierarchical("a", "a::b::c"));
        // Siblings with a shared prefix stem: NOT hierarchical (the `::` delimiter
        // is load-bearing — `gate_metrics` is not an ancestor of `gate_metrics_snapshot`).
        assert!(!is_hierarchical("gate_metrics", "gate_metrics_snapshot"));
        assert!(!is_hierarchical("a::b", "a::c"));
        assert!(!is_hierarchical("x", "y"));
    }

    #[test]
    fn parent_child_module_import_is_not_a_cycle() {
        // A parent module using a child's types AND the child using the parent's
        // is idiomatic Rust composition (containment), not a coupling cycle.
        let dir = make_crate(&[
            ("lib.rs", "pub mod conflict;\n"),
            (
                "conflict/mod.rs",
                "pub mod sla;\nuse crate::conflict::sla::Foo;\npub struct Bar;\npub fn use_foo(_: Foo) {}\n",
            ),
            (
                "conflict/sla.rs",
                "use crate::conflict::Bar;\npub struct Foo;\npub fn use_bar(_: Bar) {}\n",
            ),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(
            report.cycle_count(),
            0,
            "parent↔child containment must not be a cycle, got {:?}",
            report.cycles
        );
    }

    #[test]
    fn sibling_cycle_is_still_detected_after_parent_child_exclusion() {
        // Two SIBLING modules that genuinely use each other (the facade/back-edge
        // pattern, e.g. gate_metrics ↔ gate_metrics_snapshot) remain a real cycle.
        let dir = make_crate(&[
            (
                "lib.rs",
                "pub mod gate_metrics;\npub mod gate_metrics_snapshot;\n",
            ),
            (
                "gate_metrics.rs",
                "use crate::gate_metrics_snapshot::Snap;\npub struct Metrics;\npub fn f(_: Snap) {}\n",
            ),
            (
                "gate_metrics_snapshot.rs",
                "use crate::gate_metrics::Metrics;\npub struct Snap;\npub fn g(_: Metrics) {}\n",
            ),
        ]);
        let report = ModuleCycleAnalyzer::new().analyze(dir.path());
        assert_eq!(
            report.cycle_count(),
            1,
            "genuine sibling cycle must survive, got {:?}",
            report.cycles
        );
    }
}
