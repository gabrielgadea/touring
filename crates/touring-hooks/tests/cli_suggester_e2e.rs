//! Cross-audit E2E — Session 2026-05-10 (touring-hooks surface).
//!
//! Asserts the behavioural contracts of:
//!
//! 1. **`cli_suggester::run`** — the `cli-suggest` PreToolUse handler returns
//!    valid JSON for every (tool_name, tool_input) shape Claude Code emits.
//!    Coverage spans `Bash` / `Grep` / `Glob` / `Read` / `Edit` / `Write` and
//!    explicitly checks the silent-path contract (non-code Read returns "{}").
//!
//! 2. **`hook_registry` wiring** — `cli-suggest` (added 2026-05-10) and
//!    `cli-index-ingest` (B3 fix) are both present in `ALL_DAEMON_HOOK_NAMES`
//!    and in the dispatch table built by `build_dispatch_table`.
//!
//! 3. **JSON output schema invariant** — every non-empty response is a valid
//!    JSON object with a `hookSpecificOutput.additionalContext: String` field;
//!    every empty response is exactly `"{}"`. Claude Code's strict
//!    hook-schema validator rejects anything else.
//!
//! 4. **Confidence gate** — inputs that should not produce a suggestion
//!    (unknown tool, missing field, non-code file) emit `"{}"` rather than
//!    a low-confidence suggestion.
//!
//! 5. **Anti-pattern detection** — the classifier promotes safer Touring
//!    commands for high-cost shell patterns (`sed -i`, `git`, `cargo build`,
//!    `cat *.rs`, raw `grep PascalCase`).

use serde_json::{Value, json};
use tempfile::TempDir;

use touring_hooks::cli_suggester;
use touring_hooks::runtime::HookRuntime;

/// Build a throw-away `HookRuntime` rooted at a tempdir. The cli_suggester
/// only queries read-only state (FileKnowledge / SymbolStore) — a fresh
/// runtime is empty but functional, and that's exactly what we want for the
/// classifier paths (no enrichment data to leak between tests).
fn make_runtime() -> (TempDir, HookRuntime) {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".claude/data")).expect("mkdir");
    let _build = RUNTIME_BUILD.lock().unwrap_or_else(|e| e.into_inner());
    let rt = HookRuntime::new(tmp.path()).expect("hook runtime init");
    (tmp, rt)
}

/// Serializa a CONSTRUÇÃO do runtime — e só ela.
///
/// Capturado sob gdb em 24/08/2026: com as 13 threads de teste construindo
/// runtimes ao mesmo tempo, todas paravam em `pthread_mutex_lock` dentro de
/// `findReusableFd` → `unixOpen`, o mutex global do VFS unix do SQLite, na
/// abertura de `RlmMemory::new` (`rl/memory/rlm.rs:204`) sob
/// `build_evolution_analyzer`. Cada `HookRuntime::new` abre vários bancos; treze
/// aberturas simultâneas empilhavam nesse mutex e o processo não saía mais.
///
/// Serializar aqui NÃO mascara defeito de produção: fora de teste,
/// `HookRuntime::new` é chamado uma vez por processo (o único sítio produtivo é
/// um comando CLI single-shot), então "N runtimes ao mesmo tempo" é uma forma
/// que só o harness produz. O que estes testes exercitam é `cli_suggester::run`,
/// e o lock é solto antes dele — a concorrência que importa continua valendo.
///
/// Distinto do outro travamento desta suíte, esse sim de produção: o `Drop` de
/// conexão de escrita pedindo lock exclusivo em `sqlite3WalClose`, corrigido no
/// lado do produto por `open_lessons_db_readonly`.
static RUNTIME_BUILD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Parse the raw JSON string returned by `cli_suggester::run`, returning the
/// `additionalContext` text when present (None for the `"{}"` empty case).
fn additional_context(output: &str) -> Option<String> {
    let v: Value = serde_json::from_str(output).expect("output is valid JSON");
    if v.as_object().map(|o| o.is_empty()).unwrap_or(false) {
        return None;
    }
    v.get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
}

// ── (1) Per-tool classifier coverage ─────────────────────────────────────────

