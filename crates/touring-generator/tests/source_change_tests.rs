//! Integration tests for `SourceChange` transactional multi-file edits.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Covers:
//! - Multi-file edit commits
//! - `FileSystemEdit` operations (`CreateFile`, `OverwriteFile`, `DeleteFile`, `MoveFile`)
//! - Rollback on failure
//! - Snippet cursor positioning

use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::TempDir;
use touring_generator::source_change::{
    Applier, ApplyResult, FileId, FileSystemEdit, Indel, SnippetEdit, SourceChange, TabStop,
    TextEdit,
};

/// Create a `SourceChange` with text edits for multiple files.
fn make_multi_file_change() -> SourceChange {
    let mut change = SourceChange::new();
    let file_a: FileId = 1;
    let file_b: FileId = 2;

    let edit_a = TextEdit::try_from_iter(vec![Indel {
        delete: 0..5,
        insert: "Goodbye".into(),
    }])
    .expect("valid indels");

    let edit_b =
        TextEdit::try_from_iter(vec![Indel::insert(5, " cruel".into())]).expect("valid indels");

    change = change.with_edit(file_a, edit_a);
    change = change.with_edit(file_b, edit_b);
    change
}

#[test]
fn multi_file_edits_committed() {
    let applier = Applier::new();
    let change = make_multi_file_change();
    let mut files: BTreeMap<FileId, String> = [
        (1, "Hello World".to_string()),
        (2, "Hello Rust".to_string()),
    ]
    .into_iter()
    .collect();

    let path_for = |file_id: FileId| match file_id {
        1 => Some(PathBuf::from("/tmp/file_a.txt")),
        2 => Some(PathBuf::from("/tmp/file_b.txt")),
        _ => None,
    };

    let result = applier.commit(&change, &mut files, path_for);
    assert!(matches!(
        result,
        ApplyResult::Committed {
            files_written: 2,
            fs_ops: 0
        }
    ));
    assert_eq!(files[&1], "Goodbye World");
    assert_eq!(files[&2], "Hello cruel Rust");
}

#[test]
fn fs_edit_create_file() {
    let applier = Applier::new();
    let tmp_dir = TempDir::new().expect("temp dir");
    let new_file_path = tmp_dir.path().join("new_file.txt");

    let change = SourceChange::new().with_fs_edit(FileSystemEdit::CreateFile {
        path: new_file_path.clone(),
        content: "new content".to_string(),
    });

    let result = applier.shadow_validate(&change);
    assert!(matches!(result, ApplyResult::Valid));

    let mut files = BTreeMap::new();
    let result = applier.commit(&change, &mut files, |_| None);
    assert!(matches!(
        result,
        ApplyResult::Committed {
            files_written: 0,
            fs_ops: 1
        }
    ));
    assert!(new_file_path.exists());
    assert_eq!(
        std::fs::read_to_string(&new_file_path).unwrap(),
        "new content"
    );
}

#[test]
fn fs_edit_delete_file() {
    let applier = Applier::new();
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_to_delete = tmp_dir.path().join("delete_me.txt");
    std::fs::write(&file_to_delete, "content to delete").expect("write file");

    let change = SourceChange::new().with_fs_edit(FileSystemEdit::DeleteFile {
        path: file_to_delete.clone(),
    });

    assert!(file_to_delete.exists());
    let mut files = BTreeMap::new();
    let result = applier.commit(&change, &mut files, |_| None);
    assert!(matches!(
        result,
        ApplyResult::Committed {
            files_written: 0,
            fs_ops: 1
        }
    ));
    assert!(!file_to_delete.exists());
}

#[test]
fn fs_edit_overwrite_file() {
    let applier = Applier::new();
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("overwrite.txt");
    std::fs::write(&file_path, "original content").expect("write file");

    let change = SourceChange::new().with_fs_edit(FileSystemEdit::OverwriteFile {
        path: file_path.clone(),
        content: "new content".to_string(),
    });

    let mut files = BTreeMap::new();
    let result = applier.commit(&change, &mut files, |_| None);
    assert!(matches!(
        result,
        ApplyResult::Committed {
            files_written: 0,
            fs_ops: 1
        }
    ));
    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "new content");
}

#[test]
fn fs_edit_move_file() {
    let applier = Applier::new();
    let tmp_dir = TempDir::new().expect("temp dir");
    let from_path = tmp_dir.path().join("from.txt");
    let to_path = tmp_dir.path().join("to.txt");
    std::fs::write(&from_path, "moved content").expect("write file");

    let change = SourceChange::new().with_fs_edit(FileSystemEdit::MoveFile {
        from: from_path.clone(),
        to: to_path.clone(),
    });

    let mut files = BTreeMap::new();
    let result = applier.commit(&change, &mut files, |_| None);
    assert!(matches!(
        result,
        ApplyResult::Committed {
            files_written: 0,
            fs_ops: 1
        }
    ));
    assert!(!from_path.exists());
    assert!(to_path.exists());
    assert_eq!(std::fs::read_to_string(&to_path).unwrap(), "moved content");
}

