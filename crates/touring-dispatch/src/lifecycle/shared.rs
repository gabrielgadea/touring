//! Cross-handler helpers shared across `lifecycle/*.rs` submodules.
//!
//! Extracted from `lifecycle.rs` as part of FIX-3 D2. These helpers are
//! stateless pure functions that map file paths, subjects, and identifiers
//! to generator-kind labels, symbol names, and verification hints. They
//! have no runtime dependencies — safe to call from any submodule.
//!
//! # Visibility
//!
//! All helpers are `pub(crate)` so `lifecycle.rs` can re-export them with
//! `pub(crate) use shared::*` and every submodule can access them via
//! `super::<helper>` (Rust re-export visibility ≤ source rule).
//!
//! # Catalog
//!
//! - Path → GeneratorKind classification: `classify_file_to_generator_kind`,
//!   `classify_yaml_to_generator_kind`, `classify_rust_to_generator_kind`,
//!   `maybe_generator_kind_hint`.
//! - Path → symbol name: `file_stem`, `stem_to_camel_case`.
//! - Rust file → VGP verify hint: `maybe_vgp_verify_hint_for_rs_file`.
//! - Subject → GeneratorKind: `suggest_generator_for_task_subject`
//!   (keyword-driven via `SUBJECT_KEYWORD_MAP`).

// ── Path → GeneratorKind hint ────────────────────────────────────────────────

/// R19-S1: Map a changed file's extension/name pattern to a relevant GeneratorKind hint.
///
/// Delegates classification to `classify_file_to_generator_kind` and formats the result.
/// Returns `None` for file types with no natural generator counterpart (`.lock`, `.md`, …).
pub(crate) fn maybe_generator_kind_hint(rel_path: &str) -> Option<String> {
    let kind = classify_file_to_generator_kind(rel_path)?;
    let stem = file_stem(rel_path);
    Some(format!(
        "generator: `touring generate render {kind} --vars '{{\"module_name\":\"{stem}\"}}'` to scaffold a {kind} for this file"
    ))
}

/// Classify a file path to a GeneratorKind label (CC≤9).
///
/// Priority: exact extension first, then YAML sub-kind, then Rust sub-kind.
/// R40-S1: Added .sql → Migration, .sh/.bash → ShellCompletion, openapi/asyncapi path → specs.
pub(crate) fn classify_file_to_generator_kind(rel_path: &str) -> Option<&'static str> {
    let lower = rel_path.to_lowercase();
    if lower.ends_with(".proto") {
        return Some("ProtobufSchema");
    }
    if lower.ends_with(".tf") {
        return Some("TerraformModule");
    }
    if lower.ends_with(".py") {
        return Some("PythonScript");
    }
    if lower.ends_with(".sql") {
        return Some("Migration");
    }
    if lower.ends_with(".sh") || lower.ends_with(".bash") {
        return Some("ShellCompletion");
    }
    if lower.contains("dockerfile") {
        return Some("Dockerfile");
    }
    if lower.contains("openapi") || lower.contains("swagger") {
        return Some("OpenApiSpec");
    }
    if lower.contains("asyncapi") {
        return Some("AsyncApiSpec");
    }
    if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        return classify_yaml_to_generator_kind(&lower);
    }
    if lower.ends_with(".rs") {
        return Some(classify_rust_to_generator_kind(&lower, rel_path));
    }
    None
}

/// Classify a YAML/YML file to CiWorkflow or K8sManifest (CC≤4).
pub(crate) fn classify_yaml_to_generator_kind(lower: &str) -> Option<&'static str> {
    if lower.contains("workflow") || lower.contains(".github/workflows") {
        return Some("CiWorkflow");
    }
    if lower.contains("manifest") || lower.contains("deploy") || lower.contains("k8s") {
        return Some("K8sManifest");
    }
    None
}

/// Classify a `.rs` file to Benchmark, FuzzTarget, Test, or RustModule (CC≤5).
pub(crate) fn classify_rust_to_generator_kind(lower: &str, rel_path: &str) -> &'static str {
    let stem_lower = file_stem(rel_path).to_lowercase();
    if stem_lower.starts_with("bench") || lower.contains("/benches/") {
        return "Benchmark";
    }
    if stem_lower.starts_with("fuzz") || lower.contains("/fuzz/") {
        return "FuzzTarget";
    }
    if stem_lower.contains("test") || lower.contains("/tests/") {
        return "Test";
    }
    "RustModule"
}

// ── Path → symbol name ───────────────────────────────────────────────────────

