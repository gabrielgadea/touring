//! Memory probe for `index rebuild`, run in-process over a project root.
//!
//! Samples `/proc/self/status` every 100 ms while `cli_index_rebuild` runs and
//! keeps anonymous and file-backed RSS apart — tantivy maps its segments, and
//! file pages are reclaimable, not pressure. Compare runs with
//! `TOURING_REBUILD_SEARCH_DOCS=0` to attribute the search-index cost.
//!
//! ```text
//! cargo run -p touring-cli --features tantivy-fts --example rebuild_memory_probe -- <root> <series.txt>
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Anonymous and file RSS of this process, in MB.
fn rss_split() -> (u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| -> u64 {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|kb| kb.parse::<u64>().ok())
            .map_or(0, |kb| kb / 1024)
    };
    (field("RssAnon:"), field("RssFile:"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = std::path::PathBuf::from(args.next().ok_or("usage: <root> <series.txt>")?);
    let series_path = args.next().ok_or("usage: <root> <series.txt>")?;

    let running = Arc::new(AtomicBool::new(true));
    let sampler = {
        let running = Arc::clone(&running);
        let series_path = series_path.clone();
        std::thread::spawn(move || {
            use std::io::Write;
            let t0 = Instant::now();
            let mut series: Vec<(f64, u64, u64)> = Vec::new();
            // One line per sample, flushed: a killed run still leaves its curve.
            let mut sink = std::fs::File::create(&series_path).ok();
            while running.load(Ordering::Relaxed) {
                let (anon, file) = rss_split();
                let at = t0.elapsed().as_secs_f64();
                series.push((at, anon, file));
                if let Some(f) = sink.as_mut() {
                    let _ = writeln!(f, "{at:.1} {anon} {file}");
                    let _ = f.flush();
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            series
        })
    };

    let t0 = Instant::now();
    let mut rt = touring_cli::runtime::HookRuntime::new(&root)?;
    let out = touring_cli::cli_handlers_index::cli_index_rebuild(
        &mut rt,
        &serde_json::json!({ "dir": root.to_string_lossy() }),
    );
    running.store(false, Ordering::Relaxed);
    let series = sampler.join().map_err(|_| "sampler panicked")?;

    let payload: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
    let peak_anon = series.iter().map(|s| s.1).max().unwrap_or(0);
    let peak_file = series.iter().map(|s| s.2).max().unwrap_or(0);
    println!(
        "{}",
        serde_json::json!({
            "secs": t0.elapsed().as_secs_f64(),
            "search_docs_written": touring_cli::shared::feature_flags::rebuild_search_docs(),
            "peak_anon_mb": peak_anon,
            "peak_file_mb": peak_file,
            "files_indexed": payload["files_indexed"],
            "total_files_scanned": payload["total_files_scanned"],
            "max_rss_mb": payload["max_rss_mb"],
            "aborted_memory_pressure": payload["aborted_memory_pressure"],
            "search_documents": payload["search_documents"],
            "generation": payload["generation"],
        })
    );
    Ok(())
}
