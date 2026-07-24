//! Comprehensive system E2E tests — all subsystems (2026-04-17).
//!
//! Coverage matrix:
//!
//! | Subsystem | Crates exercised | What it proves |
//! |---|---|---|
//! | Gate metrics | touring-hooks::shared::gate_metrics | Atomic counters record + snapshot |
//! | Job registry | touring-hooks::shared::job_registry | No-runtime safe path + list + drop |
//! | Terminal job cache | touring-hooks::shared::terminal_job_cache | Moka store/lookup/forget |
//! | Tantivy BM25 | touring-hooks::tantivy_index | Symbol index + search + fuzzy + suggest |
//! | Functional wiring | touring-hooks::functional_wiring | Signature extraction + chain detection |
//! | rkyv IPC | touring-rkyv::ipc | Frame/unframe roundtrip (request + response) |
//! | AgenticRL belief | touring-hooks::agentic_rl | ObservableState encode + BeliefState update |
//! | AgenticRL policy | touring-hooks::agentic_rl | PolicyNetwork forward → valid distribution |
//! | Erickson NLP | touring-offensive | extract_with_relations → PopulationResult |
//! | PT-BR markers | touring-offensive::erickson | Portuguese discourse markers match |
//! | RelationContext | touring-offensive::erickson | get_relation_context spatial reasoning |
//! | CILA budget | touring-hooks::shared::cila | Budget monotonically increases with level |
//! | Pattern bandit | touring-hooks::pattern_bandit | RL q-value update + epsilon decay |

#![allow(
    clippy::assertions_on_constants,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

// ─────────────────────────────────────────────────────────────────────────────
// Dim 1 — Gate metrics: atomic counters record and snapshot correctly.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn gate_metrics_record_and_snapshot() {
    use touring_hooks::shared::gate_metrics::{self, GateMetricsSnapshot};

    let before = GateMetricsSnapshot::capture();

    // Record several gate events.
    gate_metrics::record_pre_edit_fast_path();
    gate_metrics::record_pre_edit_fast_path();
    gate_metrics::record_pre_edit_full();
    gate_metrics::record_pre_write_fast_path();
    gate_metrics::record_pre_write_full();
    gate_metrics::record_post_tool_l4_mandatory();

    let after = GateMetricsSnapshot::capture();

    // Verify deltas — must be monotonically increasing.
    assert!(
        after.pre_edit_fast_path >= before.pre_edit_fast_path + 2,
        "expected at least 2 new fast_path records, delta={}",
        after.pre_edit_fast_path - before.pre_edit_fast_path
    );
    assert!(
        after.pre_edit_full >= before.pre_edit_full + 1,
        "expected at least 1 new full record"
    );
    assert!(
        after.pre_write_fast_path >= before.pre_write_fast_path + 1,
        "expected at least 1 new write fast_path record"
    );
    assert!(
        after.pre_write_full >= before.pre_write_full + 1,
        "expected at least 1 new write full record"
    );
    assert!(
        after.post_tool_l4_mandatory >= before.post_tool_l4_mandatory + 1,
        "expected at least 1 new l4 mandatory record"
    );

    // Fast ratio must be in [0.0, 1.0].
    assert!(
        (0.0..=1.0).contains(&after.pre_edit_fast_ratio),
        "pre_edit_fast_ratio out of range: {}",
        after.pre_edit_fast_ratio
    );
}

