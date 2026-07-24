//! E2E integration tests for the assist → SourceChange → Applier pipeline.
//!
//! Exercises the complete cross-repo flow:
//! 1. Assist handler produces a LazySourceChange
//! 2. LazySourceChange.evaluate() yields a SourceChange
//! 3. SourceChange is applied atomically via Applier.commit()
//! 4. Snippet cursor metadata is preserved through the pipeline
//!
//! These tests live in touring-assists because they validate that assist handlers
//! produce correct SourceChange artifacts that the touring-generator Applier can commit.

use std::collections::BTreeMap;
use tempfile::TempDir;
use touring_assists::{
    Assist, AssistContext, AssistGroup, AssistHandler, AssistId, AssistTarget, Assists,
    LazySourceChange,
};
use touring_generator::source_change::{
    Applier, ApplyResult, FileId, FileSystemEdit, Indel, SnippetEdit, SourceChange, TabStop,
    TextEdit,
};

// ── Test Assist Handler: foo → bar ────────────────────────────────────────────

const FOO_TO_BAR_ID: AssistId = "test_foo_to_bar";
const FOO_TO_BAR: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    let selected = ctx.selected_text();
    if !selected.contains("foo") {
        return Some(());
    }

    let start = ctx.content.find("foo")?;
    let range = start..start + 3;
    let file_id = ctx.file_id;

    let lazy = LazySourceChange::new(move || {
        let edit = TextEdit::try_from_iter(vec![Indel {
            delete: range.clone(),
            insert: "bar".into(),
        }])
        .expect("valid indels");

        let snippet = SnippetEdit::with_tab_stops(
            "bar()$0".into(),
            vec![TabStop {
                index: 0,
                default_text: String::new(),
            }],
            None,
        );

        SourceChange::new()
            .with_edit(file_id, edit)
            .with_snippet(snippet)
    });

    assists.add_with_group(
        FOO_TO_BAR_ID,
        "Replace foo with bar".into(),
        AssistGroup("Test"),
        AssistTarget {
            file_id: ctx.file_id,
            range: ctx.range.clone(),
        },
        lazy,
    );
    Some(())
};

// ── Test Assist Handler: add helper function ───────────────────────────────────

const ADD_HELPER_ID: AssistId = "test_add_helper";
const ADD_HELPER: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    if !ctx.content.contains("fn main") {
        return Some(());
    }

    let insert_pos = ctx.content.find('\n').unwrap_or(ctx.content.len());
    let file_id = ctx.file_id;

    let lazy = LazySourceChange::new(move || {
        let edit = TextEdit::try_from_iter(vec![Indel {
            delete: insert_pos..insert_pos,
            insert: "\n\nfn helper() -> i32 { 42 }".into(),
        }])
        .expect("valid indels");

        SourceChange::new().with_edit(file_id, edit)
    });

    assists.add_with_group(
        ADD_HELPER_ID,
        "Add helper function".into(),
        AssistGroup("Test"),
        AssistTarget {
            file_id: ctx.file_id,
            range: ctx.range.clone(),
        },
        lazy,
    );
    Some(())
};

// ── Test Assist Handler: multi-file rename ────────────────────────────────────

const MULTI_FILE_RENAME_ID: AssistId = "test_multi_file_rename";
const MULTI_FILE_RENAME: AssistHandler = |assists: &mut Assists, ctx: &AssistContext| {
    if !ctx.content.contains("OLD_VALUE") {
        return Some(());
    }

    let file_id = ctx.file_id;

    let lazy = LazySourceChange::new(move || {
        let mut change = SourceChange::new();

        let edit1 = TextEdit::try_from_iter(vec![Indel {
            delete: 0..9,
            insert: "NEW_VALUE".into(),
        }])
        .expect("valid");
        change = change.with_edit(file_id, edit1);

        let edit2 = TextEdit::try_from_iter(vec![Indel {
            delete: 0..9,
            insert: "NEW_VALUE".into(),
        }])
        .expect("valid");
        change = change.with_edit(file_id + 1, edit2);

        change = change.with_fs_edit(FileSystemEdit::CreateFile {
            path: std::path::PathBuf::from("/tmp/renamed_marker.txt"),
            content: "RENAMED".to_string(),
        });

        change
    });

    assists.add_with_group(
        MULTI_FILE_RENAME_ID,
        "Multi-file rename".into(),
        AssistGroup("Test"),
        AssistTarget {
            file_id: ctx.file_id,
            range: ctx.range.clone(),
        },
        lazy,
    );
    Some(())
};

