//! F14 probe: what the additive type-qualified (R1) and receiver-method (R2) inference passes
//! do to the wiring's orphan list.
//!
//! It runs the PRODUCTION inference (`FileReferences::inferred` + `record_inferred_consumers`,
//! the one step both the rebuild and the hook path call) over every Rust file under
//! `<root>/crates`, on a COPY of a project's knowledge database (never the live one), and
//! reports what changed:
//!
//! * the in-scope orphan count before and after, with the predicate `touring wiring orphans`
//!   uses (`orphan_symbols_with_trust`, default and `trusted`);
//! * every consumer row the run ADDED, split into rows the by-name passes of the same code
//!   would have written anyway (drift between the snapshot and today's extractors) and rows
//!   only R1 or R2 explain;
//! * whether any consumer row present before vanished or changed origin (it must not);
//! * the consumer rows of the symbols named with `--watch module::symbol`.
//!
//! `--refresh <rel path>` (repeatable) first runs the hook path's own `refresh_file_wiring` on
//! that file — resolved imports included — which needs the copy to sit at
//! `<root>/.claude/touring/knowledge.db` (the import pass has no workspace root otherwise); the
//! F15 check refreshes the files that import through a raw-identifier module path.
//!
//! ```text
//! cargo run -p touring-hook-runtime --example f14_orphan_probe -- \
//!   --knowledge copy.db --root /path/to/project --out probe.json \
//!   [--refresh crates/x/src/z.rs ...] [--watch crates/x/src/y.rs::symbol ...]
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use touring_code::ast::Lang;
use touring_code::ast::graph::{FileReferences, InferredReferences};
use touring_hook_runtime::knowledge::FileKnowledgeDB;

/// `(module_file, symbol_name, consumer_file)`.
type Row = (String, String, String);
/// `(module_file, symbol_name)`.
type Symbol = (String, String);

/// Directories that never hold first-party sources.
const SKIP_DIRS: [&str; 4] = ["target", ".git", "node_modules", ".claude"];

/// The label of a row a by-name pass of today's code writes as well.
const DRIFT: &str = "by-name pass (snapshot drift)";

/// What the wiring held at one instant.
struct Snapshot {
    /// Every consumer row, with the origin that wrote it.
    rows: BTreeMap<Row, String>,
    /// In-scope orphans, default predicate.
    orphans: BTreeSet<Symbol>,
    /// In-scope orphans when only non-heuristic edges count.
    trusted: BTreeSet<Symbol>,
}

/// What the inference run learned about each file it fed.
#[derive(Default)]
struct Inference {
    /// Rows the by-name passes of the same code write for the files fed.
    by_name: HashSet<Row>,
    /// The names each file calls WITH a qualifier (`Type::name(..)`).
    qualified_names: HashMap<String, HashSet<String>>,
    files_run: usize,
    recorded: usize,
}

impl Inference {
    /// Which pass explains `row`: a by-name pass of today's code, or R1/R2.
    fn explain(&self, row: &Row) -> &'static str {
        if self.by_name.contains(row) {
            DRIFT
        } else if self
            .qualified_names
            .get(&row.2)
            .is_some_and(|names| names.contains(&row.1))
        {
            "R1-or-R2"
        } else {
            "R2"
        }
    }
}

/// `--flag value` pairs; `--watch` and `--refresh` may repeat.
type Flags = (BTreeMap<String, String>, Vec<String>, Vec<String>);

fn flags() -> Result<Flags, String> {
    let mut map = BTreeMap::new();
    let (mut watch, mut refresh) = (Vec::new(), Vec::new());
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--watch" => watch.push(value),
            "--refresh" => refresh.push(value),
            _ => {
                map.insert(flag, value);
            }
        }
    }
    Ok((map, watch, refresh))
}

/// Every `.rs` file under `<root>/crates`, skipping build and VCS directories.
fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.join("crates")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if !SKIP_DIRS.iter().any(|skip| entry.file_name() == *skip) {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The in-scope orphans under `touring wiring orphans`' own predicate.
fn orphans(db: &FileKnowledgeDB, trusted: bool) -> Result<BTreeSet<Symbol>, String> {
    let entries = db
        .orphan_symbols_with_trust(trusted)
        .map_err(|e| format!("orphan query: {e}"))?;
    Ok(entries
        .into_iter()
        .map(|e| (e.module_file, e.symbol_name))
        .collect())
}

fn snapshot(db: &FileKnowledgeDB) -> Result<Snapshot, String> {
    let mut stmt = db
        .conn_ref()
        .prepare(
            "SELECT module_file, symbol_name, consumer_file, contract_source
             FROM wiring_map WHERE consumer_file IS NOT NULL",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(((r.get(0)?, r.get(1)?, r.get(2)?), r.get::<_, String>(3)?))
        })
        .and_then(|it| it.collect::<Result<BTreeMap<Row, String>, _>>())
        .map_err(|e| e.to_string())?;
    Ok(Snapshot {
        rows,
        orphans: orphans(db, false)?,
        trusted: orphans(db, true)?,
    })
}

