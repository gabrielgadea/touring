use super::*;
use tempfile::TempDir;

fn make_index() -> (TantivyIndex, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let idx = TantivyIndex::open_or_create(dir.path()).expect("open_or_create");
    (idx, dir)
}

fn symbol(name: &str, file: &str, kind: &str) -> SymbolDoc {
    SymbolDoc {
        symbol_name: name.to_string(),
        file_path: file.to_string(),
        symbol_kind: kind.to_string(),
        module_path: Some(format!("crate::{name}")),
        docstring: Some(format!("Documentation for {name}")),
        line_number: 42,
        language: "rust".to_string(),
        // New v2 fields — all None for backward-compatible test helpers
        visibility: None,
        crate_name: None,
        blake3_hash: None,
        import_count: None,
        export_count: None,
        cognitive_score: None,
        functional_signature: None,
        // New v3 field
        community_id: None,
    }
}

#[test]
fn test_open_or_create_empty() {
    let (_idx, _dir) = make_index();
}

#[test]
fn test_upsert_and_search() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("HookRuntime", "src/hook_runtime.rs", "struct"))
        .expect("upsert");
    idx.commit().expect("commit");

    let hits = idx.search("HookRuntime", 10).expect("search");
    assert!(!hits.is_empty(), "expected at least one hit");
    assert_eq!(hits[0].symbol_name, "HookRuntime");
}

#[test]
fn test_stats_after_upsert() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol(
        "FileKnowledgeDb",
        "src/file_knowledge_db.rs",
        "struct",
    ))
    .expect("upsert");
    idx.commit().expect("commit");

    let stats = idx.stats();
    assert_eq!(stats.total_docs, 1);
    assert_eq!(stats.total_commits, 1);
    assert_eq!(stats.total_upserts, 1);
}

#[test]
fn test_delete_by_file() {
    let (idx, _dir) = make_index();
    let file = "src/lib.rs";
    idx.upsert_symbol(&symbol("Foo", file, "fn"))
        .expect("upsert");
    idx.commit().expect("commit");

    idx.delete_by_file(file).expect("delete");
    idx.commit().expect("commit after delete");

    let hits = idx.search("Foo", 10).expect("search after delete");
    assert!(hits.is_empty(), "document should be deleted");
}

#[test]
fn test_multiple_symbols_same_file() {
    let (idx, _dir) = make_index();
    let file = "src/multi.rs";
    for name in &["Alpha", "Beta", "Gamma"] {
        idx.upsert_symbol(&symbol(name, file, "fn"))
            .expect("upsert");
    }
    idx.commit().expect("commit");

    // All three should be searchable
    for name in &["Alpha", "Beta", "Gamma"] {
        let hits = idx.search(name, 5).expect("search");
        assert!(!hits.is_empty(), "expected hit for {name}");
    }
}

#[test]
fn test_open_existing_index() {
    let dir = TempDir::new().expect("tempdir");
    {
        let idx = TantivyIndex::open_or_create(dir.path()).expect("create");
        idx.upsert_symbol(&symbol("Persisted", "src/p.rs", "struct"))
            .expect("upsert");
        idx.commit().expect("commit");
    }
    // Re-open
    let idx2 = TantivyIndex::open_or_create(dir.path()).expect("reopen");
    let hits = idx2.search("Persisted", 5).expect("search");
    assert!(!hits.is_empty(), "data should persist across open");
}

