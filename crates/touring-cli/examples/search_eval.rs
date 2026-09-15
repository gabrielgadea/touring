//! Offline retrieval evaluation for `touring tantivy search` (2026-09-13).
//!
//! Tuning search against the live daemon costs a release build, a deploy and a
//! full rebuild per attempt (~15 min). This example measures the SAME ranking in
//! seconds: it builds a private Tantivy index from a read-only snapshot of a
//! project's `symbols.db` with the production document builder
//! (`shared::tantivy_docs::docs_for_file`, markdown text included), then runs
//! question sets through the production query path the CLI uses
//! (`TantivyIndex::search_with_community_boost`, `EVAL_ROUTE=community`), or
//! `search` / `search_text` for the other lanes. Ranking knobs are the same
//! environment variables the daemon reads (`TOURING_TANTIVY_*`).
//!
//! A question set is `{name, filter: [path prefixes] | null, questions: [{q,
//! truth: [paths]}]}`; hit@k is by distinct file, over the prefixes when given.
//! Held-out sets exist so a change is kept only if it generalizes.
//!
//! ```text
//! cargo run -p touring-cli --features tantivy-fts --example search_eval -- \
//!   --root ~/projects/touring --symbols-db /tmp/snap.db --sets sets.json \
//!   --index-dir /tmp/eval-index [--reuse-index] [--misses] [--skip-prefix client/]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use touring_cli::shared::tantivy_docs::docs_for_file;
use touring_cli::tantivy_index::{SearchHit, TantivyIndex};
use touring_code::ast::SymbolLocation;
use touring_hooks_shared::index_policy::IndexPolicy;

/// Command-line options.
struct Options {
    root: PathBuf,
    symbols_db: PathBuf,
    sets: PathBuf,
    index_dir: PathBuf,
    reuse_index: bool,
    misses: bool,
    /// Key prefixes left out of the private index — simulates a project's
    /// `[index] exclude_dirs` on a snapshot taken before the exclusion applied.
    skip_prefixes: Vec<String>,
    /// Queries whose top documents (file, name, line, score) are printed.
    explain: Vec<String>,
    /// Re-upsert every document once more after building (the history a live
    /// index accumulates), to measure what deleted documents do to ranking.
    churn: bool,
    /// Compact the index after building (and churning).
    compact: bool,
    /// Build no index: report the heaviest and slowest files for the builder.
    census: bool,
}

fn parse_options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let (mut root, mut db, mut sets, mut dir) = (None, None, None, None);
    let (mut reuse_index, mut misses) = (false, false);
    let mut skip_prefixes = Vec::new();
    let mut explain = Vec::new();
    let (mut churn, mut compact, mut census) = (false, false, false);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .map(PathBuf::from)
                .ok_or(format!("{arg} needs a value"))
        };
        match arg.as_str() {
            "--root" => root = Some(value()?),
            "--symbols-db" => db = Some(value()?),
            "--sets" => sets = Some(value()?),
            "--index-dir" => dir = Some(value()?),
            "--reuse-index" => reuse_index = true,
            "--misses" => misses = true,
            "--skip-prefix" => skip_prefixes.push(value()?.to_string_lossy().into_owned()),
            "--explain" => explain.push(value()?.to_string_lossy().into_owned()),
            "--churn" => churn = true,
            "--compact" => compact = true,
            "--census" => census = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Options {
        root: root.ok_or("--root is required")?,
        symbols_db: db.ok_or("--symbols-db is required")?,
        sets: sets.ok_or("--sets is required")?,
        index_dir: dir.ok_or("--index-dir is required")?,
        reuse_index,
        misses,
        skip_prefixes,
        explain,
        churn,
        compact,
        census,
    })
}