/// Extract the file stem (basename without extension) from a path string.
pub(crate) fn file_stem(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

/// R36-S2: Convert snake_case stem to CamelCase (CC≤2).
///
/// e.g. `hook_registry` → `HookRegistry`, `lifecycle` → `Lifecycle`.
/// Used to derive the dominant public symbol name from a changed Rust file's stem.
pub(crate) fn stem_to_camel_case(stem: &str) -> String {
    stem.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

/// R36-S2: Emit VGP symbol-verify hint when a Rust source file changes (CC≤3).
///
/// Converts the file stem to a likely CamelCase public symbol name and surfaces
/// `touring generate verify --symbol <Name>` so Claude Code immediately knows
/// which symbol to check after an edit. Skips lib/main/mod (too generic).
/// No DB call — purely structural from the path. Returns `None` for non-.rs files.
pub(crate) fn maybe_vgp_verify_hint_for_rs_file(rel_path: &str) -> Option<String> {
    if !rel_path.ends_with(".rs") {
        return None;
    }
    let stem = file_stem(rel_path);
    if stem.is_empty() || matches!(stem, "lib" | "main" | "mod" | "tests" | "error") {
        return None;
    }
    let symbol = stem_to_camel_case(stem);
    Some(format!(
        "vgp-verify: run `touring generate verify --symbol {symbol}` to confirm symbol in index | \
        run `touring index find {symbol}` for all definitions"
    ))
}

// ── Subject → GeneratorKind (keyword-driven) ─────────────────────────────────

/// R20-S3: Suggest a GeneratorKind for a task subject by keyword matching (CC≤3).
///
/// Data-driven: `SUBJECT_KEYWORD_MAP` maps keywords → GeneratorKind labels.
/// The first matching entry wins (priority order). Returns empty string if no match.
pub(crate) fn suggest_generator_for_task_subject(subject: &str) -> String {
    if subject.is_empty() {
        return String::new();
    }
    let lower = subject.to_lowercase();
    let Some(kind) = find_kind_by_keywords(&lower) else {
        return String::new();
    };
    let truncated = &subject[..subject.len().min(60)];
    format!(
        " | generator: `touring generate render {kind} --vars '{{\"module_name\":\"{truncated}\"}}'` suggested"
    )
}

/// Keyword → GeneratorKind priority table (first match wins).
///
/// Each entry is `(keyword, GeneratorKind label)`. Single `contains` check per keyword —
/// no branching, so CC of the calling function stays at 2.
static SUBJECT_KEYWORD_MAP: &[(&str, &str)] = &[
    ("test", "Test"),
    ("spec", "Test"),
    ("assert", "Test"),
    ("bench", "Benchmark"),
    ("perf", "Benchmark"),
    ("latency", "Benchmark"),
    ("fuzz", "FuzzTarget"),
    ("corpus", "FuzzTarget"),
    ("proto", "ProtobufSchema"),
    ("grpc", "ProtobufSchema"),
    ("terraform", "TerraformModule"),
    ("provisio", "TerraformModule"),
    ("docker", "Dockerfile"),
    ("container", "Dockerfile"),
    ("workflow", "CiWorkflow"),
    ("pipeline", "CiWorkflow"),
    ("github action", "CiWorkflow"),
    ("k8s", "K8sManifest"),
    ("kubernetes", "K8sManifest"),
    ("manifest", "K8sManifest"),
    ("migrat", "Migration"),
    ("schema change", "Migration"),
    ("hook", "HookHandler"),
    ("lifecycle", "HookHandler"),
    ("subcommand", "CliHandler"),
    ("cli", "CliHandler"),
    ("mcp tool", "McpTool"),
    ("mcp", "McpTool"),
    ("openapi", "OpenApiSpec"),
    ("swagger", "OpenApiSpec"),
    ("adr", "Adr"),
    ("decision record", "Adr"),
    ("blueprint", "PlanMd"),
    ("plan", "PlanMd"),
    ("error catalog", "ErrorCatalog"),
    ("error", "ErrorCatalog"),
    ("struct", "RustModule"),
    ("impl", "RustModule"),
    ("module", "RustModule"),
    // R121: 13 missing kinds — completes SUBJECT_KEYWORD_MAP to 30/30 GeneratorKinds.
    ("shell completion", "ShellCompletion"),
    ("autocomplete", "ShellCompletion"),
    ("man page", "ManPage"),
    ("manpage", "ManPage"),
    ("incremental patch", "IncrementalPatch"),
    ("hotfix", "IncrementalPatch"),
    ("skill document", "SkillDocument"),
    ("playbook", "SkillDocument"),
    ("diary entry", "DiaryEntry"),
    ("lesson learned", "DiaryEntry"),
    ("event consumer", "ConsumerGenerator"),
    ("message consumer", "ConsumerGenerator"),
    ("task scaffold", "TaskScaffold"),
    ("taco task", "TaskScaffold"),
    ("asyncapi", "AsyncApiSpec"),
    ("event-driven", "AsyncApiSpec"),
    ("changelog", "ChangelogEntry"),
    ("release notes", "ChangelogEntry"),
    ("ffi binding", "FfiBinding"),
    ("c binding", "FfiBinding"),
    ("derive macro", "DeriveMacro"),
    ("proc macro", "DeriveMacro"),
    ("data schema", "Schema"),
    ("json schema", "Schema"),
    ("python script", "PythonScript"),
    ("python automation", "PythonScript"),
];

/// Linear scan of `SUBJECT_KEYWORD_MAP` — O(N) with N≈40 entries, CC=2.
///
/// `pub(crate)` so inline tests in `lifecycle::tests` can exercise it
/// directly via `super::find_kind_by_keywords` and `task_create.rs` /
/// other callers can reuse the keyword-match primitive without
/// re-implementing the scan.
pub(crate) fn find_kind_by_keywords(lower: &str) -> Option<&'static str> {
    SUBJECT_KEYWORD_MAP
        .iter()
        .find(|(kw, _)| lower.contains(kw))
        .map(|(_, kind)| *kind)
}

// ── D10: cross-cutting helpers moved from lifecycle.rs ────────────────────────

/// R18-S1 + R26-S2: Persist task creation data to Tantivy + memory store (CC≤5).
///
/// Extracted from `handle_task_sync_post_create` to keep that function's CC≤15.
/// Upserts 3 subtask DAG entries to Tantivy (discoverable via BM25 search) and
/// stores a creation lesson so the full task lifecycle is queryable:
///   created (R26-S2) → completed (R23-S2) → deleted (R25-S1)
pub(crate) fn persist_task_creation(
    rt: &mut crate::runtime::HookRuntime,
    task_id: &str,
    task_subject: &str,
    s1: &str,
    s2: &str,
    s3: &str,
) {
    // Tantivy upsert — each subtask becomes a searchable task_dag SymbolDoc.
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::tantivy_for(Some(&rt.project_root)) {
            let subject_snippet = if task_subject.is_empty() {
                String::new()
            } else {
                task_subject[..task_subject.len().min(120)].to_string()
            };
            for (subtask_id, description) in [
                (
                    s1,
                    "scout: research context, blast radius, and wiring before changes",
                ),
                (
                    s2,
                    "implement: apply changes with VGP verification and speculative validation",
                ),
                (
                    s3,
                    "validate: cargo test + wiring orphans + memory store lesson",
                ),
            ] {
                let doc = crate::tantivy_index::SymbolDoc {
                    symbol_name: subtask_id.to_string(),
                    file_path: format!("task_dag:{task_id}"),
                    symbol_kind: "task_dag".to_string(),
                    module_path: Some(format!("task:{task_id}")),
                    docstring: Some(if subject_snippet.is_empty() {
                        description.to_string()
                    } else {
                        format!("{subject_snippet} — {description}")
                    }),
                    line_number: 0,
                    language: "task".to_string(),
                    visibility: None,
                    crate_name: None,
                    blake3_hash: None,
                    import_count: None,
                    export_count: None,
                    cognitive_score: None,
                    functional_signature: None,
                    community_id: None,
                };
                let _ = idx.upsert_symbol(&doc);
            }
            let _ = idx.commit();
            tracing::debug!(
                task_id,
                "task DAG scaffold upserted to Tantivy (3 subtasks)"
            );
        }
    }
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = (s1, s2, s3);

    // Memory store — persist task creation for full lifecycle audit.
    let subject_snippet = if task_subject.is_empty() {
        String::from("(no subject)")
    } else {
        task_subject[..task_subject.len().min(120)].to_string()
    };
    let _ = crate::cli_handlers::cli_memory_store(
        rt,
        &serde_json::json!({
            "key": format!("task:{task_id}:created"),
            "value": format!("Task {task_id} created via Claude Code — subject: {subject_snippet} | DAG scaffold: scout→implement→validate registered"),
            "tier": "semantic",
            "entry_type": "lesson",
        }),
    );
    tracing::debug!(task_id, "task creation persisted to memory (R26-S2)");
}

