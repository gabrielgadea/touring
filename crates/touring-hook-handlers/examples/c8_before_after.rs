//! C8 probe: what the blast readers say, and what a `pre_read` costs per call, before and
//! after the wiring's file edges are loaded into the `SymbolIndex` AND the runtime holds that
//! index in an `Arc` with one HNSW cache per generation.
//!
//! It runs the PRODUCTION reader code, unchanged, on two indexes built from the same
//! SNAPSHOT of a project's two databases (never the live ones):
//!
//! * `--mode before`: the pre-C8 shape. The index comes from `SymbolStore::load_into_index`
//!   (the `dependencies` table, empty in production) and every pipeline is built over a CLONE
//!   of it with a fresh HNSW cache, as `pre_read` did per call;
//! * `--mode after`: the C8 shape. The index comes from `load_index(&SymbolStore,
//!   &FileKnowledgeDB)` (symbols, the wiring's file edges, coverage, co-edit weights), is
//!   shared by `Arc` and the HNSW cache is built once and reused
//!   (`build_graph_pipeline_with_cache`), as the runtime's `IndexGeneration` does.
//!
//! One mode per process: `VmRSS` is only honest when nothing else was loaded before it.
//! The output is one JSON document (`--out`) that `c8_before_after_report.py` compares with
//! an independent SQL oracle.
//!
//! ```text
//! cargo run -p touring-hook-handlers --features pre-hooks,post-hooks --example c8_before_after -- \
//!   --mode after --symbols c8/symbols.db --knowledge c8/knowledge.db \
//!   --selection c8/selection.json --out c8/after.json [--reps 3]
//! ```
//!
//! Per file: reader 1 (`blast_radius_signal`), the graph signal pipeline of `pre_read` at CILA 4
//! (and at CILA 5 for a few files: the HNSW layer, first call and warm), `blast_radius_with_depth`
//! against `blast_extent` (the counts-only walk), and `weighted_blast_radius`. The retired
//! `bfs_hop_signal` (which cloned the whole index per read) is gone from `pre_read`; in `before`
//! mode its clone is reported as `retired_signal_clone_ms`, the BFS it then ran not included.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use serde_json::{Value, json};
use touring_analysis::HnswStrategy;
use touring_code::ast::graph::pheromone::PheromoneGraph;
use touring_code::ast::store::load_index;
use touring_code::ast::{SymbolIndex, SymbolStore};
use touring_hook_handlers::shared::signal_pipeline::{
    SignalContext, build_graph_pipeline, build_graph_pipeline_with_cache,
};
use touring_hook_runtime::knowledge::FileKnowledgeDB;
use touring_hook_runtime::shared::signals::blast_radius_signal;

/// Characters the pre_read pipeline may spend (the CILA-4 figure used in its own docs).
const PIPELINE_BUDGET: usize = 3200;

struct Args {
    mode: String,
    symbols: PathBuf,
    knowledge: PathBuf,
    selection: PathBuf,
    out: PathBuf,
    reps: usize,
}

/// What a `pre_read` holds while it answers: the index, the HNSW cache that goes with it,
/// and which mechanism builds the pipelines (`shared` = the C8 `Arc` + cache, else a clone).
struct World {
    idx: Arc<SymbolIndex>,
    hnsw: Arc<OnceLock<HnswStrategy>>,
    shared: bool,
}

/// `--flag value` pairs into a map; a flag with no value is an error.
fn flag_pairs() -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        map.insert(flag, value);
    }
    Ok(map)
}

fn read_args() -> Result<Args, String> {
    let map = flag_pairs()?;
    let need = |k: &str| map.get(k).cloned().ok_or_else(|| format!("missing {k}"));
    Ok(Args {
        mode: need("--mode")?,
        symbols: need("--symbols")?.into(),
        knowledge: need("--knowledge")?.into(),
        selection: need("--selection")?.into(),
        out: need("--out")?.into(),
        reps: map.get("--reps").and_then(|v| v.parse().ok()).unwrap_or(3),
    })
}

