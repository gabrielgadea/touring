//! The environment over an in-memory disk: the order CPython searches, `pyvenv.cfg`, `lib64`,
//! extension modules — and the guard that keeps `std::fs` in the declared adapter.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::*;

/// A disk of directories and files held in memory.
#[derive(Default)]
struct DiscoFalso {
    dirs: BTreeSet<PathBuf>,
    arquivos: BTreeMap<PathBuf, String>,
}

impl DiscoFalso {
    fn dir(mut self, caminho: &str) -> Self {
        let mut p = PathBuf::from(caminho);
        loop {
            self.dirs.insert(p.clone());
            if !p.pop() || p.as_os_str().is_empty() {
                break;
            }
        }
        self
    }

    fn arquivo(mut self, caminho: &str, texto: &str) -> Self {
        let p = PathBuf::from(caminho);
        if let Some(pai) = p.parent().and_then(Path::to_str) {
            self = self.dir(pai);
        }
        self.arquivos.insert(p, texto.to_string());
        self
    }
}

impl Disco for DiscoFalso {
    fn nomes(&self, dir: &Path) -> Option<Vec<String>> {
        if !self.dirs.contains(dir) {
            return None;
        }
        let filhos = self
            .dirs
            .iter()
            .chain(self.arquivos.keys())
            .filter(|p| p.parent() == Some(dir))
            .filter_map(|p| p.file_name()?.to_str().map(str::to_owned));
        Some(filhos.collect())
    }

    fn e_diretorio(&self, caminho: &Path) -> bool {
        self.dirs.contains(caminho)
    }

    fn ler(&self, arquivo: &Path) -> Option<String> {
        self.arquivos.get(arquivo).cloned()
    }
}

fn caminhos(v: &[&str]) -> Vec<PathBuf> {
    v.iter().map(PathBuf::from).collect()
}

#[test]
fn venv_roots_come_from_lib_and_lib64_versions_in_lexical_order() {
    let disco = DiscoFalso::default()
        .dir("/p/.venv/lib/python3.12/site-packages")
        .dir("/p/.venv/lib/python3.11")
        .dir("/p/.venv/lib64/python3.12")
        .arquivo("/p/.venv/lib/pythonista.txt", "not a dir");
    assert_eq!(
        raizes_do_interpretador(&disco, Path::new("/p/.venv")),
        caminhos(&[
            "/p/.venv/lib/python3.11",
            "/p/.venv/lib/python3.11/site-packages",
            "/p/.venv/lib/python3.11/dist-packages",
            "/p/.venv/lib/python3.12",
            "/p/.venv/lib/python3.12/site-packages",
            "/p/.venv/lib/python3.12/dist-packages",
            "/p/.venv/lib64/python3.12",
            "/p/.venv/lib64/python3.12/site-packages",
            "/p/.venv/lib64/python3.12/dist-packages",
        ])
    );
}

/// CPython's order: base stdlib, then the venv's packages, then the system's — only when the
/// venv includes them.
#[test]
fn pyvenv_cfg_puts_the_base_stdlib_first_and_system_packages_last() {
    let disco = DiscoFalso::default()
        .dir("/p/.venv/lib/python3.12")
        .arquivo(
            "/p/.venv/pyvenv.cfg",
            "home = /usr/bin\ninclude-system-site-packages = TRUE\nversion = 3.12.1\n",
        );
    assert_eq!(
        raizes_do_interpretador(&disco, Path::new("/p/.venv")),
        caminhos(&[
            "/usr/lib/python3.12",
            "/usr/lib64/python3.12",
            "/p/.venv/lib/python3.12",
            "/p/.venv/lib/python3.12/site-packages",
            "/p/.venv/lib/python3.12/dist-packages",
            "/usr/lib/python3.12/site-packages",
            "/usr/lib/python3.12/dist-packages",
            "/usr/lib64/python3.12/site-packages",
            "/usr/lib64/python3.12/dist-packages",
        ])
    );
    let sem_sistema = DiscoFalso::default()
        .dir("/p/.venv/lib/python3.12")
        .arquivo("/p/.venv/pyvenv.cfg", "home = /usr/bin\n");
    let raizes = raizes_do_interpretador(&sem_sistema, Path::new("/p/.venv"));
    assert_eq!(raizes.first(), Some(&PathBuf::from("/usr/lib/python3.12")));
    assert!(
        !raizes
            .iter()
            .any(|r| r.starts_with("/usr") && r.ends_with("site-packages"))
    );
}

#[test]
fn the_conventional_venv_is_dot_venv_then_venv() {
    let ambos = DiscoFalso::default().dir("/p/.venv").dir("/p/venv");
    assert_eq!(
        venv_convencional(&ambos, Path::new("/p")),
        Some(PathBuf::from("/p/.venv"))
    );
    let so_venv = DiscoFalso::default().dir("/p/venv");
    assert_eq!(
        venv_convencional(&so_venv, Path::new("/p")),
        Some(PathBuf::from("/p/venv"))
    );
    assert_eq!(
        venv_convencional(&DiscoFalso::default(), Path::new("/p")),
        None
    );
}

#[test]
fn an_extension_module_needs_the_dot_after_its_name() {
    for arquivo in [
        "etree.cpython-312-x86_64-linux-gnu.so",
        "_core.abi3.so",
        "etree.so",
        "etree.pyd",
    ] {
        let modulo = if arquivo.starts_with('_') {
            "_core"
        } else {
            "etree"
        };
        assert!(e_extensao_c(arquivo, modulo), "{arquivo}");
    }
    for arquivo in ["etree_extra.so", "etree.py", "etree", "etree.so.bak"] {
        assert!(!e_extensao_c(arquivo, "etree"), "{arquivo}");
    }
    let disco = DiscoFalso::default().arquivo("/sp/lxml/etree.cpython-312.so", "");
    assert!(tem_extensao_c(&disco, Path::new("/sp/lxml"), "etree"));
    assert!(!tem_extensao_c(&disco, Path::new("/sp/lxml"), "html"));
    assert!(!tem_extensao_c(&disco, Path::new("/nao/existe"), "etree"));
}

/// `std::fs` lives in `real.rs` only: every other read goes through the [`Disco`] the caller
/// hands in, so layer 1 keeps its containment.
#[test]
fn only_the_declared_adapter_touches_the_filesystem() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entrada in std::fs::read_dir(&src).expect("src") {
        let caminho = entrada.expect("entry").path();
        let nome = caminho
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if nome == "real.rs" || nome == "tests.rs" {
            continue;
        }
        let texto = std::fs::read_to_string(&caminho).expect("source");
        // code lines only: the docs may NAME `std::fs` (they say where it lives)
        let codigo = texto.lines().filter(|l| !l.trim_start().starts_with("//"));
        for linha in codigo {
            for proibido in [
                "std::fs",
                "read_dir(",
                "read_to_string(",
                ".is_dir()",
                ".exists()",
            ] {
                assert!(
                    !linha.contains(proibido),
                    "{nome} uses `{proibido}`: {linha}"
                );
            }
        }
    }
}