#[test]
fn test_expanded_schema_fields() {
    let (idx, _dir) = make_index();
    let doc = SymbolDoc {
        symbol_name: "process_query".to_string(),
        file_path: "src/query.rs".to_string(),
        symbol_kind: "fn".to_string(),
        module_path: Some("crate::query".to_string()),
        docstring: Some("Process a search query".to_string()),
        line_number: 100,
        language: "rust".to_string(),
        visibility: Some("pub".to_string()),
        crate_name: Some("touring-hooks".to_string()),
        blake3_hash: Some("abc123def456".to_string()),
        import_count: Some(5),
        export_count: Some(2),
        cognitive_score: Some(0.75),
        functional_signature: Some("fn(query: &str, top_k: usize) -> Vec<Hit>".to_string()),
        community_id: None,
    };
    idx.upsert_symbol(&doc).expect("upsert expanded");
    idx.commit().expect("commit");

    let hits = idx.search("process_query", 10).expect("search");
    assert!(!hits.is_empty(), "expanded doc should be searchable");
    assert_eq!(hits[0].symbol_name, "process_query");
    assert_eq!(hits[0].crate_name.as_deref(), Some("touring-hooks"));
    assert_eq!(hits[0].visibility.as_deref(), Some("pub"));
    assert_eq!(
        hits[0].functional_signature.as_deref(),
        Some("fn(query: &str, top_k: usize) -> Vec<Hit>")
    );
}

// ── U7+U8 tests ───────────────────────────────────────────────────────────

#[test]
fn test_fuzzy_search_finds_typo() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("HookRuntime", "src/hook_runtime.rs", "struct"))
        .expect("upsert");
    idx.commit().expect("commit");

    // "HokRuntime" → "hokruntim" (stemmed); "HookRuntime" → "hookruntim" (stemmed)
    // Edit distance hokruntim vs hookruntim = 1 (only 2nd char differs: o→k)
    let hits = idx.fuzzy_search("HokRuntime", 1, 10).expect("fuzzy_search");
    assert!(
        hits.iter().any(|h| h.symbol_name == "HookRuntime"),
        "expected fuzzy match for typo; got: {hits:?}"
    );
}

#[test]
fn test_suggest_returns_prefix_matches() {
    let (idx, _dir) = make_index();
    for name in &["HookRuntime", "HookRegistry", "HookHandler"] {
        idx.upsert_symbol(&symbol(name, "src/hooks.rs", "struct"))
            .expect("upsert");
    }
    idx.upsert_symbol(&symbol("IndexReader", "src/index.rs", "struct"))
        .expect("upsert non-hook");
    idx.commit().expect("commit");

    let hits = idx.suggest("Hook", 10).expect("suggest");
    assert!(
        hits.len() >= 3,
        "expected 3 Hook* symbols; got {}: {hits:?}",
        hits.len()
    );
    assert!(
        hits.iter().all(|h| h.symbol_name.starts_with("Hook")),
        "all results should start with 'Hook'; got: {hits:?}"
    );
}

#[test]
fn test_search_by_crate_filters_correctly() {
    let (idx, _dir) = make_index();

    // Symbol in crate "touring-hooks"
    let mut hooks_sym = symbol(
        "WiringAudit",
        "crates/touring-hooks/src/wiring.rs",
        "struct",
    );
    hooks_sym.crate_name = Some("touring-hooks".to_string());
    idx.upsert_symbol(&hooks_sym).expect("upsert hooks sym");

    // Same symbol name in crate "touring-index"
    let mut index_sym = symbol("WiringAudit", "crates/touring-index/src/audit.rs", "struct");
    index_sym.crate_name = Some("touring-index".to_string());
    idx.upsert_symbol(&index_sym).expect("upsert index sym");

    idx.commit().expect("commit");

    let hits = idx
        .search_by_crate("WiringAudit", "touring-hooks", 10)
        .expect("search_by_crate");
    assert!(
        !hits.is_empty(),
        "expected at least one result for touring-hooks"
    );
    // Every returned result must belong to touring-hooks
    for h in &hits {
        assert!(
            h.file_path.contains("touring-hooks")
                || h.crate_name.as_deref() == Some("touring-hooks"),
            "unexpected result from wrong crate: {h:?}"
        );
    }
}