/// A `/proc/self/status` field in kB (`VmRSS`, `VmHWM`).
fn status_kb(field: &str) -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|l| l.starts_with(field))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn median_of(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples.get(samples.len() / 2).copied().unwrap_or(0.0)
}

fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Run `f` once for its value and first-call wall time (ms), then `reps` more times for a
/// median wall time (ms).
fn timed_first<T>(reps: usize, mut f: impl FnMut() -> T) -> (T, f64, f64) {
    let t = Instant::now();
    let value = f();
    let first = ms_since(t);
    let mut samples: Vec<f64> = (0..reps.max(1))
        .map(|_| {
            let t = Instant::now();
            let _ = f();
            ms_since(t)
        })
        .collect();
    (value, first, median_of(&mut samples))
}

/// [`timed_first`] without the first-call figure.
fn timed<T>(reps: usize, f: impl FnMut() -> T) -> (T, f64) {
    let (value, _, median) = timed_first(reps, f);
    (value, median)
}

fn index_stats(idx: &SymbolIndex) -> Value {
    json!({
        "symbol_names": idx.symbols.len(),
        "files_with_symbols": idx.file_to_symbols.len(),
        "reverse_deps_keys": idx.reverse_deps.len(),
        "reverse_deps_edges": idx.reverse_deps.values().map(Vec::len).sum::<usize>(),
        "dependency_sources": idx.dependencies.len(),
        "dependency_edges": idx.dependencies.values().map(Vec::len).sum::<usize>(),
        "edge_coverage": {
            "edges": idx.edge_coverage_summary().edges,
            "covered_files": idx.edge_coverage_summary().covered_files,
        },
    })
}

/// The graph pipeline of `pre_read` the way each mode reaches it: a fresh pipeline per call,
/// over a CLONE of the index with a fresh HNSW cache (`before`), or over the shared `Arc`
/// and the generation's HNSW cache (`after`).
fn pipeline_run(world: &World, rel: &str, cila: usize) -> Option<String> {
    let graph = Arc::new(RwLock::new(PheromoneGraph::new(0.1)));
    let pipeline = if world.shared {
        build_graph_pipeline_with_cache(
            Arc::clone(&world.idx),
            graph,
            PIPELINE_BUDGET,
            Arc::clone(&world.hnsw),
        )
    } else {
        build_graph_pipeline(Arc::new(world.idx.as_ref().clone()), graph, PIPELINE_BUDGET)
    };
    pipeline.execute(&SignalContext::new(rel, "").with_cila(cila))
}

fn signal_json(signal: Option<(f32, String)>) -> Value {
    signal.map_or(
        Value::Null,
        |(score, text)| json!({"score": score, "text": text}),
    )
}

fn weighted_json(weighted: &[(String, f64)], ms: f64) -> Value {
    // Hop-count costs are whole numbers; a fractional cost is an edge a co-edit weight
    // discounted (`1 / (1 + w)`), so this is the number of entries the weights moved.
    let fractional = weighted
        .iter()
        .filter(|(_, c)| (*c - c.round()).abs() > 1e-9)
        .count();
    json!({
        "n": weighted.len(),
        "fractional_cost": fractional,
        "top3": weighted.iter().take(3).map(|(p, c)| json!([p, c])).collect::<Vec<_>>(),
        "ms": ms,
    })
}

