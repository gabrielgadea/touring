//! Class guard: the in-memory dependency index has ONE loader (C8, 12/11/2026).
//!
//! `SymbolStore::load_into_index` reads the symbols and nothing else. For years it was the whole
//! loader, called from half a dozen places, and the wiring's resolved file-to-file edges never
//! reached any of them: the readers asked `reverse_deps` for edges nobody fed. `ast::store::
//! load_index` (symbols + resolved edges + coverage + co-edit weights) is the loader now, and
//! `load_index_for` is its twin for a process that holds the two database paths and no runtime.
//! This test keeps a second, partial loader out: it scans `crates/*/src` production code and
//! fails when
//!
//! 1. a `.load_into_index(` call appears outside [`ALLOWED_PARTIAL_LOADS`] (or the count of one
//!    listed file drifts, which keeps the list honest),
//! 2. `touring-hook-runtime` calls `load_index(` anywhere but the runtime's one loader
//!    (`HookRuntime::load_symbol_index`, in `hook_runtime_ext.rs`),
//! 3. production code wraps a COPY of an index in an `Arc` (`Arc::new(idx.clone())`) instead of
//!    sharing the loaded generation,
//! 4. production code calls `.load_symbol_index(` outside `hook_runtime_ext.rs` — the index loads
//!    ON DEMAND, from `HookRuntime::index_generation` (the first read that needs it) and
//!    `reload_symbol_index_if_loaded` (the end of `index rebuild`), never straight from a caller, or
//! 5. the daemon's runtime builder (`touring-dispatch/src/daemon.rs`) touches the index at all —
//!    a project the daemon opened and nobody reads must cost no index (~133 MiB, ~259 ms on this
//!    workspace); an eager load there is the regression this item exists for.
//!
//! "Production" is the file up to the first `#[cfg(test)] mod`; a `#[cfg(test)]` on a `use` or an
//! item does not end it. `*_tests.rs`, `tests.rs` and `tests/` directories are skipped; comment
//! lines never count.

use std::path::{Path, PathBuf};

/// `(path suffix, calls, why a symbols-only load is right THERE)`. The count is exact.
const ALLOWED_PARTIAL_LOADS: &[(&str, usize, &str)] = &[
    (
        "touring-code/src/ast/store.rs",
        2,
        "the implementation of the loader itself: `load_index` (symbols, then edges) and the \
         no-wiring fallback of `load_index_for` (an absent wiring store gives what \
         `load_into_index` always gave: no resolved edge, no coverage)",
    ),
    (
        "touring-server/src/server/mod.rs",
        2,
        "the cross-project merge (`merge_cross_project_symbols`) and its periodic refresh load \
         OTHER projects' symbols into the unified index, symbols only on purpose: files of \
         different projects share keys (`src/lib.rs`), so merging their EDGES would join \
         unrelated graphs",
    ),
];

/// The one file of `touring-hook-runtime` that may call `load_index(`.
const RUNTIME_LOADER_FILE: &str = "touring-hook-runtime/src/hook_runtime_ext.rs";

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name != "target" && name != "tests")
            {
                rust_files_under(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !(name.ends_with("_tests.rs") || name == "tests.rs") {
                out.push(path);
            }
        }
    }
}

/// Every production `crates/<crate>/src/**/*.rs` of the workspace, with its path from `crates/`.
fn workspace_sources() -> Vec<(String, PathBuf)> {
    let Ok(crates) = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
    else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&crates) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        rust_files_under(&entry.path().join("src"), &mut files);
    }
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let rel = path
                .strip_prefix(&crates)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, path)
        })
        .collect()
}

/// Whether `line` opens a module (`mod x`, `pub mod x`, `pub(crate) mod x`).
fn opens_a_module(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("mod ") || line.starts_with("pub mod ") || line.starts_with("pub(crate) mod ")
}

/// The production lines of `text`, 1-based: everything except `#[cfg(test)]` MODULES. A test
/// module is skipped whole — to its `;` when out of line, else to the next line that is exactly
/// `}` (the closing brace of a top-level item in rustfmt'd code) — and production code goes on
/// after it: `server/mod.rs` has one near the top and its real code below. A `#[cfg(test)]` on a
/// `use` or an item (a test-only import) is not a module and hides nothing.
fn production_lines(text: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut production = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        if lines[at].trim_start().starts_with("#[cfg(test)]") {
            let module_line = (at + 1..lines.len()).find(|&n| {
                let l = lines[n].trim_start();
                !l.is_empty() && !l.starts_with("#[") && !l.starts_with("//")
            });
            if let Some(m) = module_line.filter(|&m| opens_a_module(lines[m])) {
                at = if lines[m].trim_end().ends_with(';') {
                    m + 1
                } else {
                    (m + 1..lines.len())
                        .find(|&n| lines[n] == "}")
                        .map_or(lines.len(), |close| close + 1)
                };
                continue;
            }
        }
        production.push((at + 1, lines[at]));
        at += 1;
    }
    production
}

