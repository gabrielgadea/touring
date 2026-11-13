//! M2 of C7 — what the pipeline costs in memory.
//!
//! Loads every parseable source file under a root into a [`SharedPipeline`] — the same
//! `process_file` the daemon's `index rebuild` runs for each walked file — and prints ONE
//! JSON line: the rope bytes (`memory_usage_mb`, the unit the budget is enforced in), the
//! documents and parse trees held, the process's anonymous RSS before and after
//! (`/proc/self/status`), and the time per file. `anon_mb_per_rope_mb` is the answer to
//! "how much does one MB of budget really cost": the trees and the symbol cache ride on top
//! of the ropes the budget counts.
//!
//! Run it under a memory cap, never bare:
//! ```text
//! cargo build -p touring-code --example pipeline_memory_probe
//! systemd-run --user --scope -p MemoryMax=8G -- \
//!     target/debug/examples/pipeline_memory_probe <root> [--budget-mb N]
//! ```
//! `--budget-mb N` also applies `enforce_budget` after each file with an empty heat map, so the
//! same run shows what a budget keeps resident. The `build` field says whether the timing came
//! from a debug build (memory is representative either way; milliseconds are not).

use std::path::{Path, PathBuf};
use std::time::Instant;

use touring_code::ast::{HeatMap, Lang, SharedPipeline};

/// The index's own ceiling per file (`touring index why`: `max_indexable_file_bytes`).
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Directories no walker of the index enters.
const SKIPPED_DIRS: &[&str] = &["target", "node_modules", "site", "client", "vendor"];

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                collect(&path, out);
            }
        } else if Lang::from_path(&path).is_some() {
            out.push(path);
        }
    }
}

/// `RssAnon` of this process in kB (`0` where `/proc` is unavailable).
fn rss_anon_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("RssAnon:"))
                .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
        })
        .unwrap_or(0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut root = PathBuf::from(".");
    let mut budget_mb = 0usize;
    while let Some(arg) = args.next() {
        if arg == "--budget-mb" {
            budget_mb = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        } else {
            root = PathBuf::from(arg);
        }
    }

    let mut files = Vec::new();
    collect(&root, &mut files);

    let pipeline = SharedPipeline::new().with_memory_budget_mb(budget_mb);
    let no_heat = HeatMap::new(0);
    let rss_before = rss_anon_kb();
    let started = Instant::now();
    let (mut loaded, mut source_bytes, mut symbols) = (0usize, 0u64, 0usize);
    for path in &files {
        let Ok(meta) = std::fs::metadata(path) else {
            continue;
        };
        if meta.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let key = path.strip_prefix(&root).unwrap_or(path).to_string_lossy();
        if let Ok(result) = pipeline.process_file(&key, &content) {
            loaded += 1;
            source_bytes += meta.len();
            symbols += result.symbols_added.len();
            pipeline.enforce_budget(&no_heat, Some(&key));
        }
    }
    let elapsed = started.elapsed();
    let rss_after = rss_anon_kb();

    let stats = pipeline.stats();
    let delta_mb = rss_after.saturating_sub(rss_before) as f64 / 1024.0;
    let per_rope = if stats.memory_mb > 0.0 {
        delta_mb / stats.memory_mb
    } else {
        0.0
    };
    println!(
        "{}",
        serde_json::json!({
            "root": root.to_string_lossy(),
            "build": if cfg!(debug_assertions) { "debug" } else { "release" },
            "files_walked": files.len(),
            "files_loaded": loaded,
            "source_mb": source_bytes as f64 / (1024.0 * 1024.0),
            "symbols": symbols,
            "budget_mb": stats.budget_mb,
            "documents": stats.documents,
            "trees": stats.trees,
            "evictions": stats.evictions,
            "memory_usage_mb": stats.memory_mb,
            "rss_anon_kb_before": rss_before,
            "rss_anon_kb_after": rss_after,
            "rss_anon_delta_mb": delta_mb,
            "anon_mb_per_rope_mb": per_rope,
            "total_secs": elapsed.as_secs_f64(),
            "mean_ms_per_file": if loaded > 0 { elapsed.as_secs_f64() * 1000.0 / loaded as f64 } else { 0.0 },
        })
    );
    Ok(())
}