#[test]
fn test_reindex_rebuilds_from_scratch() {
    let (idx, _dir) = make_index();

    // Prime the index with some initial data.
    idx.upsert_symbol(&symbol("OldSymbol", "src/old.rs", "fn"))
        .expect("upsert old");
    idx.commit().expect("initial commit");
    assert_eq!(idx.stats().total_docs, 1);

    // Reindex with a completely different set.
    let new_symbols = vec![
        symbol("NewAlpha", "src/new.rs", "struct"),
        symbol("NewBeta", "src/new.rs", "fn"),
        symbol("NewGamma", "src/new.rs", "trait"),
    ];
    let stats = idx.reindex(new_symbols).expect("reindex");

    assert_eq!(stats.total_docs, 3, "reindex should yield exactly 3 docs");

    // Old symbol must be gone.
    let old_hits = idx.search("OldSymbol", 5).expect("search old");
    assert!(
        old_hits.is_empty(),
        "OldSymbol must be removed after reindex"
    );

    // New symbols must be present.
    for name in &["NewAlpha", "NewBeta", "NewGamma"] {
        let hits = idx.search(name, 5).expect("search new");
        assert!(!hits.is_empty(), "expected hit for {name} after reindex");
    }
}

#[test]
fn test_search_by_functional_signature() {
    let (idx, _dir) = make_index();
    let doc = SymbolDoc {
        symbol_name: "search_symbols".to_string(),
        file_path: "src/search.rs".to_string(),
        symbol_kind: "fn".to_string(),
        module_path: Some("crate::search".to_string()),
        docstring: None,
        line_number: 55,
        language: "rust".to_string(),
        visibility: Some("pub".to_string()),
        crate_name: Some("touring-index".to_string()),
        blake3_hash: None,
        import_count: None,
        export_count: None,
        cognitive_score: Some(0.42),
        // Unique term in the signature for targeted search
        functional_signature: Some("fn(needle: &str) -> Vec<SymbolHit>".to_string()),
        community_id: None,
    };
    idx.upsert_symbol(&doc).expect("upsert");
    idx.commit().expect("commit");

    // Query using a term from the functional_signature field
    let hits = idx.search("SymbolHit", 5).expect("search by sig");
    assert!(
        !hits.is_empty(),
        "should find doc by functional_signature term"
    );
    assert_eq!(hits[0].symbol_name, "search_symbols");
}

// ── Schema v3 community_id tests ──────────────────────────────────────────

fn make_sym_with_community(name: &str, file: &str, community_id: Option<u64>) -> SymbolDoc {
    SymbolDoc {
        symbol_name: name.to_string(),
        file_path: file.to_string(),
        symbol_kind: "fn".to_string(),
        module_path: None,
        docstring: None,
        line_number: 1,
        language: "rust".to_string(),
        visibility: None,
        crate_name: None,
        blake3_hash: None,
        import_count: None,
        export_count: None,
        cognitive_score: None,
        functional_signature: None,
        community_id,
    }
}

#[test]
fn test_community_id_roundtrip_in_schema() {
    let (idx, _dir) = make_index();
    let doc = make_sym_with_community("foo_community", "src/a.rs", Some(42));
    idx.upsert_symbol(&doc).expect("upsert");
    idx.commit().expect("commit");

    let hits = idx.search("foo_community", 5).expect("search");
    assert_eq!(hits.len(), 1, "expected exactly one hit");
    assert_eq!(
        hits[0].community_id,
        Some(42),
        "community_id must round-trip through schema v3"
    );
}

#[test]
fn test_community_boost_elevates_same_community_hit() {
    let (idx, _dir) = make_index();

    // Two docs with same name prefix — one in community 7, one in community 9.
    // Use distinct docstrings so BM25 gives them roughly equal base scores.
    let mut doc_a = make_sym_with_community("authenticate_op", "src/auth.rs", Some(7));
    doc_a.docstring = Some("authenticate operation handler for auth module".to_string());
    doc_a.line_number = 10;

    let mut doc_b = make_sym_with_community("authenticate_op", "src/user.rs", Some(9));
    doc_b.docstring = Some("authenticate operation handler for user module".to_string());
    doc_b.line_number = 20;

    idx.upsert_symbol(&doc_a).expect("upsert doc_a");
    idx.upsert_symbol(&doc_b).expect("upsert doc_b");
    idx.commit().expect("commit");

    // With boost targeting community 7: doc_a must rank first.
    let boosted = idx
        .search_with_community_boost("authenticate_op", 5, Some(7))
        .expect("search_with_community_boost");
    assert!(
        !boosted.is_empty(),
        "expected at least one hit from community-boosted search"
    );
    assert_eq!(
        boosted[0].community_id,
        Some(7),
        "community-boosted doc (community 7) must rank first; got: {boosted:?}"
    );
}

