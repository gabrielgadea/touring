//! touring_python_env — the Python environment the two VGP layers resolve modules against.
//!
//! Layer 1 (`touring-code`, `vgp::lang::python::modules`) and layer 2 (`touring-pyright`) each
//! carried their own map of the same world: the extension-module test was copied line for line,
//! and the interpreter roots (venv `lib`/`lib64`, `pythonX.Y`, `site-packages`/`dist-packages`,
//! the `home` and `include-system-site-packages` of `pyvenv.cfg`) lived only in layer 2, so a fix
//! to one map left the other answering differently. The knowledge lives here once.
//!
//! The disk is reached only through a [`Disco`] the caller supplies — two adapters, so a real
//! seam: layer 2 passes [`DiscoReal`] (`std::fs`); layer 1 passes its contained reader, which never
//! reads outside the roots it was given. This crate depends on nothing but std, so the
//! dependency rule of `touring-pyright` (no `touring-*` crate that could close a package cycle)
//! still holds.
//!
//! What each layer DOES with the environment stays in it: the order a module is searched in
//! (`modulo_do_projeto` in layer 2, `resolve_module` in layer 1) is a verdict, and changing it
//! needs a pre-registered measurement (see `docs/plans/2026-11-09-work-outer/bfce/DECISOES.md`).

use std::path::{Path, PathBuf};

mod real;

pub use real::DiscoReal;

/// The conventional venv directories, in the order they are tried.
pub const VENV_DIRS: [&str; 2] = [".venv", "venv"];

/// How the environment reads the disk.
pub trait Disco {
    /// The entry names of `dir`, or `None` when it cannot be listed.
    fn nomes(&self, dir: &Path) -> Option<Vec<String>>;
    /// `true` when `caminho` is a directory.
    fn e_diretorio(&self, caminho: &Path) -> bool;
    /// The text of `arquivo`, or `None` when it cannot be read.
    fn ler(&self, arquivo: &Path) -> Option<String>;
}

/// The first conventional venv ([`VENV_DIRS`]) under `raiz` that is a directory.
pub fn venv_convencional(disco: &dyn Disco, raiz: &Path) -> Option<PathBuf> {
    VENV_DIRS
        .iter()
        .map(|nome| raiz.join(nome))
        .find(|candidato| disco.e_diretorio(candidato))
}

/// The interpreter's own import roots for `venv`, in CPython's order: base stdlib → venv
/// `site-packages` → system `site-packages` (only when `pyvenv.cfg` includes them). Each venv
/// `pythonX.Y` of `lib` and `lib64` contributes itself, its `site-packages` and its
/// `dist-packages`, versions in lexical order.
pub fn raizes_do_interpretador(disco: &dyn Disco, venv: &Path) -> Vec<PathBuf> {
    let mut locais = Vec::new();
    let mut versoes = Vec::new();
    for lib in [venv.join("lib"), venv.join("lib64")] {
        acrescenta_raizes_python(disco, &mut locais, &lib, &mut versoes);
    }
    let Some(config) = configuracao_python(disco, venv) else {
        return locais;
    };
    let mut raizes = Vec::new();
    let mut globais = Vec::new();
    versoes.sort();
    versoes.dedup();
    for versao in versoes {
        for lib in [config.prefixo.join("lib"), config.prefixo.join("lib64")] {
            let dir = lib.join(&versao);
            raizes.push(dir.clone());
            if config.pacotes_do_sistema {
                globais.push(dir.join("site-packages"));
                globais.push(dir.join("dist-packages"));
            }
        }
    }
    raizes.extend(locais);
    raizes.extend(globais);
    raizes
}

/// What `pyvenv.cfg` says about the base interpreter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguracaoPython {
    /// The base prefix: the parent of `home` (the base interpreter's `bin`).
    pub prefixo: PathBuf,
    /// `include-system-site-packages = true`.
    pub pacotes_do_sistema: bool,
}

/// `venv/pyvenv.cfg`, when it names a `home`.
pub fn configuracao_python(disco: &dyn Disco, venv: &Path) -> Option<ConfiguracaoPython> {
    let cfg = disco.ler(&venv.join("pyvenv.cfg"))?;
    let mut home = None;
    let mut pacotes_do_sistema = false;
    for linha in cfg.lines() {
        let Some((chave, valor)) = linha.split_once('=') else {
            continue;
        };
        match chave.trim() {
            "home" => home = Some(PathBuf::from(valor.trim())),
            "include-system-site-packages" => {
                pacotes_do_sistema = valor.trim().eq_ignore_ascii_case("true");
            }
            _ => {}
        }
    }
    let prefixo = home?.parent()?.to_path_buf();
    Some(ConfiguracaoPython {
        prefixo,
        pacotes_do_sistema,
    })
}

fn acrescenta_raizes_python(
    disco: &dyn Disco,
    raizes: &mut Vec<PathBuf>,
    lib: &Path,
    versoes: &mut Vec<String>,
) {
    let Some(nomes) = disco.nomes(lib) else {
        return;
    };
    let mut pythons: Vec<String> = nomes
        .into_iter()
        .filter(|nome| nome.starts_with("python") && disco.e_diretorio(&lib.join(nome)))
        .collect();
    pythons.sort();
    for versao in pythons {
        let dir = lib.join(&versao);
        versoes.push(versao);
        raizes.push(dir.clone());
        raizes.push(dir.join("site-packages"));
        raizes.push(dir.join("dist-packages"));
    }
}

/// `true` when `arquivo` (an entry name) is the compiled extension of module `modulo`:
/// `<modulo>.<tag>.so`, `<modulo>.so`, `<modulo>.pyd`. The dot after the name is required —
/// `etree_extra.so` is not `etree`.
pub fn e_extensao_c(arquivo: &str, modulo: &str) -> bool {
    arquivo.strip_prefix(modulo).is_some_and(|resto| {
        resto.starts_with('.') && (resto.ends_with(".so") || resto.ends_with(".pyd"))
    })
}

/// `true` when `dir` holds the compiled extension of `modulo` ([`e_extensao_c`]).
pub fn tem_extensao_c(disco: &dyn Disco, dir: &Path, modulo: &str) -> bool {
    disco
        .nomes(dir)
        .is_some_and(|nomes| nomes.iter().any(|arquivo| e_extensao_c(arquivo, modulo)))
}

#[cfg(test)]
mod tests;