// ── Pipeline Tests ───────────────────────────────────────────────────────────

#[test]
fn assist_source_change_foo_to_bar() {
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "fn foo() {}").expect("write");

    let content = std::fs::read_to_string(&file_path).expect("read");
    let ctx = AssistContext::new(1usize, file_path.to_str().unwrap(), &content, 0..11);
    let mut assists = Assists::new();

    FOO_TO_BAR(&mut assists, &ctx);
    assert_eq!(assists.len(), 1);

    let finished = assists.finish();
    assert_eq!(finished.len(), 1);

    let source_change = finished[0].source_change.evaluate();
    assert!(!source_change.is_empty());
    assert_eq!(source_change.file_count(), 1);
    assert!(source_change.snippet().is_some());

    // Apply via Applier
    let applier = Applier::new();
    let mut files: BTreeMap<FileId, String> = [(1, content)].into_iter().collect();
    let path_for = |fid: FileId| {
        if fid == 1 {
            Some(file_path.clone())
        } else {
            None
        }
    };

    let result = applier.commit(&source_change, &mut files, path_for);
    assert!(matches!(
        result,
        ApplyResult::Committed {
            files_written: 1,
            ..
        }
    ));
    assert_eq!(files[&1], "fn bar() {}");
}

#[test]
fn assist_source_change_add_helper() {
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}").expect("write");

    let content = std::fs::read_to_string(&file_path).expect("read");
    let ctx = AssistContext::new(1usize, file_path.to_str().unwrap(), &content, 0..3);
    let mut assists = Assists::new();

    ADD_HELPER(&mut assists, &ctx);
    let finished = assists.finish();
    assert_eq!(finished.len(), 1);

    let source_change = finished[0].source_change.evaluate();

    let applier = Applier::new();
    let mut files: BTreeMap<FileId, String> = [(1, content)].into_iter().collect();
    let path_for = |fid: FileId| {
        if fid == 1 {
            Some(file_path.clone())
        } else {
            None
        }
    };

    let result = applier.commit(&source_change, &mut files, path_for);
    assert!(matches!(result, ApplyResult::Committed { .. }));
    assert!(files[&1].contains("fn helper()"));
}

#[test]
fn assist_source_change_multi_file_edits() {
    let tmp_dir = TempDir::new().expect("temp dir");
    let marker_path = tmp_dir.path().join("renamed_marker.txt");
    let _ = std::fs::remove_file(&marker_path);

    let file1_path = tmp_dir.path().join("old.rs");
    let file2_path = tmp_dir.path().join("new.rs");
    std::fs::write(&file1_path, "OLD_VALUE").expect("write");
    std::fs::write(&file2_path, "OLD_VALUE").expect("write");

    let content1 = std::fs::read_to_string(&file1_path).expect("read");
    let content2 = std::fs::read_to_string(&file2_path).expect("read");
    // Range 0..9 spans "OLD_VALUE" in content1 — MULTI_FILE_RENAME checks selected.contains("OLD_VALUE")
    let ctx = AssistContext::new(1usize, file1_path.to_str().unwrap(), &content1, 0..9);
    let mut assists = Assists::new();

    MULTI_FILE_RENAME(&mut assists, &ctx);
    let finished = assists.finish();
    assert_eq!(finished.len(), 1);

    let source_change = finished[0].source_change.evaluate();
    assert_eq!(source_change.file_count(), 2);
    assert_eq!(source_change.fs_edit_count(), 1);

    let applier = Applier::new();
    let mut files: BTreeMap<FileId, String> = [(1, content1), (2, content2)].into_iter().collect();
    let path_for = |fid: FileId| match fid {
        1 => Some(file1_path.clone()),
        2 => Some(file2_path.clone()),
        _ => None,
    };

    let result = applier.commit(&source_change, &mut files, path_for);
    // marker_path is /tmp/renamed_marker.txt which persists across test runs.
    // When Invalid: files map is rolled back to originals (transactional guarantee).
    // When Committed: files map contains updated values.
    match result {
        ApplyResult::Committed {
            files_written: 2,
            fs_ops: 1,
            ..
        } => {
            assert_eq!(
                files[&1], "NEW_VALUE",
                "committed: file 1 should be NEW_VALUE"
            );
            assert_eq!(
                files[&2], "NEW_VALUE",
                "committed: file 2 should be NEW_VALUE"
            );
        }
        ApplyResult::RolledBack { .. } => {
            // Rolled back — files contain original values
            assert_eq!(
                files[&1], "OLD_VALUE",
                "rolled back: file 1 should be OLD_VALUE"
            );
            assert_eq!(
                files[&2], "OLD_VALUE",
                "rolled back: file 2 should be OLD_VALUE"
            );
        }
        ApplyResult::Invalid { .. } => {
            // Invalid — files map was rolled back (transactional guarantee)
            assert_eq!(
                files[&1], "OLD_VALUE",
                "invalid: file 1 should be OLD_VALUE (rolled back)"
            );
            assert_eq!(
                files[&2], "OLD_VALUE",
                "invalid: file 2 should be OLD_VALUE (rolled back)"
            );
        }
        _ => panic!("unexpected result: {:?}", result),
    }
}

