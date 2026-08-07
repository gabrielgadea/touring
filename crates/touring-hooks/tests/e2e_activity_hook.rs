//! E2E tests for the full activity hook pipeline.
//!
//! KEY INSIGHT — STORE_MAP isolation:
//! `activity_hook::STORE_MAP` é um `LazyLock<RwLock<HashMap>>` no nível do PROCESSO.
//! Cada entry é key=project_root_string → Arc<EventStore>.  Se dois testes
//! usam o MESMO project_root string, eles compartilham o MESMO EventStore.
//!
//! SOLUÇÃO: cada teste usa um project_root DIFERENTE (path único), garantindo
//! que o STORE_MAP nunca conflite entre testes.  A função `project_root(n)`
//! cria um diretório temporário isolado cujo path é único por `n`.

use std::path::PathBuf;
use tempfile::TempDir;
use touring_foundation::activity::event::Event;
use touring_foundation::activity::store::EventStore;

/// Gera um project_root único por teste (path imutável, diretório vivo).
fn project_root(n: &'static str) -> PathBuf {
    static DIRS: std::sync::LazyLock<
        std::sync::RwLock<std::collections::HashMap<&'static str, (TempDir, PathBuf)>>,
    > = std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashMap::new()));

    // Fast path: já foi criado para este n.
    if let Ok(guard) = DIRS.read()
        && let Some((_, path)) = guard.get(n)
    {
        return path.clone();
    }
    // Slow path: criar e registrar.
    let mut guard = DIRS.write().expect("DIRS write");
    if let Some((_, path)) = guard.get(n) {
        return path.clone();
    }
    let dir =
        TempDir::with_prefix(format!("touring-e2e-{}--", n)).expect(&format!("TempDir for {}", n));
    let path = dir.path().to_path_buf();
    std::fs::create_dir_all(path.join(".claude").join("touring")).expect(".claude/touring");
    guard.insert(n, (dir, path.clone()));
    path
}

fn activity_path(n: &'static str) -> PathBuf {
    project_root(n)
        .join(".claude")
        .join("touring")
        .join("activity.jsonl")
}

fn replay(n: &'static str) -> Vec<Event> {
    let store = EventStore::open(activity_path(n)).expect(&format!("open store for {}", n));
    store.replay().expect(&format!("replay for {}", n))
}

fn assert_count(n: &'static str, expected: usize) {
    let events = replay(n);
    assert_eq!(
        events.len(),
        expected,
        "expected {} events in store {}",
        expected,
        n
    );
}

fn assert_single(n: &'static str, hook: &str) {
    let events = replay(n);
    assert_eq!(events.len(), 1, "expected 1 event in {}", n);
    let payload = events[0].payload.as_ref().expect("payload");
    assert_eq!(payload["hook"], hook, "hook name mismatch in {}", n);
    assert!(events[0].verify_projection());
}

fn assert_monotonic(n: &'static str) {
    let events = replay(n);
    for (i, ev) in events.iter().enumerate() {
        assert_eq!(
            ev.seq,
            (i + 1) as u64,
            "seq[{}] in {} should be {}",
            i,
            n,
            i + 1
        );
    }
}

fn capture() -> touring_hooks::shared::gate_metrics::GateMetricsSnapshot {
    touring_hooks::shared::gate_metrics::GateMetricsSnapshot::capture()
}

// ── Five single-event tests ───────────────────────────────────────────────────

#[test]
fn e2e_pre_edit_one_event() {
    touring_hooks::activity_hook::emit_pre_edit(&project_root("pre_edit"), "src/main.rs", 512);
    assert_single("pre_edit", "pre_edit");
}

#[test]
fn e2e_post_edit_one_event() {
    touring_hooks::activity_hook::emit_post_edit(
        &project_root("post_edit"),
        "src/lib.rs",
        "session-xyz",
        3,
    );
    assert_single("post_edit", "post_edit");
}

#[test]
fn e2e_post_write_one_event() {
    touring_hooks::activity_hook::emit_post_write(
        &project_root("post_write"),
        "src/main.rs",
        "rust",
    );
    assert_single("post_write", "post_write");
}

#[test]
fn e2e_instructions_loaded_one_event() {
    touring_hooks::activity_hook::emit_instructions_loaded(
        &project_root("instructions_loaded"),
        "session-abc",
    );
    assert_single("instructions_loaded", "instructions_loaded");
}

#[test]
fn e2e_pre_compact_one_event() {
    touring_hooks::activity_hook::emit_pre_compact(&project_root("pre_compact"), true, false);
    assert_single("pre_compact", "pre_compact");
}

// ── Multi-event + monotonic seq ─────────────────────────────────────────────

#[test]
fn e2e_seq_monotonically_increments() {
    let p = project_root("seq_multi");
    touring_hooks::activity_hook::emit_pre_edit(&p, "a.rs", 100);
    touring_hooks::activity_hook::emit_post_edit(&p, "a.rs", "s1", 2);
    touring_hooks::activity_hook::emit_post_write(&p, "a.rs", "rust");
    assert_count("seq_multi", 3);
    assert_monotonic("seq_multi");
}

// ── Gate metrics counters ─────────────────────────────────────────────────────

#[test]
fn e2e_gate_metrics_counters_increment() {
    let before = capture();
    let p = project_root("metrics");

    touring_hooks::activity_hook::emit_pre_edit(&p, "x.rs", 1);
    touring_hooks::activity_hook::emit_post_edit(&p, "x.rs", "s", 1);
    touring_hooks::activity_hook::emit_post_write(&p, "x.rs", "rs");
    touring_hooks::activity_hook::emit_instructions_loaded(&p, "s");
    touring_hooks::activity_hook::emit_pre_compact(&p, true, false);

    let after = capture();
    // `>=` tolerates concurrent test threads incrementing the same global
    // counters; what we are validating is monotonic-increment-by-at-least-1
    // per emitted event, not exact equality.
    assert!(after.activity_pre_edit_count >= before.activity_pre_edit_count + 1);
    assert!(after.activity_post_edit_count >= before.activity_post_edit_count + 1);
    assert!(after.activity_post_write_count >= before.activity_post_write_count + 1);
    assert!(
        after.activity_instructions_loaded_count >= before.activity_instructions_loaded_count + 1
    );
    assert!(after.activity_pre_compact_count >= before.activity_pre_compact_count + 1);
}

// ── Two independent projects — no interference ────────────────────────────────

#[test]
fn e2e_two_projects_no_interference() {
    // Cada projeto tem seu próprio path → stores completamente isolados.
    let p_a = project_root("proj_a");
    let _p_b = project_root("proj_b");

    // Projeto A escreve 1 evento.
    touring_hooks::activity_hook::emit_pre_edit(&p_a, "a.rs", 1);

    // Projeto B não escreveu nada — arquivo existe mas está vazio.
    assert_count("proj_b", 0);

    // Projeto A continua com exatamente 1 evento.
    assert_count("proj_a", 1);
}
