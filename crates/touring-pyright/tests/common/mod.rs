//! Helpers shared by the real-server integration tests (each test file is its own binary, so each
//! has its own process-wide pool and its own child-process accounting).

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use touring_pyright::achado::{L2Resultado, Verdict};
use touring_pyright::pool;
use touring_pyright::servidor::Servidor;

/// A scratch project removed on drop.
pub struct Projeto {
    pub root: PathBuf,
}

impl Projeto {
    pub fn novo(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "touring_pyright_real_{tag}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&root).expect("create the scratch project");
        Self {
            root: root
                .canonicalize()
                .expect("canonicalize the scratch project"),
        }
    }

    pub fn escrever(&self, rel: &str, corpo: &str) -> PathBuf {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("create dirs");
        fs::write(&path, corpo).expect("write the fixture file");
        path
    }
}

impl Drop for Projeto {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// How long a cold start may take before the test gives up (a workspace load can be slow).
pub const LIMITE_DE_AQUECIMENTO: Duration = Duration::from_secs(240);

/// Checks until the verifier stops answering "aquecendo" (a slow verifier starts in the background and
/// the first calls only prime it). Returns the first measured result and how long it took to get it.
pub fn medir(
    servidor: Servidor,
    root: &Path,
    path: &str,
    content: &str,
    disk: Option<&str>,
) -> (L2Resultado, Duration) {
    pool::habilitar_neste_processo();
    let inicio = Instant::now();
    loop {
        let r = pool::checar_em(servidor, root, path, content, disk, &[]);
        let aquecendo = r.motivo.as_deref().is_some_and(|m| m.contains("aquecendo"));
        if !aquecendo || inicio.elapsed() > LIMITE_DE_AQUECIMENTO {
            return (r, inicio.elapsed());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `(rule, name)` of every finding, sorted — the shape the assertions compare.
pub fn achados(r: &L2Resultado) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = r
        .achados
        .iter()
        .map(|a| (a.classe.clone(), a.nome.clone()))
        .collect();
    v.sort();
    v
}

pub fn esperar_veredito(r: &L2Resultado, veredito: Verdict) {
    assert_eq!(r.verdict, veredito, "{r:#?}");
}

/// Parent pid of `pid`: field 4 of `/proc/<pid>/stat` (the `comm` field may hold spaces and
/// parentheses, so parsing starts after the LAST `)`).
fn parent_of(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(1)?.parse().ok()
}

/// Direct children of this test process (the verifier is one; its helpers are the verifier's own children).
pub fn filhos_do_processo() -> Vec<u32> {
    let me = std::process::id();
    let Ok(dir) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut pids: Vec<u32> = dir
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| parent_of(*pid) == Some(me))
        .collect();
    pids.sort_unstable();
    pids
}

/// Closes every session and waits until the test process has no child left.
pub fn encerrar_tudo_e_esperar_os_filhos() -> bool {
    pool::reap_idle(Instant::now() + Duration::from_secs(100_000));
    for _ in 0..200 {
        if filhos_do_processo().is_empty() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}
