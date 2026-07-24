//! End-to-end proof that `IncrementalIndex` behaves identically after the
//! 2026-04-16 migration from `DashMap` → `moka::sync::Cache`. The tests
//! exercise the full public API: index / query / rebuild / remove / search
//! / clear under realistic workloads.

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;
use touring_intelligence::index::cache::FileCache;
use touring_intelligence::index::incremental::IncrementalIndex;

fn make_index() -> IncrementalIndex {
    let fc = FileCache::new();
    IncrementalIndex::new(Arc::new(RwLock::new(fc)))
}

fn write_rust_file(dir: &TempDir, name: &str, body: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, body).expect("write");
    path
}

#[tokio::test]
async fn index_and_query_round_trip_after_moka_migration() {
    let idx = make_index();
    let tmp = TempDir::new().unwrap();
    let path = write_rust_file(
        &tmp,
        "demo.rs",
        "pub fn alpha() {}\npub fn beta() {}\npub struct Gamma;",
    );

    idx.index_file(&path).await.expect("index");

    // Query path: should return the cached symbols as an owned Vec.
    let symbols = idx.get_symbols(&path).expect("cached");
    assert!(
        symbols.iter().any(|s| s.name == "alpha"),
        "expected alpha in symbols, got {:?}",
        symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(symbols.iter().any(|s| s.name == "beta"));

    // Metadata path: should hydrate a FileIndexInfo.
    let info = idx.get_file_info(&path).expect("info");
    assert_eq!(info.path, path);
    assert!(info.symbol_count >= 2);
}

#[tokio::test]
async fn remove_file_clears_both_caches() {
    let idx = make_index();
    let tmp = TempDir::new().unwrap();
    let path = write_rust_file(&tmp, "removable.rs", "pub fn x() {}");
    idx.index_file(&path).await.expect("index");
    assert!(idx.is_indexed(&path));

    idx.remove_file(&path).await.expect("remove");
    assert!(!idx.is_indexed(&path));
    assert!(idx.get_symbols(&path).is_none());
    assert!(idx.get_file_info(&path).is_none());
}

#[tokio::test]
async fn rebuild_index_for_file_returns_fresh_symbols() {
    let idx = make_index();
    let tmp = TempDir::new().unwrap();
    let path = write_rust_file(&tmp, "rebuild.rs", "pub fn one() {}");
    idx.index_file(&path).await.expect("index");

    // Overwrite content to change the symbol set.
    std::fs::write(&path, "pub fn one() {}\npub fn two() {}").unwrap();
    let rebuilt = idx
        .rebuild_index_for_file(&path)
        .await
        .expect("rebuild succeeds");
    assert!(
        rebuilt.iter().any(|s| s.name == "two"),
        "new symbol visible"
    );
}

#[tokio::test]
async fn indexed_paths_reflects_all_inserted_files() {
    let idx = make_index();
    let tmp = TempDir::new().unwrap();
    let p1 = write_rust_file(&tmp, "a.rs", "pub fn a() {}");
    let p2 = write_rust_file(&tmp, "b.rs", "pub fn b() {}");
    let p3 = write_rust_file(&tmp, "c.rs", "pub fn c() {}");
    idx.index_file(&p1).await.unwrap();
    idx.index_file(&p2).await.unwrap();
    idx.index_file(&p3).await.unwrap();

    let paths = idx.indexed_paths();
    assert_eq!(paths.len(), 3, "expected 3 paths, got {paths:?}");
    assert!(paths.contains(&p1));
    assert!(paths.contains(&p2));
    assert!(paths.contains(&p3));
}

#[tokio::test]
async fn search_symbols_finds_across_files() {
    let idx = make_index();
    let tmp = TempDir::new().unwrap();
    let p1 = write_rust_file(&tmp, "file1.rs", "pub fn compute_score() {}");
    let p2 = write_rust_file(
        &tmp,
        "file2.rs",
        "pub fn compute_total() {}\npub fn compute_delta() {}",
    );
    idx.index_file(&p1).await.unwrap();
    idx.index_file(&p2).await.unwrap();

    let hits = idx.search_symbols("compute");
    let names: Vec<&str> = hits.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(
        names.contains(&"compute_score"),
        "missing compute_score: {names:?}"
    );
    assert!(names.contains(&"compute_total"), "missing compute_total");
    assert!(names.contains(&"compute_delta"), "missing compute_delta");
}

#[tokio::test]
async fn clear_empties_everything() {
    let idx = make_index();
    let tmp = TempDir::new().unwrap();
    for n in 0..5 {
        let p = write_rust_file(&tmp, &format!("x{n}.rs"), &format!("pub fn f{n}() {{}}"));
        idx.index_file(&p).await.unwrap();
    }
    assert_eq!(idx.indexed_paths().len(), 5);
    idx.clear().await;
    assert!(idx.indexed_paths().is_empty());
}

#[tokio::test]
async fn moka_cache_survives_concurrent_inserts() {
    let idx = Arc::new(make_index());
    let tmp = Arc::new(TempDir::new().unwrap());

    let mut handles = Vec::new();
    for n in 0..16 {
        let idx = Arc::clone(&idx);
        let tmp = Arc::clone(&tmp);
        handles.push(tokio::spawn(async move {
            let p = tmp.path().join(format!("conc_{n}.rs"));
            std::fs::write(&p, format!("pub fn concurrent_{n}() {{}}")).unwrap();
            idx.index_file(&p).await.expect("concurrent index");
            p
        }));
    }

    let mut paths = Vec::new();
    for h in handles {
        paths.push(h.await.expect("join"));
    }
    // All inserts must be observable (no lost writes under contention).
    for p in &paths {
        assert!(idx.get_symbols(p).is_some(), "lost entry for {p:?}");
    }
}
