//! Fixtures shared by the unit tests: scratch directories, a project config, a portfolio index
//! with one artifact, a memory db with one ranked snippet.

use std::path::{Path, PathBuf};

/// A path under the temp dir, unique per process and slug, empty on return.
pub(super) fn tmpdir(slug: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("p2-{slug}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Writes `<dir>/.touring/touring.toml`.
pub(super) fn escreve_config(dir: &Path, corpo: &str) {
    let t = dir.join(".touring");
    std::fs::create_dir_all(&t).expect("mkdir .touring");
    std::fs::write(t.join("touring.toml"), corpo).expect("write toml");
}

/// Declares `[code_mode] mode = "code"` in the root.
pub(super) fn escopo_code(root: &Path) {
    escreve_config(root, "[code_mode]\nmode = \"code\"\n");
}

/// A 200-line file in the `code` scope, with line 120 unique — large enough for
/// G3 to hand back a read window.
pub(super) fn projeto_com_arquivo_grande() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let fp = tmp.path().join("grande.rs");
    let corpo: String = (1..=200).map(|i| format!("linha {i}\n")).collect();
    std::fs::write(&fp, corpo).expect("grande.rs");
    (tmp, fp)
}

/// Builds a scratch index with ONE artifact whose purpose matches the burst.
/// The dir is UNIQUE per call: the pid is the same for all the tests of the
/// process, and without the seq two parallel tests write the SAME
/// `index.json.tmp{pid}` — one rename moves the other's tmp and `save_to`
/// fails with ENOENT (race measured 08/10: 3 failures in ~8 rounds; the
/// old `#[serial]` masked THIS file race, not only the env one).
pub(super) fn monta_portfolio_scratch() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use touring_foundation::portfolio::store::{PortfolioIndex, save_to};
    use touring_foundation::portfolio::{CapabilityEntry, CapabilityKind, Evidence};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "s6-portfolio-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("mkdir scratch portfolio");
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
    save_to(&dir, &index).expect("save scratch portfolio");
    dir
}

/// An in-memory memory db holding one snippet (`key` -> `body`) with 12 successful runs.
pub(super) fn mem_with_body(key: &str, body: &str) -> rusqlite::Connection {
    use touring_intelligence::rl::memory::snippet_stats::record_execution;
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch("CREATE TABLE memory_entries (key TEXT PRIMARY KEY, value TEXT);")
        .expect("entries table");
    conn.execute(
        "INSERT INTO memory_entries (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, body],
    )
    .expect("insert body");
    for _ in 0..12 {
        record_execution(&conn, key, true, "sig-x").expect("record");
    }
    conn
}