#[test]
fn classifier_grep_pascalcase_emits_symbol_lookup() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Grep",
        "tool_input": { "pattern": "DomainCircuitBreaker" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("symbol-lookup"), "cluster missing: {ctx}");
    // Uma asserção sem diagnóstico só diz que algo mudou, nunca o quê — e foi
    // exatamente isso que atrasou o diagnóstico do flaky de 24/08/2026: a falha
    // apontava para a linha e escondia o contexto que a explicava.
    assert!(
        ctx.contains("touring index find DomainCircuitBreaker"),
        "MUST sem `index find` do símbolo: {ctx}"
    );
    assert!(
        ctx.contains("touring wiring impact"),
        "MUST sem `wiring impact`: {ctx}"
    );
}

#[test]
fn classifier_grep_free_text_routes_to_tantivy() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Grep",
        "tool_input": { "pattern": "TODO fix the thing" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("free-text-search"), "cluster wrong: {ctx}");
    assert!(ctx.contains("tantivy search"));
}

#[test]
fn classifier_bash_sed_inplace_promotes_taco_forge_perfect_edit() {
    let (_tmp, rt) = make_runtime();
    // session_id próprio: os caches de gate (G3/G5 streak) são por sessão e
    // globais ao processo — sem ele, um Edit de OUTRO teste faz o G5 falar
    // aqui em vez do classificador (flaky que troca de vítima).
    let payload = json!({
        "tool_name": "Bash",
        "session_id": "e2e-sed",
        "tool_input": { "command": "sed -i 's/old/new/g' foo.rs" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(
        ctx.contains("anti-pattern-bash-edit"),
        "cluster wrong: {ctx}"
    );
    assert!(ctx.contains("Edit tool"));
}

/// REGRA #11 v2: git de leitura é PERMITIDO, nada é exigido, e o hook cala.
///
/// Este teste nasceu afirmando o oposto — que um arm `regra-11-git-safe`
/// seria emitido. Ele falhou no gate de convergência de 26/08 e revelou que
/// aquele arm tinha confiança 0.55, abaixo do gate conformal
/// (`LEGACY_THRESHOLD = 0.7`): era código que nunca executou. O arm foi
/// removido; o contrato REAL é o silêncio, e é o que se afirma aqui.
///
/// O par unitário (`classify_bash_...`) não podia pegar isso: ele chama
/// `classify_bash` DIRETO, antes do gate. Só o caminho E2E atravessa
/// `select_classifier`, que é onde a supressão acontece.
#[test]
fn classifier_bash_git_readonly_stays_silent() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Bash",
        "session_id": "e2e-git",
        "tool_input": { "command": "git log --oneline" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).unwrap_or_default();
    assert!(
        !ctx.contains("prohibited"),
        "REGRA #11 v2 revogou a proibição; o nudge não pode reivindicá-la: {ctx}"
    );
    assert!(
        ctx.is_empty(),
        "git de leitura não exige ação — o hook deve calar, não emitir: {ctx}"
    );
}

#[test]
fn classifier_bash_git_destructive_reaches_the_ritual_arm() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Bash",
        "session_id": "e2e-git-destructive",
        "tool_input": { "command": "git reset --hard HEAD~1" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(
        ctx.contains("regra-11-git-destructive"),
        "cluster wrong: {ctx}"
    );
    assert!(
        ctx.contains("GIT_DESTRUCTIVE_OK=1"),
        "the ritual token must travel: {ctx}"
    );
}

#[test]
fn classifier_bash_cargo_routes_to_doctor() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Bash",
        "session_id": "e2e-cargo",
        "tool_input": { "command": "cargo build -p touring-core --release" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(
        ctx.contains("system-health-precheck"),
        "cluster wrong: {ctx}"
    );
    assert!(ctx.contains("touring doctor"));
}

#[test]
fn classifier_read_rust_emits_rust_semantic_and_tdg() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "crates/foo/src/lib.rs" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("read-rust-comprehend"));
    assert!(ctx.contains("rust-semantic"));
    assert!(ctx.contains("touring ast tdg"));
}

