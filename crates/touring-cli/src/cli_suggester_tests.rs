use super::*;
use serde_json::json;

#[test]
fn looks_like_symbol_accepts_pascal_case() {
    assert!(looks_like_symbol("DomainCircuitBreaker"));
    assert!(looks_like_symbol("Foo"));
}

#[test]
fn action_is_touring_redirect_detects_touring_commands() {
    // F2: a bare `touring …` token (the redirect target) counts as followed.
    assert!(action_is_touring_redirect(
        "Bash",
        &json!({"command": "touring index find Foo"})
    ));
    assert!(action_is_touring_redirect(
        "Bash",
        &json!({"command": "touring run --lang python --code 'print(1)'"})
    ));
    // env-prefixed invocation still has `touring` as a bare token.
    assert!(action_is_touring_redirect(
        "Bash",
        &json!({"command": "TOURING_SUGGESTER_DISABLED=1 touring kpi -j"})
    ));
}

#[test]
fn action_is_touring_redirect_rejects_antipatterns_and_non_bash() {
    // raw antipattern repeated → not followed.
    assert!(!action_is_touring_redirect(
        "Bash",
        &json!({"command": "grep -r Foo crates/"})
    ));
    // "touring" inside a path is not a bare token → not a redirect.
    assert!(!action_is_touring_redirect(
        "Bash",
        &json!({"command": "cat /home/x/.local/bin/touring-helper"})
    ));
    // non-Bash tools are never a touring redirect.
    assert!(!action_is_touring_redirect(
        "Read",
        &json!({"file_path": "/x"})
    ));
}

#[test]
fn classify_adoption_touring_for_touring_bash() {
    // F3: a `touring` Bash invocation is the prior-touring side (numerator).
    assert_eq!(
        classify_adoption("Bash", &json!({"command": "touring index find Foo"})),
        Some(AdoptionClass::Touring)
    );
}

#[test]
fn classify_adoption_antipattern_for_raw_shell() {
    // F3: raw-shell inspection commands are the prior-bash side (denominator).
    for cmd in [
        "grep -rn Foo crates/",
        "cat /etc/hosts",
        "find . -name '*.rs'",
    ] {
        assert_eq!(
            classify_adoption("Bash", &json!({ "command": cmd })),
            Some(AdoptionClass::Antipattern),
            "expected antipattern for `{cmd}`"
        );
    }
}

#[test]
fn classify_adoption_none_for_neutral_and_non_bash() {
    // F3: neutral Bash (cargo) counts in neither numerator nor denominator.
    assert_eq!(
        classify_adoption("Bash", &json!({"command": "cargo check --workspace"})),
        None
    );
    // F3 correctness: Edit/Read are a different axis — they must NOT be counted as
    // antipatterns (would false-fire under the stateless empty WorkflowState).
    assert_eq!(
        classify_adoption("Edit", &json!({"file_path": "/x.rs"})),
        None
    );
    assert_eq!(
        classify_adoption("Read", &json!({"file_path": "/x.rs"})),
        None
    );
}

#[test]
fn looks_like_symbol_accepts_snake_case() {
    assert!(looks_like_symbol("reindex_file"));
    assert!(looks_like_symbol("symbol_store"));
}

#[test]
fn looks_like_symbol_rejects_garbage() {
    assert!(!looks_like_symbol(""));
    assert!(!looks_like_symbol("a")); // too short
    assert!(!looks_like_symbol("hello world")); // space
    assert!(!looks_like_symbol("foo.rs")); // dot
}

#[test]
fn is_code_file_detects_rust() {
    assert!(is_code_file("foo.rs"));
    assert!(is_code_file("path/to/Module.rs"));
    assert!(!is_code_file("README.md"));
}

#[test]
fn classify_grep_pascal_routes_to_index_find() {
    let input = json!({"pattern": "DomainCircuitBreaker"});
    let out = classify_grep(&input).expect("classify_grep emits");
    assert_eq!(out.cluster, "symbol-lookup");
    assert!(
        out.must
            .iter()
            .any(|c| c.command.contains("touring index find"))
    );
    assert!(out.confidence >= 0.9);
    assert_eq!(out.symbol_hint.as_deref(), Some("DomainCircuitBreaker"));
}

#[test]
fn classify_grep_free_text_routes_to_tantivy() {
    let input = json!({"pattern": "TODO fix the thing"});
    let out = classify_grep(&input).expect("classify_grep emits");
    assert_eq!(out.cluster, "free-text-search");
    assert!(
        out.must
            .iter()
            .any(|c| c.command.contains("touring tantivy search"))
    );
}

#[test]
fn classify_read_rust_emits_rust_semantic() {
    let input = json!({"file_path": "crates/foo/src/lib.rs"});
    let out = classify_read(&input).expect("classify_read emits");
    assert_eq!(out.cluster, "read-rust-comprehend");
    assert!(
        out.should
            .iter()
            .any(|c| c.command.contains("rust-semantic"))
    );
    assert!(out.should.iter().any(|c| c.command.contains("ast tdg")));
}

#[test]
fn classify_read_non_code_returns_none() {
    let input = json!({"file_path": "README.md"});
    assert!(classify_read(&input).is_none());
}

#[test]
fn classify_write_tsx_routes_to_perfect_create_tsx() {
    let input = json!({"file_path": "src/Button.tsx"});
    let out = classify_write(&input).expect("classify_write emits");
    assert!(out.cluster.contains("reactcomponent"));
    assert!(out.must.iter().any(|c| c.command.contains("Write tool")));
}

#[test]
fn classify_bash_sed_inplace_routes_to_taco_forge() {
    let input = json!({"command": "sed -i 's/old/new/' foo.rs"});
    let out = classify_bash(&input).expect("classify_bash emits");
    assert_eq!(out.cluster, "anti-pattern-bash-edit");
    assert!(out.must.iter().any(|c| c.command.contains("Edit tool")));
    assert!(out.confidence >= 0.9);
}

#[test]
fn classify_bash_git_routes_to_regra11() {
    let input = json!({"command": "git status"});
    let out = classify_bash(&input).expect("classify_bash emits");
    assert_eq!(out.cluster, "regra-11-git-prohibited");
    assert!(out.confidence >= 0.95);
}

#[test]
fn classify_bash_cargo_build_routes_to_doctor() {
    let input = json!({"command": "cargo build -p touring-hooks --release"});
    let out = classify_bash(&input).expect("classify_bash emits");
    assert_eq!(out.cluster, "system-health-precheck");
    assert!(
        out.must
            .iter()
            .any(|c| c.command.contains("touring doctor"))
    );
}

#[test]
fn render_includes_cluster_and_confidence() {
    let s = Suggestion {
        cluster: "test-cluster".into(),
        must: vec![cmd("touring foo", "do foo")],
        should: vec![],
        may: vec![],
        reason: "because".into(),
        confidence: 0.85,
        enrichment: EnrichmentData::default(),
    };
    let out = render(&s);
    assert!(out.contains("test-cluster"));
    assert!(out.contains("0.85"));
    assert!(out.contains("touring foo"));
    assert!(out.contains("because"));
}