/// R30-S1: Emit a ready-to-paste GeneratorPlan JSON stub when subject maps to a GeneratorKind (CC≤3).
///
/// Returns a compact `plan.json` stub the user can save and submit via
/// `touring generate plan-submit --plan-file plan.json`. Closes the CC TaskCreate →
/// GeneratorPlan pipeline: subject → keyword match → concrete stub → immediate submission.
/// Returns `None` when subject is empty or no keyword matches.
pub(crate) fn plan_scaffold_for_subject(subject: &str, task_id: &str) -> Option<String> {
    let lower = subject.to_lowercase();
    let kind = find_kind_by_keywords(&lower)?;
    let intent = &subject[..subject.len().min(80)];
    let module_name = intent
        .split_whitespace()
        .next()
        .unwrap_or("module")
        .to_lowercase();
    let stub = serde_json::json!({
        "kind": kind,
        "intent": intent,
        "variables": {"module_name": &module_name, "task_id": task_id},
        "output_path": format!("./generated/{kind}/{module_name}.rs"),
    });
    let stub_str = serde_json::to_string(&stub).unwrap_or_default();
    if stub_str.is_empty() {
        return None;
    }
    Some(format!(
        "generator-plan: save as plan.json → `touring generate plan-submit --plan-file plan.json` | stub: {stub_str}"
    ))
}