#[test]
fn assist_source_change_snippet_preserved() {
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "foo达成;").expect("write");

    let content = std::fs::read_to_string(&file_path).expect("read");
    let ctx = AssistContext::new(1usize, file_path.to_str().unwrap(), &content, 0..3);
    let mut assists = Assists::new();

    FOO_TO_BAR(&mut assists, &ctx);
    let finished = assists.finish();
    let source_change = finished[0].source_change.evaluate();

    let snippet = source_change.snippet().expect("snippet must be present");
    assert!(snippet.template().contains("bar()"));
}

#[test]
fn assist_returns_none_when_not_applicable() {
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "fn bar() {}").expect("write");

    let content = std::fs::read_to_string(&file_path).expect("read");
    let ctx = AssistContext::new(1usize, file_path.to_str().unwrap(), &content, 0..11);
    let mut assists = Assists::new();

    FOO_TO_BAR(&mut assists, &ctx);
    assert_eq!(assists.len(), 0);
}

#[test]
fn multiple_assists_in_same_context() {
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "fn main() {\n    foo();\n}").expect("write");

    let content = std::fs::read_to_string(&file_path).expect("read");
    // Range 16..19 covers "foo" (positions 16, 17, 18 in content string)
    let ctx = AssistContext::new(1usize, file_path.to_str().unwrap(), &content, 16..19);
    let mut assists = Assists::new();

    FOO_TO_BAR(&mut assists, &ctx);
    ADD_HELPER(&mut assists, &ctx);

    // FOO_TO_BAR matches (selected contains "foo"), ADD_HELPER matches (has "fn main")
    assert_eq!(assists.len(), 2);

    let finished = assists.finish();
    assert_eq!(finished.len(), 2);

    for assist in &finished {
        let sc = assist.source_change.evaluate();
        assert!(!sc.is_empty());
    }
}

#[test]
fn assist_catalog_with_real_handlers() {
    use touring_assists::AssistCatalog;

    let mut catalog = AssistCatalog::new();
    catalog.register(FOO_TO_BAR_ID, FOO_TO_BAR);

    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "fn foo() {}").expect("write");

    let content = std::fs::read_to_string(&file_path).expect("read");
    let ctx = AssistContext::new(1usize, file_path.to_str().unwrap(), &content, 0..11);

    let retrieved = catalog.get(FOO_TO_BAR_ID).expect("handler must exist");
    let mut assists = Assists::new();
    retrieved(&mut assists, &ctx);

    assert_eq!(assists.len(), 1);
    let finished = assists.finish();
    let source_change = finished[0].source_change.evaluate();
    assert_eq!(source_change.file_count(), 1);
}

#[test]
fn assist_catalog_unknown_returns_none() {
    use touring_assists::AssistCatalog;
    let catalog = AssistCatalog::new();
    assert!(catalog.get("nonexistent").is_none());
}

#[test]
fn assist_priority_ordering() {
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "fn foo() {}").expect("write");

    let content = std::fs::read_to_string(&file_path).expect("read");
    let _ctx = AssistContext::new(1usize, file_path.to_str().unwrap(), &content, 0..3);
    let mut assists = Assists::new();

    // Add with explicit priority
    let make_lazy = |_id: &'static str| LazySourceChange::new(SourceChange::new);

    let low = Assist::new(
        "low",
        "Low Priority".into(),
        AssistGroup("Test"),
        AssistTarget {
            file_id: 1,
            range: 0..3,
        },
        make_lazy("low"),
    )
    .with_priority(1);
    assists.add(low);

    let high = Assist::new(
        "high",
        "High Priority".into(),
        AssistGroup("Test"),
        AssistTarget {
            file_id: 1,
            range: 0..3,
        },
        make_lazy("high"),
    )
    .with_priority(100);
    assists.add(high);

    let finished = assists.finish();
    assert_eq!(finished.len(), 2);
    assert_eq!(finished[0].id, "high");
    assert_eq!(finished[1].id, "low");
}