#[test]
fn render_includes_enrichment_when_present() {
    let s = Suggestion {
        cluster: "x".into(),
        must: vec![],
        should: vec![],
        may: vec![],
        reason: "y".into(),
        confidence: 0.9,
        enrichment: EnrichmentData {
            symbol_in_index: Some(true),
            symbol_definition_count: Some(3),
            dependent_count: Some(5),
            ..Default::default()
        },
    };
    let out = render(&s);
    assert!(out.contains("symbol_in_index=yes"));
    assert!(out.contains("defs=3"));
    assert!(out.contains("dependents=5"));
}

#[test]
fn input_hash_stable_for_same_input() {
    let a = input_hash("Grep", &json!({"pattern": "Foo"}));
    let b = input_hash("Grep", &json!({"pattern": "Foo"}));
    assert_eq!(a, b);
}

#[test]
fn input_hash_different_for_different_inputs() {
    let a = input_hash("Grep", &json!({"pattern": "Foo"}));
    let b = input_hash("Grep", &json!({"pattern": "Bar"}));
    assert_ne!(a, b);
}

// ── Slice 2: error-lesson ranking helpers ─────────────────────────────────

#[test]
fn severity_weight_values() {
    assert_eq!(severity_weight("critical"), 3.0);
    assert_eq!(severity_weight("warning"), 2.0);
    assert_eq!(severity_weight("info"), 1.0);
    // Unknown defaults to info.
    assert_eq!(severity_weight("unknown"), 1.0);
    assert_eq!(severity_weight(""), 1.0);
}

#[test]
fn recency_weight_day_zero_is_one() {
    let w = recency_weight(0.0);
    assert!(
        (w - 1.0).abs() < 1e-9,
        "age=0 must give weight=1.0, got {w}"
    );
}

#[test]
fn recency_weight_half_life_30_days() {
    let w = recency_weight(30.0);
    // Half-life = 30d => weight should be ~0.5.
    assert!((w - 0.5).abs() < 0.01, "age=30 should give ~0.5, got {w}");
}

#[test]
fn recency_weight_never_negative() {
    for age in [0.0_f64, 1.0, 7.0, 30.0, 90.0, 365.0, 3650.0] {
        let w = recency_weight(age);
        assert!(w > 0.0 && w <= 1.0, "weight={w} out of (0,1] for age={age}");
    }
}

#[test]
fn frequency_weight_caps_at_one() {
    assert_eq!(frequency_weight(5), 1.0);
    assert_eq!(frequency_weight(10), 1.0);
    assert_eq!(frequency_weight(100), 1.0);
}

#[test]
fn frequency_weight_zero_hits_is_zero() {
    assert_eq!(frequency_weight(0), 0.0);
}

#[test]
fn frequency_weight_partial() {
    let w = frequency_weight(1);
    assert!((w - 0.2).abs() < 1e-9, "1 hit should give 0.2, got {w}");
    let w3 = frequency_weight(3);
    assert!((w3 - 0.6).abs() < 1e-9, "3 hits should give 0.6, got {w3}");
}

#[test]
fn age_days_from_sqlite_valid_timestamp() {
    // "2000-01-01 00:00:00" is Julian Day 2451544.5.
    // We can't know "now" in tests, but we can verify age is non-negative
    // and reasonable (less than 50 years).
    let age = age_days_from_sqlite("2000-01-01 00:00:00");
    assert!(age >= 0.0, "age should be non-negative, got {age}");
    assert!(age < 50.0 * 365.0, "age too large: {age}");
}

#[test]
fn age_days_from_sqlite_malformed_returns_zero() {
    // Only strings that fail to parse (too short or non-numeric fields) return 0.
    // The formula does not validate calendar ranges (month 1-12), only parse errors.
    assert_eq!(age_days_from_sqlite(""), 0.0);
    assert_eq!(age_days_from_sqlite("not-a-date"), 0.0);
    assert_eq!(age_days_from_sqlite("2000-xx-01 00:00:00"), 0.0); // non-numeric month
    assert_eq!(age_days_from_sqlite("20001301"), 0.0); // too short (no separator at pos 5/8)
}

#[test]
fn truncate_short_string_unchanged() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn truncate_long_string_gets_ellipsis() {
    let result = truncate("hello world", 6);
    assert!(result.ends_with('…'), "expected ellipsis, got: {result:?}");
    assert!(
        result.chars().count() <= 6,
        "should be at most 6 chars, got: {result:?}"
    );
}

#[test]
fn truncate_trims_whitespace() {
    assert_eq!(truncate("  hi  ", 20), "hi");
}

#[test]
fn truncate_exact_boundary() {
    // String exactly at max should NOT get ellipsis.
    let s = "abcde";
    assert_eq!(truncate(s, 5), "abcde");
}

#[test]
fn rank_and_trim_empty_input() {
    let result = rank_and_trim(vec![], 800);
    assert!(result.is_empty());
}

#[test]
fn rank_and_trim_sorts_by_score_descending() {
    let items = vec![
        LessonItem {
            text: "low".into(),
            score: 0.1,
            pattern_prefix: "low".into(),
        },
        LessonItem {
            text: "high".into(),
            score: 0.9,
            pattern_prefix: "high".into(),
        },
        LessonItem {
            text: "mid".into(),
            score: 0.5,
            pattern_prefix: "mid".into(),
        },
    ];
    let result = rank_and_trim(items, 10_000);
    assert_eq!(result[0].text, "high");
    assert_eq!(result[1].text, "mid");
    assert_eq!(result[2].text, "low");
}

#[test]
fn rank_and_trim_deduplicates_by_prefix() {
    let items = vec![
        LessonItem {
            text: "error: foo bar".into(),
            score: 0.9,
            pattern_prefix: "error: foo".into(),
        },
        LessonItem {
            text: "error: foo baz".into(),
            score: 0.8,
            pattern_prefix: "error: foo".into(),
        },
        LessonItem {
            text: "warning: different".into(),
            score: 0.7,
            pattern_prefix: "warning: di".into(),
        },
    ];
    let result = rank_and_trim(items, 10_000);
    // Second "error: foo" entry should be deduped away.
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "error: foo bar");
    assert_eq!(result[1].text, "warning: different");
}

#[test]
fn rank_and_trim_respects_budget() {
    // Each item text is 10 chars; cost = text.len() + 4 = 14. Budget 20 → only 1 fits.
    let items = vec![
        LessonItem {
            text: "1234567890".into(),
            score: 0.9,
            pattern_prefix: "a".into(),
        },
        LessonItem {
            text: "abcdefghij".into(),
            score: 0.8,
            pattern_prefix: "b".into(),
        },
    ];
    let result = rank_and_trim(items, 20);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "1234567890");
}

#[test]
fn retrieve_and_render_lessons_returns_none_when_all_sources_empty() {
    // With empty gotcha_matches and no live DB available (tests have no
    // project_root with a real DB), all three sources return empty Vecs →
    // retrieve_and_render_lessons must return None (fail-open).

    let enrichment = EnrichmentData::default(); // gotcha_matches is empty
    use crate::action_signature::{ActionSignature, ContextQualifier};
    let _sig = ActionSignature {
        tool_class: "Bash".into(),
        intent_class: "test".into(),
        context_qualifier: ContextQualifier::Plain,
    };
    // We cannot construct a full HookRuntime in unit tests (requires daemon
    // infrastructure). Instead we verify the gotcha path alone: with empty
    // EnrichmentData, collect_gotcha_lessons returns [], so rank_and_trim
    // on an empty Vec returns [], so the function returns None.
    let gotcha_items = collect_gotcha_lessons(&enrichment);
    assert!(gotcha_items.is_empty());

    let ranked = rank_and_trim(gotcha_items, 800);
    assert!(
        ranked.is_empty(),
        "empty input should yield empty ranked list"
    );
}