/// The rows the by-name passes of today's code write for `rel`, read straight from the
/// producer rows (they never depend on consumer rows).
fn by_name_rows(db: &FileKnowledgeDB, rel: &str, refs: &InferredReferences) -> Vec<Symbol> {
    let mut found = db
        .find_producer_modules_for_qualified(&refs.qualified_calls, Some(rel))
        .unwrap_or_default();
    found.extend(
        db.find_producer_modules_for_methods(&refs.method_names, 4, Some(rel))
            .unwrap_or_default(),
    );
    found.extend(
        db.find_producer_modules_for_types(&refs.type_refs, 4, Some(rel))
            .unwrap_or_default(),
    );
    found
}

/// Feeds every Rust file under `root/crates` to the production inference step.
fn infer_all(db: &FileKnowledgeDB, root: &Path) -> Inference {
    let mut run = Inference::default();
    for path in rust_files(root) {
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().to_string();
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let refs = FileReferences::new(&content, Lang::Rust).inferred();
        if refs.method_names.is_empty()
            && refs.type_refs.is_empty()
            && refs.qualified_calls.is_empty()
        {
            continue;
        }
        run.files_run += 1;
        for (module, symbol) in by_name_rows(db, &rel, &refs) {
            run.by_name.insert((module, symbol, rel.clone()));
        }
        run.qualified_names.insert(
            rel.clone(),
            refs.qualified_calls
                .iter()
                .map(|(_, n)| n.clone())
                .collect(),
        );
        run.recorded += db.record_inferred_consumers(
            &rel,
            &refs.method_names,
            &refs.type_refs,
            &refs.qualified_calls,
        );
    }
    run
}

/// The consumer rows of each `module::symbol` the caller asked to watch.
fn watched(watch: &[String], before: &Snapshot, after: &Snapshot) -> Vec<Value> {
    watch
        .iter()
        .map(|w| {
            let (module, symbol) = w.split_once("::").unwrap_or((w.as_str(), ""));
            let hit = |(m, s, _): &Row| m == module && s == symbol;
            let consumers: Vec<Value> = after
                .rows
                .iter()
                .filter(|(row, _)| hit(row))
                .map(|((_, _, c), origin)| json!({"consumer": c, "origin": origin}))
                .collect();
            let was = before.rows.keys().filter(|row| hit(row)).count();
            json!({"symbol": w, "consumer_rows_before": was, "consumers_after": consumers})
        })
        .collect()
}

/// One symbol that left the orphan list, with the rows that credit it now.
fn credit(symbol: &Symbol, after: &Snapshot, inf: &Inference) -> Value {
    let consumers: Vec<Value> = after
        .rows
        .iter()
        .filter(|((m, s, _), _)| *m == symbol.0 && *s == symbol.1)
        .map(|(row, origin)| {
            json!({"consumer": row.2, "origin": origin, "explained_by": inf.explain(row)})
        })
        .collect();
    json!({"module": symbol.0, "symbol": symbol.1, "consumers": consumers})
}

/// The rows present only in `after`, then those the by-name passes also explain (drift)
/// against those only R1/R2 explain.
fn added_rows<'a>(
    before: &Snapshot,
    after: &'a Snapshot,
    inf: &Inference,
) -> (Vec<&'a Row>, Vec<&'a Row>) {
    let added = after
        .rows
        .keys()
        .filter(|row| !before.rows.contains_key(*row));
    added.partition(|row| inf.by_name.contains(*row))
}

/// Rows of `before` that are gone from `after`, and rows whose origin changed.
fn lost_and_changed(before: &Snapshot, after: &Snapshot) -> (Vec<Row>, Vec<Value>) {
    let lost = before
        .rows
        .keys()
        .filter(|row| !after.rows.contains_key(*row))
        .cloned()
        .collect();
    let changed = before
        .rows
        .iter()
        .filter_map(|(row, was)| {
            let now = after.rows.get(row)?;
            (now != was).then(|| json!({"row": row, "before": was, "after": now}))
        })
        .collect();
    (lost, changed)
}