#[test]
fn classifier_write_tsx_routes_to_perfect_create_tsx() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "src/components/Button.tsx" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("new-reactcomponent"), "cluster wrong: {ctx}");
    assert!(ctx.contains("Write tool"));
}

#[test]
fn classifier_edit_rust_emits_pre_edit_triage_with_tdg_gate() {
    let (_tmp, rt) = make_runtime();
    // O pipeline tem os gates ANTES do classificador: um Edit sem Read recente
    // leva o advisory G3 e o classificador nunca fala. O Read satisfaz o gate;
    // o objeto deste teste é o classificador de Edit.
    let read = json!({
        "tool_name": "Read",
        "session_id": "e2e-edit-rust",
        "tool_input": { "file_path": "crates/foo/src/lib.rs" }
    });
    let _ = cli_suggester::run(&rt, &read);
    let payload = json!({
        "tool_name": "Edit",
        "session_id": "e2e-edit-rust",
        "tool_input": { "file_path": "crates/foo/src/lib.rs" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("pre-edit-triage-rust"));
    assert!(ctx.contains("touring ast tdg"));
    assert!(ctx.contains("STOP at grade D/F") || ctx.contains("STOP at TDG D/F"));
}

#[test]
fn classifier_glob_rust_pattern_suggests_workspace_info() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Glob",
        "tool_input": { "pattern": "**/*.rs" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("file-enumeration"));
    assert!(ctx.contains("touring index files"));
}

// ── (2) Silent path — empty response when no high-confidence suggestion ──────

#[test]
fn classifier_silent_for_non_code_read() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "README.md" }
    });
    let out = cli_suggester::run(&rt, &payload);
    assert_eq!(out, "{}", "non-code Read must emit empty {{}}");
}

#[test]
fn classifier_silent_for_unknown_tool() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "FuturisticToolFromMars",
        "tool_input": { "some": "value" }
    });
    let out = cli_suggester::run(&rt, &payload);
    assert_eq!(out, "{}");
}

#[test]
fn classifier_silent_for_missing_tool_name() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({ "tool_input": { "x": 1 } });
    let out = cli_suggester::run(&rt, &payload);
    assert_eq!(out, "{}");
}

// ── (3) Output schema invariant ──────────────────────────────────────────────

#[test]
fn every_non_empty_output_is_a_valid_json_object_with_additional_context() {
    let (_tmp, rt) = make_runtime();
    let payloads = vec![
        json!({"tool_name": "Grep", "tool_input": {"pattern": "FooBar"}}),
        json!({"tool_name": "Bash", "tool_input": {"command": "sed -i 's/a/b/' x.rs"}}),
        json!({"tool_name": "Bash", "tool_input": {"command": "cargo test"}}),
        json!({"tool_name": "Read", "tool_input": {"file_path": "x.py"}}),
        json!({"tool_name": "Edit", "tool_input": {"file_path": "x.ts"}}),
        json!({"tool_name": "Write", "tool_input": {"file_path": "x.rs"}}),
    ];
    for p in payloads {
        let out = cli_suggester::run(&rt, &p);
        let v: Value = serde_json::from_str(&out)
            .unwrap_or_else(|e| panic!("invalid JSON for {p}: {e}\nout={out}"));
        assert!(v.is_object(), "must be object: {out}");
        if !v.as_object().expect("obj").is_empty() {
            let h = v
                .get("hookSpecificOutput")
                .unwrap_or_else(|| panic!("missing hookSpecificOutput in {out}"));
            assert_eq!(
                h.get("hookEventName").and_then(|x| x.as_str()),
                Some("PreToolUse")
            );
            assert!(
                h.get("additionalContext")
                    .and_then(|x| x.as_str())
                    .is_some()
            );
        }
    }
}

// ── (4) Hook registry wiring — cli-suggest + cli-index-ingest both present ──

#[test]
fn hook_registry_contains_session_handlers() {
    use touring_hooks::hook_registry::ALL_DAEMON_HOOK_NAMES;
    assert!(
        ALL_DAEMON_HOOK_NAMES.contains(&"cli-suggest"),
        "cli-suggest missing from ALL_DAEMON_HOOK_NAMES"
    );
    assert!(
        ALL_DAEMON_HOOK_NAMES.contains(&"cli-index-ingest"),
        "cli-index-ingest missing from ALL_DAEMON_HOOK_NAMES"
    );
}