/// Every definition row of the snapshot, grouped by storage key.
fn definitions_by_file(
    db: &PathBuf,
) -> Result<BTreeMap<String, Vec<SymbolLocation>>, rusqlite::Error> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut stmt = conn.prepare(
        "SELECT name, file_path, line, column_offset, kind FROM symbols WHERE is_definition = 1 ORDER BY file_path, line",
    )?;
    let mut by_file: BTreeMap<String, Vec<SymbolLocation>> = BTreeMap::new();
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    for row in rows {
        let (name, file, line, col, kind) = row?;
        let loc = SymbolLocation::new(
            file.as_str(),
            name,
            usize::try_from(line).unwrap_or(0),
            usize::try_from(col).unwrap_or(0),
            true,
        )
        .with_kind(kind);
        by_file.entry(file).or_default().push(loc);
    }
    Ok(by_file)
}

/// Builds the private index with the production document builder.
fn build_index(opts: &Options, idx: &TantivyIndex) -> Result<(usize, usize), String> {
    let policy = IndexPolicy::for_root(&opts.root);
    let files = definitions_by_file(&opts.symbols_db).map_err(|e| format!("snapshot: {e}"))?;
    let mut docs_written = 0usize;
    for pass in 0..(1 + usize::from(opts.churn)) {
        for (key, symbols) in files
            .iter()
            .filter(|(k, _)| !opts.skip_prefixes.iter().any(|p| k.starts_with(p.as_str())))
        {
            let content = std::fs::read_to_string(policy.path_for_key(key)).ok();
            for doc in docs_for_file(key, symbols, content.as_deref()) {
                idx.upsert_symbol(&doc)
                    .map_err(|e| format!("upsert {key}: {e}"))?;
                docs_written += usize::from(pass == 0);
            }
        }
        idx.commit().map_err(|e| format!("commit: {e}"))?;
    }
    if opts.compact {
        let merged = idx.compact().map_err(|e| format!("compact: {e}"))?;
        eprintln!("compacted {merged} segments");
    }
    Ok((files.len(), docs_written))
}

/// Runs the document builder over every file WITHOUT an index and prints the
/// files whose documents weigh the most (bytes of text fields) and take longest:
/// the census that finds a pathological input before blaming the writer.
fn census(opts: &Options) -> Result<(), String> {
    let policy = IndexPolicy::for_root(&opts.root);
    let files = definitions_by_file(&opts.symbols_db).map_err(|e| format!("snapshot: {e}"))?;
    let mut rows: Vec<(usize, usize, u128, usize, String)> = Vec::new();
    let (mut total_docs, mut total_bytes) = (0usize, 0usize);
    for (key, symbols) in &files {
        let t = std::time::Instant::now();
        let content = std::fs::read_to_string(policy.path_for_key(key)).ok();
        let content_len = content.as_ref().map_or(0, String::len);
        let docs = docs_for_file(key, symbols, content.as_deref());
        let opt = |o: &Option<String>| o.as_ref().map_or(0, String::len);
        let bytes: usize = docs
            .iter()
            .map(|d| {
                d.symbol_name.len()
                    + d.file_path.len()
                    + opt(&d.module_path)
                    + opt(&d.docstring)
                    + opt(&d.functional_signature)
                    + opt(&d.crate_name)
                    + opt(&d.blake3_hash)
            })
            .sum();
        total_docs += docs.len();
        total_bytes += bytes;
        rows.push((
            bytes,
            docs.len(),
            t.elapsed().as_millis(),
            content_len,
            key.clone(),
        ));
    }
    println!(
        "files {} docs {total_docs} text_bytes {total_bytes}",
        files.len()
    );
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    println!("== heaviest documents (bytes, docs, ms, file bytes, key)");
    for r in rows.iter().take(15) {
        println!("{:>11} {:>7} {:>6} {:>9} {}", r.0, r.1, r.2, r.3, r.4);
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));
    println!("== slowest files");
    for r in rows.iter().take(10) {
        println!("{:>11} {:>7} {:>6} {:>9} {}", r.0, r.1, r.2, r.3, r.4);
    }
    Ok(())
}

/// The ranking the evaluated command runs.
fn run_route(idx: &TantivyIndex, q: &str) -> Vec<SearchHit> {
    let route = std::env::var("EVAL_ROUTE").unwrap_or_else(|_| "community".to_string());
    let found = match route.as_str() {
        "bm25" => idx.search(q, 60),
        "text" => idx.search_text(q, 60),
        // The CLI asks for 60 (`touring tantivy search q 60` in the live check).
        _ => idx.search_with_community_boost(q, 60, None),
    };
    found.unwrap_or_default()
}