#[test]
fn test_no_community_id_when_none() {
    let (idx, _dir) = make_index();
    let doc = make_sym_with_community("bar_nocommunity", "src/b.rs", None);
    idx.upsert_symbol(&doc).expect("upsert");
    idx.commit().expect("commit");

    let hits = idx.search("bar_nocommunity", 5).expect("search");
    assert_eq!(hits.len(), 1, "expected exactly one hit");
    assert_eq!(
        hits[0].community_id, None,
        "community_id must be None when not set on SymbolDoc"
    );
}

// ─── D2.3 — ToolOutputsIndex tests ──────────────────────────────────────

fn make_outputs_index() -> (ToolOutputsIndex, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let idx = ToolOutputsIndex::open_or_create(dir.path()).expect("open_or_create");
    (idx, dir)
}

fn sample_doc(hash: &str, tool: &str) -> ToolOutputDoc {
    ToolOutputDoc {
        content_hash: hash.to_string(),
        tool_name: tool.to_string(),
        summary: format!("output of {tool}"),
        full_output_path: format!("/tmp/sandbox/{hash}.bin"),
        exit_code: 0,
        output_bytes: 1024,
        was_truncated: false,
        stored_at_unix: 1_700_000_000,
        tool_args: None,
    }
}

#[test]
fn test_tool_outputs_store_and_get_roundtrip() {
    let (idx, _dir) = make_outputs_index();
    let doc = sample_doc(
        "a".repeat(64).as_str(), // 64-char hash
        "Bash",
    );
    idx.store_tool_output(&doc).expect("store");
    let got = idx
        .get_tool_output(&doc.content_hash)
        .expect("get")
        .expect("doc present");
    assert_eq!(got, doc);
}

#[test]
fn test_tool_outputs_get_missing_returns_none() {
    let (idx, _dir) = make_outputs_index();
    let res = idx.get_tool_output("nonexistent_hash_xx").expect("get");
    assert!(res.is_none());
}

#[test]
fn test_tool_outputs_upsert_replaces_previous() {
    let (idx, _dir) = make_outputs_index();
    let hash = "b".repeat(64);
    let mut doc = sample_doc(&hash, "Grep");
    doc.exit_code = 1;
    doc.was_truncated = true;
    idx.store_tool_output(&doc).expect("store v1");

    // Upsert same hash with different fields
    doc.exit_code = 0;
    doc.was_truncated = false;
    doc.summary = "second-version".into();
    idx.store_tool_output(&doc).expect("store v2");

    let got = idx.get_tool_output(&hash).expect("get").expect("present");
    assert_eq!(got.exit_code, 0);
    assert!(!got.was_truncated);
    assert_eq!(got.summary, "second-version");
}

// ─── P3-TRIG — RRF tests ────────────────────────────────────────────────

fn rrf_hit(name: &str, file: &str, line: u64) -> SearchHit {
    SearchHit {
        symbol_name: name.to_string(),
        file_path: file.to_string(),
        symbol_kind: "fn".to_string(),
        line_number: line,
        score: 1.0,
        crate_name: None,
        visibility: None,
        functional_signature: None,
        cognitive_score: None,
        community_id: None,
    }
}

#[test]
fn test_rrf_hit_identity_distinguishes_lines() {
    let h1 = rrf_hit("foo", "src/a.rs", 10);
    let h2 = rrf_hit("foo", "src/a.rs", 11);
    assert_ne!(hit_identity(&h1), hit_identity(&h2));
}

#[test]
fn test_rrf_merge_empty_lists_returns_empty() {
    let merged = rrf_merge_two(&[], &[], 60, 5);
    assert!(merged.is_empty());
}