fn report(watch: &[String], before: &Snapshot, after: &Snapshot, inf: &Inference) -> Value {
    let (drift, only_new) = added_rows(before, after, inf);
    let (lost, changed) = lost_and_changed(before, after);
    let by_tag = only_new
        .iter()
        .fold(BTreeMap::<&str, usize>::new(), |mut m, row| {
            *m.entry(inf.explain(row)).or_default() += 1;
            m
        });
    let left: Vec<Symbol> = before.orphans.difference(&after.orphans).cloned().collect();
    let credits: Vec<Value> = left.iter().map(|s| credit(s, after, inf)).collect();
    let only_new_credits = credits
        .iter()
        .filter(|c| {
            c["consumers"]
                .as_array()
                .is_some_and(|rows| rows.iter().all(|r| r["explained_by"] != DRIFT))
        })
        .count();
    json!({
        "files_run": inf.files_run,
        "record_inferred_consumers_returned": inf.recorded,
        "consumer_rows": {
            "before": before.rows.len(), "after": after.rows.len(),
            "added": drift.len() + only_new.len(),
            "added_also_written_by_by_name_passes_snapshot_drift": drift.len(),
            "added_only_r1_r2_explain": only_new.len(),
            "added_only_r1_r2_explain_by_tag": by_tag,
            "lost": lost.len(), "origin_changed": changed.len(),
        },
        "orphans_in_scope": {
            "default_before": before.orphans.len(), "default_after": after.orphans.len(),
            "trusted_before": before.trusted.len(), "trusted_after": after.trusted.len(),
            "left_the_list": left.len(),
            "left_the_list_with_no_by_name_credit": only_new_credits,
            "entered_the_list": after.orphans.difference(&before.orphans).count(),
        },
        "lost_rows_sample": lost.iter().take(10).collect::<Vec<_>>(),
        "origin_changed_sample": changed.iter().take(10).collect::<Vec<_>>(),
        "watched": watched(watch, before, after),
        "credited": credits,
        "added_only_r1_r2_sample": only_new.iter().take(60)
            .map(|row| json!({"row": row, "tag": inf.explain(row)})).collect::<Vec<_>>(),
    })
}

/// The hook path's own refresh (`refresh_file_wiring`: producers, resolved imports, inferred
/// edges) for each `--refresh` file, so a change to the import resolver shows in the rows it
/// writes. Needs a database at `<root>/.claude/touring/knowledge.db`: the import pass is
/// skipped without a workspace root.
fn refresh_files(db: &FileKnowledgeDB, root: &Path, files: &[String]) -> usize {
    files
        .iter()
        .filter(|rel| match std::fs::read_to_string(root.join(rel)) {
            Ok(content) => {
                touring_hook_runtime::wiring::refresh_file_wiring(db, rel, "rust", &content);
                true
            }
            Err(_) => false,
        })
        .count()
}

fn run(
    map: &BTreeMap<String, String>,
    watch: &[String],
    refresh: &[String],
) -> Result<Value, String> {
    let need = |k: &str| {
        map.get(k)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{k} is required"))
    };
    let knowledge = need("--knowledge")?;
    let root = need("--root")?;
    let db = FileKnowledgeDB::new(&knowledge).map_err(|e| format!("open {knowledge:?}: {e}"))?;
    let before = snapshot(&db)?;
    let refreshed = refresh_files(&db, &root, refresh);
    eprintln!(
        "f14_orphan_probe: refreshed {refreshed} of {} --refresh files",
        refresh.len()
    );
    let inference = infer_all(&db, &root);
    let after = snapshot(&db)?;
    Ok(report(watch, &before, &after, &inference))
}

fn main() {
    let (map, watch, refresh) = match flags() {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("f14_orphan_probe: {e}");
            std::process::exit(2);
        }
    };
    let report = match run(&map, &watch, &refresh) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("f14_orphan_probe: {e}");
            std::process::exit(2);
        }
    };
    let text = serde_json::to_string_pretty(&report).unwrap_or_default();
    match map.get("--out") {
        Some(out) => {
            if let Err(e) = std::fs::write(out, &text) {
                eprintln!("f14_orphan_probe: write {out}: {e}");
                std::process::exit(1);
            }
            println!("{}", report["orphans_in_scope"]);
        }
        None => println!("{text}"),
    }
}