fn main() {
    let opts = match parse_options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("search_eval: {e}");
            std::process::exit(2);
        }
    };
    if opts.census {
        if let Err(e) = census(&opts) {
            eprintln!("search_eval: {e}");
            std::process::exit(2);
        }
        return;
    }
    let t0 = std::time::Instant::now();
    if !opts.reuse_index {
        let _ = std::fs::remove_dir_all(&opts.index_dir);
    }
    let idx = match TantivyIndex::open_or_create(&opts.index_dir) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("search_eval: open index: {e}");
            std::process::exit(2);
        }
    };
    if !opts.reuse_index || idx.is_empty() {
        match build_index(&opts, &idx) {
            Ok((files, docs)) => eprintln!(
                "indexed {files} files / {docs} docs in {:.1}s",
                t0.elapsed().as_secs_f32()
            ),
            Err(e) => {
                eprintln!("search_eval: {e}");
                std::process::exit(2);
            }
        }
    }
    for q in &opts.explain {
        println!("== explain {q:?}");
        for hit in run_route(&idx, q).into_iter().take(12) {
            println!(
                "  {:>8.3}  {}:{}  {}",
                hit.score, hit.file_path, hit.line_number, hit.symbol_name
            );
        }
    }
    if !opts.explain.is_empty() {
        return;
    }
    let sets: serde_json::Value =
        match std::fs::read_to_string(&opts.sets).map(|t| serde_json::from_str(&t)) {
            Ok(Ok(v)) => v,
            other => {
                eprintln!("search_eval: sets: {other:?}");
                std::process::exit(2);
            }
        };
    let mut total = (0usize, 0usize, 0usize);
    for set in sets.as_array().into_iter().flatten() {
        let name = set["name"].as_str().unwrap_or("?");
        let filter: Vec<String> = set["filter"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let (mut h1, mut h3, mut n) = (0usize, 0usize, 0usize);
        for item in set["questions"].as_array().into_iter().flatten() {
            let q = item["q"].as_str().unwrap_or("");
            let truth: Vec<&str> = item["truth"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str())
                .collect();
            let mut top: Vec<String> = Vec::new();
            // Every distinct file in ranking order with its best score, so a miss
            // reports how far the truth is and by how much, not only that it lost.
            let mut ranked: Vec<(String, f32)> = Vec::new();
            for hit in run_route(&idx, q) {
                let p = hit.file_path.trim_start_matches("./").to_string();
                if (filter.is_empty() || filter.iter().any(|f| p.starts_with(f.as_str())))
                    && !ranked.iter().any(|(f, _)| *f == p)
                {
                    if top.len() < 3 {
                        top.push(p.clone());
                    }
                    ranked.push((p, hit.score));
                }
            }
            n += 1;
            let first = top.first().is_some_and(|t| truth.contains(&t.as_str()));
            let any = top.iter().any(|t| truth.contains(&t.as_str()));
            h1 += usize::from(first);
            h3 += usize::from(any);
            if opts.misses && !first {
                let truth_at = ranked
                    .iter()
                    .position(|(f, _)| truth.contains(&f.as_str()))
                    .map_or("absent".to_string(), |i| {
                        format!("#{} {:.2}", i + 1, ranked[i].1)
                    });
                let first_score = ranked.first().map_or(0.0, |(_, s)| *s);
                eprintln!(
                    "  [{name}] miss@1 {q:?} truth={} ({truth_at} vs {first_score:.2}) top={top:?}",
                    truth.first().copied().unwrap_or("")
                );
            }
        }
        println!(
            "{}",
            serde_json::json!({ "set": name, "n": n, "hit@1": h1, "hit@3": h3 })
        );
        total = (total.0 + n, total.1 + h1, total.2 + h3);
    }
    println!(
        "{}",
        serde_json::json!({ "set": "TOTAL", "n": total.0, "hit@1": total.1, "hit@3": total.2 })
    );
}