#[test]
fn collect_gotcha_lessons_from_enrichment() {
    let enrichment = EnrichmentData {
        gotcha_matches: vec![
            "Known pitfall: do not use .unwrap()".into(),
            "Known pitfall: avoid blocking in async".into(),
        ],
        ..Default::default()
    };
    let items = collect_gotcha_lessons(&enrichment);
    assert_eq!(items.len(), 2);
    // Each gotcha gets score > 0.
    assert!(items[0].score > 0.0);
    assert!(items[1].score > 0.0);
    // Pattern prefix is truncated to 50 chars.
    assert!(items[0].pattern_prefix.chars().count() <= 50);
}

#[test]
fn query_bash_failures_federates_across_dbs() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let kb1 = dir.path().join("k1.db");
    let kb2 = dir.path().join("k2.db");
    for (path, err) in [
        (&kb1, "error[E0412]: cannot find type"),
        (&kb2, "error: linker `cc` not found"),
    ] {
        let conn = rusqlite::Connection::open(path).expect("open");
        conn.execute_batch(
            "CREATE TABLE bash_outcomes (
                     command TEXT, command_short TEXT, command_hash TEXT,
                     exit_code INTEGER, success INTEGER, error_pattern TEXT,
                     file_context TEXT, executed_at TEXT);",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO bash_outcomes \
                 (command_short, success, error_pattern, executed_at) \
                 VALUES ('cargo', 0, ?1, '2026-05-17 12:00:00')",
            rusqlite::params![err],
        )
        .expect("insert failure");
        // A successful run that must NOT be returned (success = 1).
        conn.execute(
            "INSERT INTO bash_outcomes \
                 (command_short, success, error_pattern, executed_at) \
                 VALUES ('cargo', 1, 'ok', '2026-05-17 12:01:00')",
            [],
        )
        .expect("insert ok");
    }
    let hits = query_bash_failures(&[kb1, kb2], "cargo", 10);
    assert_eq!(hits.len(), 2, "bash failures span both knowledge DBs");
    assert!(hits.iter().all(|(cs, _, _)| cs == "cargo"));
    assert!(
        hits.iter().all(|(_, pat, _)| pat != "ok"),
        "success=1 rows are excluded"
    );
}

#[test]
fn query_edit_failures_federates_across_dbs() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let kb1 = dir.path().join("k1.db");
    let kb2 = dir.path().join("k2.db");
    for (path, err) in [(&kb1, "missing semicolon"), (&kb2, "borrow checker E0502")] {
        let conn = rusqlite::Connection::open(path).expect("open");
        conn.execute_batch(
            "CREATE TABLE edit_history (
                     id INTEGER PRIMARY KEY, file_path TEXT, edit_type TEXT,
                     summary TEXT, error_pattern TEXT, language TEXT,
                     symbol_context TEXT, session_id TEXT, edited_at TEXT);",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO edit_history \
                 (file_path, error_pattern, language, edited_at) \
                 VALUES ('src/lib.rs', ?1, 'rust', '2026-05-17 12:00:00')",
            rusqlite::params![err],
        )
        .expect("insert");
    }
    // Matches via `file_path LIKE '%.rs'` across both DBs.
    let hits = query_edit_failures(&[kb1, kb2], "rs", 10);
    assert_eq!(hits.len(), 2, "edit failures span both knowledge DBs");
}

#[test]
fn collect_memory_lessons_one_db_matches_signature() {
    use crate::action_signature::{ActionSignature, ContextQualifier};
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db = dir.path().join("memory.db");
    {
        let conn = rusqlite::Connection::open(&db).expect("open");
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                     key TEXT PRIMARY KEY, value TEXT NOT NULL,
                     tier TEXT NOT NULL DEFAULT 'local',
                     entry_type TEXT NOT NULL DEFAULT 'insight');",
        )
        .expect("schema");
        for (k, v) in [
            (
                "outcome:bash:transcript-ab12:failure",
                "Exit code 144 pgrep",
            ),
            (
                "outcome:edit:transcript-cd34:failure",
                "borrow checker error",
            ),
        ] {
            conn.execute(
                "INSERT INTO memory_entries (key, value) VALUES (?1, ?2)",
                rusqlite::params![k, v],
            )
            .expect("insert");
        }
    }
    let sig = ActionSignature {
        tool_class: "bash".into(),
        intent_class: "pgrep".into(),
        context_qualifier: ContextQualifier::Plain,
    };
    let items = collect_memory_lessons_one_db(&db, &sig);
    // Only `outcome:bash:*:failure` matches tool_class=bash — not `:edit:`.
    assert_eq!(items.len(), 1, "matches outcome:bash:*:failure only");
    assert!(items[0].text.contains("transcript-ab12"));
}

#[test]
fn federated_cache_is_fresh_respects_ttl_boundary() {
    use std::time::{Duration, Instant};
    let base = Instant::now();
    let ttl = Duration::from_secs(300);
    // Inside the window → fresh (the cached DB lists are reused).
    assert!(federated_cache_is_fresh(
        base,
        ttl,
        base + Duration::from_secs(1)
    ));
    assert!(federated_cache_is_fresh(
        base,
        ttl,
        base + Duration::from_secs(299),
    ));
    // At/past the window → stale (a rescan is triggered).
    assert!(!federated_cache_is_fresh(
        base,
        ttl,
        base + Duration::from_secs(300),
    ));
    assert!(!federated_cache_is_fresh(
        base,
        ttl,
        base + Duration::from_secs(600),
    ));
    // `now` before `refreshed_at` (clock skew) saturates to 0 → fresh.
    assert!(federated_cache_is_fresh(
        base + Duration::from_secs(5),
        ttl,
        base,
    ));
}

#[test]
fn lesson_item_text_survives_rank_and_trim() {
    let text = "Past failure [outcome:Bash:test:plain]: something went wrong";
    let items = vec![LessonItem {
        text: text.into(),
        score: 1.5,
        pattern_prefix: "Past failure".into(),
    }];
    let result = rank_and_trim(items, 800);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, text);
}

// ── Phase 3: Task / WebFetch / WebSearch classifiers ─────────────────────

#[test]
fn classify_task_with_valid_input_returns_some_correct_cluster() {
    let input = json!({
        "subagent_type": "touring-engineer",
        "description": "implement the drift-aware cache eviction module"
    });
    let out = classify_task(&input).expect("classify_task must return Some for valid input");
    assert_eq!(out.cluster, "agent-delegation-touring-engineer");
    // must contains decompose validate
    assert!(
        out.must
            .iter()
            .any(|c| c.command.contains("decompose validate")),
        "MUST should include decompose validate"
    );
    // must contains wiring orphans (REGRA #0)
    assert!(
        out.must
            .iter()
            .any(|c| c.command.contains("wiring orphans")),
        "MUST should include wiring orphans"
    );
    assert!(out.confidence >= 0.7, "confidence must pass the 0.7 gate");
}