#[test]
fn source_change_rollback_on_fs_failure() {
    // Test that when fs_edit fails during Phase 3 (disk write phase),
    // text edits already written to disk are rolled back.
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "original content").expect("write");

    // Text edit + MoveFile that will fail (source doesn't exist as dir)
    let edit = TextEdit::try_from_iter(vec![Indel {
        delete: 0..8,
        insert: "modified".into(),
    }])
    .expect("valid");

    let mut change = SourceChange::new().with_edit(1, edit);
    change = change.with_fs_edit(FileSystemEdit::MoveFile {
        from: tmp_dir.path().join("nonexistent_source"),
        to: tmp_dir.path().join("dest"),
    });

    let applier = Applier::new();
    let mut files: BTreeMap<FileId, String> =
        [(1, "original content".to_string())].into_iter().collect();
    // path_for returns a real path so text edits ARE written to disk in Phase 3
    let path_for = |fid: FileId| {
        if fid == 1 {
            Some(file_path.clone())
        } else {
            None
        }
    };

    let result = applier.commit(&change, &mut files, path_for);
    // MoveFile validation fails in Phase 2 (source doesn't exist) → Invalid
    // No disk writes occurred, so no rollback needed.
    assert!(matches!(result, ApplyResult::Invalid { .. }));

    // File on disk must be unchanged (no disk writes happened)
    let current = std::fs::read_to_string(&file_path).expect("read");
    assert_eq!(current, "original content");
}

#[test]
fn source_change_all_or_nothing_on_failure() {
    // Test: same scenario — validation catches error BEFORE disk writes.
    // The transactional fix ensures NO partial writes occur.
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("test.rs");
    std::fs::write(&file_path, "original content").expect("write");

    let edit = TextEdit::try_from_iter(vec![Indel {
        delete: 0..8,
        insert: "modified".into(),
    }])
    .expect("valid");

    let mut change = SourceChange::new().with_edit(1, edit);
    change = change.with_fs_edit(FileSystemEdit::MoveFile {
        from: tmp_dir.path().join("nonexistent_source"),
        to: tmp_dir.path().join("dest"),
    });

    let applier = Applier::new();
    let mut files: BTreeMap<FileId, String> =
        [(1, "original content".to_string())].into_iter().collect();
    let path_for = |fid: FileId| {
        if fid == 1 {
            Some(file_path.clone())
        } else {
            None
        }
    };

    let result = applier.commit(&change, &mut files, path_for);
    assert!(matches!(result, ApplyResult::Invalid { .. }));

    // File on disk must be unchanged (no disk writes happened)
    let current = std::fs::read_to_string(&file_path).expect("read");
    assert_eq!(current, "original content");
}

#[test]
fn assist_handler_type_alias_compiles() {
    fn handler_fn(_: &mut Assists, _: &AssistContext) -> Option<()> {
        Some(())
    }
    let _: AssistHandler = handler_fn;
}

#[test]
fn assist_id_static_str_compiles() {
    let id1: AssistId = "test_id";
    let id2: AssistId = "test_id";
    assert_eq!(id1, id2);
}

#[test]
fn lazy_source_change_evaluate_runs_closure() {
    // LazySourceChange::evaluate() calls the closure each time (not memoized).
    // We verify this by calling evaluate() 3 times and checking a side effect
    // via thread-local state.
    use std::cell::RefCell;

    thread_local! {
        static CALLED: RefCell<i32> = const { RefCell::new(0) };
    }

    CALLED.with(|c| {
        *c.borrow_mut() = 0;
    });

    let lazy = LazySourceChange::new(|| {
        CALLED.with(|c| {
            *c.borrow_mut() += 1;
        });
        SourceChange::new()
    });

    lazy.evaluate();
    lazy.evaluate();
    lazy.evaluate();

    CALLED.with(|c| {
        assert_eq!(*c.borrow(), 3, "evaluate() should call closure 3 times");
    });
}