#[test]
fn rollback_on_fs_edit_failure() {
    let applier = Applier::new();
    // Create a file, then try to move it to a path that will fail
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("file.txt");
    std::fs::write(&file_path, "content").expect("write file");

    let mut change = SourceChange::new();
    // Add a text edit to file 1
    let edit =
        TextEdit::try_from_iter(vec![Indel::insert(0, "modified ".into())]).expect("valid indels");
    change = change.with_edit(1, edit);

    // Add a MoveDir that will fail (from doesn't exist as dir)
    change = change.with_fs_edit(FileSystemEdit::MoveDir {
        from: tmp_dir.path().join("nonexistent_dir"),
        to: tmp_dir.path().join("dest_dir"),
    });

    let mut files: BTreeMap<FileId, String> = [(1, "original".to_string())].into_iter().collect();
    let path_for = |fid: FileId| {
        if fid == 1 {
            Some(file_path.clone())
        } else {
            None
        }
    };

    // MoveDir validation fails in Phase 2 (source doesn't exist) → Invalid.
    // No disk writes occurred, in-memory files rolled back to originals.
    let result = applier.commit(&change, &mut files, path_for);
    assert!(
        matches!(result, ApplyResult::Invalid { .. }),
        "result = {result:?}"
    );

    // Files map should be rolled back to originals
    assert_eq!(files[&1], "original");
    // Disk file should be unchanged (no disk writes happened)
    let disk_content = std::fs::read_to_string(&file_path).expect("read");
    assert_eq!(disk_content, "content");
}

#[test]
fn snippet_cursor_position() {
    let applier = Applier::new();
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("snippet.txt");
    std::fs::write(&file_path, "fn test() {\n    $0\n}").expect("write file");

    let snippet = SnippetEdit::with_tab_stops(
        "fn test() {\n    let x = 42;\n    $0\n}".to_string(),
        vec![TabStop {
            index: 0,
            default_text: String::new(),
        }],
        None,
    );

    let change = SourceChange::new()
        .with_edit(
            1,
            TextEdit::try_from_iter(vec![Indel {
                delete: 13..14,
                insert: "let x = 42;\n    ".into(),
            }])
            .expect("valid indels"),
        )
        .with_snippet(snippet);

    let mut files: BTreeMap<FileId, String> = [(1, "fn test() {\n    $0\n}".to_string())]
        .into_iter()
        .collect();

    let result = applier.commit(&change, &mut files, |_| None);
    assert!(matches!(result, ApplyResult::Committed { .. }));
    assert!(files[&1].contains("let x = 42"));
}

#[test]
fn empty_source_change_commits() {
    let applier = Applier::new();
    let change = SourceChange::new();
    let mut files = BTreeMap::new();

    let result = applier.commit(&change, &mut files, |_| None);
    assert!(matches!(
        result,
        ApplyResult::Committed {
            files_written: 0,
            fs_ops: 0
        }
    ));
}

#[test]
fn shadow_validate_accepts_valid_fs_edits() {
    let applier = Applier::new();
    let tmp_dir = TempDir::new().expect("temp dir");

    let change = SourceChange::new().with_fs_edit(FileSystemEdit::CreateFile {
        path: tmp_dir.path().join("new.txt"),
        content: "hello".to_string(),
    });

    let result = applier.shadow_validate(&change);
    assert!(matches!(result, ApplyResult::Valid));
}

#[test]
fn shadow_validate_rejects_overwrite_nonexistent() {
    let applier = Applier::new();
    let change = SourceChange::new().with_fs_edit(FileSystemEdit::OverwriteFile {
        path: PathBuf::from("/nonexistent/path/file.txt"),
        content: "content".to_string(),
    });

    let result = applier.shadow_validate(&change);
    let errors_vec: Vec<String> = match &result {
        ApplyResult::Invalid { errors } => errors
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        _ => vec![],
    };
    assert!(!errors_vec.is_empty());
}

#[test]
fn commit_with_disk_write_via_path_for() {
    let applier = Applier::new();
    let tmp_dir = TempDir::new().expect("temp dir");
    let file_path = tmp_dir.path().join("written.txt");
    std::fs::write(&file_path, "original").expect("write file");

    let file_id: FileId = 42;
    let edit = TextEdit::try_from_iter(vec![Indel {
        delete: 0..8,
        insert: "modified".into(),
    }])
    .expect("valid indels");

    let change = SourceChange::new().with_edit(file_id, edit);

    let mut files: BTreeMap<FileId, String> =
        [(file_id, "original".to_string())].into_iter().collect();

    let path_for = |fid: FileId| {
        if fid == file_id {
            Some(file_path.clone())
        } else {
            None
        }
    };

    let result = applier.commit(&change, &mut files, path_for);
    assert!(matches!(
        result,
        ApplyResult::Committed {
            files_written: 1,
            ..
        }
    ));
    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "modified");
}