fn probe_file(world: &World, rel: &str, reps: usize, deep_cila: bool) -> Value {
    let idx = world.idx.as_ref();
    let (verbose, r1_ms) = timed(reps, || blast_radius_signal(Some(idx), rel, true));
    let compact = blast_radius_signal(Some(idx), rel, false);
    let (blast, r4_ms) = timed(reps, || idx.blast_radius_with_depth(rel, 4));
    let (extent, r4x_ms) = timed(reps, || idx.blast_extent(rel, 4));
    let (weighted, r4w_ms) = timed(reps, || idx.weighted_blast_radius(rel));
    let (cila4, p4_ms) = timed(reps.min(2), || pipeline_run(world, rel, 4));
    let cila5 = deep_cila.then(|| {
        let (out, first_ms, warm_ms) = timed_first(1, || pipeline_run(world, rel, 5));
        json!({
            "first_ms": first_ms,
            "ms": warm_ms,
            "len": out.as_ref().map(String::len),
            "text": out,
            "pre_read_first_ms": r1_ms + first_ms,
            "pre_read_warm_ms": r1_ms + warm_ms,
        })
    });
    json!({
        "file": rel,
        "own_key_importers": idx.reverse_deps.get(rel).map_or(0, Vec::len),
        "importers_of": idx.importers_of(rel).len(),
        "importers_status": format!("{:?}", idx.importers_status(rel)).chars().take(40).collect::<String>(),
        "r1_verbose": signal_json(verbose),
        "r1_compact": signal_json(compact),
        "r1_ms": r1_ms,
        "r4_blast": {
            "file_count": blast.file_count,
            "max_distance": blast.max_distance,
            "affected": blast.affected_files.len(),
            "ms": r4_ms,
        },
        "r4_extent": {
            "file_count": extent.file_count,
            "max_distance": extent.max_distance,
            "same_counts": extent.file_count == blast.file_count
                && extent.max_distance == blast.max_distance,
            "ms": r4x_ms,
        },
        "r4_weighted": weighted_json(&weighted, r4w_ms),
        "r345_pipeline_cila4": {"ms": p4_ms, "len": cila4.as_ref().map(String::len), "text": cila4},
        "pre_read_cila4_ms": r1_ms + p4_ms,
        "r345_pipeline_cila5": cila5,
    })
}

/// The `(file, group)` pairs of the selection document, in order.
fn selection_files(path: &PathBuf) -> Result<Vec<(String, String)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read selection: {e}"))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| format!("parse selection: {e}"))?;
    let files = doc["files"].as_array().ok_or("selection has no `files`")?;
    let field = |f: &Value, k: &str| f[k].as_str().unwrap_or_default().to_owned();
    Ok(files
        .iter()
        .map(|f| (field(f, "file"), field(f, "group")))
        .collect())
}

/// The index the mode names: the pre-C8 loader, or the one that also reads the wiring.
fn build_index(
    mode: &str,
    store: &SymbolStore,
    knowledge: &FileKnowledgeDB,
) -> Result<SymbolIndex, String> {
    match mode {
        "before" => {
            let mut idx = SymbolIndex::new();
            store
                .load_into_index(&mut idx)
                .map_err(|e| format!("load_into_index: {e}"))?;
            Ok(idx)
        }
        "after" => load_index(store, knowledge).map_err(|e| format!("load_index: {e}")),
        other => Err(format!("--mode must be before|after, got {other}")),
    }
}

/// `(median, max)` ms of 15 whole-index clones: what `pre_read` paid per call before the
/// `Arc` (once for the pipeline, once more for the retired `bfs_hop_signal`).
fn clone_cost(idx: &SymbolIndex) -> (f64, f64) {
    let mut samples: Vec<f64> = (0..15)
        .map(|_| {
            let t = Instant::now();
            let copy = idx.clone();
            let ms = ms_since(t);
            drop(copy);
            ms
        })
        .collect();
    let max = samples.iter().copied().fold(0.0, f64::max);
    (median_of(&mut samples), max)
}

/// The loader's SQL halves alone (page cache is warm by now: an upper bound of what the
/// loader adds over `load_into_index`, not a cold-start figure).
fn loader_sql_ms(knowledge: &FileKnowledgeDB) -> Value {
    json!({
        "file_dependency_edges": timed(3, || knowledge.file_dependency_edges().map(|e| e.len())).1,
        "wired_files": timed(3, || knowledge.wired_files().map(|e| e.len())).1,
        "co_edit_pairs": timed(3, || knowledge.co_edit_pairs().map(|e| e.len())).1,
    })
}