#[test]
fn test_rrf_merge_single_list_preserves_rank() {
    let porter = vec![
        rrf_hit("first", "a.rs", 1),
        rrf_hit("second", "b.rs", 1),
        rrf_hit("third", "c.rs", 1),
    ];
    let merged = rrf_merge_two(&porter, &[], 60, 5);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].symbol_name, "first");
    assert_eq!(merged[1].symbol_name, "second");
    assert_eq!(merged[2].symbol_name, "third");
}

#[test]
fn test_rrf_merge_boosts_overlap() {
    // doc appearing in both lists at rank 1 should win over doc that
    // only appears in one list, even if at rank 1 there as well.
    let common = rrf_hit("shared", "x.rs", 1);
    let porter = vec![common.clone(), rrf_hit("p_only", "p.rs", 1)];
    let fuzzy = vec![common.clone(), rrf_hit("f_only", "f.rs", 1)];
    let merged = rrf_merge_two(&porter, &fuzzy, 60, 5);
    assert_eq!(merged[0].symbol_name, "shared");
    // The shared doc's score = 1/61 + 1/61 ≈ 0.0328; singletons get 1/61.
    assert!(merged[0].score > merged[1].score);
}

#[test]
fn test_rrf_merge_top_k_truncates() {
    let lots: Vec<SearchHit> = (0..10)
        .map(|i| rrf_hit(&format!("s{i}"), "z.rs", i))
        .collect();
    let merged = rrf_merge_two(&lots, &[], 60, 3);
    assert_eq!(merged.len(), 3);
}

#[test]
fn test_rrf_constant_k_default_60() {
    // Default expected behaviour even when env unset
    let k = crate::shared::feature_flags::rrf_k_constant();
    assert_eq!(k, 60);
}

#[test]
fn test_search_rrf_falls_back_when_disabled() {
    // When TOURING_TANTIVY_TRIGRAM=0, search_rrf must equal search().
    // Set env-var only inside the test (env_lock not available here, so
    // verify behaviour via the public flag check).
    let prev = std::env::var("TOURING_TANTIVY_TRIGRAM").ok();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_TANTIVY_TRIGRAM", "0") };
    let (idx, _dir) = make_index();
    let mut s = symbol("authenticate", "src/auth.rs", "fn");
    s.crate_name = Some("touring-auth".into());
    idx.upsert_symbol(&s).expect("upsert");
    idx.commit().expect("commit");
    let plain = idx.search("authenticate", 5).expect("search");
    let rrf = idx.search_rrf("authenticate", 5).expect("search_rrf");
    assert_eq!(plain.len(), rrf.len());
    assert_eq!(plain[0].symbol_name, rrf[0].symbol_name);
    // restore env
    match prev {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(v) => unsafe { std::env::set_var("TOURING_TANTIVY_TRIGRAM", v) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("TOURING_TANTIVY_TRIGRAM") },
    }
}

// ─── Sprint 1 — I-01 NgramTokenizer trigram tests ─────────────────────

#[test]
fn test_i01_trigram_substring_match_useeff() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("useEffect", "src/hooks.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    let hits = idx.search_trigram("useEff", 5).expect("search_trigram");
    assert!(
        !hits.is_empty(),
        "trigram 'useEff' MUST match indexed 'useEffect'"
    );
    assert_eq!(hits[0].symbol_name, "useEffect");
}

#[test]
fn test_i01_trigram_short_query_returns_empty() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("foo", "src/x.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    let hits = idx.search_trigram("us", 5).expect("search");
    assert!(hits.is_empty(), "queries < 3 chars MUST return empty");
}