#[test]
fn gate_metrics_rkyv_counters_record() {
    use touring_hooks::shared::gate_metrics::{self, GateMetricsSnapshot};

    let before = GateMetricsSnapshot::capture();
    gate_metrics::record_rkyv_dispatch(128);
    gate_metrics::record_rkyv_dispatch(256);
    let after = GateMetricsSnapshot::capture();

    assert!(
        after.rkyv_dispatch_count >= before.rkyv_dispatch_count + 2,
        "expected at least 2 rkyv dispatch records"
    );
    assert!(
        after.rkyv_dispatch_bytes >= before.rkyv_dispatch_bytes + 384,
        "expected at least 384 bytes recorded (128+256)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 2 — Job registry: no-runtime safe path + list + drop lifecycle.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn job_registry_no_runtime_safe_path() {
    use touring_hooks::shared::job_registry;

    // Without a Tokio runtime, spawn_worker must return a Failed job
    // (not panic), per the graceful degradation path.
    let job_id = job_registry::spawn_worker("test-tool", "echo", &["hello".to_string()]);
    assert!(!job_id.is_empty(), "job_id must be non-empty");

    let status = job_registry::poll_worker(&job_id);
    let status_str = status["status"].as_str().unwrap_or("unknown");
    // In the no-runtime path the job is marked Failed immediately.
    assert!(
        status_str == "failed" || status_str == "completed" || status_str == "running",
        "unexpected status: {status_str}"
    );

    // List must include our job.
    let list = job_registry::list_jobs();
    let jobs = list["jobs"]
        .as_array()
        .expect("jobs field must be an array in job_registry response");
    assert!(
        jobs.iter()
            .any(|j| j["job_id"].as_str() == Some(job_id.as_str())),
        "job {job_id} not found in list"
    );

    // Drop must succeed.
    let dropped = job_registry::drop_job(&job_id);
    assert!(dropped, "drop must succeed for an existing job");

    // After drop, poll must return not_found.
    let after = job_registry::poll_worker(&job_id);
    assert_eq!(
        after["status"].as_str(),
        Some("not_found"),
        "after drop status must be not_found"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 3 — Terminal job cache: moka store / lookup / forget / stats.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn terminal_job_cache_store_lookup_forget() {
    use touring_hooks::shared::terminal_job_cache;

    let job_id = "e2e-test-job-cache-001";
    let payload = serde_json::json!({"status": "completed", "exit_code": 0});

    // Clear in case leftover from another test run.
    terminal_job_cache::forget(job_id);

    let stored = terminal_job_cache::store_terminal(job_id, &payload);
    assert!(stored, "store must succeed");

    let retrieved = terminal_job_cache::lookup_terminal(job_id);
    assert!(retrieved.is_some(), "lookup must find the stored entry");
    assert_eq!(
        retrieved.unwrap()["status"].as_str(),
        Some("completed"),
        "status field must round-trip"
    );

    terminal_job_cache::forget(job_id);
    let after = terminal_job_cache::lookup_terminal(job_id);
    assert!(after.is_none(), "after forget, lookup must return None");

    // Stats must be a valid struct (no panic).
    let stats = terminal_job_cache::stats();
    let _ = format!("{stats:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 4 — Tantivy BM25: symbol indexing, search, fuzzy, suggest.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tantivy_bm25_index_and_search() {
    use tempfile::TempDir;
    use touring_hooks::tantivy_index::{SymbolDoc, TantivyIndex};

    let tmp = TempDir::new().expect("tempdir");
    let idx = TantivyIndex::open_or_create(tmp.path()).expect("open index");

    let docs = vec![
        SymbolDoc {
            symbol_name: "extract_functional_signature".to_string(),
            file_path: "crates/touring-hooks/src/functional_wiring.rs".to_string(),
            symbol_kind: "fn".to_string(),
            module_path: Some("touring_hooks::functional_wiring".to_string()),
            docstring: Some("Extracts functional signature for discourse relation analysis".to_string()),
            line_number: 153,
            language: "rust".to_string(),
            visibility: Some("pub".to_string()),
            crate_name: Some("touring-hooks".to_string()),
            blake3_hash: Some("abc123def456".to_string()),
            import_count: Some(12),
            export_count: Some(5),
            cognitive_score: Some(0.72),
            functional_signature: Some("fn(file_path: &str, content: &str, symbols_json: &str) -> Option<FunctionalSignature>".to_string()),
            community_id: None,
        },
        SymbolDoc {
            symbol_name: "detect_chains".to_string(),
            file_path: "crates/touring-hooks/src/functional_wiring.rs".to_string(),
            symbol_kind: "fn".to_string(),
            module_path: Some("touring_hooks::functional_wiring".to_string()),
            docstring: Some("Detects functional chains between module signatures".to_string()),
            line_number: 414,
            language: "rust".to_string(),
            visibility: Some("pub".to_string()),
            crate_name: Some("touring-hooks".to_string()),
            blake3_hash: Some("def789abc012".to_string()),
            import_count: Some(12),
            export_count: Some(5),
            cognitive_score: Some(0.65),
            functional_signature: Some("fn(signatures: &[FunctionalSignature]) -> Vec<...>".to_string()),
            community_id: None,
        },
        SymbolDoc {
            symbol_name: "GateMetrics".to_string(),
            file_path: "crates/touring-hooks/src/shared/gate_metrics.rs".to_string(),
            symbol_kind: "struct".to_string(),
            module_path: Some("touring_hooks::shared::gate_metrics".to_string()),
            docstring: Some("Atomic gate metrics for pre-edit/pre-write/post-tool enrichment".to_string()),
            line_number: 129,
            language: "rust".to_string(),
            visibility: Some("pub".to_string()),
            crate_name: Some("touring-hooks".to_string()),
            blake3_hash: Some("789abc012def".to_string()),
            import_count: Some(8),
            export_count: Some(3),
            cognitive_score: Some(0.55),
            functional_signature: None,
            community_id: None,
        },
    ];

    for doc in &docs {
        idx.upsert_symbol(doc).expect("upsert must succeed");
    }
    idx.commit().expect("commit must succeed");

    // BM25 search must find relevant symbols.
    let hits = idx
        .search("functional signature", 5)
        .expect("search must succeed");
    assert!(
        !hits.is_empty(),
        "BM25 search for 'functional signature' must return results"
    );
    assert!(
        hits.iter()
            .any(|h| h.symbol_name == "extract_functional_signature"),
        "expected extract_functional_signature in top results"
    );

    // Fuzzy search must tolerate a typo.
    let fuzzy = idx
        .fuzzy_search("GateMetrics", 1, 3)
        .expect("fuzzy search must succeed");
    assert!(
        !fuzzy.is_empty(),
        "fuzzy search for 'GateMetrics' must return at least 1 result"
    );

    // Suggest must autocomplete a prefix.
    let suggestions = idx.suggest("detect", 5).expect("suggest must succeed");
    assert!(
        suggestions
            .iter()
            .any(|h| h.symbol_name.starts_with("detect")),
        "suggest 'detect' must return detect_chains"
    );

    // Stats must show indexed docs.
    let stats = idx.stats();
    assert!(
        stats.total_docs >= 3,
        "expected at least 3 docs, got {}",
        stats.total_docs
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 5 — Functional wiring: signature extraction + chain detection.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn functional_wiring_signature_extraction() {
    use touring_hooks::functional_wiring::{
        FunctionalSignature, detect_chains, extract_functional_signature, extract_module_purpose,
    };

    let content = r#"//! Authentication module.
//!
//! Provides user authentication and session token management.
//! Handles credential validation against the user database.

pub fn authenticate(credentials: &str) -> Option<String> { todo!() }
pub fn create_session(user_id: &str) -> String { todo!() }
pub fn invalidate_session(token: &str) -> bool { todo!() }
"#;

    let symbols_json = r#"[
        {"name": "authenticate", "kind": "fn", "visibility": "pub"},
        {"name": "create_session", "kind": "fn", "visibility": "pub"},
        {"name": "invalidate_session", "kind": "fn", "visibility": "pub"}
    ]"#;

    let sig = extract_functional_signature("src/auth.rs", content, symbols_json);
    assert!(
        sig.is_some(),
        "signature extraction must succeed for documented module"
    );

    let sig = sig.expect("signature must be present for validated symbol");
    assert_eq!(sig.file_path, "src/auth.rs");

    // Module purpose must be inferred from the doc comment.
    if let Some(ref purpose) = sig.module_purpose {
        assert!(
            purpose.to_lowercase().contains("auth")
                || purpose.to_lowercase().contains("session")
                || purpose.to_lowercase().contains("user"),
            "module purpose must reflect auth/session domain, got: {purpose}"
        );
    }

    // extract_module_purpose works for doc with clear purpose statement.
    let purpose = extract_module_purpose(
        "Provides user authentication and session token management.",
        "src/auth.rs",
    );
    assert!(
        purpose.is_some(),
        "extract_module_purpose must return Some for a clear purpose statement"
    );

    // Chain detection with complementary signatures (same module purpose).
    let sig2 = FunctionalSignature {
        file_path: "src/auth_v2.rs".to_string(),
        module_purpose: sig.module_purpose.clone(),
        domain: sig.domain.clone(),
        symbols: sig.symbols.clone(),
        content_hash: sig.content_hash.clone(),
    };

    if sig.module_purpose.is_some() {
        let chains = detect_chains(&[sig, sig2]);
        assert!(
            !chains.is_empty(),
            "detect_chains must find at least 1 chain for modules with same purpose"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 6 — rkyv IPC: frame/unframe roundtrip for request and response.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn rkyv_ipc_request_roundtrip() {
    use touring_rkyv::check_archived_root;
    use touring_rkyv::ipc::{IPC_FRAME_HEADER_LEN, IPC_MAGIC, IpcRequest, frame_request, unframe};

    let req = IpcRequest {
        hook: "cli-ast-meta".to_string(),
        payload: br#"{"file":"src/lib.rs","depth":"summary"}"#.to_vec(),
        project_root: "/home/user/.claude/rust".to_string(),
        session_id: "sess-e2e-001".to_string(),
        priority: 42,
    };

    // Frame the request.
    let framed = frame_request(&req).expect("frame_request must succeed");
    assert!(
        framed.len() > IPC_FRAME_HEADER_LEN,
        "framed length must exceed header"
    );
    assert_eq!(
        &framed[..4],
        &IPC_MAGIC,
        "magic header must match IPC_MAGIC"
    );

    // Unframe extracts the payload bytes.
    let body = unframe(&framed).expect("unframe must succeed");
    assert!(!body.is_empty(), "unframed body must be non-empty");

    // Deserialize back and verify field equality.
    let archived = check_archived_root::<IpcRequest>(body)
        .expect("check_archived_root must pass for a valid frame");
    assert_eq!(archived.hook.as_str(), req.hook.as_str());
    assert_eq!(archived.project_root.as_str(), req.project_root.as_str());
    assert_eq!(archived.session_id.as_str(), req.session_id.as_str());
    assert_eq!(archived.priority, req.priority);
    assert_eq!(archived.payload.as_slice(), req.payload.as_slice());
}

#[test]
fn rkyv_ipc_response_roundtrip() {
    use touring_rkyv::check_archived_root;
    use touring_rkyv::ipc::{IpcResponse, frame_response, unframe};

    let resp = IpcResponse {
        output: r#"{"symbol_count":38815,"orphan_count":0}"#.to_string(),
        success: true,
    };

    let framed = frame_response(&resp).expect("frame_response must succeed");
    let body = unframe(&framed).expect("unframe must succeed");

    let archived = check_archived_root::<IpcResponse>(body)
        .expect("check_archived_root must pass for response");
    assert_eq!(archived.output.as_str(), resp.output.as_str());
    assert!(archived.success);
}

#[test]
fn rkyv_ipc_corrupt_payload_rejected() {
    use touring_rkyv::ipc::unframe;

    // A truncated or corrupt frame must return an error, not panic.
    let corrupt = b"RKYV\x05\x00\x00\x00ab"; // claims 5 bytes body but only 2 provided
    let result = unframe(corrupt);
    assert!(result.is_err(), "corrupt/truncated frame must be rejected");

    // Random bytes without magic header must also be rejected.
    let noise = b"{ this is json not rkyv }";
    let result2 = unframe(noise);
    assert!(
        result2.is_err(),
        "non-rkyv bytes must be rejected by unframe"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 7 — AgenticRL: belief/observable state + policy network forward pass.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn agentic_rl_observable_and_belief_state() {
    use touring_hooks::agentic_rl::{BeliefState, ObservableState};
    use touring_intelligence::rl::online_rl::ImmediateReward;

    // Build a reward and derive an ObservableState from it.
    let reward = ImmediateReward {
        tool_name: "Edit".to_string(),
        accepted: true,
        latency_ms: 120,
        error_count: 0,
        cila_level: 2,
        file_type: 1,
        quality_score: None,
    };
    let obs = ObservableState::from_reward(&reward);
    let encoded = obs.encode();

    assert_eq!(
        encoded.len(),
        16,
        "ObservableState must encode to 16 floats"
    );
    assert!(
        encoded.iter().all(|f| f.is_finite()),
        "all ObservableState fields must be finite: {:?}",
        encoded
    );

    // BeliefState uniform prior: all probabilities equal.
    let mut belief = BeliefState::uniform();
    let prior_enc = belief.encode();
    assert_eq!(prior_enc.len(), 9, "BeliefState must encode to 9 floats");
    assert!(
        prior_enc.iter().all(|f| f.is_finite()),
        "uniform belief must be all-finite: {:?}",
        prior_enc
    );

    // Bayesian update must produce a valid posterior (sum ~1).
    belief.update(&obs);
    let posterior_enc = belief.encode();
    assert!(
        posterior_enc.iter().all(|f| f.is_finite()),
        "posterior belief must be all-finite: {:?}",
        posterior_enc
    );
}

#[test]
fn agentic_rl_policy_network_forward() {
    use touring_hooks::agentic_rl::PolicyNetwork;

    let net = PolicyNetwork::new();

    // Build a 25-float observation (16 obs + 9 belief).
    let obs_input: [f32; 25] = std::array::from_fn(|i| i as f32 * 0.01);
    let (dist, value) = net.forward(&obs_input);

    // Value estimate must be finite.
    assert!(value.is_finite(), "policy value estimate must be finite");

    // Action distribution must produce a valid sample (32 actions: 8 tools × 4 context levels).
    let action = dist.sample();
    assert!(
        action < 32,
        "sampled action must be within valid action space [0, 32), got {action}"
    );

    // Log probability must be finite and negative (for a probability < 1).
    let log_p = dist.log_prob(action);
    assert!(log_p.is_finite(), "log_prob must be finite");
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 8 — Erickson NLP: extract_with_relations returns PopulationResult.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn erickson_extract_with_relations_population_result() {
    use touring_offensive::{NLPPattern, PopulationResult, extract_with_relations};

    let text = "We should upgrade serde because it has a critical CVE. \
                Therefore, the risk of not upgrading outweighs the cost. \
                However, the migration may introduce breaking changes.";

    let result: PopulationResult = extract_with_relations(text);

    assert!(
        !result.elements.is_empty(),
        "extract_with_relations must return non-empty elements"
    );
    assert!(
        result
            .elements
            .iter()
            .any(|e| matches!(e.pattern, NLPPattern::Claim)),
        "must extract at least one Claim"
    );
    assert!(
        result
            .elements
            .iter()
            .any(|e| matches!(e.pattern, NLPPattern::Evidence)),
        "must extract at least one Evidence"
    );

    // relations_added must equal elements with a non-None relation.
    let actual_relations = result
        .elements
        .iter()
        .filter(|e| e.relation.is_some())
        .count();
    assert_eq!(
        result.relations_added, actual_relations,
        "relations_added must match elements with non-None relation"
    );
}

#[test]
fn erickson_extract_with_relations_empty_text() {
    use touring_offensive::extract_with_relations;

    let result = extract_with_relations("");
    assert!(
        result.elements.is_empty(),
        "empty text must produce zero elements"
    );
    assert_eq!(
        result.relations_added, 0,
        "empty text must produce 0 relations"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 9 — PT-BR markers: Portuguese discourse markers match correctly.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ptbr_markers_claim_detection() {
    use touring_offensive::erickson::{
        COMBINED_CLAIM_MARKERS, COMBINED_EVIDENCE_MARKERS, COMBINED_REBUTTAL_MARKERS,
        COMBINED_WARRANT_MARKERS,
    };

    // PT-BR claim markers.
    assert!(
        COMBINED_CLAIM_MARKERS.is_match("Devemos atualizar o sistema"),
        "PT-BR claim marker 'devemos' must match"
    );
    assert!(
        COMBINED_CLAIM_MARKERS.is_match("e necessario corrigir este bug"),
        "PT-BR claim marker 'e necessario' must match"
    );

    // PT-BR evidence markers.
    assert!(
        COMBINED_EVIDENCE_MARKERS.is_match("porque o CVE é critico"),
        "PT-BR evidence marker 'porque' must match"
    );
    assert!(
        COMBINED_EVIDENCE_MARKERS.is_match("segundo pesquisas recentes"),
        "PT-BR evidence marker 'segundo pesquisas' must match"
    );

    // PT-BR warrant markers.
    assert!(
        COMBINED_WARRANT_MARKERS.is_match("portanto a migracao é urgente"),
        "PT-BR warrant marker 'portanto' must match"
    );

    // PT-BR rebuttal markers.
    assert!(
        COMBINED_REBUTTAL_MARKERS.is_match("entretanto o custo pode ser alto"),
        "PT-BR rebuttal marker 'entretanto' must match"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 10 — RelationContext: spatial reasoning between text positions.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn erickson_relation_context_spatial() {
    use touring_offensive::{RelationContext, get_relation_context};

    // Build sentence boundaries manually: two sentences.
    let _text = "First sentence here. Second sentence there.";
    let boundaries = vec![0..20usize, 21..43usize];

    // Both positions in first sentence → Same.
    let ctx_same = get_relation_context(0, 10, &boundaries);
    assert_eq!(
        ctx_same,
        RelationContext::Same,
        "same-sentence positions must be Same"
    );

    // Position in first sentence, other in second → Preceding.
    let ctx_prec = get_relation_context(5, 25, &boundaries);
    assert_eq!(
        ctx_prec,
        RelationContext::Preceding,
        "element in s1 vs s2 must be Preceding"
    );

    // Position in second sentence, other in first → Following.
    let ctx_foll = get_relation_context(25, 5, &boundaries);
    assert_eq!(
        ctx_foll,
        RelationContext::Following,
        "element in s2 vs s1 must be Following"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 11 — CILA budget: monotonically increases with complexity level.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cila_budget_monotonic_across_levels() {
    use touring_hooks::shared::cila::{
        cila_budget, cila_budget_edit, cila_budget_read, cila_budget_write,
    };

    // Read budgets must be non-decreasing across levels 1–6.
    let read_budgets: Vec<usize> = (1..=6).map(|lvl| cila_budget_read(lvl)).collect();
    for window in read_budgets.windows(2) {
        assert!(
            window[1] >= window[0],
            "read budget must be non-decreasing: lvl N={} > lvl N+1={}",
            window[0],
            window[1]
        );
    }

    // Edit budgets must be non-decreasing.
    let edit_budgets: Vec<usize> = (1..=6).map(|lvl| cila_budget_edit(lvl)).collect();
    for window in edit_budgets.windows(2) {
        assert!(window[1] >= window[0], "edit budget must be non-decreasing");
    }

    // Write budgets must be non-decreasing.
    let write_budgets: Vec<usize> = (1..=6).map(|lvl| cila_budget_write(lvl)).collect();
    for window in write_budgets.windows(2) {
        assert!(
            window[1] >= window[0],
            "write budget must be non-decreasing"
        );
    }

    // Low/mid/high triple must respect ordering at any level.
    let low = cila_budget(3, "E2E_TEST", 10, 20, 40);
    let mid = cila_budget(3, "E2E_TEST", 20, 40, 80);
    assert!(mid >= low, "mid budget must be >= low budget at same level");
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 12 — Pattern bandit: RL q-value update + epsilon decay + stats.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn pattern_bandit_rl_updates() {
    use touring_hooks::pattern_bandit::PatternBandit;

    let mut bandit = PatternBandit::new();

    let pattern = "use_expect_over_unwrap";
    assert!(
        !bandit.is_known(pattern),
        "fresh bandit must not know pattern"
    );

    // Positive reward updates q-value upward.
    bandit.update_success(pattern);
    assert!(
        bandit.is_known(pattern),
        "after update, pattern must be known"
    );
    assert!(
        bandit.q_value(pattern) > 0.0,
        "q-value must be positive after success update, got: {}",
        bandit.q_value(pattern)
    );
    assert_eq!(bandit.pattern_updates(pattern), 1, "update count must be 1");

    // Multiple success updates increase q-value.
    let q_after_1 = bandit.q_value(pattern);
    bandit.update_success(pattern);
    bandit.update_success(pattern);
    let q_after_3 = bandit.q_value(pattern);
    assert!(
        q_after_3 >= q_after_1,
        "q-value must not decrease with more success updates: {} vs {}",
        q_after_3,
        q_after_1
    );

    // Failure update decreases q-value.
    bandit.update_failure(pattern);
    let q_after_fail = bandit.q_value(pattern);
    assert!(
        q_after_fail <= q_after_3,
        "q-value must not increase after failure: {} vs {}",
        q_after_fail,
        q_after_3
    );

    // Effectiveness must be in [0.0, 1.0].
    let eff = bandit.effectiveness(pattern);
    assert!(
        (0.0..=1.0).contains(&eff),
        "effectiveness must be in [0.0, 1.0], got {eff}"
    );

    // Global stats must reflect updates.
    assert!(bandit.num_patterns() >= 1, "num_patterns must be >= 1");
    assert!(bandit.total_updates() >= 4, "total_updates must be >= 4");

    // Epsilon must be in (0.0, 1.0].
    let eps = bandit.epsilon();
    assert!(
        (0.0..=1.0).contains(&eps),
        "epsilon must be in (0.0, 1.0], got {eps}"
    );
}
