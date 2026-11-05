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

/// Um arquivo de 200 linhas no escopo `code`, com a linha 120 única — grande o bastante para o
/// G3 devolver uma janela de leitura.
pub(super) fn projeto_com_arquivo_grande() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let fp = tmp.path().join("grande.rs");
    let corpo: String = (1..=200).map(|i| format!("linha {i}\n")).collect();
    std::fs::write(&fp, corpo).expect("grande.rs");
    (tmp, fp)
}

/// Monta um índice scratch com UM artefato cujo propósito casa a rajada.
/// O dir é ÚNICO por chamada: o pid é o mesmo para todos os testes do
/// processo, e sem o seq dois testes paralelos gravam o MESMO
/// `index.json.tmp{pid}` — um rename move o tmp do outro e o `save_to`
/// falha com ENOENT (race medida 08/10: 3 falhas em ~8 rodadas; o
/// `#[serial]` antigo mascarava ESTA race de arquivo, não só a do env).
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