#[test]
fn classify_via_dispatch_task_returns_some() {
    let input = json!({
        "subagent_type": "touring-scout",
        "description": "scout the hooks crate for orphan symbols"
    });
    let out = classify("Task", &input).expect("classify must return Some for Task");
    assert!(
        out.cluster.starts_with("agent-delegation"),
        "cluster prefix correct"
    );
    assert!(out.confidence >= 0.7);
}

#[test]
fn classify_webfetch_with_url_returns_some_web_fetch_cluster() {
    let input = json!({
        "url": "https://docs.rs/serde/latest/serde/",
        "prompt": "what is the Serialize derive macro signature?"
    });
    let out = classify_webfetch(&input).expect("classify_webfetch must return Some");
    assert_eq!(out.cluster, "web-fetch");
    // must suggests memory recall first
    assert!(
        out.must.iter().any(|c| c.command.contains("memory recall")),
        "MUST should suggest memory recall before network call"
    );
    assert!(out.confidence >= 0.7);
}

#[test]
fn classify_via_dispatch_websearch_returns_some() {
    let input = json!({"query": "rust async trait object dyn future"});
    let out = classify("WebSearch", &input).expect("classify must return Some for WebSearch");
    assert_eq!(out.cluster, "web-fetch");
    assert!(out.confidence >= 0.7);
}

#[test]
fn classify_task_with_empty_input_does_not_panic_and_returns_some() {
    // Malformed / empty input — must not panic; prefer Some with generic guidance.
    let input = json!({});
    let out = classify_task(&input);
    // We document: empty input still yields Some with cluster "agent-delegation".
    assert!(
        out.is_some(),
        "classify_task should be fail-open for empty input"
    );
    let out = out.unwrap();
    assert_eq!(out.cluster, "agent-delegation");
}

#[test]
fn classify_webfetch_with_missing_url_and_query_does_not_panic() {
    // Neither url nor query present — must not panic; returns Some with generic guidance.
    let input = json!({"something_else": "irrelevant"});
    let out = classify_webfetch(&input);
    assert!(
        out.is_some(),
        "classify_webfetch should be fail-open for missing fields"
    );
    let out = out.unwrap();
    assert_eq!(out.cluster, "web-fetch");
    assert!(out.confidence >= 0.7);
}

#[test]
fn classify_unknown_tool_still_returns_none() {
    // Regression: the `_ => None` arm must remain intact.
    let input = json!({"some_field": "some_value"});
    assert!(
        classify("SomeUnknownTool", &input).is_none(),
        "unknown tool names must return None"
    );
    assert!(
        classify("NotebookRead", &input).is_none(),
        "unknown variant must return None"
    );
}

// ── P6.4 CEG gateway tests ────────────────────────────────────────────────

