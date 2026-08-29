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

/// REGRA #11 v2 (23/08/2026): read-only git is PERMITTED and the executor
/// allows it. Until 2026-08-25 this test asserted the opposite — it encoded
/// the very claim the nudge got wrong, and so defended the defect.
#[test]
fn classify_bash_git_readonly_is_permitted_not_prohibited() {
    let input = json!({"command": "git status"});
    // Nada é emitido: o comando é permitido e não exige ação. O arm
    // `regra-11-git-safe` que existia aqui tinha confiança 0.55 e era
    // descartado pelo gate conformal (LEGACY_THRESHOLD = 0.7) — código
    // aparentemente correto e provadamente inerte, removido em 26/08.
    assert!(
        classify_bash(&input).is_none(),
        "git de leitura não exige ação: o classificador deve calar"
    );
}

/// The DESTRUCTIVE class carries the ritual, and it is where the confidence
/// belongs — that is the half of REGRA #11 v2 with real consequence.
#[test]
fn classify_bash_git_destructive_demands_the_ritual() {
    for cmd_str in [
        "git reset --hard HEAD~1",
        "git stash",
        "git clean -fd",
        "git push --force origin main",
        "git rebase -i main",
        "git branch -D feature",
        "git checkout -- src/lib.rs",
    ] {
        let input = json!({ "command": cmd_str });
        let out = classify_bash(&input).unwrap_or_else(|| panic!("no output for {cmd_str}"));
        assert_eq!(out.cluster, "regra-11-git-destructive", "cmd: {cmd_str}");
        assert!(out.confidence >= 0.95, "cmd: {cmd_str}");
        assert!(
            out.must.iter().any(|c| c.command.contains("GIT_DESTRUCTIVE_OK=1")),
            "ritual token missing for {cmd_str}"
        );
        assert!(
            out.must.iter().any(|c| c.command.contains("safety/")),
            "safety-branch snapshot missing for {cmd_str}"
        );
    }
}

/// The executor's carve-outs are mirrored exactly: these forms do NOT touch
/// the working tree, so gating them would tax a safe command.
#[test]
fn git_destructive_mirrors_the_executor_carve_outs() {
    for safe in [
        "git stash list",
        "git stash show",
        "git restore --staged foo.rs",
        "git reset HEAD~1",
        "git reset --soft HEAD~1",
        "git clean -n",
        "git status",
        "git commit -m x",
        "git push origin main",
        "git branch -d merged",
    ] {
        assert!(!git_is_destructive(safe), "false positive: {safe}");
    }
    for dangerous in [
        "git restore src/lib.rs",
        "git restore --staged --worktree src/lib.rs",
        "git reset --keep HEAD~1",
        "git push -f",
        "git reflog expire --all",
        "git gc --prune=now",
        "git filter-branch --tree-filter x",
    ] {
        assert!(git_is_destructive(dangerous), "false negative: {dangerous}");
    }
}

/// Every verb DECLARED in `GIT_DESTRUCTIVE_VERBS` must actually be gated by
/// the predicate. Without this the list is decoration: it could name a verb
/// the regex never matches and nothing would notice (the "comment asserts a
/// symmetry that does not exist" failure). This makes the declaration a
/// contract the executor has to honour.
#[test]
fn every_declared_destructive_verb_is_actually_gated() {
    for verb in GIT_DESTRUCTIVE_VERBS {
        // Build the most ordinary command carrying that verb.
        let command = match *verb {
            "stash" => "git stash".to_string(),
            "restore" => "git restore src/lib.rs".to_string(),
            "checkout --" => "git checkout -- src/lib.rs".to_string(),
            "gc --prune" => "git gc --prune=now".to_string(),
            "reflog expire" => "git reflog expire --all".to_string(),
            v => format!("git {v} target"),
        };
        assert!(
            git_is_destructive(&command),
            "declared verb `{verb}` is not gated by the predicate (command: `{command}`)"
        );
    }
}

/// The deliberate per-command token means the ritual already happened
/// upstream — re-gating it would make the approved path unusable.
#[test]
fn git_destructive_respects_the_completed_ritual_token() {
    assert!(!git_is_destructive("GIT_DESTRUCTIVE_OK=1 git reset --hard HEAD~1"));
    let input = json!({"command": "GIT_DESTRUCTIVE_OK=1 git reset --hard HEAD~1"});
    let out = classify_bash(&input);
    assert!(
        out.as_ref().map(|o| o.cluster.as_str()) != Some("regra-11-git-destructive"),
        "a completed ritual must not be re-gated"
    );
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
    let a = input_hash(Path::new("/t/ih"), "Grep", &json!({"pattern": "Foo"}));
    let b = input_hash(Path::new("/t/ih"), "Grep", &json!({"pattern": "Foo"}));
    assert_eq!(a, b);
}

#[test]
fn input_hash_different_for_different_inputs() {
    let a = input_hash(Path::new("/t/ih"), "Grep", &json!({"pattern": "Foo"}));
    let b = input_hash(Path::new("/t/ih"), "Grep", &json!({"pattern": "Bar"}));
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
        cluster_dedupe_key(Path::new("/t/a"), "system-health-precheck"),
        cluster_dedupe_key(Path::new("/t/a"), "system-health-precheck")
    );
    // Different cluster names → different keys.
    assert_ne!(
        cluster_dedupe_key(Path::new("/t/a"), "system-health-precheck"),
        cluster_dedupe_key(Path::new("/t/a"), "regra-11-git-prohibited")
    );
}

/// O dedupe de banner é por PROJETO, não por processo.
///
/// [`cache`] é um `static` do processo e o daemon que avalia este hook é
/// longo-vivo, atendendo mais de um projeto. Sem o escopo por raiz, o banner
/// emitido enquanto se trabalhava no projeto A calava o mesmo banner no projeto
/// B — para um leitor que nunca o tinha visto. "Uma vez por janela" é
/// propriedade do leitor de um projeto, não do processo.
///
/// A asserção é escrita sobre o par (mesmo cluster, raízes distintas) porque é
/// exatamente esse par que a versão anterior colapsava.
#[test]
fn a_generic_banner_is_deduped_per_project_not_per_process() {
    let generic = ClassifierOutput {
        cluster: "test-unique-banner-c4-per-project".to_owned(),
        symbol_hint: None,
        file_hint: None,
        ..Default::default()
    };
    // O projeto A gasta a janela do cluster…
    assert!(matches!(
        cluster_dedupe_gate(Path::new("/t/projeto-a"), &generic),
        ClusterDecision::Proceed
    ));
    assert!(matches!(
        cluster_dedupe_gate(Path::new("/t/projeto-a"), &generic),
        ClusterDecision::Suppress
    ));
    // …e o projeto B continua recebendo o banner, porque seu leitor não o viu.
    assert!(
        matches!(
            cluster_dedupe_gate(Path::new("/t/projeto-b"), &generic),
            ClusterDecision::Proceed
        ),
        "banner calado num projeto que nunca o recebeu — o dedupe vazou entre projetos"
    );
}