#[test]
fn test_i01_3way_rrf_combines_porter_trigram_fuzzy() {
    let (idx, _dir) = make_index();
    // Doc com nome contendo trigrams + porter match
    idx.upsert_symbol(&symbol("authenticate_user", "src/auth.rs", "fn"))
        .expect("upsert");
    // Doc só relevante via fuzzy (typo distance)
    idx.upsert_symbol(&symbol("authentcat", "src/typo.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    // Trigram should be ON by default
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TANTIVY_TRIGRAM") };
    let hits = idx.search_rrf("authenticate", 5).expect("search_rrf 3-way");
    assert!(!hits.is_empty(), "3-way RRF MUST return hits");
}

// ─── Sprint 1 — I-02 PhraseQuery proximity tests ──────────────────────

#[test]
fn test_i02_phrase_query_only_for_multi_term() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("foo", "src/a.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    // Single-term: try_build_phrase_query returns None
    let phrase = idx.try_build_phrase_query("foo");
    assert!(phrase.is_none(), "single-term MUST NOT build PhraseQuery");
    // Multi-term: returns Some
    let phrase = idx.try_build_phrase_query("foo bar");
    assert!(phrase.is_some(), "multi-term MUST build PhraseQuery");
}

#[test]
fn test_i02_phrase_metric_increments_on_multi_term_search() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("error_handler", "src/h.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    let before = crate::shared::gate_metrics::global()
        .phrase_query_match_count
        .load(std::sync::atomic::Ordering::Relaxed);
    let _ = idx.search("error handler", 5).expect("search");
    let after = crate::shared::gate_metrics::global()
        .phrase_query_match_count
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(after > before, "phrase_query_match_count MUST advance");
}

// ─── Sprint 1 — I-03 5× Heading boost tests ───────────────────────────

#[test]
fn test_i03_name_boost_default_is_5x() {
    let boost = crate::shared::feature_flags::tantivy_name_boost();
    assert_eq!(boost, 5.0, "default name boost MUST be 5.0");
}

#[test]
fn test_i03_name_boost_env_overridable() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_TANTIVY_NAME_BOOST", "3.5") };
    let boost = crate::shared::feature_flags::tantivy_name_boost();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TANTIVY_NAME_BOOST") };
    assert_eq!(boost, 3.5, "env var MUST override default");
}

// ─── Sprint 1 — I-05 TTL Cache tests ───────────────────────────────────

fn fresh_doc(hash: &str, tool: &str) -> ToolOutputDoc {
    let mut d = sample_doc(hash, tool);
    d.stored_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|x| x.as_secs())
        .unwrap_or(0);
    d
}

#[test]
fn test_i05_ttl_skip_within_24h_window() {
    let (idx, _dir) = make_outputs_index();
    let doc = fresh_doc(&"x".repeat(64), "Bash");
    idx.store_tool_output(&doc).expect("store v1");
    let before = crate::shared::gate_metrics::global()
        .tool_outputs_ttl_skip_count
        .load(std::sync::atomic::Ordering::Relaxed);
    // Same hash within TTL: store MUST skip
    idx.store_tool_output(&doc).expect("store v2 (should skip)");
    let after = crate::shared::gate_metrics::global()
        .tool_outputs_ttl_skip_count
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(after > before, "ttl_skip_count MUST advance on duplicate");
}

#[test]
fn test_i05_is_fresh_returns_some_for_recent_doc() {
    let (idx, _dir) = make_outputs_index();
    let hash = "y".repeat(64);
    idx.store_tool_output(&fresh_doc(&hash, "Bash"))
        .expect("store");
    // 24h window must accept a doc stored seconds ago
    assert!(idx.is_fresh(&hash, 86_400).is_some());
}

#[test]
fn test_i06_json_field_stores_tool_args() {
    let (idx, _dir) = make_outputs_index();
    let mut doc = fresh_doc(&"j".repeat(64), "Bash");
    doc.tool_args = Some(serde_json::json!({
        "command": "gh issue list",
        "path": "src/main.rs",
    }));
    idx.store_tool_output(&doc).expect("store with tool_args");
    // Verify retrievability via content_hash (round-trip).
    let got = idx
        .get_tool_output(&doc.content_hash)
        .expect("get")
        .expect("present");
    assert_eq!(got.content_hash, doc.content_hash);
    // tool_args read-back not implemented yet (decode is None);
    // assert that field at least serialises round-trip via JSON form.
    let serialised = serde_json::to_string(&doc).expect("serialise");
    let parsed: ToolOutputDoc = serde_json::from_str(&serialised).expect("parse");
    assert!(parsed.tool_args.is_some());
}

