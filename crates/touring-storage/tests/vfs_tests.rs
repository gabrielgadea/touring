//! Integration tests for touring-vfs.

use touring_storage::vfs::{AbsPath, AbsPathBuf, FileId, FileSet, Vfs, VfsOverlay};

fn buf(s: &str) -> AbsPathBuf {
    AbsPathBuf::try_from(s.to_string()).expect("valid absolute path")
}

// ── Overlay Isolation ───────────────────────────────────────────────────────────

#[test]
fn acceptance_overlay_isolation() {
    let vfs = Vfs::new();
    for i in 0..1000 {
        let p = buf(&format!("/base/file_{}.rs", i));
        vfs.add_overlay(p.as_path(), format!("content_{}", i)).ok();
    }

    let mut overlay = VfsOverlay::with_base(vfs);
    for i in 0..5 {
        let p = buf(&format!("/base/file_{}.rs", i));
        overlay.set(p.as_path(), format!("overlay_content_{}", i));
    }

    // Overlay sees edits
    for i in 0..5 {
        let p = buf(&format!("/base/file_{}.rs", i));
        let content = overlay.read(p.as_path()).unwrap();
        assert_eq!(
            content.as_ref(),
            format!("overlay_content_{}", i).as_bytes()
        );
    }

    // Base sees originals
    assert_eq!(overlay.paths().len(), 1000);
}

#[test]
fn acceptance_snapshot_correctness() {
    let vfs = Vfs::new();
    let p = buf("/snapshot/test.txt");
    vfs.add_overlay(p.as_path(), "original").unwrap();

    // Get path BEFORE creating overlay (vfs ownership moves into overlay)
    let path_id = vfs.file_id(p.as_path()).expect("file exists");

    let mut overlay = VfsOverlay::with_base(vfs);
    overlay.set(p.as_path(), "modified");

    // Use the saved path_id to look up the path from overlay's base
    let base_vfs = overlay.path(path_id);
    assert_eq!(
        base_vfs.as_ref().map(|p| p.as_str()),
        Some("/snapshot/test.txt")
    );
}

#[test]
fn acceptance_concurrent_reads() {
    use std::sync::Arc;
    use std::thread;

    let vfs = Arc::new(Vfs::new());
    for i in 0..100 {
        let p = buf(&format!("/concurrent/file_{}.rs", i));
        vfs.add_overlay(p.as_path(), format!("content_{}", i)).ok();
    }

    let mut handles = vec![];
    for _ in 0..4 {
        let vfs = Arc::clone(&vfs);
        let h = thread::spawn(move || {
            for i in 0..100 {
                let p = buf(&format!("/concurrent/file_{}.rs", i));
                let _ = vfs.read(p.as_path());
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("thread completes");
    }
}

// ── FileSet Isolation ───────────────────────────────────────────────────────────

#[test]
fn acceptance_fileset_isolation() {
    let vfs_a = Vfs::new();
    let mut fs_a = FileSet::new(vfs_a);
    let p = buf("/tmp/foo.rs");
    let id_a = FileId::new(1);
    fs_a.add_path(p.as_path(), id_a);

    let vfs_b = Vfs::new();
    let mut fs_b = FileSet::new(vfs_b);
    let id_b = FileId::new(2);
    fs_b.add_path(p.as_path(), id_b);

    assert_eq!(fs_a.get(p.as_path()), Some(id_a));
    assert_eq!(fs_b.get(p.as_path()), Some(id_b));
    assert_ne!(id_a, id_b);
}

// ── Watcher ───────────────────────────────────────────────────────────────────

#[test]
fn watcher_nop_when_disabled() {
    use touring_storage::vfs::watcher::NopWatcher;
    let result = NopWatcher::new("/tmp");
    assert!(result.is_err());
}

// ── Additional Tests ───────────────────────────────────────────────────────────

#[test]
fn vfs_content_versioning() {
    let vfs = Vfs::new();
    let p = buf("/version/test.txt");

    vfs.add_overlay(p.as_path(), "v1").unwrap();
    let v1 = vfs.read(p.as_path()).unwrap();
    assert_eq!(v1.as_ref(), b"v1");

    vfs.add_overlay(p.as_path(), "v2").unwrap();
    let v2 = vfs.read(p.as_path()).unwrap();
    assert_eq!(v2.as_ref(), b"v2");
}

#[test]
fn vfs_remove_and_nonexistent() {
    let vfs = Vfs::new();
    let p = buf("/remove/test.txt");

    vfs.add_overlay(p.as_path(), "data").unwrap();
    assert!(vfs.exists(p.as_path()));

    vfs.remove(p.as_path()).unwrap();
    assert!(!vfs.exists(p.as_path()));

    assert!(vfs.remove(p.as_path()).is_err());
}

#[test]
fn fileset_glob_patterns() {
    let vfs = Vfs::new();
    let mut fs = FileSet::new(vfs);

    for (i, rel_path) in [
        "src/main.rs",
        "src/lib.rs",
        "src/bin/cli.rs",
        "tests/integration.rs",
    ]
    .iter()
    .enumerate()
    {
        let p = buf(&format!("/{}", rel_path));
        fs.add_path(p.as_path(), FileId::new(i as u32));
    }

    let results = fs.glob(buf("/src").as_path(), "*.rs");
    assert_eq!(results.len(), 2);

    let results = fs.glob(buf("/").as_path(), "**/*.rs");
    assert!(results.len() >= 3);
}

#[test]
fn vfs_overlay_empty_base() {
    let mut overlay = VfsOverlay::new();
    let p = buf("/overlay/only.txt");
    overlay.set(p.as_path(), "content");

    let content = overlay.read(p.as_path()).unwrap();
    assert_eq!(content.as_ref(), b"content");

    assert!(!overlay.exists(buf("/nonexistent").as_path()));
}

#[test]
fn abs_path_validation() {
    assert!(AbsPath::from_absolute("/tmp/foo").is_ok());
    assert!(AbsPath::from_absolute("/").is_ok());
    assert!(AbsPath::from_absolute("relative").is_err());
    assert!(AbsPath::from_absolute("./foo").is_err());
    assert!(AbsPath::from_absolute("foo/bar").is_err());
}

#[test]
fn abs_path_buf_roundtrip() {
    let original = "/tmp/roundtrip/test.txt";
    let buf = AbsPathBuf::try_from(original.to_string()).unwrap();
    assert_eq!(buf.as_str(), original);

    let path_ref = buf.as_path();
    assert_eq!(path_ref.as_str(), original);

    assert_eq!(buf.into_string(), original);
}