/// The production code lines of `text` that hold `needle`, comment lines and `fn` definitions
/// skipped. Returns the 1-based line numbers.
fn production_calls(text: &str, needle: &str) -> Vec<usize> {
    production_lines(text)
        .into_iter()
        .filter(|(_, line)| {
            let code = line.trim_start();
            !code.starts_with("//") && code.contains(needle) && !code.contains("fn ")
        })
        .map(|(number, _)| number)
        .collect()
}

/// The detector detects: the shapes it promises to count, and the ones it must not.
#[test]
fn the_detector_counts_calls_and_skips_tests_comments_and_definitions() {
    let source = "\
pub fn load_into_index(&self) {}
// store.load_into_index(&mut idx);
let n = store.load_into_index(&mut idx)?;
let i = touring_code::ast::store::load_index(store, db)?;
#[cfg(test)]
use helper::only_for_tests;
let late = other.load_into_index(&mut idx)?;
#[cfg(test)]
#[allow(dead_code)]
mod tests {
    fn t() { store.load_into_index(&mut idx).unwrap(); }
}
";
    assert_eq!(
        production_calls(source, ".load_into_index("),
        vec![3, 7],
        "a cfg(test) `use` does not end production code; the test module does"
    );
    assert_eq!(production_calls(source, "::load_index("), vec![4]);
}

#[test]
fn nothing_loads_symbols_without_the_edges_except_where_a_reason_is_written() {
    let sources = workspace_sources();
    assert!(
        sources.len() > 500,
        "the scan must see the workspace (found {} files)",
        sources.len()
    );
    let mut offenders = Vec::new();
    let mut seen_allowed = vec![0usize; ALLOWED_PARTIAL_LOADS.len()];
    for (rel, path) in &sources {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let calls = production_calls(&text, ".load_into_index(");
        if calls.is_empty() {
            continue;
        }
        match ALLOWED_PARTIAL_LOADS
            .iter()
            .position(|(suffix, _, _)| rel.ends_with(suffix))
        {
            Some(slot) => seen_allowed[slot] += calls.len(),
            None => offenders.extend(calls.iter().map(|line| format!("{rel}:{line}"))),
        }
    }
    assert!(
        offenders.is_empty(),
        "`load_into_index` loads symbols only and leaves the wiring's file edges out; load the \
         index with `ast::store::load_index` (open runtime) or `ast::store::load_index_for` \
         (a store and a knowledge path), or list the site in ALLOWED_PARTIAL_LOADS with the \
         reason; calls at: {offenders:#?}"
    );
    for ((suffix, expected, _), seen) in ALLOWED_PARTIAL_LOADS.iter().zip(&seen_allowed) {
        assert_eq!(
            seen, expected,
            "{suffix} is allowed {expected} partial load(s) and has {seen}: update the list \
             (and its reason) when a call is added or removed"
        );
    }
}

/// Whether `line` wraps a COPY of an index in an `Arc` — `Arc::new(idx.clone())`,
/// `Arc::new(self.symbol_index.clone())` — the way `pre_read`, `pre_tool_use` and `post_write`
/// shared the index before the runtime held it in an `Arc` (the whole index copied per hook call).
fn arcs_a_copy_of_an_index(line: &str) -> bool {
    line.match_indices("Arc::new(").any(|(at, opener)| {
        let tail = &line[at + opener.len()..];
        tail.find(".clone())").is_some_and(|end| {
            let operand = tail[..end]
                .trim_start_matches(['&', '*', '('])
                .trim_end_matches(')');
            let name = operand.rsplit('.').next().unwrap_or(operand);
            !operand.contains('(') && (name == "idx" || name == "index" || name.ends_with("_index"))
        })
    })
}

#[test]
fn the_copy_detector_flags_arc_new_of_a_cloned_index_and_nothing_else() {
    for flagged in [
        "BlastRadiusEngine::bfs_only(Arc::new(idx.clone()))",
        "let shared = Arc::new(index.clone());",
        "builder.with_symbol_index(std::sync::Arc::new(runtime.infra.symbol_index.clone()))",
    ] {
        assert!(arcs_a_copy_of_an_index(flagged), "must flag: {flagged}");
    }
    for clean in [
        "let shared = Arc::new(config.clone());",
        "builder.with_symbol_index(Arc::clone(generation.index()))",
        "let empty = Arc::new(SymbolIndex::new());",
        "let engine = Arc::new(make_index().clone());",
    ] {
        assert!(!arcs_a_copy_of_an_index(clean), "must not flag: {clean}");
    }
}