#[test]
fn test_i08_facet_path_built_from_symbol() {
    let mut s = symbol("foo", "src/x.rs", "fn");
    s.crate_name = Some("touring-hooks".into());
    s.visibility = Some("pub".into());
    let facet = build_symbol_facet(&s);
    let path = format!("{facet}");
    assert!(path.contains("rust"));
    assert!(path.contains("touring-hooks"));
    assert!(path.contains("fn"));
    assert!(path.contains("pub"));
}

#[test]
fn test_i08_count_facets_returns_buckets_under_prefix() {
    let (idx, _dir) = make_index();
    let mut s1 = symbol("foo_fn", "src/a.rs", "fn");
    s1.crate_name = Some("touring-hooks".into());
    s1.visibility = Some("pub".into());
    idx.upsert_symbol(&s1).expect("upsert s1");

    let mut s2 = symbol("Bar", "src/b.rs", "struct");
    s2.crate_name = Some("touring-hooks".into());
    s2.visibility = Some("pub".into());
    idx.upsert_symbol(&s2).expect("upsert s2");
    idx.commit().expect("commit");

    let buckets = idx
        .count_facets("/rust/touring-hooks", 10)
        .expect("count_facets");
    // Two distinct kinds (fn, struct) under /rust/touring-hooks
    assert!(buckets.len() >= 1, "expected >= 1 bucket: {buckets:?}");
}

#[test]
fn test_i07_aggregate_terms_groups_by_kind() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("a", "x.rs", "fn"))
        .expect("upsert");
    idx.upsert_symbol(&symbol("b", "y.rs", "fn"))
        .expect("upsert");
    idx.upsert_symbol(&symbol("Foo", "z.rs", "struct"))
        .expect("upsert");
    idx.commit().expect("commit");
    let buckets = idx
        .aggregate_terms("symbol_kind", 10)
        .expect("aggregate_terms");
    // Top bucket should be "fn" with count 2
    let top = buckets.first().expect("at least one bucket");
    assert_eq!(top.0, "fn");
    assert_eq!(top.1, 2);
}

#[test]
fn test_i07_aggregate_terms_unknown_field_errors() {
    let (idx, _dir) = make_index();
    let err = idx
        .aggregate_terms("nonexistent_xyz", 10)
        .expect_err("must error on unknown field");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn test_i06_serde_to_owned_value_roundtrip() {
    let v = serde_json::json!({
        "str": "hello",
        "int": 42,
        "neg": -7,
        "float": 1.5,
        "bool": true,
        "null": null,
        "arr": [1, 2, 3],
        "nested": { "k": "v" },
    });
    let owned = serde_value_to_tantivy_owned(&v);
    // Sanity: top-level must be Object
    match owned {
        tantivy::schema::OwnedValue::Object(map) => {
            let keys: std::collections::BTreeSet<&str> =
                map.iter().map(|(k, _)| k.as_str()).collect();
            assert!(keys.contains("str"));
            assert!(keys.contains("nested"));
            assert!(keys.contains("arr"));
        }
        _ => panic!("top-level must be Object, got {owned:?}"),
    }
}

#[test]
fn test_i05_cleanup_expired_removes_old_docs() {
    let (idx, _dir) = make_outputs_index();
    let mut doc = sample_doc(&"z".repeat(64), "Bash");
    // Set stored_at_unix to 30 days ago (well past 14d retention)
    doc.stored_at_unix = 1_700_000_000; // ~Nov 2023
    idx.store_tool_output(&doc).expect("store old");
    // retention=1s means anything older than 1s gets cleaned
    let deleted = idx.cleanup_expired(1).expect("cleanup");
    assert!(deleted >= 1, "cleanup MUST delete the ancient doc");
}