/// A chave carrega a raiz: mesmo cluster em projetos distintos são chaves
/// distintas, e a mesma raiz é estável entre chamadas.
#[test]
fn cluster_dedupe_key_separates_projects() {
    assert_eq!(
        cluster_dedupe_key(Path::new("/t/a"), "system-health-precheck"),
        cluster_dedupe_key(Path::new("/t/a"), "system-health-precheck"),
        "a chave deve ser estável para (raiz, cluster) idênticos"
    );
    assert_ne!(
        cluster_dedupe_key(Path::new("/t/a"), "system-health-precheck"),
        cluster_dedupe_key(Path::new("/t/b"), "system-health-precheck"),
        "projetos distintos devem ocupar chaves distintas"
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
        cluster_dedupe_gate(Path::new("/t/spec"), &specific),
        ClusterDecision::Proceed
    ));
    // Repeating the same specific suggestion still proceeds (never deduped).
    assert!(matches!(
        cluster_dedupe_gate(Path::new("/t/spec"), &specific),
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
        cluster_dedupe_gate(Path::new("/t/generic"), &generic),
        ClusterDecision::Proceed
    ));
    // …and a subsequent emission within the window is deduped.
    assert!(matches!(
        cluster_dedupe_gate(Path::new("/t/generic"), &generic),
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
    let out = detect_code_mode(Path::new("/t/cm-loop"), "Bash", &loop_cmd).expect("loop should fire");
    assert_eq!(out.cluster, "code-mode-loop");
    // A non-scan/non-loop tool never fires.
    assert!(detect_code_mode(Path::new("/t/cm-cargo"), "Bash", &json!({"command": "cargo build"})).is_none());
    assert!(detect_code_mode(Path::new("/t/cm-edit"), "Edit", &json!({"file_path": "x.rs"})).is_none());
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

/// A emenda do Gabriel (25/08/2026): *"o nudge de persuasão deve injetar
/// contexto com snippet que substitua as n+ tool calls"*.
///
/// Enunciada na forma POSITIVA e sobre TODOS os gatilhos Bash: o comando
/// emitido tem de conter o comando do gatilho. A forma negativa que existia —
/// "não contém `<`" — deixou passar um placeholder que vestia outra roupa: o
/// corpo python trazia o glob real e o comentário `# then your per-file op
/// over files`, sem um único `<`. Verificar a ausência de UMA forma conhecida
/// de defeito é o que faz o defeito voltar na próxima forma.
#[test]
fn todo_nudge_de_bash_carrega_o_comando_do_gatilho_inteiro() {
    let gatilhos = [
        "for f in *.md; do wc -l \"$f\"; done",
        "for i in 1 2 3; do echo $i; done",
        "for f in crates/*/src/lib.rs; do grep -c fn \"$f\"; done",
        "while read -r l; do echo \"$l\"; done < lista.txt",
    ];
    for cmd in gatilhos {
        let out = code_mode_output(&CodeModeKind::Loop, "Bash", &json!({ "command": cmd }));
        let must = &out.must[0].command;
        // O comando do gatilho viaja INTEIRO (com o escape de aspas do shell).
        let esperado = cmd.replace('\'', r"'\''");
        assert!(
            must.contains(&esperado),
            "o snippet tem de substituir a chamada, carregando-a inteira.\n\
             gatilho: {cmd}\n emitido: {must}"
        );
    }
}

/// Nenhum comando emitido ADIA trabalho para o leitor.
///
/// Um snippet que diz "agora faça a sua operação" não substitui N chamadas —
/// substitui zero. Cobre o vocabulário de adiamento inteiro, não só `<…>`.
#[test]
fn nenhum_nudge_adia_o_trabalho_para_o_leitor() {
    const ADIAMENTO: &[&str] = &[
        "then your",
        "your per-file",
        "your per-item",
        "sua operação",
        "TODO",
        "FIXME",
        "…",
        "...",
    ];
    let casos: Vec<(CodeModeKind, serde_json::Value)> = vec![
        (
            CodeModeKind::Loop,
            json!({"command": "for f in crates/*/src/*.rs; do wc -l \"$f\"; done"}),
        ),
        (
            CodeModeKind::Loop,
            json!({"command": "for i in $(seq 1 5); do echo $i; done"}),
        ),
        (
            CodeModeKind::Scan,
            json!({"command": "grep -rn \"AuthValidator\" crates/"}),
        ),
        (
            CodeModeKind::Scan,
            json!({"pattern": "AuthValidator", "path": "crates/", "glob": "*.rs"}),
        ),
    ];
    for (kind, input) in casos {
        let tool = if input.get("command").is_some() {
            "Bash"
        } else {
            "Grep"
        };
        let out = code_mode_output(&kind, tool, &input);
        for sug in out.must.iter().chain(out.should.iter()) {
            for termo in ADIAMENTO {
                assert!(
                    !sug.command.contains(termo),
                    "comando emitido adia trabalho ('{termo}'): {}",
                    sug.command
                );
            }
        }
    }
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
        Path::new("/t/cm-sub"),
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
        cluster_dedupe_gate(Path::new("/t/loop"), &loop_nudge),
        ClusterDecision::Proceed
    ));
    assert!(matches!(
        cluster_dedupe_gate(Path::new("/t/loop"), &loop_nudge),
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
    let intent =
        intent_for_new_file("/tmp/x/generate_pdf_report.py", None).expect("stem yields an intent");
    assert_eq!(intent, "generate pdf report");
}

#[test]
fn intent_is_never_a_placeholder() {
    // The invariant: a derivable value travels verbatim; `<...>` never appears.
    for (path, body) in [
        ("/tmp/x/generate_pdf.py", None),
        ("/tmp/x/a_b.rs", Some("//! Something short.\n")),
        (
            "/tmp/x/render_map.py",
            Some("\"\"\"Render the dependency map as SVG output.\"\"\"\n"),
        ),
    ] {
        if let Some(intent) = intent_for_new_file(path, body) {
            assert!(
                !intent.contains('<'),
                "placeholder leaked for {path}: {intent}"
            );
            assert!(!intent.trim().is_empty(), "empty intent for {path}");
        }
    }
}

#[test]
fn uninformative_stems_yield_no_intent_rather_than_noise() {
    // Querying the portfolio for "mod" or "lib" would return noise; better to
    // stay silent than to inject a meaningless nudge.
    for path in [
        "/tmp/x/mod.rs",
        "/tmp/x/lib.rs",
        "/tmp/x/main.rs",
        "/tmp/x/a.py",
    ] {
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

/// The hook that fires on every session must feed its own counters.
///
/// Origin 2026-08-20: `cli_suggester` called ten recorders and ran
/// `advise_next_step` / `detect_antipattern` three times without recording
/// either. `workflow_advice_emitted_count` and
/// `workflow_antipattern_detected_count` read 0 while advice was demonstrably
/// injected ~10× in a 25-minute window — the counters were incremented only
/// from `touring-ceg/gateway/metrics.rs` and the manual `touring gate` verb,
/// neither of which is the path that actually fires.
#[test]
fn workflow_enrichment_feeds_its_own_counters() {
    use std::sync::atomic::Ordering;

    let m = touring_foundation::gate_metrics::global();
    let advice_before = m.workflow_advice_emitted_count.load(Ordering::Relaxed);

    // A Bash classifier output is the shape that reaches the workflow builder.
    let classifier = ClassifierOutput {
        cluster: "file-enumeration".to_owned(),
        ..Default::default()
    };
    let _ = workflow_enrichment_hint(&classifier);

    let advice_after = m.workflow_advice_emitted_count.load(Ordering::Relaxed);
    assert!(
        advice_after > advice_before,
        "workflow_advice_emitted_count must rise when advice is built \
         ({advice_before} → {advice_after})"
    );
}

// ── E4 (2026-08-24) — a hand-written adw-run loop is a campaign ─────────────

#[test]
fn adw_run_loop_nudges_the_campaign_layer_with_the_real_flow() {
    let cmd = "for i in 2 3 4; do touring adw run error-teach --var batch=30; done";
    let out = super::campaign_code_mode_command(cmd).expect("adw-run loop must specialize");
    assert!(
        out.contains("touring adw campaign error-teach"),
        "the REAL flow name travels (injection-density), got: {out}"
    );
    assert!(out.contains("--until"), "the predicate contract is taught");
}

#[test]
fn ordinary_loops_do_not_nudge_campaign() {
    assert!(super::campaign_code_mode_command("for f in *.rs; do wc -l $f; done").is_none());
    // and the flow name never comes from a flag
    let out = super::campaign_code_mode_command("while true; do touring adw run --mock x; done")
        .expect("still a campaign");
    assert!(out.contains("<flow>"), "flag is not a flow name, got: {out}");
}

// ── Guard estrutural: escopo por projeto de TODO cache do suggester ──────────

/// Toda chave de cache deste módulo carrega a raiz do projeto — verificado
/// sobre o FONTE, não sobre uma lista de casos conhecidos.
///
/// Origem (24/08/2026): a suíte `cli_suggester_e2e` era flaky e a investigação
/// achou o MESMO defeito em quatro caches independentes — `input_hash`,
/// `cluster_dedupe_key`, `scan_class_key` e o τ conformal. Todos são `static`,
/// e o daemon que os hospeda é longo-vivo e serve mais de um projeto: sem a
/// raiz na chave, o estado de um repositório decide o que outro vê. Consertar
/// só os dois que doíam deixaria os outros dois de pé e o quinto nasceria igual
/// — por isso o guard é escrito sobre a FAMÍLIA, varrendo o arquivo, e não
/// sobre as instâncias que já conheço.
///
/// Ficam legitimamente de fora os caches chaveados por SESSÃO
/// (`pending_suggestion`, `pending_pillar`): uma sessão CC trabalha num projeto,
/// então a chave de sessão já escopa por construção. O guard cobre as chaves
/// `u64` derivadas de hasher — que são as que precisam carregar a raiz à mão.
#[test]
fn every_suggester_cache_key_is_scoped_by_project_root() {
    let src = include_str!("cli_suggester.rs");

    // As funções que compõem chave de cache: assinatura `fn <nome>(…) -> u64`
    // cujo corpo instancia um hasher.
    let mut checadas = 0;
    for (i, linha) in src.lines().enumerate() {
        let assinatura = linha.trim_start();
        if !assinatura.starts_with("fn ") || !assinatura.contains("-> u64") {
            continue;
        }
        let nome = assinatura
            .trim_start_matches("fn ")
            .split('(')
            .next()
            .unwrap_or("?");
        // Corpo = da assinatura até a próxima linha que fecha no nível zero.
        let corpo: String = src
            .lines()
            .skip(i)
            .take_while(|l| !l.starts_with('}'))
            .collect::<Vec<_>>()
            .join("\n");
        if !corpo.contains("DefaultHasher") {
            continue; // não é função de chave
        }
        checadas += 1;
        assert!(
            assinatura.contains("project_root: &Path"),
            "`{nome}` compõe chave de cache sem receber `project_root`: o cache é \
             um `static` de processo e o daemon serve vários projetos, então a \
             chave sem raiz deixa um projeto decidir o que o outro vê"
        );
        assert!(
            corpo.contains("project_root.hash("),
            "`{nome}` recebe `project_root` mas não o inclui no hash — o parâmetro \
             sozinho não escopa nada"
        );
    }

    // Sem esta âncora o teste passaria vacuamente se o padrão de detecção
    // deixasse de casar (renomear `DefaultHasher`, por exemplo).
    assert!(
        checadas >= 3,
        "esperava encontrar as funções de chave do módulo, achei {checadas} — o \
         detector deixou de casar e o guard virou vácuo"
    );
}

/// Nenhum DB de lições é aberto em modo de escrita neste módulo.
///
/// O suggester só faz `SELECT`. Abrir para escrita traz `SQLITE_OPEN_CREATE`
/// (fabrica banco vazio num caminho federado ausente) e faz o `Drop` da conexão
/// pedir lock EXCLUSIVO de arquivo para o checkpoint do WAL — que foi o deadlock
/// capturado sob gdb em 24/08/2026, com todas as threads em `pthread_mutex_lock`
/// via `sqlite3WalClose` → `unixLock`.
///
/// Escrito sobre o FONTE porque o defeito estava em três sítios ao mesmo tempo:
/// consertar os que doem e deixar o padrão de pé só adia a próxima ocorrência.
#[test]
fn no_lessons_db_is_opened_for_writing() {
    let src = include_str!("cli_suggester.rs");
    let diretas: Vec<(usize, &str)> = src
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("Connection::open(") && !l.trim_start().starts_with("///"))
        .map(|(i, l)| (i + 1, l.trim()))
        .collect();
    assert!(
        diretas.is_empty(),
        "abertura de DB em modo de escrita (traz CREATE e checkpoint no Drop) — \
         use `open_lessons_db_readonly`: {diretas:?}"
    );
    // Âncora: o helper precisa existir e pedir READ_ONLY de fato.
    assert!(
        src.contains("fn open_lessons_db_readonly")
            && src.contains("SQLITE_OPEN_READ_ONLY"),
        "o helper read-only sumiu — o guard acima passaria por vácuo"
    );
}

// ── W1 (plano code-mode-total): G2 deny + G6 escalada ─────────────────────────
//
// Os caches dos gates são globais de processo: cada teste usa um PROJETO único
// (o hash e a época são por projeto) para não interferir nos vizinhos.

mod code_mode_gates_w1 {
    use serial_test::serial;
    use super::super::code_mode_gates;
    use serde_json::json;
    use std::path::Path;

    fn bash(cmd: &str) -> serde_json::Value {
        json!({ "command": cmd })
    }

    #[test]
    #[serial(t3_env)]
    fn g2_reescreve_por_default_e_nega_sob_fallback() {
        // S-8.6 (spike SUPORTADO): o default é REWRITE — allow + updatedInput
        // com o comando REAL prefixado; o deny fica atrás do env de fallback.
        let proj = Path::new("/tmp/w1-g2-deny");
        let cmd = "cargo test 2>&1 | tail -3; echo EXIT=$?";
        let resp = code_mode_gates(proj, "s1", "Bash", &bash(cmd))
            .expect("G2 deve falar");
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            v["hookSpecificOutput"]["updatedInput"]["command"],
            format!("set -o pipefail; {cmd}")
        );
        // fallback deny (env no MESMO teste — sequencial, sem corrida entre testes)
        unsafe { std::env::set_var("TOURING_G2_REWRITE_DISABLED", "1") };
        let resp = code_mode_gates(proj, "s1", "Bash", &bash(cmd))
            .expect("G2 deve negar sob fallback");
        unsafe { std::env::remove_var("TOURING_G2_REWRITE_DISABLED") };
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        let reason = v["hookSpecificOutput"]["permissionDecisionReason"].as_str().unwrap();
        assert!(reason.contains(&format!("set -o pipefail; {cmd}")), "{reason}");
    }

    #[test]
    #[serial(t3_env)]
    fn g2_bypass_token_passa_e_conta() {
        let proj = Path::new("/tmp/w1-g2-bypass");
        let cmd = "TOURING_GATE_OK=1 cargo test | tail -1; echo $?";
        assert!(code_mode_gates(proj, "s1", "Bash", &bash(cmd)).is_none());
    }

    #[test]
    #[serial(t3_env)]
    fn g2_heredoc_e_dado_nunca_nega() {
        // Escrever um TESTE que contém o padrão não é cometer o padrão.
        let proj = Path::new("/tmp/w1-g2-heredoc");
        let cmd = "cat >> t.py <<'EOF'\nassert exit_pipe('a | b; echo $?')\nEOF";
        assert!(code_mode_gates(proj, "s1", "Bash", &bash(cmd)).is_none());
    }

    // ── N3a (26/08): G9 escrita-cega-inline ──────────────────────────────

    #[test]
    #[serial(t3_env)]
    fn g9_sed_i_nega_com_rotas_derivadas() {
        let proj = Path::new("/tmp/n3a-g9-deny");
        let cmd = "sed -i 's/old/new/' src/foo.rs";
        let resp = code_mode_gates(proj, "g9-a", "Bash", &bash(cmd)).expect("G9 deve negar");
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        let reason = v["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        assert!(reason.contains("Edit tool"), "{reason}");
        assert!(reason.contains("src/foo.rs"), "alvo real, não placeholder: {reason}");
        // a rota sandbox carrega o comando REAL com aspas escapadas (lição G8)
        assert!(reason.contains("sed -i '\\''s/old/new/'\\'' src/foo.rs"), "{reason}");
        assert!(!reason.contains("<o arquivo>"), "placeholder só sem alvo derivável: {reason}");
        // a conversão (Edit) fecha o continuation-check sem pânico
        let edit = json!({"file_path": "src/foo.rs", "old_string": "old", "new_string": "new"});
        let _ = code_mode_gates(proj, "g9-a", "Edit", &edit);
    }

    #[test]
    #[serial(t3_env)]
    fn g9_cobre_as_variantes_e_poupa_as_leituras() {
        let proj = Path::new("/tmp/n3a-g9-variantes");
        for cmd in [
            "sed --in-place 's/a/b/' f.rs",
            "sed -i.bak 's/a/b/' f.rs",
            "awk -i inplace '{print}' f.txt",
            "perl -pi -e 's/a/b/' f.txt",
            "cat f | sed -i 's/a/b/'",
            "cd x && sed -i 's/a/b/' f",
        ] {
            assert!(
                code_mode_gates(proj, "g9-b", "Bash", &bash(cmd)).is_some(),
                "deny: {cmd}"
            );
        }
        for cmd in [
            "sed -n '5p' f.rs",          // leitura
            "sed 's/a/b/' f.rs",         // sem -i: stdout, não escrita
            "echo \"rode sed -i aqui\"", // prosa, não invocação (âncora de posição)
            "cat >> s.sh <<'EOF'\nsed -i 's/a/b/' f\nEOF", // heredoc é dado
        ] {
            assert!(
                code_mode_gates(proj, "g9-b", "Bash", &bash(cmd)).is_none(),
                "passa: {cmd}"
            );
        }
    }

    #[test]
    #[serial(t3_env)]
    fn g9_bypass_por_comando_passa() {
        let proj = Path::new("/tmp/n3a-g9-bypass");
        let cmd = "TOURING_GATE_OK=1 sed -i 's/a/b/' f.rs";
        assert!(code_mode_gates(proj, "g9-c", "Bash", &bash(cmd)).is_none());
    }

    // ── N5 (26/08): classe da escolha no eixo da injeção nativa ──────────

    #[test]
    fn n5_classe_da_escolha_no_eixo() {
        use super::super::native_injection_class;
        let bash = |c: &str| json!({"command": c});
        // seguida: Bash onde a tool dedicada existia (wrappers transparentes — S1)
        assert_eq!(
            native_injection_class("Bash", &bash("grep -rn foo src/")),
            Some("followed")
        );
        assert_eq!(
            native_injection_class("Bash", &bash("time grep foo f")),
            Some("followed")
        );
        assert_eq!(
            native_injection_class("Bash", &bash("find . -name '*.rs'")),
            Some("followed")
        );
        assert_eq!(
            native_injection_class("Bash", &bash("cat README.md")),
            Some("followed")
        );
        assert_eq!(
            native_injection_class("Bash", &bash("sed -n '5p' f.rs")),
            Some("followed")
        );
        // escrita não é leitura (a classe `cat` exclui redirect — P2.3)
        assert_eq!(native_injection_class("Bash", &bash("cat > out.txt")), None);
        // resistida: a tool dedicada
        assert_eq!(
            native_injection_class("Grep", &json!({"pattern": "x"})),
            Some("resisted")
        );
        assert_eq!(
            native_injection_class("Read", &json!({"file_path": "f"})),
            Some("resisted")
        );
        assert_eq!(
            native_injection_class("Glob", &json!({"pattern": "*.rs"})),
            Some("resisted")
        );
        // terceira via: o sandbox (mesmo com grep DENTRO — a escolha foi a rota)
        assert_eq!(
            native_injection_class("Bash", &bash("touring run --lang bash --code 'grep x f'")),
            Some("code_route")
        );
        // fora do eixo
        assert_eq!(native_injection_class("Bash", &bash("cargo test")), None);
        assert_eq!(native_injection_class("Edit", &json!({"file_path": "f"})), None);
    }

    // ── S4 (26/08): G10 exec-burst — a rajada desenrolada vira 1 programa ──

    #[test]
    fn g10_exec_class_reconhece_o_executor_e_exclui_o_resto() {
        use super::super::exec_class_of;
        assert_eq!(exec_class_of(".venv/bin/python3 scripts/verifica.py"), Some("python"));
        assert_eq!(exec_class_of("python3 -m pytest tests/test_a.py"), Some("python"));
        assert_eq!(exec_class_of("python3.11 runner.py"), Some("python"));
        assert_eq!(exec_class_of("pytest tests/test_a.py -x"), Some("pytest"));
        assert_eq!(exec_class_of(".venv/bin/pytest tests/ -q"), Some("pytest"));
        assert_eq!(exec_class_of("time python3 runner.py"), Some("python"));
        assert_eq!(exec_class_of("cd /x && pytest tests/"), Some("pytest"));
        // inline entrou na rajada como classe PRÓPRIA (aperto 29/08): o
        // remédio é 1:1 (o corpo verbatim), não a fusão R9 que justificava a
        // exclusão histórica
        assert_eq!(exec_class_of("python3 -c 'print(1)'"), Some("python-inline"));
        assert_eq!(
            exec_class_of("python3 - <<'EOF'\nprint(1)\nEOF"),
            Some("python-inline")
        );
        assert_eq!(exec_class_of("python3 - arg1"), Some("python-inline"));
        // mutação marcada fica de fora por construção (P2.3)
        assert_eq!(exec_class_of("python3 setup.py install"), None);
        assert_eq!(exec_class_of("python3 runner.py > out.txt"), None);
        assert_eq!(exec_class_of("python3 -m pip install x"), None);
        // não-executor
        assert_eq!(exec_class_of("cargo test"), None);
        assert_eq!(exec_class_of("grep foo f"), None);
    }

    // ── S5 (29/08): os furos do turno de 60 do `analise` viram guards ──
    // Provados ao vivo antes do fix: 12 execuções python com `2>&1` e 12
    // atrás de `VAR=...\n` — zero denies do G10 em ambos os casos.

    #[test]
    fn s5_redirect_de_fd_nao_anula_a_classe_exec() {
        use super::super::exec_class_of;
        // O furo B: `contains(">")` sobre o blob inteiro cegava o G10.
        assert_eq!(
            exec_class_of("cd /tmp && python3 medir.py 2>&1 | head -5"),
            Some("python")
        );
        assert_eq!(exec_class_of("python3 medir.py 2>/dev/null"), Some("python"));
        assert_eq!(exec_class_of("pytest tests/ -q 2>&1"), Some("pytest"));
        // Redirect REAL de saída segue fora — escrita não é rajada exec.
        assert_eq!(exec_class_of("python3 runner.py > out.txt"), None);
        assert_eq!(exec_class_of("python3 runner.py >> log.txt"), None);
    }

    #[test]
    fn s5_assignment_prefixo_e_transparente_para_a_classe() {
        use super::super::{effective_tokens, exec_class_of};
        // O furo C: segmento assignment-only devolvia tokens vazios e TODA
        // classificação morria ali.
        assert_eq!(
            exec_class_of("P=docs/x\npython3 medir.py --map $P"),
            Some("python")
        );
        assert_eq!(
            effective_tokens("P=docs/x\nR=docs/y\ngrep -n foo arq.rs").first(),
            Some(&"grep")
        );
        // Assignment sozinho continua sem classe (não há verbo).
        assert_eq!(exec_class_of("P=docs/x"), None);
    }

    #[test]
    fn s5_corpo_de_heredoc_nao_alimenta_o_filtro_de_mutacao() {
        use super::super::{exec_class_of, mutation_scan_view};
        // O corpo é DADO no stdin — um `git ` ou `>` lá dentro não é comando.
        let cmd = "python3 medir.py <<'EOF'\nx = \"git checkout\"\ny = 1 > 0\nEOF";
        assert!(!mutation_scan_view(cmd).contains("git "));
        assert_eq!(exec_class_of(cmd), Some("python"));
        // Herestring não é heredoc — a view não pode engolir o resto.
        assert!(mutation_scan_view("cat <<< 'x' && rm f").contains("rm "));
    }

    #[test]
    fn s5_par_write_run_detecta_escrita_e_execucao_do_mesmo_script() {
        use super::super::{script_run_targets, script_write_target};
        // A escrita registra o alvo (heredoc típico do loop execute-observe).
        assert_eq!(
            script_write_target("cat > /tmp/s/medir.py <<'PYEOF'\nprint(1)\nPYEOF"),
            Some("/tmp/s/medir.py".to_string())
        );
        assert_eq!(
            script_write_target("cd /proj\ncat > scratch/x.sh <<'EOF'\nls\nEOF"),
            Some("scratch/x.sh".to_string())
        );
        assert_eq!(script_write_target("cat > notas.md <<'EOF'\noi\nEOF"), None);
        // A execução resolve o path mesmo atrás de cd/assignment.
        assert_eq!(
            script_run_targets("cd /proj\n.venv/bin/python3 /tmp/s/medir.py"),
            vec!["/tmp/s/medir.py".to_string()]
        );
        assert_eq!(
            script_run_targets("bash scratch/x.sh"),
            vec!["scratch/x.sh".to_string()]
        );
        // O padrão CANÔNICO do analise (24 dos 60): escreve E executa no MESMO
        // tool_use multi-linha — o run da linha pós-heredoc TEM que aparecer.
        assert_eq!(
            script_run_targets(
                "cat > /tmp/s/perfil.py <<'PYEOF'\nimport json\nPYEOF\n\npython3 /tmp/s/perfil.py"
            ),
            vec!["/tmp/s/perfil.py".to_string()]
        );
        // pytest/módulos nunca casam — o limiar baixo não taxa o caso comum.
        assert!(script_run_targets("python3 -m pytest tests/test_a.py").is_empty());
        assert!(script_run_targets("pytest tests/test_a.py").is_empty());
        // Redirect real no SEGMENTO do run: o remédio --file não reproduziria
        // a escrita — aquele segmento não conta.
        assert!(script_run_targets("python3 /tmp/s/medir.py > out.json").is_empty());
    }

    #[test]
    fn s5_par_write_run_nega_no_segundo_par_com_o_proprio_arquivo() {
        use super::super::{
            write_run_key, write_run_pair_gate, written_scripts_ledger,
        };
        let root = std::path::Path::new("/tmp/s5-par-teste-isolado");
        // Simula as escritas: 2 scripts registrados na janela.
        for p in ["/tmp/s5/a.py", "/tmp/s5/b.py"] {
            written_scripts_ledger().insert(write_run_key(root, p), ());
        }
        let d1 = write_run_pair_gate(root, "sess-s5", "python3 /tmp/s5/a.py");
        assert!(d1.is_none(), "1º par passa (criar e testar UM script é legítimo)");
        // O 2º par na forma canônica do analise: write+run no MESMO comando —
        // nega (aperto 29/08: era o 3º).
        let deny = write_run_pair_gate(
            root,
            "sess-s5",
            "cat > /tmp/s5/b.py <<'PYEOF'\nprint(2)\nPYEOF\npython3 /tmp/s5/b.py",
        )
        .expect("2º par nega");
        assert!(deny.contains("write→run"), "deny nomeia o gate: {deny}");
        assert!(
            deny.contains("touring run --file /tmp/s5/b.py"),
            "o remédio é o PRÓPRIO script, sem reescrita: {deny}"
        );
        // Um script que NÃO foi escrito na janela nunca conta como par.
        assert!(
            write_run_pair_gate(root, "sess-s5", "python3 /tmp/s5/alheio.py").is_none()
        );
    }

    #[test]
    fn s5_python_inline_tem_remedio_um_para_um() {
        use super::super::{python_inline_body, python_inline_remedy};
        // heredoc: o corpo verbatim, sem fusão R9
        let cmd = "python3 - <<'PYEOF'\nimport json\nprint(1)\nPYEOF";
        assert_eq!(
            python_inline_body(cmd).as_deref(),
            Some("import json\nprint(1)")
        );
        let r = python_inline_remedy(cmd);
        assert!(r.starts_with("touring run --lang python --code '"), "{r}");
        assert!(r.contains("import json"), "{r}");
        // -c: o literal entre as aspas externas
        assert_eq!(
            python_inline_body("python3 -c 'print(42)'").as_deref(),
            Some("print(42)")
        );
        // inextraível → placeholder honesto, nunca comando quebrado
        assert!(python_inline_remedy("python3 -").contains("verbatim"));
    }

    #[test]
    fn g10_programa_r9_agrega_as_chamadas_reais_em_python_valido() {
        use super::super::r9_exec_program;
        let cmds = vec![
            "pytest tests/test_a.py -x".to_string(),
            "pytest tests/test_b.py -x".to_string(),
            "python3 -c 'nao-entra-mas-aspas-sobrevivem'".to_string(),
        ];
        let p = r9_exec_program(&cmds);
        assert!(p.starts_with("touring run --lang python --code '"));
        assert!(p.ends_with('\''));
        for c in &cmds {
            // o comando viaja como literal JSON (aspas duplas) — Python válido
            // — COM o escape do embrulho shell aplicado por cima (o bash
            // desfaz na entrega; o Python recebe a aspa original)
            let literal = serde_json::to_string(c).unwrap().replace('\'', "'\\''");
            assert!(p.contains(&literal), "faltou {literal} no programa");
        }
        // aspas simples do corpo escapadas para o embrulho do shell
        assert!(p.contains("'\\''ok'\\''"), "escape do corpo: {p}");
        assert!(p.contains("RESUMO"), "digest agregado: {p}");
    }

    #[test]
    #[serial(t3_env)]
    fn g10_quinta_chamada_nega_com_o_programa_e_run_zera() {
        let proj = Path::new("/tmp/s4-g10-burst");
        let sess = "g10-a";
        // 4 seriadas passam intactas (aperto 29/08: DENY_AT 10→5)
        for i in 1..=4 {
            let cmd = format!("pytest tests/test_{i}.py -x");
            assert!(
                code_mode_gates(proj, sess, "Bash", &bash(&cmd)).is_none(),
                "#{i} passa: {cmd}"
            );
        }
        // a 5ª nega com o programa carregando TODAS as 5 reais
        let cmd5 = "pytest tests/test_5.py -x";
        let resp = code_mode_gates(proj, sess, "Bash", &bash(cmd5)).expect("5ª nega");
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        let reason = v["hookSpecificOutput"]["permissionDecisionReason"].as_str().unwrap();
        assert!(reason.contains("touring run --lang python --code '"), "{reason}");
        for i in 1..=5 {
            let literal = serde_json::to_string(&format!("pytest tests/test_{i}.py -x")).unwrap();
            assert!(reason.contains(&literal), "programa sem a chamada {i}: {reason}");
        }
        // ledger zerado após o deny: a próxima rajada recomeça do zero
        assert!(
            code_mode_gates(proj, sess, "Bash", &bash("pytest tests/test_6.py -x")).is_none(),
            "um deny por lote, nunca fadiga"
        );
    }

    #[test]
    #[serial(t3_env)]
    fn g10_touring_run_no_meio_zera_a_rajada() {
        let proj = Path::new("/tmp/s4-g10-reset");
        let sess = "g10-b";
        for i in 1..=4 {
            let cmd = format!("pytest tests/test_{i}.py -x");
            let _ = code_mode_gates(proj, sess, "Bash", &bash(&cmd));
        }
        // a rota tomada entre elas zera a condição "0 run"
        let _ = code_mode_gates(proj, sess, "Bash", &bash("touring run --lang bash --code 'ls'"));
        // mais 4: sem o reset, a acumulada (4+4=8) teria negado na 5ª absoluta
        for i in 11..=14 {
            let cmd = format!("pytest tests/test_{i}.py -x");
            assert!(
                code_mode_gates(proj, sess, "Bash", &bash(&cmd)).is_none(),
                "pós-reset a contagem é nova: {cmd}"
            );
        }
    }

    #[test]
    #[serial(t3_env)]
    fn g10_bypass_por_comando_passa() {
        let proj = Path::new("/tmp/s4-g10-bypass");
        let sess = "g10-c";
        for i in 1..=4 {
            let cmd = format!("pytest tests/test_{i}.py -x");
            let _ = code_mode_gates(proj, sess, "Bash", &bash(&cmd));
        }
        let cmd = "TOURING_GATE_OK=1 pytest tests/test_5.py -x";
        assert!(code_mode_gates(proj, sess, "Bash", &bash(cmd)).is_none());
    }

    // ── S6 (26/08): o remédio consulta o portfólio com a rajada real ─────

    #[test]
    fn s6_intent_carrega_classe_e_alvos_do_trabalho() {
        use super::super::exec_burst_intent;
        let cmds = vec![
            "pytest tests/test_a.py -x".to_string(),
            "pytest tests/test_b.py -x".to_string(),
            "pytest tests/test_a.py -x".to_string(), // duplicado: dedup
        ];
        let intent = exec_burst_intent("pytest", &cmds);
        assert!(intent.starts_with("pytest"), "{intent}");
        // componentes de trabalho (dir + stem), não o caminho inteiro — são
        // eles que casam o propósito do artefato (required_matches do BM25)
        assert!(intent.contains("tests"), "{intent}");
        assert!(intent.contains("test"), "{intent}");
        assert!(!intent.contains("test_a.py"), "caminho inteiro dilui: {intent}");
        assert_eq!(
            intent.split(' ').filter(|t| *t == "test").count(),
            1,
            "dedup exato (substring não conta — `pytest` e `tests` contêm `test`): {intent}"
        );
        assert!(!intent.contains("-x"), "flags não são termo de trabalho: {intent}");
        // classe solitária quando a rajada não tem alvos
        assert_eq!(exec_burst_intent("python", &[]), "python");
    }

    #[test]
    fn s6_sem_portfolio_ou_sem_match_o_remedio_e_none_e_o_r9_basta() {
        use super::super::portfolio_remedy_for_burst;
        // fail-open: sem índice legível (ou vazio), o deny sai só com o R9
        let cmds = vec!["pytest tests/xyzzy_inexistente_q123.py".to_string()];
        let _ = portfolio_remedy_for_burst("pytest", &cmds); // nunca panica
    }

    /// Monta um índice scratch com UM artefato cujo propósito casa a rajada.
    /// `TOURING_PORTFOLIO_DIR` aponta o store para o scratch; o cache do
    /// suggester (chave mtime) recarrega porque o arquivo é novo.
    fn monta_portfolio_scratch() -> std::path::PathBuf {
        use touring_foundation::portfolio::store::{PortfolioIndex, save_to};
        use touring_foundation::portfolio::{CapabilityEntry, CapabilityKind, Evidence};
        let dir = std::env::temp_dir().join(format!("s6-portfolio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = CapabilityEntry {
            id: CapabilityEntry::make_id(CapabilityKind::Script, "~/x/rodar_suite.py"),
            display_path: "~/x/rodar_suite.py".into(),
            kind: CapabilityKind::Script,
            name: "rodar_suite".into(),
            purpose: "executar suite de testes pytest agregada com digest resumido".into(),
            language: "python".into(),
            entry_point: Some("python3 ~/x/rodar_suite.py --fast".into()),
            provenance: "teste s6".into(),
            keywords: vec!["pytest".into(), "suite".into(), "tests".into()],
            evidence: Evidence::default(),
            purpose_inherited: false,
        };
        let mut index = PortfolioIndex::empty();
        index.entries.push(entry);
        save_to(&dir, &index).unwrap();
        dir
    }

    #[test]
    #[serial(t3_env)]
    fn s6_prior_art_instanciado_quando_o_portfolio_tem_programa_proximo() {
        use super::super::portfolio_remedy_for_burst;
        let dir = monta_portfolio_scratch();
        unsafe { std::env::set_var("TOURING_PORTFOLIO_DIR", &dir) };
        let cmds = (1..=3).map(|i| format!("pytest tests/test_{i}.py -x")).collect::<Vec<_>>();
        let remedy = portfolio_remedy_for_burst("pytest", &cmds);
        unsafe { std::env::remove_var("TOURING_PORTFOLIO_DIR") };
        let remedy = remedy.expect("prior art encontrado para a rajada de pytest");
        assert!(remedy.contains("python3 ~/x/rodar_suite.py --fast"), "{remedy}");
        assert!(remedy.contains("~/x/rodar_suite.py"), "{remedy}");
    }

    #[test]
    #[serial(t3_env)]
    fn s6_deny_g10_carrega_o_prior_art_instanciado() {
        let dir = monta_portfolio_scratch();
        unsafe { std::env::set_var("TOURING_PORTFOLIO_DIR", &dir) };
        let proj = Path::new("/tmp/s6-g10-prior");
        let sess = "g10-prior";
        for i in 1..=4 {
            let cmd = format!("pytest tests/test_{i}.py -x");
            let _ = code_mode_gates(proj, sess, "Bash", &bash(&cmd));
        }
        let resp = code_mode_gates(proj, sess, "Bash", &bash("pytest tests/test_5.py -x"))
            .expect("5ª nega");
        unsafe { std::env::remove_var("TOURING_PORTFOLIO_DIR") };
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        let reason = v["hookSpecificOutput"]["permissionDecisionReason"].as_str().unwrap();
        assert!(reason.contains("touring run --lang python --code '"), "R9 presente: {reason}");
        assert!(reason.contains("prior art"), "prior art presente: {reason}");
        assert!(reason.contains("python3 ~/x/rodar_suite.py --fast"), "instanciado: {reason}");
    }

    #[test]
    #[serial(t3_env)]
    fn g6_escala_advisory_e_depois_deny() {
        let proj = Path::new("/tmp/w1-g6-escalada");
        let cmd = bash("rg -n 'padrao' src/lib.rs");
        // 1ª vista: silêncio (registra).
        assert!(code_mode_gates(proj, "s1", "Bash", &cmd).is_none());
        // 1ª repetição: advisory (additionalContext, sem deny).
        let advisory = code_mode_gates(proj, "s1", "Bash", &cmd).expect("advisory");
        let v: serde_json::Value = serde_json::from_str(&advisory).unwrap();
        assert!(v["hookSpecificOutput"]["permissionDecision"].is_null());
        assert!(
            v["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .contains("G6")
        );
        // 2ª repetição: deny.
        let deny = code_mode_gates(proj, "s1", "Bash", &cmd).expect("deny");
        let v: serde_json::Value = serde_json::from_str(&deny).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
    }

    #[test]
    #[serial(t3_env)]
    fn g6_allowlist_de_estado_vivo_nunca_dispara() {
        let proj = Path::new("/tmp/w1-g6-allow");
        let cmd = bash("touring doctor -j");
        for _ in 0..4 {
            assert!(code_mode_gates(proj, "s1", "Bash", &cmd).is_none());
        }
    }

    #[test]
    #[serial(t3_env)]
    fn g6_projeto_diferente_nao_herda_contador() {
        // regressão do escopo por projeto (a flakiness de 24/08).
        let a = Path::new("/tmp/w1-g6-proj-a");
        let b = Path::new("/tmp/w1-g6-proj-b");
        let cmd = bash("rg -n 'x' src/main.rs");
        assert!(code_mode_gates(a, "s1", "Bash", &cmd).is_none());
        assert!(code_mode_gates(a, "s1", "Bash", &cmd).is_some()); // advisory em A
        // B nunca viu o comando: silêncio.
        assert!(code_mode_gates(b, "s1", "Bash", &cmd).is_none());
    }

    #[test]
    #[serial(t3_env)]
    fn g6_mutacao_no_meio_reseta_a_repeticao() {
        // ler-depois-de-editar NÃO é retry cego: a época avança com o Edit.
        let proj = Path::new("/tmp/w1-g6-epoch");
        let cmd = bash("sed -n '10,20p' src/lib.rs");
        assert!(code_mode_gates(proj, "s1", "Bash", &cmd).is_none());
        // Edit no projeto → época avança (Read antes, para o G3 — W3 — ficar quieto).
        assert!(code_mode_gates(proj, "s1", "Read",
                &json!({"file_path": "src/lib.rs"})).is_none());
        assert!(
            code_mode_gates(proj, "s1", "Edit", &json!({"file_path": "src/lib.rs"}))
                .is_none()
        );
        // mesma leitura: época mudou → fresh, sem advisory.
        assert!(code_mode_gates(proj, "s1", "Bash", &cmd).is_none());
    }
}

// ── W2 (plano code-mode-total): G1 teeth — rajada nega na 3ª (aperto 29/08) ──

mod burst_gate_w2 {
    use serial_test::serial;
    use super::super::{code_mode_gates, g1_should_deny};
    use serde_json::json;
    use std::path::Path;

    fn bash(cmd: &str) -> serde_json::Value {
        json!({ "command": cmd })
    }

    /// Rajada SERIADA: entre duas chamadas da sequência há um PostToolUse —
    /// é o que a torna seriada e não batch paralelo (§T3-B). Fechar o turno
    /// após cada chamada é a simulação honesta desse intercalado.
    fn inspecoes_com_base(base: usize, n: usize, proj: &Path, sess: &str) -> Vec<Option<String>> {
        (base..base + n)
            .map(|i| {
                let r = code_mode_gates(proj, sess, "Bash", &bash(&format!("rg -n 'p{i}' src/f{i}.rs")));
                r
            })
            .collect()
    }

    fn inspecoes(n: usize, proj: &Path, sess: &str) -> Vec<Option<String>> {
        inspecoes_com_base(0, n, proj, sess)
    }

    #[test]
    fn g1_segunda_passa_terceira_nega_com_a_rajada_no_remedio() {
        let proj = Path::new("/tmp/w2-g1-escala");
        let r = inspecoes(3, proj, "s1");
        assert!(r[0].is_none() && r[1].is_none(),
                "1ª-2ª ficam com o advisory legado");
        let deny = r[2].as_ref().expect("3ª mesma classe nega");
        let v: serde_json::Value = serde_json::from_str(deny).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        let reason = v["hookSpecificOutput"]["permissionDecisionReason"].as_str().unwrap();
        // o remédio carrega a RAJADA REAL como corpo do programa
        assert!(reason.contains("touring run --lang bash"), "{reason}");
        // as aspas do corpo vão escapadas para dentro do '...' externo —
        // asserimos o conteúdo, não a forma escapada
        assert!(reason.contains("p0") && reason.contains("src/f0.rs"), "{reason}");
        assert!(reason.contains("p2") && reason.contains("src/f2.rs"), "{reason}");
    }

    #[test]
    #[serial(t3_env)]
    fn g1_terceira_de_classe_diferente_passa() {
        let proj = Path::new("/tmp/w2-g1-classes");
        let _ = inspecoes(2, proj, "s1"); // 2 da classe grep (a 1 da borda)
        // 3ª chamada é de OUTRA classe (cat) → não nega
        assert!(code_mode_gates(proj, "s1", "Bash", &bash("cat src/lib.rs")).is_none());
    }

    #[test]
    #[serial(t3_env)]
    fn g1_projeto_diferente_nao_herda_rajada() {
        let a = Path::new("/tmp/w2-g1-proj-a");
        let b = Path::new("/tmp/w2-g1-proj-b");
        let _ = inspecoes(2, a, "s1");
        // 1ª do projeto B — mesmo que A esteja a 1 da borda
        assert!(code_mode_gates(b, "s1", "Bash", &bash("rg -n 'x' src/y.rs")).is_none());
    }

    #[test]
    #[serial(t3_env)]
    fn g1_bypass_token_reseta_a_janela() {
        let proj = Path::new("/tmp/w2-g1-bypass");
        let _ = inspecoes(2, proj, "s1");
        // bypass consciente na borda → reseta
        assert!(code_mode_gates(proj, "s1", "Bash",
                &bash("TOURING_GATE_OK=1 rg -n 'w' src/z.rs")).is_none());
        // recomeça do zero: mais 2 passam (padrões NOVOS — repetir os mesmos
        // acionaria o G6, que é outro gate fazendo o trabalho dele)
        let r = inspecoes_com_base(10, 2, proj, "s1");
        assert!(r.iter().all(Option::is_none), "janela resetada recomeça");
    }

    #[test]
    #[serial(t3_env)]
    fn g1_continuation_check_fecha_o_ab() {
        use crate::shared::gate_metrics as gm;
        let proj = Path::new("/tmp/w2-g1-continuation");
        let base_same = gm::global().g1_post_deny_same_class_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let r = inspecoes(3, proj, "sess-cont");
        assert!(r[2].is_some(), "3ª negou");
        // a PRÓXIMA chamada da sessão é a MESMA classe → same_class++
        // (o deny do G1 já fechou o turno — ela chega ao burst_gate por desenho)
        let _ = code_mode_gates(proj, "sess-cont", "Bash", &bash("rg -n 'de novo' src/a.rs"));
        let depois = gm::global().g1_post_deny_same_class_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(depois > base_same, "continuation same_class contado");
    }

    #[test]
    fn g1_autodemote_e_puro_e_exige_volume() {
        assert!(g1_should_deny(None), "sem dado → deny (o simulado 90% sustenta)");
        assert!(g1_should_deny(Some((0.9, 200))), "precisão alta → deny");
        assert!(g1_should_deny(Some((0.5, 50))), "pouco volume nunca demove");
        assert!(!g1_should_deny(Some((0.5, 150))), "precisão < 0.70 com 100+ → advisory");
    }
}

// ── W3 (plano code-mode-total): G3/G7 gates de modo + G4/G5 telemetria + E3 ──

mod mode_gates_w3 {
    use serial_test::serial;
    use super::super::code_mode_gates;
    use serde_json::json;
    use std::path::Path;

    fn edit(fp: &str) -> serde_json::Value {
        json!({ "file_path": fp, "new_string": "let x = 1;" })
    }

    fn read(fp: &str) -> serde_json::Value {
        json!({ "file_path": fp })
    }

    #[test]
    #[serial(t3_env)]
    fn g3_um_advisory_depois_deny_e_read_reseta() {
        let proj = Path::new("/tmp/w3-g3");
        let s = "sess-g3";
        // 1º Edit sem Read: advisory (aperto 29/08: o 2º seguido nega)
        let r = code_mode_gates(proj, s, "Edit", &edit("src/a.rs")).expect("advisory");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert!(v["hookSpecificOutput"]["permissionDecision"].is_null(), "1º é advisory");
        // 2º: deny
        let r = code_mode_gates(proj, s, "Edit", &edit("src/a.rs")).expect("deny");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        // Read do arquivo reseta: o Edit seguinte passa em silêncio
        assert!(code_mode_gates(proj, s, "Read", &read("src/b.rs")).is_none());
        assert!(code_mode_gates(proj, s, "Edit", &edit("src/b.rs")).is_none());
    }

    #[test]
    #[serial(t3_env)]
    fn g3_sessao_nova_zera() {
        let proj = Path::new("/tmp/w3-g3-sessoes");
        let _ = code_mode_gates(proj, "sess-a", "Edit", &edit("src/x.rs"));
        let _ = code_mode_gates(proj, "sess-a", "Edit", &edit("src/x.rs"));
        // sessão B começa do 1º advisory, nunca herda o streak de A
        let r = code_mode_gates(proj, "sess-b", "Edit", &edit("src/x.rs")).expect("advisory");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert!(v["hookSpecificOutput"]["permissionDecision"].is_null());
        assert!(v["hookSpecificOutput"]["additionalContext"].as_str().unwrap().contains("1/1"));
    }

    #[test]
    #[serial(t3_env)]
    fn g7_segunda_advisory_terceira_deny_arquivos_distintos_nada() {
        let proj = Path::new("/tmp/w3-g7");
        let s = "sess-g7";
        let fp = "crates/x/src/lib.rs";
        // 1ª leitura: silêncio (aperto 29/08, ordem de Gabriel: era 3ª/5ª)
        assert!(code_mode_gates(proj, s, "Read", &read(fp)).is_none());
        // 2ª: advisory com R1 instanciado citando o ARQUIVO real
        let r = code_mode_gates(proj, s, "Read", &read(fp)).expect("advisory");
        assert!(r.contains("G7") && r.contains(fp) && r.contains("r1_varredura_agregado"));
        // 3ª: deny
        let r = code_mode_gates(proj, s, "Read", &read(fp)).expect("deny");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        // arquivo DIFERENTE: nada
        assert!(code_mode_gates(proj, s, "Read", &read("crates/y/src/lib.rs")).is_none());
    }

    /// M3 (29/08/2026) — o 10º arquivo DISTINTO lido na janela dispara o
    /// advisory de delegação UMA vez; o 11º fica em silêncio.
    #[test]
    #[serial(t3_env)]
    fn m3_decimo_arquivo_distinto_aconselha_delegacao_uma_vez() {
        let proj = Path::new("/tmp/m3-deleg");
        let s = "sess-m3";
        for i in 0..9 {
            assert!(
                code_mode_gates(proj, s, "Read", &read(&format!("src/m3_{i}.rs"))).is_none(),
                "até o 9º arquivo distinto: silêncio"
            );
        }
        let r = code_mode_gates(proj, s, "Read", &read("src/m3_9.rs")).expect("10º aconselha");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert!(
            v["hookSpecificOutput"]["permissionDecision"].is_null(),
            "advisory, nunca deny"
        );
        let ctx = v["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
        assert!(
            ctx.contains("M3 delegação") && ctx.contains("subagentes read-only"),
            "{ctx}"
        );
        // 11º distinto: silêncio (1 advisory por sessão/janela)
        assert!(code_mode_gates(proj, s, "Read", &read("src/m3_10.rs")).is_none());
    }

    /// M3 — releitura não conta como arquivo distinto (ela fala G7, nunca M3).
    #[test]
    #[serial(t3_env)]
    fn m3_releitura_nao_conta_distinto() {
        let proj = Path::new("/tmp/m3-deleg-reread");
        let s = "sess-m3-r";
        for i in 0..8 {
            assert!(code_mode_gates(proj, s, "Read", &read(&format!("src/r{i}.rs"))).is_none());
        }
        // releitura do 1º: arquivo_novo=false → cai no G7 (2ª leitura), nunca M3
        let r = code_mode_gates(proj, s, "Read", &read("src/r0.rs"));
        if let Some(r) = &r {
            assert!(r.contains("G7") && !r.contains("M3"), "releitura fala G7: {r}");
        }
        // o próximo DISTINTO é o 9º, não o 10º — silêncio
        assert!(code_mode_gates(proj, s, "Read", &read("src/r8.rs")).is_none());
        // e o 10º distinto aconselha
        let r = code_mode_gates(proj, s, "Read", &read("src/r9.rs")).expect("10º");
        assert!(r.contains("M3 delegação"));
    }

    #[test]
    fn g8_laco_de_inspecao_vira_uma_varredura() {
        use super::super::loop_rewrite_candidate;
        // O caso medido no transcript de 25/08: laço de inspeção pura.
        let cmd = r#"for f in a.rs b.rs; do grep -n "fn main" $f; done"#;
        let got = loop_rewrite_candidate(cmd).expect("laço de inspeção é candidato");
        assert!(got.starts_with("touring run --lang bash --code '"));
        assert!(got.contains("for f in a.rs b.rs"), "carrega o comando REAL, não placeholder");
    }

    #[test]
    fn g8_nunca_reescreve_laco_com_efeito() {
        use super::super::loop_rewrite_candidate;
        // Estes quebrariam sob as capabilities do sandbox — ficam de fora por
        // construção, não por sorte. O gate que converte o que não devia é
        // pior que gate nenhum.
        for cmd in [
            "for p in $(pgrep x); do kill -9 $p; done",
            "for c in a b; do cargo test -p $c; done",
            "for f in *.tmp; do rm $f; done",
            "for d in a b; do git -C $d status; done",
            "for i in 1 2; do echo $i >> saida.txt; done",
            "for f in a b; do python3 -c 'print(1)'; done",
        ] {
            assert!(
                loop_rewrite_candidate(cmd).is_none(),
                "laço com efeito NÃO pode ser reescrito: {cmd}"
            );
        }
    }

    #[test]
    fn g8_ignora_heredoc_aspas_impares_e_o_que_ja_e_code_mode() {
        use super::super::loop_rewrite_candidate;
        assert!(
            loop_rewrite_candidate("for f in a; do cat <<EOF\n$f\nEOF\ndone").is_none(),
            "heredoc é dado, e o quoting não sobrevive"
        );
        assert!(
            loop_rewrite_candidate(r#"for f in a; do grep 'x $f; done"#).is_none(),
            "aspas simples ímpares quebrariam o --code"
        );
        assert!(
            loop_rewrite_candidate("touring run --lang bash --code 'for f in a; do ls $f; done'")
                .is_none(),
            "já é code mode"
        );
        assert!(
            loop_rewrite_candidate("grep -rn foo crates/").is_none(),
            "sem laço, nada a converter"
        );
    }

    #[test]
    fn g8_escapa_aspas_simples_preservando_o_comando() {
        use super::super::loop_rewrite_candidate;
        let cmd = r#"for f in a b; do grep -n 'fn main' $f; done"#;
        let got = loop_rewrite_candidate(cmd).expect("candidato");
        // O corpo entra escapado, de modo que o shell externo reentregue o
        // comando original ao sandbox — o rewrite não pode alterar semântica.
        assert!(got.contains(r"'\''fn main'\''"), "aspas internas escapadas: {got}");
    }

    #[test]
    #[serial(t3_env)]
    fn g3_write_de_criacao_nao_conta_e_vale_como_read() {
        let proj = Path::new("/tmp/w3-g3-write");
        let s = "sess-g3w";
        // 3 Writes seguidos de arquivos novos: o G3 fica em silêncio (criação
        // não tem o que ler — o FP vivo de 24/08 negou um Write de strategy doc)
        for f in ["docs/a.md", "docs/b.md", "docs/c.md"] {
            let w = json!({ "file_path": f, "content": "conteudo novo" });
            assert!(code_mode_gates(proj, s, "Write", &w).is_none(), "Write de {f} não dispara G3");
        }
        // Edit do arquivo recém-escrito passa sem Read: o Write valeu como Read
        assert!(code_mode_gates(proj, s, "Edit", &edit("docs/a.md")).is_none());
        // Edit de arquivo jamais lido/escrito continua contando (sem regressão)
        let r = code_mode_gates(proj, s, "Edit", &edit("src/nunca_visto.rs")).expect("advisory");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert!(v["hookSpecificOutput"]["permissionDecision"].is_null());
        assert!(v["hookSpecificOutput"]["additionalContext"].as_str().unwrap().contains("1/1"));
    }

    #[test]
    #[serial(t3_env)]
    fn g7_touring_run_citando_o_arquivo_reseta() {
        let proj = Path::new("/tmp/w3-g7-reset");
        let s = "sess-g7r";
        let fp = "crates/z/src/mod.rs";
        for _ in 0..2 {
            let _ = code_mode_gates(proj, s, "Read", &read(fp));
        }
        // touring run citando o caminho zera a contagem do alvo
        let cmd = json!({ "command": format!("touring run --lang python --args '[\"{fp}\"]' --file r1.py") });
        let _ = code_mode_gates(proj, s, "Bash", &cmd);
        // recomeça: a 3ª leitura absoluta é a 1ª da nova janela → silêncio
        assert!(code_mode_gates(proj, s, "Read", &read(fp)).is_none());
    }

    #[test]
    #[serial(t3_env)]
    fn g5_advisory_unico_no_fim_da_rajada_sem_validacao() {
        let proj = Path::new("/tmp/w3-g5");
        let s = "sess-g5";
        // 3 edits de arquivos previamente lidos (para o G3 ficar quieto)
        for f in ["a.rs", "b.rs", "c.rs"] {
            assert!(code_mode_gates(proj, s, "Read", &read(f)).is_none());
            assert!(code_mode_gates(proj, s, "Edit", &edit(f)).is_none());
        }
        // primeira ação Bash NÃO-validação → 1 advisory G5
        let r = code_mode_gates(proj, s, "Bash", &json!({"command": "echo done"}))
            .expect("advisory G5");
        assert!(r.contains("G5") && r.contains("3 edits"));
        // segunda ação: streak já zerou → silêncio
        assert!(code_mode_gates(proj, s, "Bash", &json!({"command": "echo again"})).is_none());
    }

    #[test]
    #[serial(t3_env)]
    fn g5_validacao_encerra_sem_advisory() {
        let proj = Path::new("/tmp/w3-g5-ok");
        let s = "sess-g5ok";
        for f in ["d.rs", "e.rs", "f.rs"] {
            assert!(code_mode_gates(proj, s, "Read", &read(f)).is_none());
            assert!(code_mode_gates(proj, s, "Edit", &edit(f)).is_none());
        }
        assert!(code_mode_gates(proj, s, "Bash",
                &json!({"command": "cargo check -p x"})).is_none());
    }

    #[test]
    #[serial(t3_env)]
    fn e3_contrafactual_em_comentario_gera_advisory_e_run_id_silencia() {
        let proj = Path::new("/tmp/w3-e3");
        let s = "sess-e3";
        // arquivo lido antes (G3 quieto)
        assert!(code_mode_gates(proj, s, "Read", &read("m.rs")).is_none());
        let com_modal = json!({
            "file_path": "m.rs",
            "new_string": "// sandboxar este nó quebraria a escrita do marcador\nlet x = 1;"
        });
        let r = code_mode_gates(proj, s, "Edit", &com_modal).expect("advisory E3");
        assert!(r.contains("contrafactual"));
        // re-Read para o G3 ficar quieto — a 2ª leitura do MESMO arquivo agora
        // carrega o advisory do G7 (aperto 29/08), irrelevante para o E3
        let _ = code_mode_gates(proj, s, "Read", &read("m.rs"));
        let com_endereco = json!({
            "file_path": "m.rs",
            "new_string": "// quebraria sem pipefail — provado em run-1787618052969\nlet x = 1;"
        });
        assert!(code_mode_gates(proj, s, "Edit", &com_endereco).is_none());
    }
}

/// O remédio que a injeção entrega tem de RODAR.
///
/// Emenda do Gabriel (25/08): *"o nudge de persuasão deve injetar contexto com
/// snippet que substitua as n+ tool calls"*. Um snippet truncado não substitui
/// nada — e era o que saía, observado ao vivo nesta sessão: uma rajada de
/// greps longos produzia um corpo cortado em `crates/tou'`, com as aspas
/// equilibradas e o comando pela metade.
mod apresentacao_por_escopo {
    use super::super::{
        CODE_MODE_COLLAPSED_CLASSES, CodeModePresentation, INSPECT_BURST_DENY_AT,
        INSPECT_BURST_WINDOW_SECS, code_mode_gates, code_mode_presentation, is_scan_command,
        project_presentation, scan_class_of,
    };

    /// S3 (27/08/2026) — a calibração deixou de ser por NOME de classe e passou
    /// a ser por RAJADA, e a medição é a razão.
    ///
    /// A versão anterior deste teste afirmava o oposto: que `ls`/`wc`/`sed-n`
    /// NÃO podiam colapsar porque "são chamada única". Medindo 115 transcripts
    /// (`scripts/s3_burst_distribution.py`, janela 300s) isso se mostrou falso
    /// justamente para as duas de maior volume — `sed-n` tem 408 chamadas com
    /// 81,4% do volume em rajadas ≥2, `ls` tem 338 com 71,3% — enquanto `find`,
    /// que a lista negava, é 56,2% isolada e não produziu UMA rajada ≥3.
    ///
    /// O que o teste guarda agora é o invariante que sobrevive à recalibração:
    /// toda classe que o gate acompanha é uma que o classificador emite, e o
    /// que discrimina não é a lista e sim o limiar de rajada.
    #[test]
    fn a_calibracao_bate_com_a_medicao() {
        for classe in ["grep", "cat", "find", "ls", "wc", "sed-n"] {
            assert!(
                CODE_MODE_COLLAPSED_CLASSES.contains(&classe),
                "`{classe}` é inspeção reconhecida — o ledger da rajada tem de vê-la, \
                 senão o volume fan-out dela fica invisível ao gate"
            );
        }
        assert_eq!(
            INSPECT_BURST_DENY_AT, 2,
            "a 1ª tem de passar: cobrar da isolada é taxar o caso comum, a recusa \
             que o próprio DeepSeek documentou"
        );
    }

    /// S3 — o fluxo do predicado de rajada sobre o caminho REAL (`code_mode_gates`),
    /// não sobre o classificador isolado.
    ///
    /// Isolamento é ESTRUTURAL, não por `#[serial]`: o ledger é global mas sua
    /// chave inclui o `project_root`, então cada teste com raiz própria tem
    /// contadores próprios. Serializar seria tratar o sintoma da colisão em vez
    /// da causa — e este workspace já pagou 49 marcadores seriais por isso.
    #[test]
    #[serial_test::serial(t3_env)]
    fn rajada_de_inspecao_nega_da_segunda_em_diante() {
        let tmp = tempfile::tempdir().expect("tempdir");
        escopo_code(tmp.path());
        let s = "s3-burst-1";

        assert!(
            code_mode_gates(tmp.path(), s, "Bash", &bash("grep -rn alfa src/")).is_none(),
            "a 1ª inspeção da classe executa INTACTA — é o caso comum que o S3 destaxa"
        );
        let d2 = code_mode_gates(tmp.path(), s, "Bash", &bash("grep -rn beta src/"))
            .expect("a 2ª da mesma classe na janela é negada");
        assert!(d2.contains("[CODE MODE · rajada]"), "QUEM negou? {d2}");
        assert!(d2.contains("grep -rn alfa src/"), "a rota funde a 1ª: {d2}");
        assert!(d2.contains("grep -rn beta src/"), "a rota funde a 2ª: {d2}");
        assert!(
            d2.contains(&INSPECT_BURST_WINDOW_SECS.to_string()),
            "o deny declara a janela que o executor aplica: {d2}"
        );
    }

    /// O deny zera o lote (mesma regra do G10): a chamada seguinte volta a ser
    /// "a 1ª". Sem isto o modelo levaria um deny por chamada até a janela
    /// expirar — fadiga de gate, que é como um gate deixa de ser lido.
    #[test]
    #[serial_test::serial(gate_metrics, t3_env)]
    fn deny_zera_o_lote_e_a_seguinte_volta_a_passar() {
        let tmp = tempfile::tempdir().expect("tempdir");
        escopo_code(tmp.path());
        let s = "s3-burst-2";
        assert!(code_mode_gates(tmp.path(), s, "Bash", &bash("cat a.md")).is_none());
        let d2 = code_mode_gates(tmp.path(), s, "Bash", &bash("cat b.md")).expect("2ª nega");
        assert!(d2.contains("[CODE MODE · rajada]"), "o deny é o do S3: {d2}");
        // A 3ª: o predicado do S3 tem de deixá-la passar (o lote zerou). O
        // T3-B ainda fala aqui — turno nunca fecha em teste unitário —, por
        // isso a asserção é sobre o marcador do S3, não sobre o silêncio total.
        let d3 = code_mode_gates(tmp.path(), s, "Bash", &bash("cat c.md"));
        assert!(
            d3.as_deref().is_none_or(|x| !x.contains("[CODE MODE · rajada]")),
            "após o deny o lote zera — um deny por rajada, nunca fadiga: {d3:?}"
        );
    }

    /// Classes DIFERENTES não somam entre si: um `grep` seguido de um `cat` são
    /// duas inspeções isoladas, não uma rajada de duas. A chave do ledger
    /// inclui a classe exatamente por isso.
    ///
    /// O teste afirma o que o predicado do S3 controla — que o deny
    /// `[CODE MODE · rajada]` NÃO sai — e não "nenhum gate falou", porque aqui
    /// o T3-B ainda intercepta: sem `PostToolUse` entre as chamadas o turno
    /// nunca fecha, e ele funde qualquer classe. Em produção esse braço mede
    /// `t3_turn_fused = 0` justamente porque o PostToolUse de cada chamada
    /// fecha o turno — ou seja, o caminho existe no código e não no mundo.
    /// Evidência direta para o S10 (T3-BURIAL): são dois gates a dizer a mesma
    /// coisa, e o que sobrevive é o que dispara.
    #[test]
    #[serial_test::serial(gate_metrics, t3_env)]
    fn classes_distintas_nao_formam_rajada() {
        let tmp = tempfile::tempdir().expect("tempdir");
        escopo_code(tmp.path());
        let s = "s3-burst-3";
        assert!(code_mode_gates(tmp.path(), s, "Bash", &bash("grep -rn x src/")).is_none());
        for cmd in ["cat README.md", "ls -la src/"] {
            let d = code_mode_gates(tmp.path(), s, "Bash", &bash(cmd));
            assert!(
                d.as_deref().is_none_or(|x| !x.contains("[CODE MODE · rajada]")),
                "`{cmd}` é classe diferente — o predicado do S3 não pode somá-la \
                 à rajada do `grep`: {d:?}"
            );
        }
    }

    /// As classes que a lista fixa deixava passar SEMPRE (`ls`/`wc`/`sed-n`)
    /// agora respondem ao mesmo predicado — é o volume fan-out que a medição
    /// de 27/08 mostrou estar escapando (746 chamadas, ~76% em rajada).
    #[test]
    #[serial_test::serial(gate_metrics, t3_env)]
    fn classes_antes_isentas_agora_colapsam_em_rajada() {
        for (classe, a, b) in [
            ("sed-n", "sed -n 1,20p a.rs", "sed -n 30,50p b.rs"),
            ("ls", "ls -la src/", "ls -la crates/"),
            ("wc", "wc -l a.rs", "wc -l b.rs"),
        ] {
            let tmp = tempfile::tempdir().expect("tempdir");
            escopo_code(tmp.path());
            let s = format!("s3-antes-isenta-{classe}");
            assert!(
                code_mode_gates(tmp.path(), &s, "Bash", &bash(a)).is_none(),
                "`{classe}`: a 1ª segue passando"
            );
            let d = code_mode_gates(tmp.path(), &s, "Bash", &bash(b))
                .unwrap_or_else(|| panic!("`{classe}`: a 2ª na janela tem de colapsar"));
            assert!(
                d.contains("[CODE MODE · rajada]"),
                "`{classe}`: o deny é o do S3, não outro gate: {d}"
            );
        }
    }

    /// Um escopo que NÃO declara `code` não colapsa nada, por mais rajada que
    /// seja: o predicado é a discriminação DENTRO do modo, não um modo novo.
    #[test]
    #[serial_test::serial(t3_env)]
    fn escopo_sem_declaracao_nao_colapsa_rajada() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // sem .touring/touring.toml — o default é `both`
        let s = "s3-burst-sem-escopo";
        assert!(code_mode_gates(tmp.path(), s, "Bash", &bash("grep -rn x src/")).is_none());
        let d2 = code_mode_gates(tmp.path(), s, "Bash", &bash("grep -rn y src/"));
        assert!(
            d2.as_deref().is_none_or(|d| !d.contains("[CODE MODE · rajada]")),
            "fora do modo `code` o deny de rajada não existe: {d2:?}"
        );
    }

    /// Mutação e build NUNCA entram no predicado, em rajada ou não —
    /// `scan_class_of` só reconhece inspeção, e é ele quem porteia.
    #[test]
    #[serial_test::serial(t3_env)]
    fn mutacao_e_build_nao_entram_na_rajada() {
        let tmp = tempfile::tempdir().expect("tempdir");
        escopo_code(tmp.path());
        let s = "s3-burst-mutacao";
        for cmd in ["cargo build", "cargo build", "cat > out.txt <<EOF", "cat >> out.txt <<EOF"] {
            let d = code_mode_gates(tmp.path(), s, "Bash", &bash(cmd));
            assert!(
                d.as_deref().is_none_or(|x| !x.contains("[CODE MODE · rajada]")),
                "`{cmd}` não é inspeção e não pode colapsar: {d:?}"
            );
        }
    }

    /// Declara `[code_mode] mode = "code"` na raiz temporária.
    fn escopo_code(root: &std::path::Path) {
        std::fs::create_dir_all(root.join(".touring")).expect("mkdir");
        std::fs::write(root.join(".touring/touring.toml"), "[code_mode]\nmode = \"code\"\n")
            .expect("write");
    }

    fn bash(cmd: &str) -> serde_json::Value {
        serde_json::json!({ "command": cmd })
    }

    /// Toda classe da lista tem de ser uma que `scan_class_of` realmente emite.
    /// Sem isto a lista pode nomear algo que jamais chega ao gate — o modo de
    /// falha de `teste-do-componente-nao-e-teste-do-caminho`.
    #[test]
    fn toda_classe_colapsada_e_produzivel_pelo_classificador() {
        let amostras = [
            ("grep", "grep -rn foo src/"),
            ("cat", "cat README.md"),
            ("find", "find . -name x.rs"),
            ("ls", "ls -la src/"),
            ("wc", "wc -l README.md"),
            ("sed-n", "sed -n 1,20p README.md"),
        ];
        for (classe, cmd) in amostras {
            assert_eq!(scan_class_of(cmd), Some(classe), "classificador: {cmd}");
        }
        for classe in CODE_MODE_COLLAPSED_CLASSES {
            assert!(
                amostras.iter().any(|(c, _)| c == classe),
                "classe `{classe}` colapsa sem amostra que a produza"
            );
        }
    }

    /// P2.3 (calibração, 1.312 chamadas reais): `cat > f`/`cat >> f` é
    /// heredoc de ESCRITA (T4), não inspeção. Sem a exclusão, o modo `code` —
    /// que nega na 1ª chamada — negava 27 escritas (20,6% do que a matriz
    /// pegaria). Provado por mutação: remover o braço do redirect em
    /// `scan_class_of` reprova os três primeiros casos.
    #[test]
    fn cat_com_redirect_e_escrita_nao_inspecao() {
        // escrita: primeiro operando já é o redirect
        assert_eq!(scan_class_of("cat > out.txt <<'EOF'"), None);
        assert_eq!(scan_class_of("cat >> log.txt <<'EOF'"), None);
        assert_eq!(scan_class_of("head > out.txt"), None);
        assert_eq!(scan_class_of("tail -n +3 >> out.txt"), None);
        // inspeção preservada
        assert_eq!(scan_class_of("cat README.md"), Some("cat"));
        assert_eq!(scan_class_of("cat -n README.md"), Some("cat"));
        // inspeção cujo RESULTADO vai a arquivo: o operando de leitura vem
        // primeiro — o programa fundido reproduziria o redirect
        assert_eq!(scan_class_of("cat README.md > out.txt"), Some("cat"));
        assert_eq!(scan_class_of("head -5 README.md"), Some("cat"));
    }

    /// S1 (26/08/2026) — wrappers de execução são transparentes à
    /// classificação: `time grep` é `grep`. Antes o 1º token decidia, e
    /// `time`/`nice`/`sudo`/`env` anulavam a classe para gates E nudges ao
    /// mesmo tempo. Cada assert reprova uma mutação do resolvedor (remover
    /// `skip_wrapper` devolve `None` em todos os casos positivos).
    #[test]
    fn s1_wrappers_resolve_o_verbo_real() {
        assert_eq!(scan_class_of("time grep -rn foo src/"), Some("grep"));
        assert_eq!(scan_class_of("nice -n 5 find . -name x.rs"), Some("find"));
        assert_eq!(scan_class_of("sudo cat README.md"), Some("cat"));
        assert_eq!(scan_class_of("env FOO=1 rg pattern"), Some("grep"));
        assert_eq!(scan_class_of("FOO=1 time grep x"), Some("grep"));
        assert_eq!(scan_class_of("timeout 30 ls -la"), Some("ls"));
        assert_eq!(scan_class_of("stdbuf -o0 cat f"), Some("cat"));
        assert_eq!(scan_class_of("command grep x f"), Some("grep"));
        assert_eq!(scan_class_of("sudo -u root env A=1 wc -l f"), Some("wc"));
        assert_eq!(scan_class_of("time sed -n 5p f"), Some("sed-n"));
        assert_eq!(
            scan_class_of("chrt -f 10 nice -n 5 grep x f"),
            Some("grep")
        );
    }

    /// As exclusões pré-S1 atravessam o invólucro: escrita continua escrita,
    /// invólucro puro não é classe, e o `-n` do `nice` NÃO vira o `-n` do sed.
    #[test]
    fn s1_wrappers_preservam_as_exclusoes() {
        assert_eq!(scan_class_of("nice cat > out.txt <<'EOF'"), None);
        assert_eq!(scan_class_of("sudo head > out.txt"), None);
        assert_eq!(scan_class_of("nice -n 5 sed 5p f"), None);
        assert_eq!(scan_class_of("time"), None);
        assert_eq!(scan_class_of("env"), None);
        assert_eq!(scan_class_of("sudo"), None);
        assert_eq!(scan_class_of("env FOO=1"), None);
    }

    /// Prefixo `VAR=valor`: o comportamento pré-S1, bit a bit — o bypass
    /// `TOURING_GATE_OK=1`/`TOURING_CODE_MODE=<v>` segue transparente.
    #[test]
    fn s1_prefixo_var_valor_segue_transparente() {
        assert_eq!(scan_class_of("FOO=1 grep x f"), Some("grep"));
        assert_eq!(
            scan_class_of("TOURING_CODE_MODE=native grep x f"),
            Some("grep")
        );
        assert_eq!(scan_class_of("FOO=1"), None);
    }

    /// O complemento que a estratégia S1 nomeou e a primeira entrega deixou
    /// passar: prefixos `cd <dir>` e operadores de sequência (`&&`, `;`,
    /// quebra de linha) são transparentes — `cd /x && grep` é `grep`.
    #[test]
    fn s1_prefixos_cd_e_sequencia_sao_transparentes() {
        assert_eq!(scan_class_of("cd /tmp && grep foo f"), Some("grep"));
        assert_eq!(scan_class_of("cd /x; cat f"), Some("cat"));
        assert_eq!(scan_class_of("cd /x\ncat f"), Some("cat"));
        assert_eq!(scan_class_of("cd /x && cd /y && find . -name z"), Some("find"));
        assert_eq!(scan_class_of("time cd /x && grep p f"), Some("grep"));
        assert_eq!(scan_class_of("cd /x && timeout 30 ls"), Some("ls"));
        // navegação pura e não-inspeção seguem sem classe
        assert_eq!(scan_class_of("cd /x"), None);
        assert_eq!(scan_class_of("cd /x && cd /y"), None);
        assert_eq!(scan_class_of("cd /x && cargo test"), None);
        // escrita continua escrita atrás do cd
        assert_eq!(scan_class_of("cd /x && cat > out.txt"), None);
        // is_scan_command (o nudge C8) herda o mesmo resolvedor
        assert!(is_scan_command("cd /tmp && grep p f"));
        assert!(!is_scan_command("cd /tmp && cargo test"));
    }

    /// O 2º sítio (nudge C8) resolve os mesmos wrappers — gates e nudges
    /// nascem do MESMO resolvedor, nunca de cópias do predicado.
    #[test]
    fn s1_is_scan_command_resolve_wrappers() {
        assert!(is_scan_command("time grep pattern file"));
        assert!(is_scan_command("sudo find . -name x"));
        assert!(is_scan_command("env A=1 rg pattern"));
        assert!(!is_scan_command("nice ls -la"));
        assert!(!is_scan_command("time"));
        assert!(is_scan_command("grep pattern file"));
        assert!(!is_scan_command("grep"));
        assert!(is_scan_command("find . -name x"));
    }

    /// RETOMAR-AQUI P2 item 1: `TOURING_CODE_MODE=<v>` no PREFIXO do comando
    /// é o nível mais externo da resolução — a via que atravessa processos
    /// irmãos (o shell da Bash tool e o hook não compartilham env; medido
    /// 25/08: o processo do CC não tem nenhuma var TOURING_*). Útil sobretudo
    /// para RELAXAR por-comando, simétrico ao TOURING_GATE_OK.
    #[test]
    fn prefixo_do_comando_vence_todos_os_niveis() {
        let tmp = tmpdir("prefixo");
        escreve_config(&tmp, "[code_mode]\nmode = \"code\"\n");
        // relaxa um projeto code
        assert_eq!(
            code_mode_presentation(&tmp, "TOURING_CODE_MODE=native grep foo src/"),
            CodeModePresentation::Native
        );
        assert_eq!(
            code_mode_presentation(&tmp, "TOURING_CODE_MODE=both grep foo src/"),
            CodeModePresentation::Both
        );
        // sem prefixo, o projeto manda
        assert_eq!(
            code_mode_presentation(&tmp, "grep foo src/"),
            CodeModePresentation::Code
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prefixo_aperta_projeto_sem_configuracao() {
        let tmp = tmpdir("prefixo-aperta");
        // sem .touring/touring.toml: default seria Both — o prefixo aperta
        assert_eq!(
            code_mode_presentation(&tmp, "TOURING_CODE_MODE=code grep foo"),
            CodeModePresentation::Code
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prefixo_invalido_nao_cala_os_niveis_seguintes() {
        let tmp = tmpdir("prefixo-invalido");
        escreve_config(&tmp, "[code_mode]\nmode = \"code\"\n");
        assert_eq!(
            code_mode_presentation(&tmp, "TOURING_CODE_MODE=explode grep foo"),
            CodeModePresentation::Code,
            "valor inválido cai para o nível do projeto, nunca para um colapso não pedido"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prefixo_depois_de_outras_vars_e_valor_com_aspas() {
        let tmp = tmpdir("prefixo-vars");
        assert_eq!(
            code_mode_presentation(&tmp, "FOO=1 TOURING_CODE_MODE=native grep foo"),
            CodeModePresentation::Native
        );
        assert_eq!(
            code_mode_presentation(&tmp, "TOURING_CODE_MODE=\"native\" grep foo"),
            CodeModePresentation::Native
        );
        // parou de ser prefixo (token sem '='): a var no meio do comando NÃO vale
        assert_eq!(
            code_mode_presentation(&tmp, "echo x TOURING_CODE_MODE=native"),
            CodeModePresentation::Both
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── P3/T3-B — fusão automática da rajada do turno ────────────────────────

    /// O predicado puro: closed ou sem 1ª → FirstPass; turno aberto com 1ª →
    /// Fold acumulando o comando. Separar decisão de estado é o que deixa o
    /// gate mais fácil de testar do que de contornar.
    #[test]
    fn rota_tomada_e_rota_recusada_tem_vereditos_opostos() {
        use crate::cli_suggester::classify_route_outcome;
        // Rodar o programa É a rota. O VALOR carrega a economia (P5b): uma casca
        // sobre uma operação vale 0.5; o que importa aqui é que é positivo,
        // enquanto recusar vale 0.0.
        assert_eq!(classify_route_outcome("touring run --lang bash --code 'ls'"), Some(0.5));
        assert_eq!(
            classify_route_outcome(
                "touring run --lang bash --code 'cat /a/x.rs /a/y.rs /a/z.rs'"
            ),
            Some(1.0),
            "o programa que funde vale a rota inteira"
        );
        // Relaxar o gate é recusá-la.
        assert_eq!(classify_route_outcome("TOURING_CODE_MODE=native grep -rn foo ."), Some(0.0));
        assert_eq!(classify_route_outcome("TOURING_GATE_OK=1 cat x"), Some(0.0));
    }

    /// A falha que isto pega: ler "fez outra coisa" como fracasso. Ausência de
    /// sinal é desconhecido — creditar 0.0 aqui puniria o braço por silêncio.
    #[test]
    fn desfecho_desconhecido_nao_vira_veredito() {
        use crate::cli_suggester::classify_route_outcome;
        assert_eq!(classify_route_outcome("cargo test -p touring-cli"), None);
        assert_eq!(classify_route_outcome(""), None);
    }

    /// Ordem: ter relaxado o gate no caminho não desfaz ter rodado o programa.
    #[test]
    fn relaxar_o_gate_e_ainda_assim_rodar_conta_como_rota_tomada() {
        use crate::cli_suggester::classify_route_outcome;
        let v = classify_route_outcome(
            "TOURING_CODE_MODE=native touring run --lang bash --code 'ls'",
        )
        .expect("rodar o programa é um desfecho legível");
        assert!(
            v > 0.0,
            "rodar o programa domina o token de relaxamento — senão o braço \
             aprenderia o oposto do que houve (o valor exato é a ECONOMIA, \
             coberta por outro teste)"
        );
    }

    /// A falha que isto pega — e que existiu por alguns minutos hoje: um
    /// PostToolUse de ferramenta SEM comando (um `Read`) consumia a oferta sem
    /// veredito, e a rota tomada logo depois já não achava nada para creditar.
    #[test]
    fn desfecho_ilegivel_nao_consome_a_oferta() {
        use crate::cli_suggester::{RouteOffer, claim_route_reward, turn_ledger_insert_for_test};
        let raiz = std::path::Path::new("/tmp/p2-teste-ordem");
        let payload = serde_json::json!({"session_id": "sessao-ordem"});
        turn_ledger_insert_for_test(
            raiz,
            &payload,
            RouteOffer { mode: "code".into(), offered_secs: 7 },
        );

        // Um `Read` não carrega comando: nada a classificar.
        assert!(claim_route_reward(raiz, &payload, "").is_none());
        // …e a oferta TEM de continuar lá para o veredito real.
        let (offer, value) =
            claim_route_reward(raiz, &payload, "touring run --lang bash --code 'ls'")
                .expect("a oferta sobreviveu ao desfecho ilegível");
        assert_eq!(offer.mode, "code");
        assert_eq!(value, 0.5, "programa de um alvo: rota tomada, economia nula");
        // E agora sim ela some.
        assert!(claim_route_reward(raiz, &payload, "touring run --code 'x'").is_none());
    }

    /// Uma oferta reivindicada some: dois leitores nunca creditam a mesma
    /// decisão duas vezes (a disciplina dos ledgers de decisão e de casos).
    #[test]
    fn a_oferta_de_rota_e_reivindicada_uma_unica_vez() {
        use crate::cli_suggester::{RouteOffer, take_route_offer, turn_ledger_insert_for_test};
        let raiz = std::path::Path::new("/tmp/p2-teste-rota");
        let payload = serde_json::json!({"session_id": "sessao-p2"});
        turn_ledger_insert_for_test(
            raiz,
            &payload,
            RouteOffer { mode: "code".into(), offered_secs: 42 },
        );
        let primeira = take_route_offer(raiz, &payload);
        assert_eq!(primeira.map(|o| o.mode), Some("code".to_string()));
        assert!(
            take_route_offer(raiz, &payload).is_none(),
            "a segunda leitura não pode reencontrar a oferta já creditada"
        );
    }



    /// Chamada sem classe fan-out não abre turno nem é negada: mutação/build
    /// nunca é rajada (o desenho §T3-B é sobre inspeção).
    #[test]
    fn similaridade_pega_o_fanout_serial_que_os_outros_gates_nao_veem() {
        use super::super::{G1_SIMILARITY_AT, command_similarity};
        // O caso canônico: mesma inspeção, arquivo diferente. O G6 não pega
        // (não é byte-idêntico) e o G7 não pega (arquivo diferente).
        let s = command_similarity("grep X a.rs", "grep X b.rs");
        assert!(s >= G1_SIMILARITY_AT, "esperado >= 0.5, obtido {s}");

        // Mesma família, faixa diferente — o trabalho repetido é quase todo.
        let s2 = command_similarity("sed -n 1,20p f.rs", "sed -n 40,60p f.rs");
        assert!(s2 >= G1_SIMILARITY_AT, "esperado >= 0.5, obtido {s2}");
    }

    /// Sem falso positivo: comandos de propósitos distintos não podem ser
    /// julgados repetição. Um gate que negasse isto seria pior que nenhum.
    #[test]
    fn comandos_diferentes_nao_sao_similares() {
        use super::super::{G1_SIMILARITY_AT, command_similarity};
        for (a, b) in [
            ("grep X src/", "cargo test -p touring-cli"),
            ("cat /a/b.rs", "touring doctor -j"),
            ("", "grep X src/"),
        ] {
            let s = command_similarity(a, b);
            assert!(s < G1_SIMILARITY_AT, "{a:?} vs {b:?} não deviam casar: {s}");
        }
    }

    /// A calibração dispara na SEGUNDA chamada quase-idêntica, sem esperar o
    /// contador do G1 chegar a `G1_DENY_AT` — que era o buraco.
    /// D8 dentro do próprio gate: a razão DECLARADA tem de ser a do gatilho que
    /// disparou. Um deny na 2ª chamada exibindo "90% das rajadas ≥4" justificaria
    /// o bloqueio por uma estatística que não se aplica a ele.
    #[test]
    fn a_razao_declarada_e_a_do_gatilho_que_disparou() {
        use super::super::{G1_DENY_AT, burst_gate};
        let proj = std::path::Path::new("/tmp/g1-sim-3");
        let s = "g1-sim-3";
        assert!(burst_gate(proj, s, "grep alfa /x/a.rs").is_none());
        let d = burst_gate(proj, s, "grep alfa /x/b.rs").expect("2ª negada por similaridade");
        assert!(
            d.contains("dos tokens da inspeção anterior"),
            "o deny tem de citar a SIMILARIDADE, que foi o gatilho: {d}"
        );
        assert!(
            !d.contains(&format!("rajadas ≥{G1_DENY_AT}")),
            "não pode justificar por uma estatística de rajada que não disparou: {d}"
        );
    }

    #[test]
    fn a_segunda_quase_identica_ja_e_negada() {
        use super::super::burst_gate;
        let proj = std::path::Path::new("/tmp/g1-sim-1");
        let s = "g1-sim-1";
        assert!(burst_gate(proj, s, "grep alfa /x/a.rs").is_none(), "a 1ª passa");
        let d = burst_gate(proj, s, "grep alfa /x/b.rs")
            .expect("a 2ª quase-idêntica é negada pela similaridade");
        assert!(d.contains("touring run --lang bash --code"), "a rota é um programa: {d}");
        assert!(d.contains("/x/a.rs") && d.contains("/x/b.rs"), "funde as duas: {d}");
    }

    /// E uma 2ª chamada DIFERENTE da mesma classe continua passando — a
    /// calibração não pode transformar o G1 num gate de duas-da-mesma-classe.
    #[test]
    fn segunda_da_mesma_classe_mas_diferente_ainda_passa() {
        use super::super::burst_gate;
        let proj = std::path::Path::new("/tmp/g1-sim-2");
        let s = "g1-sim-2";
        assert!(burst_gate(proj, s, "grep alfa /x/a.rs").is_none());
        assert!(
            burst_gate(proj, s, "grep beta_totalmente_outro /y/z/outro_arquivo.toml").is_none(),
            "propósito diferente não é repetição"
        );
    }

    /// Kill switch humano: TOURING_T3_FUSE_DISABLED=1 desliga a fusão inteira.
    ///

    fn escreve_config(dir: &std::path::Path, corpo: &str) {
        let t = dir.join(".touring");
        std::fs::create_dir_all(&t).expect("mkdir .touring");
        std::fs::write(t.join("touring.toml"), corpo).expect("write toml");
    }

    fn tmpdir(slug: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("p2-{slug}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn le_o_modo_da_secao_do_projeto() {
        let tmp = tmpdir("cfg");
        escreve_config(
            &tmp,
            "[daemon]\nper_project = true\n\n[code_mode]\nmode = \"code\"\n",
        );
        assert_eq!(project_presentation(&tmp), Some(CodeModePresentation::Code));
        escreve_config(&tmp, "[code_mode]\nmode = \"native\"\n");
        assert_eq!(
            project_presentation(&tmp),
            Some(CodeModePresentation::Native)
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `mode` fora de `[code_mode]` NÃO conta: sem o estado de seção, a chave
    /// homônima de outra tabela colapsaria o projeto por acidente.
    #[test]
    fn chave_de_outra_secao_nao_vale() {
        let tmp = tmpdir("outra");
        escreve_config(&tmp, "[toolchain]\nmode = \"code\"\nchannel = \"dev\"\n");
        assert_eq!(project_presentation(&tmp), None, "só [code_mode] decide");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Sem config e com valor desconhecido: default, nunca um colapso que
    /// ninguém pediu. Falha para o comportamento de hoje.
    #[test]
    fn politica_desarmada_nao_muda_absolutamente_nada() {
        use super::super::{CodeModePresentation, code_mode_presentation};
        // Sem a env, o caminho é byte-idêntico ao de antes: nenhum sinal lido.
        let vazio = tmpdir("arm-desarmado");
        std::fs::create_dir_all(&vazio).expect("mkdir");
        assert_eq!(code_mode_presentation(&vazio, "grep x"), CodeModePresentation::Both);
        let _ = std::fs::remove_dir_all(&vazio);
    }

    /// A invariante que protege Gabriel: a política preenche o espaço que o
    /// humano deixou aberto — nunca sobrescreve o que ele declarou. Uma
    /// política que vencesse o `touring.toml` não estaria aprendendo, estaria
    /// desobedecendo.
    ///
    /// Testado no PREDICADO PURO: a versão anterior mutava
    /// `TOURING_CODE_MODE_ARM_ARMED` no processo, e a var passou a ser lida
    /// também pelo caminho da fusão — a mutação vazava para testes paralelos e
    /// derrubava um vizinho (`--test-threads=1` passava; a assinatura de estado
    /// global).
    #[test]
    fn declaracao_humana_vence_a_politica_mesmo_armada() {
        use super::super::{CodeModePresentation, resolve_with_policy};
        // evidência que, sozinha, faria a política escolher `code`
        let counts = [(20, 4), (20, 10), (20, 20)];
        assert_eq!(
            resolve_with_policy(Some(CodeModePresentation::Native), true, counts),
            CodeModePresentation::Native,
            "o `touring.toml` declarou `native`; a política não pode passar por cima"
        );
        // sem declaração, armada e com evidência: a política escolhe
        assert_eq!(
            resolve_with_policy(None, true, counts),
            CodeModePresentation::Code
        );
        // sem declaração e DESARMADA: default, nenhum sinal lido
        assert_eq!(
            resolve_with_policy(None, false, counts),
            CodeModePresentation::Both
        );
    }

    /// Evidência fina não promove nada — o ponto inteiro de "promoção medida".
    #[test]
    fn programa_de_um_alvo_e_uma_operacao_nao_fundiu_nada() {
        use super::super::{ProgramShape, program_shape};
        assert_eq!(program_shape("cat /a/b/c.rs"), ProgramShape::Trivial);
        assert_eq!(program_shape("sed -n 1,20p /a/b/c.rs"), ProgramShape::Trivial);
    }

    #[test]
    fn programa_que_toca_varios_alvos_ou_operacoes_e_fusao() {
        use super::super::{ProgramShape, program_shape};
        // três alvos, uma operação
        assert!(matches!(
            program_shape("cat /a/x.rs /a/y.rs /a/z.rs"),
            ProgramShape::Fused(n) if n >= 3
        ));
        // um alvo, três operações
        assert!(matches!(
            program_shape("grep a /f/g.rs; wc -l /f/g.rs; sed -n 1p /f/g.rs"),
            ProgramShape::Fused(n) if n >= 3
        ));
    }

    #[test]
    fn a_aspa_que_fecha_e_a_primeira_nao_a_ultima() {
        use super::super::{ProgramShape, extract_run_body, program_shape};
        // O caso REAL que quebrou a medição ao vivo em 26/08: o comando externo
        // trazia mais aspas depois do corpo, e `rfind` engolia o pipe inteiro.
        let cmd = "touring run --lang bash --code 'cat /a/b/Cargo.toml' 2>/dev/null \
                   | python3 -c \"print('x')\"";
        assert_eq!(extract_run_body(cmd), Some("cat /a/b/Cargo.toml"));
        assert_eq!(
            program_shape(extract_run_body(cmd).unwrap()),
            ProgramShape::Trivial,
            "um arquivo, uma operação — o pipe externo não é parte do programa"
        );
    }

    /// `2>/dev/null` é plumbing do shell, não um alvo de inspeção. Contá-lo
    /// fazia um programa de um arquivo parecer que tocava dois.
    #[test]
    fn dispositivos_nao_contam_como_alvo() {
        use super::super::{ProgramShape, program_shape};
        assert_eq!(program_shape("cat /a/b.rs 2>/dev/null"), ProgramShape::Trivial);
        assert_eq!(program_shape("cat /a/b.rs >/dev/null"), ProgramShape::Trivial);
    }

    #[test]
    fn o_corpo_do_programa_e_extraido_de_ambas_as_aspas() {
        use super::super::extract_run_body;
        assert_eq!(
            extract_run_body("touring run --lang bash --code 'cat /a/b'"),
            Some("cat /a/b")
        );
        assert_eq!(
            extract_run_body("touring run --lang python --code \"print(1)\""),
            Some("print(1)")
        );
        // sem --code não há o que classificar — jamais adivinhar
        assert_eq!(extract_run_body("touring run --file x.py"), None);
    }

    /// O ponto do P5b: obedecer não é economizar. Uma casca sobre uma chamada
    /// só vale metade — e NÃO zero, porque sob `code` a atômica é negada e a
    /// casca é obrigatória: o custo é da apresentação, não do modelo.
    #[test]
    fn casca_sobre_chamada_unica_vale_metade_da_rota_que_funde() {
        use super::super::classify_route_outcome;
        assert_eq!(
            classify_route_outcome("touring run --lang bash --code 'cat /a/b/c.rs'"),
            Some(0.5)
        );
        assert_eq!(
            classify_route_outcome(
                "touring run --lang bash --code 'grep a /f/g.rs; wc -l /f/h.rs; cat /f/i.rs'"
            ),
            Some(1.0)
        );
    }

    #[test]
    #[serial_test::serial(gate_metrics)]
    fn a_evidencia_do_braco_sobrevive_ao_processo() {
        use super::super::{ArmAxis, arm_counts_durable, bump_arm, read_arm_file};
        let proj = tmpdir("arm-duravel");
        std::fs::create_dir_all(&proj).expect("mkdir");

        for _ in 0..3 {
            bump_arm(&proj, "code", ArmAxis::Offered);
        }
        bump_arm(&proj, "code", ArmAxis::Followed);
        bump_arm(&proj, "code", ArmAxis::Economical);

        // Leitura DIRETA do arquivo — sem cache, que é o que um processo novo
        // (daemon reiniciado) faria. Contadores em memória zeraram de 2 para 0
        // em dois minutos hoje; a decisão não pode depender deles.
        let do_disco = read_arm_file(&proj);
        assert_eq!(do_disco[2], (3, 1, 1), "o braço `code` tem de estar no disco");
        assert_eq!(arm_counts_durable(&proj)[2], (3, 1));

        let _ = std::fs::remove_dir_all(&proj);
    }

    /// O durável (arquivo por projeto) e o volátil (contador de processo)
    /// contam a MESMA decisão.
    ///
    /// **Serial por necessidade, não por hábito.** O `tmpdir` isola o durável,
    /// mas `code_mode_arm_counts()` é um contador ÚNICO do processo: o teste
    /// compara um delta global com um valor local, e qualquer outro teste que
    /// chame `bump_arm` entre as duas leituras quebra a igualdade. Medido em
    /// 27/08: serial 458/458, paralelo falhando 1 em 3.
    ///
    /// É a exceção ao isolamento estrutural que o resto deste módulo usa (chave
    /// com `project_root`, ver os testes de rajada do S3): ali existe uma chave
    /// que separa; aqui o recurso é global por design, e fingir que não é seria
    /// esconder a corrida em vez de declará-la.
    #[test]
    #[serial_test::serial(gate_metrics)]
    fn bump_arm_mantem_duravel_e_volatil_em_sincronia() {
        use super::super::{ArmAxis, bump_arm, read_arm_file};
        use touring_foundation::gate_metrics_snapshot::code_mode_arm_counts;
        let proj = tmpdir("arm-sincronia");
        std::fs::create_dir_all(&proj).expect("mkdir");

        // índice 2 = `code` na ordem (native, both, code).
        let antes = code_mode_arm_counts()[2];
        bump_arm(&proj, "code", ArmAxis::Offered);
        bump_arm(&proj, "code", ArmAxis::Offered);
        bump_arm(&proj, "code", ArmAxis::Followed);
        let depois = code_mode_arm_counts()[2];
        let disco = read_arm_file(&proj)[2];

        assert_eq!(disco.0, 2, "durável: 2 ofertas");
        assert_eq!(disco.1, 1, "durável: 1 tomada");
        // `>=`, não `==`, e a razão importa: o durável é POR PROJETO (isolado
        // pelo tmpdir acima) enquanto o volátil é um contador ÚNICO do
        // processo, incrementado por qualquer caminho que ofereça uma rota —
        // inclusive os testes de rajada do S3, que produzem denies. Exigir
        // igualdade era exigir EXCLUSIVIDADE sobre um recurso compartilhado, e
        // o teste passava por sorte de escalonamento (falhava 1 em 3).
        //
        // O que a asserção protege continua de pé: se o volátil deixasse de
        // acompanhar o durável, o delta seria MENOR que o disco e isto reprova.
        // O caso oposto — o volátil andar mais que o durável — é esperado aqui
        // e é justamente o que a igualdade não sabia distinguir de um bug.
        assert!(
            depois.0 - antes.0 >= disco.0,
            "o volátil não acompanhou o durável nas ofertas: {} < {}",
            depois.0 - antes.0,
            disco.0
        );
        assert!(
            depois.1 - antes.1 >= disco.1,
            "o volátil não acompanhou o durável nas tomadas: {} < {}",
            depois.1 - antes.1,
            disco.1
        );

        // Economia é durável-only por design: não existe átomo correspondente,
        // então o volátil NÃO pode mexer quando só a economia é registrada.
        let pre_eco = code_mode_arm_counts()[2];
        bump_arm(&proj, "code", ArmAxis::Economical);
        assert_eq!(
            code_mode_arm_counts()[2],
            pre_eco,
            "economia não tem par volátil; nada no gate-metrics pode mudar"
        );
        assert_eq!(read_arm_file(&proj)[2].2, 1, "economia foi ao disco");

        let _ = std::fs::remove_dir_all(&proj);
    }

    /// Um nome de braço desconhecido não pode cair num balde: contar errado é
    /// pior que não contar, porque a política acreditaria na contagem.
    #[test]
    #[serial_test::serial(gate_metrics)]
    fn braco_desconhecido_nao_e_contado() {
        use super::super::{ArmAxis, bump_arm, read_arm_file};
        let proj = tmpdir("arm-desconhecido");
        std::fs::create_dir_all(&proj).expect("mkdir");
        bump_arm(&proj, "agressivo", ArmAxis::Offered);
        assert_eq!(read_arm_file(&proj), [(0, 0, 0); 3]);
        let _ = std::fs::remove_dir_all(&proj);
    }

    /// Arquivo ilegível ⇒ zeros ⇒ política calada. Evidência que não se pode
    /// ler é evidência que não existe; inventar uma escolha seria pior.
    #[test]
    fn evidencia_corrompida_deixa_a_politica_calada() {
        use super::super::{arm_choice_from_counts, read_arm_file};
        let proj = tmpdir("arm-corrompido");
        std::fs::create_dir_all(proj.join(".claude/touring")).expect("mkdir");
        std::fs::write(proj.join(".claude/touring/code_mode_arm.json"), "{nao json")
            .expect("write");
        assert_eq!(read_arm_file(&proj), [(0, 0, 0); 3]);
        assert_eq!(arm_choice_from_counts([(0, 0); 3]), None);
        let _ = std::fs::remove_dir_all(&proj);
    }

    #[test]
    fn abaixo_do_piso_de_amostra_a_politica_se_cala() {
        use super::super::arm_choice_from_counts;
        assert_eq!(arm_choice_from_counts([(19, 19), (19, 0), (19, 5)]), None);
        assert_eq!(arm_choice_from_counts([(0, 0), (0, 0), (0, 0)]), None);
    }

    /// Um único braço com amostra não é uma escolha: é o único observado.
    /// Chamar isso de aprendizado seria confirmar a configuração vigente.
    #[test]
    fn um_braco_sozinho_nao_conta_como_comparacao() {
        use super::super::arm_choice_from_counts;
        assert_eq!(arm_choice_from_counts([(100, 90), (3, 3), (0, 0)]), None);
    }

    #[test]
    fn diferenca_dentro_do_ruido_nao_e_evidencia() {
        use super::super::arm_choice_from_counts;
        // 90% vs 88%: diferença real, mas menor que a margem — silêncio.
        assert_eq!(arm_choice_from_counts([(0, 0), (100, 90), (100, 88)]), None);
        // Empate exato jamais pode desempatar pela ordem do vetor.
        assert_eq!(arm_choice_from_counts([(20, 10), (20, 10), (20, 10)]), None);
    }

    #[test]
    fn com_dois_bracos_medidos_vence_a_maior_taxa() {
        use super::super::{CodeModePresentation, arm_choice_from_counts};
        // native 50%, both 90% → both.
        assert_eq!(
            arm_choice_from_counts([(20, 10), (20, 18), (0, 0)]),
            Some(CodeModePresentation::Both)
        );
        // code 100% supera both 80% — folga clara, não fronteira de margem.
        // (Uma asserção exatamente NO limiar dependeria da representação IEEE754
        // da subtração e falharia por motivo nenhum a ver com a política.)
        assert_eq!(
            arm_choice_from_counts([(0, 0), (20, 16), (20, 20)]),
            Some(CodeModePresentation::Code)
        );
    }

    #[test]
    fn ausencia_e_valor_invalido_caem_no_default() {
        let vazio = tmpdir("vazio");
        std::fs::create_dir_all(&vazio).expect("mkdir");
        assert_eq!(project_presentation(&vazio), None);
        let _ = std::fs::remove_dir_all(&vazio);

        let invalido = tmpdir("invalido");
        escreve_config(&invalido, "[code_mode]\nmode = \"agressivo\"\n");
        assert_eq!(project_presentation(&invalido), None);
        let _ = std::fs::remove_dir_all(&invalido);
    }
}

mod fusao_do_remedio {
    use super::super::{G1_BODY_BUDGET, fuse_burst_program};

    /// A invariante central: todo comando que ENTRA no corpo entra inteiro.
    ///
    /// Enunciada como asserção positiva sobre TODOS os comandos, não como
    /// ausência de um caso conhecido — a lição de `every_derivable_nudge_...`:
    /// invariante verificada numa instância recorre na próxima não coberta.
    #[test]
    fn nenhum_comando_entra_pela_metade() {
        // Excede o orçamento SOZINHO (22 chars × 200 ≈ 4400 > 3000), que é a
        // única forma de forçar a omissão — a primeira versão deste teste usava
        // ×60 (~1334 chars), cabia, e o `omitidos > 0` reprovou o TESTE, não o
        // código: a invariante do "nada pela metade" havia segurado.
        let longo = format!("grep -rn \"x\" {}", "crates/touring-ceg/src/".repeat(200));
        let cmds = vec![
            "grep -n foo a.rs".to_string(),
            longo,
            "grep -n bar b.rs".to_string(),
        ];
        let (corpo, omitidos) = fuse_burst_program(&cmds);
        for cmd in &cmds {
            let inteiro = corpo.contains(&cmd.replace('\'', "'\\''"));
            let prefixo: String = cmd.chars().take(24).collect();
            let ausente = !corpo.contains(&prefixo);
            assert!(
                inteiro || ausente,
                "comando entrou pela metade — nem inteiro nem ausente"
            );
        }
        assert!(omitidos > 0, "o comando gigante devia ter sido omitido");
    }

    #[test]
    fn corpo_respeita_o_orcamento() {
        let cmds: Vec<String> = (0..8)
            .map(|i| format!("grep -rn \"pat{i}\" {}", "crates/x/".repeat(50)))
            .collect();
        let (corpo, omitidos) = fuse_burst_program(&cmds);
        assert!(corpo.chars().count() <= G1_BODY_BUDGET);
        assert!(omitidos > 0, "com 8 comandos gigantes algo tem de sobrar");
    }

    #[test]
    fn rajada_normal_entra_inteira_e_nada_e_omitido() {
        let cmds = vec![
            "grep -n alpha a.rs".to_string(),
            "grep -n beta b.rs".to_string(),
            "grep -n gamma c.rs".to_string(),
        ];
        let (corpo, omitidos) = fuse_burst_program(&cmds);
        assert_eq!(omitidos, 0);
        for cmd in &cmds {
            assert!(corpo.contains(cmd), "faltou {cmd}");
        }
        assert_eq!(corpo.lines().count(), 3);
    }

    /// As aspas simples do comando original têm de sobreviver ao embrulho em
    /// `--code '<corpo>'`, senão o programa entregue quebra na primeira aspa.
    #[test]
    fn aspas_simples_sao_escapadas_para_o_embrulho() {
        let cmds = vec!["grep -rn 'foo bar' src/".to_string()];
        let (corpo, _) = fuse_burst_program(&cmds);
        assert!(corpo.contains("'\\''foo bar'\\''"), "corpo: {corpo}");
        let programa = format!("touring run --lang bash --code '{corpo}'");
        assert!(programa.ends_with('\''));
        assert!(programa.starts_with("touring run --lang bash --code '"));
    }

    #[test]
    fn lista_vazia_devolve_corpo_vazio_sem_omissao() {
        let (corpo, omitidos) = fuse_burst_program(&[]);
        assert!(corpo.is_empty());
        assert_eq!(omitidos, 0);
    }
}

/// Nenhuma família de indução dispara sobre um comando que JÁ é code mode.
///
/// Observado 6× em 25/08/2026: o nudge `code-mode-loop` recebia
/// `touring run --lang python --code '…'` e emitia como MUST
/// `touring run --lang bash --code 'touring run --lang python …'` — ensinando
/// exatamente o antipadrão que existe para evitar. Asserção sobre TODAS as
/// portas de indução, não só a que foi vista falhar.
mod inducao_nao_reincide_sobre_si {
    use super::super::{code_mode_kind, master_cli_command};
    use serde_json::json;

    fn bash(cmd: &str) -> serde_json::Value {
        json!({ "command": cmd })
    }

    #[test]
    fn code_mode_kind_ignora_comando_que_ja_e_programa() {
        // Um laço DENTRO do sandbox continua sendo um laço — mas já está no
        // programa, então não há nada a induzir.
        let com_laco = bash("touring run --lang bash --code 'for f in *.rs; do wc -l $f; done'");
        assert!(code_mode_kind("Bash", &com_laco).is_none());

        // Uma varredura dentro do sandbox, idem.
        let com_scan = bash("touring run --lang python --code 'import glob; print(glob.glob(\"**/*.rs\"))'");
        assert!(code_mode_kind("Bash", &com_scan).is_none());

        // `touring exec` é o outro canal do mesmo transporte.
        let via_exec = bash("touring exec \"grep -rn foo crates/\"");
        assert!(code_mode_kind("Bash", &via_exec).is_none());
    }

    /// O guard não pode calar a indução legítima — senão a correção troca um
    /// defeito por outro (falso negativo no lugar de falso positivo).
    #[test]
    fn code_mode_kind_ainda_dispara_no_laco_atomico_de_verdade() {
        let laco = bash("for f in crates/*/src/lib.rs; do wc -l $f; done");
        assert!(
            code_mode_kind("Bash", &laco).is_some(),
            "o laço de shell cru continua sendo o caso canônico de indução"
        );
    }

    /// A mesma pergunta feita à outra porta de indução — a assimetria entre
    /// guards é o modo de falha de `comentario-afirma-simetria-inexistente`.
    #[test]
    fn master_cli_nao_induz_sobre_o_transporte() {
        assert!(
            master_cli_command("touring run --lang python --code 'print(1)'").is_none(),
            "`touring run` é o transporte, não um atômico com master equivalente"
        );
    }
}

// ── P3 — harness de replay do research loop ──────────────────────────────────
//
// NÃO é uma asserção: é um EXPERIMENTO determinístico sobre corpus CONGELADO
// (`eval/autoresearch/corpus.json`, extraído de transcripts reais). Usa os
// predicados REAIS (`scan_class_of`, `command_similarity`) e não uma cópia —
// um verificador que reimplementasse a regra mediria a cópia, não o gate.
//
// Roda sob demanda:
//   cargo test -p touring-cli --lib autoresearch_replay -- --ignored --nocapture
mod autoresearch_replay {
    use super::super::{command_similarity, scan_class_of};

    /// Uma varredura por limiar sobre o corpus congelado.
    ///
    /// Simula com estado LOCAL (um mapa por sessão), nunca os caches globais do
    /// processo: um experimento que escrevesse no ledger vivo contaminaria o
    /// sistema que ele mede.
    fn varre(sessoes: &[Vec<String>], limiar: f64) -> (usize, usize, f64) {
        let (mut inspecoes, mut colapsadas, mut soma_sim) = (0usize, 0usize, 0.0f64);
        for cmds in sessoes {
            let mut ultima_por_classe: std::collections::HashMap<&str, &str> =
                std::collections::HashMap::new();
            for cmd in cmds {
                let Some(classe) = scan_class_of(cmd) else { continue };
                inspecoes += 1;
                if let Some(anterior) = ultima_por_classe.get(classe) {
                    let sim = command_similarity(anterior, cmd);
                    if sim >= limiar {
                        colapsadas += 1;
                        soma_sim += sim;
                    }
                }
                ultima_por_classe.insert(classe, cmd);
            }
        }
        let media = if colapsadas > 0 { soma_sim / colapsadas as f64 } else { 0.0 };
        (inspecoes, colapsadas, media)
    }

    #[test]
    #[ignore = "experimento sob demanda — precisa do corpus extraído"]
    fn varre_o_limiar_de_similaridade_sobre_corpus_congelado() {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join("eval/autoresearch/corpus.json");
        let Ok(txt) = std::fs::read_to_string(&raiz) else {
            println!("CORPUS AUSENTE em {} — rode eval/autoresearch/extract_corpus.py",
                     raiz.display());
            return;
        };
        let v: serde_json::Value = serde_json::from_str(&txt).expect("corpus json");
        let sessoes: Vec<Vec<String>> = v["sessions"].as_array().unwrap().iter()
            .map(|s| s.as_array().unwrap().iter()
                 .map(|c| c.as_str().unwrap_or("").to_string()).collect())
            .collect();

        println!("corpus: {} sessões, meta={}", sessoes.len(), v["meta"]);
        println!("{:>7} {:>12} {:>12} {:>10}", "limiar", "inspeções", "colapsadas", "sim_média");
        for passo in 0..=10 {
            let limiar = passo as f64 / 10.0;
            let (insp, col, media) = varre(&sessoes, limiar);
            let pct = if insp > 0 { 100.0 * col as f64 / insp as f64 } else { 0.0 };
            println!("{limiar:>7.1} {insp:>12} {col:>12} {media:>10.3}   ({pct:.1}% das inspeções)");
        }
        // O escalar que uma campanha leria. DECLARADO como proxy: maximizá-lo
        // sozinho empurra o limiar para 0 (colapsar tudo), então o keep/discard
        // sobre ele PRECISA de gate humano — que é exatamente a partição pela
        // fronteira do verificador que o plano exige.
        let (_, col, _) = varre(&sessoes, super::super::G1_SIMILARITY_AT);
        println!("METRIC={col}");
    }
}