#[test]
fn no_production_code_copies_the_whole_index_to_share_it() {
    let mut offenders = Vec::new();
    for (rel, path) in workspace_sources() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (number, line) in production_lines(&text) {
            let code = line.trim_start();
            if !code.starts_with("//") && arcs_a_copy_of_an_index(code) {
                offenders.push(format!("{rel}:{number}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "an index is shared by `Arc` (`IndexSlot::get` hands out the loaded generation: \
         `Arc::clone(generation.index())`), never copied per call — the clone cost 136 MB of \
         allocation churn per hook on a real project; copies at: {offenders:#?}"
    );
}

#[test]
fn the_runtime_has_exactly_one_caller_of_the_full_loader() {
    let mut callers = Vec::new();
    for (rel, path) in workspace_sources() {
        if !rel.starts_with("touring-hook-runtime/src/") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in production_calls(&text, "::load_index(") {
            callers.push(format!("{rel}:{line}"));
        }
    }
    assert_eq!(
        callers.len(),
        1,
        "the runtime's index is loaded by `HookRuntime::load_symbol_index` and nothing else \
         (`index_generation` and `reload_symbol_index_if_loaded` call that method); callers: \
         {callers:#?}"
    );
    assert!(
        callers[0].starts_with(RUNTIME_LOADER_FILE),
        "the one loader lives in {RUNTIME_LOADER_FILE}, found {callers:?}"
    );
}

/// The daemon's runtime builder, as a path from `crates/`.
const DAEMON_FILE: &str = "touring-dispatch/src/daemon.rs";

/// The calls that read or load the dependency index. The daemon's runtime builder makes none.
const INDEX_TOUCHING_CALLS: &[&str] = &[
    ".load_symbol_index(",
    ".reload_symbol_index_if_loaded(",
    ".index_generation(",
    ".get_symbol_index(",
];

#[test]
fn the_index_loads_on_demand_so_only_the_runtime_calls_its_loader() {
    // Item 4: `.load_symbol_index(` is called by `index_generation` and
    // `reload_symbol_index_if_loaded`, both in the runtime's loader file, and by no one else.
    let mut callers = Vec::new();
    for (rel, path) in workspace_sources() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in production_calls(&text, ".load_symbol_index(") {
            callers.push(format!("{rel}:{line}"));
        }
    }
    assert!(
        callers.iter().all(|c| c.starts_with(RUNTIME_LOADER_FILE)),
        "the index loads on demand: `HookRuntime::index_generation` on the first read that needs \
         it, `reload_symbol_index_if_loaded` when `index rebuild` ends. A caller that loads it \
         straight (the daemon, a handler) makes every project it opens pay for an index nobody \
         read; call `index_generation()` where the graph is needed instead. Callers: {callers:#?}"
    );
    assert_eq!(
        callers.len(),
        2,
        "`index_generation` and `reload_symbol_index_if_loaded` are the two callers; {callers:#?}"
    );
}

#[test]
fn the_daemon_does_not_touch_the_index_when_it_builds_a_project_runtime() {
    let sources = workspace_sources();
    let (_, daemon) = sources
        .iter()
        .find(|(rel, _)| rel == DAEMON_FILE)
        .unwrap_or_else(|| {
            panic!(
                "the scan must see {DAEMON_FILE} (found {} files): the guard is blind otherwise",
                sources.len()
            )
        });
    let text = std::fs::read_to_string(daemon).expect("the daemon source is readable");
    let mut offenders = Vec::new();
    for needle in INDEX_TOUCHING_CALLS {
        offenders.extend(
            production_calls(&text, needle)
                .into_iter()
                .map(|line| format!("{DAEMON_FILE}:{line} {needle}")),
        );
    }
    assert!(
        offenders.is_empty(),
        "the daemon builds a runtime for every project it is asked about; the dependency index \
         (symbols + wiring edges: ~133 MiB and ~259 ms on this workspace) loads on the FIRST read \
         that needs it (`HookRuntime::index_generation`), not here. Remove the call; calls at: \
         {offenders:#?}"
    );
    // What the daemon DOES do is tell the runtime it may load on demand (and log the load): a
    // runtime without it never loads for the hooks' graph signals, which is how a one-hook
    // `touring-hook` process stays as it was before C8 — and, if the daemon forgot it, how every
    // graph signal would go quiet again with no test failing.
    assert!(
        !production_calls(&text, "index_on_demand = true").is_empty(),
        "the daemon must set `infra.index_on_demand = true` on the runtime it builds: it keeps \
         the runtime across hooks, so the index loads once on the first read that needs it"
    );
}

#[test]
fn the_daemon_guard_sees_an_eager_load_when_one_is_written() {
    // The detector for the test above, on a source that carries the regression: it must count
    // the call, and must not count the comment that explains why the load is not there.
    let source = "\
// the index loads lazily: `index_generation()` on the first read
let rt = build_runtime(root);
eprintln!(\"{}\", new_rt.load_symbol_index().describe());
";
    assert_eq!(production_calls(source, ".load_symbol_index("), vec![3]);
    assert!(production_calls(source, ".index_generation(").is_empty());
}