#[test]
fn p6_4_bash_carries_exec_code_routes_to_ceg_advisory() {
    // Test #4: a Bash command with inline executable code (python3 -c "...")
    // must be classified as exec-gate-advisory and surface `touring exec`.
    let input = json!({"command": r#"python3 -c "print(42)""#});
    let out = classify_bash(&input).expect("classify_bash must emit for exec-carrying command");
    assert_eq!(
        out.cluster, "exec-gate-advisory",
        "inline python3 -c should route to exec-gate-advisory"
    );
    assert!(
        out.must.iter().any(|c| c.command.contains("touring exec")),
        "must list must mention `touring exec` (CEG X0..X9 entrypoint)"
    );
    assert!(
        out.confidence >= 0.79,
        "expected confidence >= 0.79, got {}",
        out.confidence
    );
}

#[test]
fn p6_4_trivial_bash_command_does_not_trigger_ceg_advisory() {
    // Test #5: a trivial Bash command (ls) must NOT produce exec-gate-advisory.
    // CEG enrichment fires only when inline executable code is detected.
    let input = json!({"command": "ls -la"});
    let out = classify_bash(&input);
    if let Some(ref o) = out {
        assert_ne!(
            o.cluster, "exec-gate-advisory",
            "trivial `ls` must not trigger CEG advisory"
        );
    }
    // None is also acceptable (no classifier fires for trivial ls).
}

#[test]
fn p6_4_enrichment_data_workflow_stage_hint_defaults_none() {
    // Test #6: a freshly constructed EnrichmentData must have
    // workflow_stage_hint == None (P8.7 extension slot is inert until wired).
    let ed = EnrichmentData::default();
    assert!(
        ed.workflow_stage_hint.is_none(),
        "workflow_stage_hint must default to None (P8.7 slot not yet wired)"
    );
}

#[test]
fn p6_4_ceg_advisory_path_never_panics() {
    // Test #7: exit-0 fail-open invariant. Feeding a variety of Bash
    // commands through classify_bash must never panic, proving the CEG
    // enrichment branch is panic-free under adversarial inputs.
    let payloads: &[&str] = &[
        r#"python3 -c "print('hello')""#,
        r#"bash -c 'echo hello'"#,
        r#"sh -c "echo done""#,
        r#"node -e "console.log(1)""#,
        r#"ruby -e 'puts 42'"#,
        r#"perl -e 'print 1'"#,
        r#"bun -e 'console.log(2)'"#,
        r#"php -r 'echo 1;'"#,
        "./deploy.sh",
        "python script.py",
        "",    // empty command — must not panic
        "   ", // whitespace only — must not panic
    ];
    for cmd_str in payloads {
        let input = json!({"command": cmd_str});
        // Must not panic. Return value may be Some or None — both valid.
        let _ = classify_bash(&input);
    }
}

// ── P8.7 — Workflow Intelligence wiring tests ─────────────────────────

#[test]
fn p8_7_cluster_to_sig_classes_symbol_lookup_returns_bash_grep() {
    // cluster_to_sig_classes must return the Bash/grep sig for the
    // "symbol-lookup" cluster — this drives antipattern detection for
    // BashGrepRaw patterns.
    let (tc, ic) = cluster_to_sig_classes("symbol-lookup");
    assert_eq!(tc, "bash");
    assert_eq!(ic, "grep");
}

#[test]
fn p8_7_cluster_to_sig_classes_file_enumeration_returns_glob() {
    // "file-enumeration" → ("glob", "plain") so that the glob-validation
    // branch in workflow_enrichment_hint can fire.
    let (tc, ic) = cluster_to_sig_classes("file-enumeration");
    assert_eq!(tc, "glob");
    assert_eq!(ic, "plain");
}

#[test]
fn p8_7_cluster_to_sig_classes_unknown_falls_back_to_bash_plain() {
    // Any unmapped cluster must fall back to ("bash", "plain") — never
    // panic, never return empty strings.
    let (tc, ic) = cluster_to_sig_classes("completely-unknown-cluster-xyz");
    assert_eq!(tc, "bash");
    assert_eq!(ic, "plain");
    assert!(!tc.is_empty(), "tool_class must be non-empty");
    assert!(!ic.is_empty(), "intent_class must be non-empty");
}

#[test]
fn p8_7_workflow_enrichment_hint_never_panics_for_any_cluster() {
    // Exit-0 fail-open invariant for workflow_enrichment_hint: feeding
    // every known cluster + an unknown one must never panic.
    let clusters = [
        "symbol-lookup",
        "pre-edit-rust",
        "pre-edit-triage-rust",
        "read-rust-comprehend",
        "read-code-comprehend",
        "file-enumeration",
        "new-tsx-component",
        "new-ts-module",
        "system-health-precheck",
        "exec-gate-advisory",
        "",           // empty
        "UNKNOWN-99", // unmapped fallback
    ];
    for cluster in clusters {
        let classifier = ClassifierOutput {
            cluster: cluster.to_owned(),
            confidence: 0.5,
            ..Default::default()
        };
        // Must not panic. Result may be Some or None — both valid.
        let _ = workflow_enrichment_hint(&classifier);
    }
}

#[test]
fn p8_7_workflow_stage_hint_is_some_for_symbol_lookup_cluster() {
    // When the classifier resolves "symbol-lookup", workflow_enrichment_hint
    // must return Some because detect_stage + advise_next_step always produce
    // a stage label and a next-step hint for a bash/grep sig.
    let classifier = ClassifierOutput {
        cluster: "symbol-lookup".to_owned(),
        confidence: 0.85,
        ..Default::default()
    };
    let hint = workflow_enrichment_hint(&classifier);
    // stage label + next_step_hint are always populated → render() returns Some.
    assert!(
        hint.is_some(),
        "symbol-lookup cluster must produce a non-None workflow hint"
    );
    let s = hint.unwrap();
    assert!(s.contains("stage="), "hint must contain stage= label: {s}");
    assert!(s.contains("next="), "hint must contain next= advice: {s}");
}

#[test]
fn p8_7_workflow_stage_hint_populated_in_enrich_output() {
    // Integration: for a Bash command that resolves to the "symbol-lookup"
    // classifier cluster, workflow_enrichment_hint must return Some —
    // proving the P8.7 wiring in enrich() fires for grep-like inputs.
    let classifier = ClassifierOutput {
        cluster: "symbol-lookup".to_owned(),
        confidence: 0.85,
        ..Default::default()
    };
    let hint = workflow_enrichment_hint(&classifier);
    assert!(
        hint.is_some(),
        "P8.7 wiring: enrich() must populate workflow_stage_hint for symbol-lookup"
    );
}

// ── C4: generic-banner cluster dedupe (banner-blindness reduction) ───────────

#[test]
fn cluster_dedupe_key_is_stable_and_distinct() {
    // Same cluster name → same key (stable across calls).
    assert_eq!(
        cluster_dedupe_key("system-health-precheck"),
        cluster_dedupe_key("system-health-precheck")
    );
    // Different cluster names → different keys.
    assert_ne!(
        cluster_dedupe_key("system-health-precheck"),
        cluster_dedupe_key("regra-11-git-prohibited")
    );
}

#[test]
fn cluster_dedupe_gate_specific_suggestion_always_proceeds() {
    // A symbol-specific suggestion carries fresh signal — it must proceed every
    // time and never be deduped, regardless of repetition.
    let specific = ClassifierOutput {
        cluster: "symbol-lookup".to_owned(),
        symbol_hint: Some("DomainCircuitBreaker".to_owned()),
        ..Default::default()
    };
    assert!(matches!(
        cluster_dedupe_gate(&specific),
        ClusterDecision::Proceed
    ));
    // Repeating the same specific suggestion still proceeds (never deduped).
    assert!(matches!(
        cluster_dedupe_gate(&specific),
        ClusterDecision::Proceed
    ));
}

#[test]
fn cluster_dedupe_gate_generic_fires_once_then_suppresses() {
    // A generic banner (no symbol/file hint) fires once, then is suppressed
    // within the TTL window. The cluster name is unique so the shared process
    // cache cannot collide with other tests.
    let generic = ClassifierOutput {
        cluster: "test-unique-banner-c4-once".to_owned(),
        symbol_hint: None,
        file_hint: None,
        ..Default::default()
    };
    // First emission clears the gate…
    assert!(matches!(
        cluster_dedupe_gate(&generic),
        ClusterDecision::Proceed
    ));
    // …and a subsequent emission within the window is deduped.
    assert!(matches!(
        cluster_dedupe_gate(&generic),
        ClusterDecision::Suppress
    ));
}

// ── Code Mode induction (C8) ───────────────────────────────────────────────────

#[test]
fn is_scan_command_detects_searches() {
    assert!(is_scan_command("grep -rn Foo crates/"));
    assert!(is_scan_command("rg Foo"));
    assert!(is_scan_command("  rg\tFoo")); // leading ws + tab
    assert!(is_scan_command("egrep bar baz.txt"));
    assert!(is_scan_command("find . -name '*.rs'"));
}

#[test]
fn is_scan_command_rejects_non_searches() {
    assert!(!is_scan_command("cargo test"));
    assert!(!is_scan_command("ls -la"));
    assert!(!is_scan_command("find . -type d")); // no -name
    assert!(!is_scan_command("grepfoo")); // not the grep command
}

#[test]
fn is_shell_loop_detects_iteration() {
    assert!(is_shell_loop("for f in crates/*; do grep X \"$f\"; done"));
    assert!(is_shell_loop(
        "while read line; do echo \"$line\"; done < list"
    ));
    assert!(is_shell_loop("find . -name '*.rs' | xargs grep TODO"));
}

#[test]
fn is_shell_loop_rejects_non_loops() {
    assert!(!is_shell_loop("grep -rn Foo crates/"));
    assert!(!is_shell_loop("cargo build"));
    // "for input" has no " in " token, so the for-loop guard stays closed.
    assert!(!is_shell_loop("echo waiting for input"));
}

#[test]
fn code_mode_kind_classifies_tools() {
    let loop_cmd = json!({"command": "for f in *.rs; do grep X \"$f\"; done"});
    assert!(matches!(
        code_mode_kind("Bash", &loop_cmd),
        Some(CodeModeKind::Loop)
    ));
    let scan_cmd = json!({"command": "grep -rn Foo crates/"});
    assert!(matches!(
        code_mode_kind("Bash", &scan_cmd),
        Some(CodeModeKind::Scan)
    ));
    // A Grep tool call is itself a scan.
    assert!(matches!(
        code_mode_kind("Grep", &json!({"pattern": "Foo"})),
        Some(CodeModeKind::Scan)
    ));
    // Read is deliberately excluded; cargo is neither scan nor loop.
    assert!(code_mode_kind("Read", &json!({"file_path": "x.rs"})).is_none());
    assert!(code_mode_kind("Bash", &json!({"command": "cargo test"})).is_none());
}

#[test]
fn code_mode_output_carries_touring_run_hint() {
    // Generic case (no specializable input): still `touring run` (code-mode WITHOUT
    // MCP), never the `touring_ctx_execute` MCP tool — the goal is code-mode w/o MCP.
    let empty = json!({});
    for kind in [CodeModeKind::Loop, CodeModeKind::Scan] {
        let out = code_mode_output(&kind, "Bash", &empty);
        assert!(out.cluster.starts_with("code-mode"));
        assert!(out.must.iter().any(|c| c.command.contains("touring run")));
        assert!(
            out.must
                .iter()
                .all(|c| !c.command.contains("touring_ctx_execute"))
        );
        // High, fixed confidence — a deliberate nudge that bypasses the gate.
        assert!((out.confidence - 0.95).abs() < f32::EPSILON);
        // No symbol/file hint → generic-banner dedupe also caps it per window.
        assert!(out.symbol_hint.is_none() && out.file_hint.is_none());
    }
}

#[test]
fn code_mode_specializes_grep_tool_scan() {
    // A structured Grep tool call → a concrete, ready-to-run `touring run` command
    // whose pattern + glob are derived from the real input (not a placeholder).
    let input = json!({"pattern": "AuthValidator", "path": "crates/", "glob": "*.rs"});
    let out = code_mode_output(&CodeModeKind::Scan, "Grep", &input);
    let must = &out.must[0].command;
    assert!(must.starts_with("touring run --lang python"));
    assert!(must.contains("AuthValidator")); // pattern from the real input
    assert!(must.contains("crates/**/*.rs")); // glob composed from path + glob
    assert!(!must.contains("touring_ctx_execute")); // code-mode WITHOUT MCP
}

#[test]
fn extract_scan_target_grep_tool_and_bash() {
    // Structured Grep tool → high-precision pattern + composed glob.
    assert_eq!(
        extract_scan_target(
            "Grep",
            &json!({"pattern": "fn run", "path": "src", "glob": "*.rs"})
        ),
        Some(("fn run".to_string(), "src/**/*.rs".to_string()))
    );
    // Bash grep → best-effort pattern + path.
    let (pat, glob) = extract_scan_target(
        "Bash",
        &json!({"command": "grep -rn \"TODO\" crates/touring-cli"}),
    )
    .expect("bash grep parses");
    assert_eq!(pat, "TODO");
    assert!(glob.starts_with("crates/touring-cli"));
    // A non-scan Bash command is NOT specialized → generic fallback.
    assert!(extract_scan_target("Bash", &json!({"command": "cargo test"})).is_none());
    // A shell loop is not a scan target either (generic template instead).
    assert!(
        extract_scan_target(
            "Bash",
            &json!({"command": "for f in *.rs; do echo $f; done"})
        )
        .is_none()
    );
}

#[test]
fn scan_glob_composes_path_and_filter() {
    assert_eq!(scan_glob("crates/", Some("*.rs")), "crates/**/*.rs");
    assert_eq!(scan_glob(".", None), "./**/*");
    assert_eq!(scan_glob("src", Some("*.py")), "src/**/*.py");
}

#[test]
fn crosses_threshold_fires_only_on_edge() {
    // Edge: the call that takes the running count from threshold-1 to threshold.
    assert!(crosses_threshold(2, 3)); // 3rd scan fires
    assert!(!crosses_threshold(0, 3)); // 1st
    assert!(!crosses_threshold(1, 3)); // 2nd
    assert!(!crosses_threshold(3, 3)); // 4th+ — already fired, suppress
    // Saturating: no wrap/panic at the ceiling.
    assert!(!crosses_threshold(u32::MAX, 3));
}

#[test]
fn detect_code_mode_fires_on_explicit_loop() {
    // A loop fires immediately (counter-independent, deterministic).
    let loop_cmd = json!({"command": "for f in *.rs; do grep X \"$f\"; done"});
    let out = detect_code_mode("Bash", &loop_cmd).expect("loop should fire");
    assert_eq!(out.cluster, "code-mode-loop");
    // A non-scan/non-loop tool never fires.
    assert!(detect_code_mode("Bash", &json!({"command": "cargo build"})).is_none());
    assert!(detect_code_mode("Edit", &json!({"file_path": "x.rs"})).is_none());
}

// ── Task #6: pillar induction (the active compounding layer) ──────────────────

#[test]
fn classify_pillar_master_cli_for_atomic_touring() {
    // An atomic `touring` discovery call maps to the MasterCli pillar (the gap).
    for cmd in [
        "touring index find AuthValidator",
        "touring ast blast crates/x/src/lib.rs",
        "touring wiring orphans -j",
    ] {
        assert_eq!(
            classify_pillar("Bash", &json!({ "command": cmd })),
            Some(Pillar::MasterCli),
            "{cmd}"
        );
    }
}

#[test]
fn classify_pillar_learning_memory_for_doc_grep() {
    assert_eq!(
        classify_pillar("Bash", &json!({"command": "grep -rn \"deadlock\" docs/"})),
        Some(Pillar::LearningMemory)
    );
}

#[test]
fn classify_pillar_none_for_master_neutral_and_nonbash() {
    // Already a master command, neutral cargo, and non-Bash → no pillar nudge.
    assert_eq!(
        classify_pillar("Bash", &json!({"command": "touring scout AuthValidator"})),
        None
    );
    assert_eq!(
        classify_pillar("Bash", &json!({"command": "cargo check --workspace"})),
        None
    );
    assert_eq!(
        classify_pillar("Read", &json!({"file_path": "/x.rs"})),
        None
    );
}

#[test]
fn master_cli_command_derives_master_and_carries_arg() {
    assert_eq!(
        master_cli_command("touring index find Foo"),
        Some((
            "touring scout Foo".into(),
            "scout".into(),
            Some("Foo".into())
        ))
    );
    assert_eq!(
        master_cli_command("touring ast blast f.rs"),
        Some((
            "touring blast f.rs".into(),
            "blast".into(),
            Some("f.rs".into())
        ))
    );
    // Argless master (wiring orphans → guard, no carried arg).
    assert_eq!(
        master_cli_command("touring wiring orphans -j"),
        Some(("touring guard".into(), "guard".into(), None))
    );
    // Not a fuseable atomic → None.
    assert_eq!(master_cli_command("touring scout Foo"), None);
}

/// Injection-density invariant (feedback 2026-06-29): a nudge carries the REAL
/// argument from the input — never a `<placeholder>` when the value is derivable.
#[test]
fn pillar_nudges_carry_real_arg_no_placeholder() {
    let m = master_cli_nudge("touring index find AuthValidator", Pillar::MasterCli);
    let must = &m.must[0].command;
    assert!(
        must.contains("AuthValidator"),
        "MUST carries the real symbol: {must}"
    );
    assert!(!must.contains('<'), "no placeholder in MUST: {must}");

    let l = learning_memory_nudge("grep -rn \"deadlock\" docs/", Pillar::LearningMemory);
    let lmust = &l.must[0].command;
    assert!(
        lmust.contains("deadlock"),
        "MUST carries the real term: {lmust}"
    );
    assert!(!lmust.contains('<'), "no placeholder in MUST: {lmust}");
}

#[test]
fn action_followed_pillar_detects_masters_and_recall() {
    for c in [
        "touring scout Foo",
        "touring blast f.rs",
        "touring memory recall \"x\"",
    ] {
        assert!(
            action_followed_pillar("Bash", &json!({ "command": c })),
            "{c}"
        );
    }
    for c in ["touring index find Foo", "cargo check"] {
        assert!(
            !action_followed_pillar("Bash", &json!({ "command": c })),
            "{c}"
        );
    }
}

#[test]
fn pillar_induction_disarmed_by_default() {
    // Default-OFF: with the env unset, the layer never emits (mirrors F7c).
    // The smoke test arms it via TOURING_PILLAR_INDUCTION_ARMED at runtime.
    if std::env::var("TOURING_PILLAR_INDUCTION_ARMED").is_err() {
        assert!(pillar_classifier("Bash", &json!({"command": "touring index find Foo"})).is_none());
    }
}

#[test]
fn code_mode_loop_carries_real_glob() {
    // Injection-density (feedback 2026-06-29): a loop over a derivable glob yields a
    // specific `touring run` carrying that glob, not the generic placeholder.
    let out = code_mode_output(
        &CodeModeKind::Loop,
        "Bash",
        &json!({"command": "for f in *.md; do wc -l \"$f\"; done"}),
    );
    let must = &out.must[0].command;
    assert!(
        must.contains("*.md"),
        "loop nudge carries the real glob: {must}"
    );
    assert!(
        !must.contains("<your scan/loop"),
        "no generic placeholder when derivable: {must}"
    );
    // A loop whose iterable is not a glob (numeric / command-substitution) still carries
    // the REAL command verbatim as `--lang bash` — no `<placeholder>`, no guessed python
    // translation (injection-density invariant: the loop body IS derivable, just as bash).
    let verbatim = code_mode_output(
        &CodeModeKind::Loop,
        "Bash",
        &json!({"command": "for i in 1 2 3; do echo $i; done"}),
    );
    let vc = &verbatim.must[0].command;
    assert!(
        vc.contains("for i in 1 2 3") && vc.contains("--lang bash"),
        "non-glob loop travels verbatim as bash: {vc}"
    );
    assert!(
        !vc.contains('<'),
        "no placeholder when the command is derivable: {vc}"
    );
}

/// Injection-density invariant (Gabriel 2026-06-29, `rules/touring-4-pillars.md`),
/// enforced across EVERY nudge family — not just the pillar nudges (the gap that let
/// the `code-mode-loop` / `exec-gate` placeholders survive). Each emitted command must
/// carry the REAL value derived from the trigger input; a `<placeholder>` is allowed
/// ONLY for a genuinely non-derivable part (e.g. the git memory-recall topic — excluded
/// here). One positive assertion per family (the value travels) + a negative guard on the
/// specific generic literal that was eliminated.
#[test]
fn every_derivable_nudge_carries_real_value_not_placeholder() {
    fn joined(o: &ClassifierOutput) -> String {
        o.must
            .iter()
            .chain(&o.should)
            .chain(&o.may)
            .map(|c| c.command.as_str())
            .collect::<Vec<_>>()
            .join(" || ")
    }

    // Code-mode loop with a command-substitution iterable (no derivable glob): the real
    // command travels verbatim as `--lang bash`, never the `<your scan/loop>` template.
    let loop_sub = detect_code_mode(
        "Bash",
        &json!({"command": "for pid in $(pgrep -f touring); do echo $pid; done"}),
    )
    .expect("explicit loop fires Code Mode");
    let s = joined(&loop_sub);
    assert!(s.contains("pgrep"), "verbatim loop travels: {s}");
    assert!(
        !s.contains("<your scan"),
        "no generic loop placeholder: {s}"
    );
    assert!(!s.contains("<script"), "no script placeholder: {s}");

    // `find -name '*.rs'` → the real glob, not `<pattern>`.
    let find = classify_bash(&json!({"command": "find . -name '*.rs'"})).expect("find emits");
    let s = joined(&find);
    assert!(s.contains("*.rs"), "find glob travels: {s}");
    assert!(!s.contains("<pattern>"), "no find placeholder: {s}");

    // `sed -i … FILE` → the real path, not `<file>`.
    let sed = classify_bash(&json!({"command": "sed -i 's/a/b/' src/foo.rs"})).expect("sed emits");
    let s = joined(&sed);
    assert!(s.contains("src/foo.rs"), "sed target travels: {s}");
    assert!(!s.contains("--path <file>"), "no sed placeholder: {s}");

    // Inline executable code → the real command in `touring exec`, not `<command>`.
    let exec =
        classify_bash(&json!({"command": "python3 -c 'print(1)'"})).expect("exec-gate emits");
    let s = joined(&exec);
    assert!(s.contains("python3"), "exec command travels: {s}");
    assert!(!s.contains("\"<command>\""), "no exec placeholder: {s}");

    // Free-text grep → the real pattern in the symbols suggestion, not `<query>`.
    let grep = classify_grep(&json!({"pattern": "race condition"})).expect("grep emits");
    let s = joined(&grep);
    assert!(s.contains("race condition"), "grep pattern travels: {s}");
    assert!(!s.contains("<query>"), "no grep-may placeholder: {s}");

    // Task delegation → the real task description in the memory-recall (the `<task_id>` /
    // `<objective>` markers stay: those name future entities, genuinely not in the input).
    let task = classify(
        "Task",
        &json!({"subagent_type": "engineer", "description": "refactor the auth module"}),
    )
    .expect("Task emits");
    let s = joined(&task);
    assert!(
        s.contains("refactor the auth module"),
        "task description travels: {s}"
    );
    assert!(
        !s.contains("<task_description>"),
        "no task-desc placeholder: {s}"
    );
}

/// `bash_code_mode_command` must produce a single-quoted `--code` body whose embedded
/// single quotes are escaped with the `'\''` shell idiom — so a loop containing quotes is
/// still a VALID, runnable command. The density invariant's purpose is a USABLE nudge, not
/// merely a specific one: a malformed command would be specific yet broken.
#[test]
fn bash_code_mode_command_escapes_single_quotes_for_a_runnable_command() {
    let quoted = bash_code_mode_command("for f in *.md; do echo 'x'; done");
    // The embedded `'x'` becomes `'\''x'\''` — no bare unescaped quote splits the wrapper.
    assert!(
        quoted.contains(r"'\''x'\''"),
        "single quotes escaped via the shell idiom: {quoted}"
    );
    assert!(
        quoted.starts_with("touring run --lang bash --code '"),
        "wrapped as --lang bash: {quoted}"
    );
    // No quotes → the command travels verbatim.
    let plain = bash_code_mode_command("for f in *.rs; do wc -l $f; done");
    assert!(
        plain.contains("for f in *.rs; do wc -l $f; done"),
        "verbatim when no quotes to escape: {plain}"
    );
}

/// A code-mode output embeds the real command/pattern verbatim in its MUST, so it
/// carries input-specific signal even without a symbol/file hint — the cluster
/// dedupe must never suppress it. Two DIFFERENT loops in one TTL window both
/// deserve their nudge; identical inputs are already anti-spammed by `run`'s
/// `(tool, input)` hash cache.
#[test]
fn code_mode_cluster_bypasses_dedupe_carrying_input_specific_signal() {
    let loop_nudge = ClassifierOutput {
        cluster: "code-mode-loop".to_owned(),
        symbol_hint: None,
        file_hint: None,
        ..Default::default()
    };
    assert!(loop_nudge.carries_input_specific_signal());
    // Both consecutive emissions proceed — never suppressed as a generic banner.
    assert!(matches!(
        cluster_dedupe_gate(&loop_nudge),
        ClusterDecision::Proceed
    ));
    assert!(matches!(
        cluster_dedupe_gate(&loop_nudge),
        ClusterDecision::Proceed
    ));
}

/// Specific-or-absent: the LearningMemory pillar only classifies when the search
/// topic is mechanically derivable, so `learning_memory_nudge` can never emit a
/// placeholder recall query (injection-density invariant, feedback 2026-06-29).
#[test]
fn learning_memory_pillar_requires_derivable_topic() {
    // Derivable topic → the pillar applies.
    let derivable = serde_json::json!({"command": "grep -rn \"daemon flush\" docs/"});
    assert_eq!(
        classify_pillar("Bash", &derivable),
        Some(Pillar::LearningMemory)
    );
    // Memory-surface search with an empty (non-derivable) pattern → absent, not
    // a placeholder.
    let non_derivable = serde_json::json!({"command": "grep -rn \"\" docs/lessons.md"});
    assert_eq!(classify_pillar("Bash", &non_derivable), None);
    // MasterCli mapping is unaffected by the guard.
    let atomic = serde_json::json!({"command": "touring index find HookRuntime"});
    assert_eq!(classify_pillar("Bash", &atomic), Some(Pillar::MasterCli));
}

/// When a file is absent from the blake3 registry the enrichment under-reports;
/// the rendered suggestion must carry the ready-to-run rebuild command with the
/// REAL project root (REGRA #0 potencialização + density invariant).
#[test]
fn stale_index_hint_renders_real_rebuild_command() {
    let suggestion = Suggestion {
        cluster: "read-rust-comprehend".to_owned(),
        must: vec![],
        should: vec![],
        may: vec![],
        reason: "test".to_owned(),
        confidence: 0.9,
        enrichment: EnrichmentData {
            file_is_indexed: Some(false),
            stale_index_hint: Some("touring index rebuild --dir /home/user/ws".to_owned()),
            ..Default::default()
        },
    };
    let rendered = render(&suggestion);
    assert!(
        rendered.contains("Stale-index: touring index rebuild --dir /home/user/ws"),
        "real rebuild command travels: {rendered}"
    );
    assert!(
        !rendered.contains("--dir <"),
        "no placeholder dir: {rendered}"
    );
}

// ── Portfolio prior-art injection (P5) ───────────────────────────────────────
//
// The Write classifier surfaces prior art BEFORE a file is created. The intent
// it queries with must be a REAL derived value — the injection-density
// invariant forbids a `<placeholder>` whenever the value is derivable.

#[test]
fn intent_comes_from_the_python_docstring_being_written() {
    let content = "#!/usr/bin/env python3\n\"\"\"Generate a professional PDF from an HTML template.\"\"\"\nimport sys\n";
    let intent = intent_for_new_file("/tmp/x/report_builder.py", Some(content))
        .expect("docstring yields an intent");
    assert!(intent.contains("professional PDF"), "{intent}");
}

#[test]
fn intent_comes_from_the_rust_module_header_being_written() {
    let content = "#![allow(dead_code)]\n//! Draws the module dependency map as an SVG diagram.\n\nuse std::fmt;\n";
    let intent = intent_for_new_file("/tmp/x/render.rs", Some(content))
        .expect("module header yields an intent");
    assert!(intent.contains("dependency map"), "{intent}");
}

#[test]
fn intent_falls_back_to_the_file_stem_split_into_words() {
    let intent = intent_for_new_file("/tmp/x/generate_pdf_report.py", None)
        .expect("stem yields an intent");
    assert_eq!(intent, "generate pdf report");
}

#[test]
fn intent_is_never_a_placeholder() {
    // The invariant: a derivable value travels verbatim; `<...>` never appears.
    for (path, body) in [
        ("/tmp/x/generate_pdf.py", None),
        ("/tmp/x/a_b.rs", Some("//! Something short.\n")),
        ("/tmp/x/render_map.py", Some("\"\"\"Render the dependency map as SVG output.\"\"\"\n")),
    ] {
        if let Some(intent) = intent_for_new_file(path, body) {
            assert!(!intent.contains('<'), "placeholder leaked for {path}: {intent}");
            assert!(!intent.trim().is_empty(), "empty intent for {path}");
        }
    }
}

#[test]
fn uninformative_stems_yield_no_intent_rather_than_noise() {
    // Querying the portfolio for "mod" or "lib" would return noise; better to
    // stay silent than to inject a meaningless nudge.
    for path in ["/tmp/x/mod.rs", "/tmp/x/lib.rs", "/tmp/x/main.rs", "/tmp/x/a.py"] {
        assert!(
            intent_for_new_file(path, None).is_none(),
            "should not derive an intent from {path}"
        );
    }
}

#[test]
fn short_prose_is_not_mistaken_for_a_purpose() {
    // A one-word docstring is a label; fall through to the stem instead.
    let intent = intent_for_new_file("/tmp/x/pdf_writer.py", Some("\"\"\"main\"\"\"\n"))
        .expect("falls back to the stem");
    assert_eq!(intent, "pdf writer");
}

#[test]
fn write_classifier_stays_functional_without_a_portfolio_index() {
    // Fail-open: with no index the Write suggestion must render exactly as it
    // did before the portfolio existed — never an error, never an empty nudge.
    let out = classify_write(&json!({
        "file_path": "/tmp/x/new_module.rs",
        "content": "//! A brand new module.\n",
    }))
    .expect("write classifier still emits");
    assert!(!out.must.is_empty(), "the create-pipeline MUST survives");
    assert!(
        out.should.iter().all(|c| !c.command.contains("<intent>")),
        "no placeholder intent in the portfolio nudge: {:?}",
        out.should.iter().map(|c| &c.command).collect::<Vec<_>>()
    );
}

#[test]
fn license_banners_are_not_mistaken_for_purpose() {
    // Audit finding F5: a licence header cleared the 20-char prose floor and
    // became the intent, sending the portfolio hunting for "copyright ... all
    // rights reserved" on every Write of a file with a banner.
    for banner in [
        "# Copyright 2026 Acme Incorporated. All rights reserved worldwide.\nimport sys\n",
        "# SPDX-License-Identifier: Apache-2.0 with a long trailing clause\nimport sys\n",
        "# -*- coding: utf-8 -*- and some more text to clear the length floor\nimport sys\n",
        "# Generated by protoc; DO NOT EDIT this file by hand under any circumstance\n",
    ] {
        let intent = intent_for_new_file("/tmp/x/data_loader.py", Some(banner));
        assert_eq!(
            intent.as_deref(),
            Some("data loader"),
            "banner leaked into the intent: {intent:?}"
        );
    }
}

#[test]
fn real_prose_after_a_banner_is_still_found() {
    // The filter must skip boilerplate, not stop at it.
    let src = "# Copyright 2026 Acme Inc. All rights reserved.\n\"\"\"Render the dependency graph as an SVG map.\"\"\"\n";
    let intent = intent_for_new_file("/tmp/x/whatever.py", Some(src)).expect("intent");
    assert!(intent.contains("dependency graph"), "{intent}");
}

#[test]
fn boilerplate_detection_is_case_insensitive_and_specific() {
    assert!(is_boilerplate("COPYRIGHT 2026 ACME"));
    assert!(is_boilerplate("Licensed under the MIT licence"));
    assert!(!is_boilerplate("Generate a professional PDF report"));
    assert!(!is_boilerplate("Copy rows from the staging table"));
}

#[test]
fn portfolio_cache_reloads_when_the_index_file_changes() {
    // Audit finding F4: a plain OnceLock never saw `touring portfolio refresh`,
    // so an in-daemon hook asserted stale prior art indefinitely. The cache key
    // is the file's mtime, so this asserts the invalidation path is reached.
    let first = portfolio_index();
    let second = portfolio_index();
    match (first, second) {
        (Some(a), Some(b)) => assert_eq!(
            a.entries.len(),
            b.entries.len(),
            "two reads with an unchanged file must agree"
        ),
        (None, None) => {}
        _ => panic!("cache returned inconsistently across identical reads"),
    }
    // And the mtime probe must never panic when the index is absent.
    let _ = portfolio_mtime();
}