/// The first (cold) load of the mode, with the memory it cost, then the same loaders warm
/// (the page cache holds both databases by then): the loader's own cost without the
/// cold-read noise of the measured first load.
fn measure_load(
    mode: &str,
    store: &SymbolStore,
    knowledge: &FileKnowledgeDB,
) -> Result<(SymbolIndex, Value), String> {
    let (rss_before, anon_before) = (status_kb("VmRSS:"), status_kb("RssAnon:"));
    let t = Instant::now();
    let idx = build_index(mode, store, knowledge)?;
    let load_ms = ms_since(t);
    let (rss_after, anon_after, hwm_after_load) = (
        status_kb("VmRSS:"),
        status_kb("RssAnon:"),
        status_kb("VmHWM:"),
    );

    let warm_symbols_ms = timed(3, || {
        let mut fresh = SymbolIndex::new();
        store
            .load_into_index(&mut fresh)
            .map(|n| (n, fresh.symbols.len()))
    })
    .1;
    let warm_loader_ms = timed(3, || load_index(store, knowledge).map(|i| i.symbols.len())).1;
    let doc = json!({
        "load_ms": load_ms,
        "warm_load_ms": {"load_into_index": warm_symbols_ms, "load_index": warm_loader_ms},
        "rss_kb": {
            "before_load": rss_before,
            "after_load": rss_after,
            "anon_before_load": anon_before,
            "anon_after_load": anon_after,
            "hwm_after_load": hwm_after_load,
        },
        "loader_sql_ms": loader_sql_ms(knowledge),
    });
    Ok((idx, doc))
}

/// Every selected file through every reader. CILA 5 (the HNSW layer) runs on the first 3
/// hubs and on every 5th wired draw: it is the slow one.
fn probe_all(world: &World, files: &[(String, String)], reps: usize) -> Vec<Value> {
    files
        .iter()
        .enumerate()
        .map(|(n, (rel, group))| {
            let deep =
                (group == "top_indegree" && n < 3) || (group == "random_wired" && n % 5 == 0);
            let mut rec = probe_file(world, rel, reps, deep);
            rec["group"] = json!(group);
            rec
        })
        .collect()
}

fn run(args: &Args) -> Result<(), String> {
    let files = selection_files(&args.selection)?;
    let store = SymbolStore::new(&args.symbols).map_err(|e| format!("open symbols: {e}"))?;
    let knowledge =
        FileKnowledgeDB::new(&args.knowledge).map_err(|e| format!("open knowledge: {e}"))?;

    let (idx, mut doc) = measure_load(&args.mode, &store, &knowledge)?;
    let (clone_median, clone_max) = clone_cost(&idx);
    let t = Instant::now();
    let _hnsw = HnswStrategy::new(&idx, 10);
    let hnsw_ms = ms_since(t);
    drop(_hnsw);

    doc["mode"] = json!(args.mode);
    doc["reps"] = json!(args.reps);
    doc["index"] = index_stats(&idx);
    doc["clone_ms"] = json!({"median": clone_median, "max_of_15": clone_max});
    if args.mode == "before" {
        doc["retired_signal_clone_ms"] = json!(clone_median);
    }
    doc["hnsw_strategy_new_ms"] = json!(hnsw_ms);
    let world = World {
        idx: Arc::new(idx),
        hnsw: Arc::new(OnceLock::new()),
        shared: args.mode == "after",
    };
    doc["files"] = json!(probe_all(&world, &files, args.reps));
    // After the pipelines ran: what the process held at its peak, with the HNSW cache built
    // (`after`) or rebuilt and dropped per call (`before`).
    doc["hwm_end_kb"] = json!(status_kb("VmHWM:"));
    doc["rss_end_kb"] = json!({"rss": status_kb("VmRSS:"), "anon": status_kb("RssAnon:")});

    let text = serde_json::to_string_pretty(&doc).map_err(|e| format!("json: {e}"))?;
    std::fs::write(&args.out, text).map_err(|e| format!("write {}: {e}", args.out.display()))?;
    println!(
        "{}: load {:.1} ms, clone median {clone_median:.2} ms, hnsw {hnsw_ms:.1} ms -> {}",
        args.mode,
        doc["load_ms"].as_f64().unwrap_or(0.0),
        args.out.display()
    );
    Ok(())
}

fn main() {
    let outcome = read_args().and_then(|args| run(&args));
    if let Err(e) = outcome {
        eprintln!("c8_before_after: {e}");
        std::process::exit(2);
    }
}