#[test]
fn hook_registry_dispatch_table_routes_session_handlers() {
    let dispatch = touring_hooks::hook_registry::build_dispatch_table();
    assert!(
        dispatch.contains_key("cli-suggest"),
        "cli-suggest missing from dispatch table"
    );
    assert!(
        dispatch.contains_key("cli-index-ingest"),
        "cli-index-ingest missing from dispatch table"
    );
}

// ── (5) Disable-via-env contract ─────────────────────────────────────────────

/// A env de desligamento é exercitada num SUBPROCESSO, nunca neste.
///
/// MEDIDO em 24/08/2026: esta suíte era flaky (6 falhas em 15 execuções), e a
/// vítima trocava a cada rodada — ora `classifier_bash_cargo_routes_to_doctor`,
/// ora `classifier_grep_pascalcase_emits_symbol_lookup`, sempre com a saída
/// vazia. A causa era esta função: ela fazia `set_var` e, **variável de ambiente
/// é global ao processo, não à thread**, todo teste que rodasse dentro dessa
/// janela via o suggester desligado e recebia `"{}"`.
///
/// O `ENV_LOCK` que existia aqui não podia cobrir isso: ele serializava apenas
/// os testes que MUTAVAM a env, e os 16 que apenas a LEEM nunca o pegavam. O
/// comentário de segurança anterior declarava a premissa "requires
/// single-threaded execution" — que `cargo test` não garante e a suíte não
/// pedia; o texto afirmava um isolamento que o ambiente não dava. Rodar com
/// `--test-threads=1` passava 20/20 e era exatamente essa a pista.
///
/// Reexecutar o próprio binário preserva o e2e real (a env é lida pelo código
/// de produção, não simulada) e devolve a mutação para um processo onde ela é a
/// única coisa acontecendo.
#[test]
fn classifier_respects_disable_env_var() {
    let exe = std::env::current_exe().expect("caminho do binário de teste");
    let status = std::process::Command::new(exe)
        .args(["--exact", "disable_env_var_worker", "--test-threads=1"])
        .env("TOURING_SUGGESTER_DISABLED", "1")
        .status()
        .expect("spawn do worker");
    assert!(
        status.success(),
        "com TOURING_SUGGESTER_DISABLED=1 o worker deveria observar o classificador silenciado"
    );
}

/// Metade executora de [`classifier_respects_disable_env_var`].
///
/// Só tem o que provar quando o pai o invoca com a env armada; na varredura
/// normal da suíte ele não encontra a env e retorna sem asserção — de propósito,
/// já que armá-la aqui reintroduziria exatamente a corrida descrita acima.
#[test]
fn disable_env_var_worker() {
    if std::env::var("TOURING_SUGGESTER_DISABLED").as_deref() != Ok("1") {
        return;
    }
    let (_tmp, rt) = make_runtime();
    let out = cli_suggester::run(
        &rt,
        &json!({"tool_name": "Grep", "tool_input": {"pattern": "DomainCircuitBreaker"}}),
    );
    assert_eq!(out, "{}", "DISABLED env var must silence the classifier");
}

// ── (6) TTL cache de-duplicates identical inputs in the same process ─────────

#[test]
fn classifier_ttl_cache_suppresses_duplicate_input_in_same_process() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Grep",
        "tool_input": { "pattern": "UniqueAuditSymbolName_E2E" }
    });

    let first = cli_suggester::run(&rt, &payload);
    assert_ne!(first, "{}", "first call must emit");

    let second = cli_suggester::run(&rt, &payload);
    assert_eq!(
        second, "{}",
        "second call with identical input must hit the TTL cache"
    );

    // A different pattern bypasses the cache.
    let different = cli_suggester::run(
        &rt,
        &json!({"tool_name": "Grep", "tool_input": {"pattern": "AnotherSymbol_E2E"}}),
    );
    assert_ne!(different, "{}", "different input must NOT hit the cache");
}

