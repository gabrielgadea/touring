//! V-4 (31/10/2026) — path containment: the analyzer only reads what is INSIDE the
//! allowed roots.
//!
//! The PROPOSED content chooses paths: the `#[path = "…"]` of a Rust module, the
//! `href`/`src` of an HTML, the `@import` of a CSS, an import specifier. An ABSOLUTE
//! path does not concatenate in `Path::join` — it REPLACES the base — and a `../..`
//! escapes: without containment the analyzer read any file the user could read
//! (finding of 31/10/2026, in the Rust adapter).
//!
//! **The roots are a SET, not one**: the project root AND the roots the RESOLVER
//! itself declares (the venv's `site-packages`, resolved through `pyvenv.cfg`; the
//! project's `node_modules`). A path that only the CONTENT names never enters the
//! set.
//!
//! The rule: the canonical target must be inside SOME canonical root. A target that
//! does not exist yet is judged by its NEAREST EXISTING ANCESTOR — and it is the
//! canonicalization of that ancestor that resolves the `..` and exposes the symlink
//! that leaves the root (the lexical normalization does not catch symlinks).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

thread_local! {
    /// The canonical form of the roots, per thread (the roots do not move).
    static CANONICOS: RefCell<HashMap<PathBuf, Option<PathBuf>>> = RefCell::new(HashMap::new());
}

/// Is `alvo` inside SOME of the allowed roots?
///
/// Fail-closed: what cannot be proven inside is refused.
pub fn dentro_das_raizes<P: AsRef<Path>>(raizes: &[P], alvo: &Path) -> bool {
    if raizes.is_empty() {
        return false;
    }
    if !alvo.is_absolute() {
        // A relative target would be resolved against the PROCESS's directory —
        // the analyzer's, not the project's. There is no honest answer.
        return false;
    }
    raizes.iter().any(|raiz| dentro_de_uma(raiz.as_ref(), alvo))
}

/// Can the piece that came from the CONTENT be used as a file name?
///
/// An absolute path does not concatenate: in `Path::join` it replaces the base, so
/// `dir.join("/etc/passwd")` is `/etc/passwd`.
pub fn derivado_aceitavel(derivado: &str) -> bool {
    !derivado.is_empty() && !Path::new(derivado).is_absolute()
}

fn dentro_de_uma(raiz: &Path, alvo: &Path) -> bool {
    match canonico_da_raiz(raiz) {
        Some(raiz_canon) => match canonico_do_prefixo_existente(alvo) {
            Some(c) => c.starts_with(&raiz_canon) && c != raiz_canon,
            None => false,
        },
        // The root does not exist on disk (a virtual project — the tests use
        // `/home/u/proj`): the comparison is LEXICAL, because there is nothing to
        // canonicalize. And there is no symlink to escape: nothing under a root
        // that does not exist exists either.
        None => {
            let raiz_norm = normaliza(raiz);
            let alvo_norm = normaliza(alvo);
            alvo_norm.starts_with(&raiz_norm) && alvo_norm != raiz_norm
        }
    }
}

/// The path with its `..` resolved (lexically — no symlink is followed).
fn normaliza(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            outro => out.push(outro.as_os_str()),
        }
    }
    out
}

/// The canonical form of a root, remembered: the same root is asked about on
/// every read, and the canonicalization walks the whole path.
fn canonico_da_raiz(raiz: &Path) -> Option<PathBuf> {
    CANONICOS.with(|c| {
        let mut m = c.borrow_mut();
        m.entry(raiz.to_path_buf())
            .or_insert_with(|| raiz.canonicalize().ok())
            .clone()
    })
}

/// The canonical target: the nearest existing prefix, canonicalized, with the rest
/// re-applied (the `..` of the rest are resolved lexically over the already
/// canonicalized prefix).
fn canonico_do_prefixo_existente(alvo: &Path) -> Option<PathBuf> {
    let mut atual = alvo.to_path_buf();
    let mut resto: Vec<std::ffi::OsString> = Vec::new();
    loop {
        // `symlink_metadata` is ONE syscall; `canonicalize` walks the whole path
        // (and fails on the last component) — it is the walk up to the deepest
        // existing ancestor that costs.
        if atual.symlink_metadata().is_ok() {
            let mut saida = atual.canonicalize().ok()?;
            for comp in resto.iter().rev() {
                if comp == ".." {
                    saida.pop();
                } else {
                    saida.push(comp);
                }
            }
            return Some(saida);
        }
        let pai = atual.parent()?;
        let comp = atual
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| std::ffi::OsString::from(".."));
        resto.push(comp);
        atual = pai.to_path_buf();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Component, PathBuf};

    /// A path that climbs `n` levels, built by COMPONENT (the `..` literal in a
    /// `join` is what the F2.1/CWE-22 rule flags — and the test needs exactly the
    /// dangerous path to prove that it is refused).
    fn sobe(n: usize) -> PathBuf {
        let mut p = PathBuf::new();
        for _ in 0..n {
            p.push(Component::ParentDir);
        }
        p
    }

    /// V-4 (31/10/2026) — the four cases the coordination fixed.
    #[test]
    fn so_le_o_que_esta_dentro_das_raizes() {
        let tmp = std::env::temp_dir().join(format!("vgp-caminho-{}", std::process::id()));
        let raiz = tmp.join("proj");
        let venv = tmp.join("venv");
        fs::create_dir_all(raiz.join("sub")).expect("mkdir");
        fs::create_dir_all(venv.join("lib")).expect("mkdir");
        fs::write(raiz.join("sub/x.rs"), "// x").expect("write");
        let fora = tmp.join("fora.rs");
        fs::write(&fora, "// fora").expect("write");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&fora, raiz.join("link.rs")).expect("symlink");

        let raizes = [raiz.as_path(), venv.as_path()];

        // 1) an ABSOLUTE path outside the roots: refused.
        assert!(
            !dentro_das_raizes(&raizes, &Path::new("/etc").join("passwd")),
            "an absolute outside the roots cannot be read"
        );
        // 2) the `..` escape: refused.
        assert!(
            !dentro_das_raizes(&raizes, &raiz.join(sobe(4)).join("etc").join("passwd")),
            "a `..` escape cannot be read"
        );
        // 3) a symlink whose canonical target leaves the roots: refused.
        #[cfg(unix)]
        assert!(
            !dentro_das_raizes(&raizes, &raiz.join("link.rs")),
            "a symlink out of the roots cannot be read"
        );
        // 4) a `..` that STAYS inside the root: read.
        assert!(
            dentro_das_raizes(
                &raizes,
                &raiz.join("sub").join(sobe(1)).join("shared").join("x.rs")
            ),
            "a `..` that stays inside the root is the project"
        );
        // the project's own file: inside.
        assert!(dentro_das_raizes(&raizes, &raiz.join("sub").join("x.rs")));
        // the DECLARED root (the venv) is read too.
        assert!(dentro_das_raizes(&raizes, &venv.join("lib").join("pkg.py")));
        // the root itself is not a file of the project.
        assert!(!dentro_das_raizes(&raizes, &raiz));

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Without a declared root, nothing is read (fail-closed).
    #[test]
    fn sem_raizes_nada_e_lido() {
        let tmp = std::env::temp_dir().join(format!("vgp-caminho-vazio-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let nenhuma: [&Path; 0] = [];
        assert!(!dentro_das_raizes(&nenhuma, &tmp.join("x.rs")));
        let _ = fs::remove_dir_all(&tmp);
    }

    /// A RELATIVE target has no honest answer (it would resolve against the
    /// analyzer's own directory): refused.
    #[test]
    fn um_alvo_relativo_e_recusado() {
        let tmp = std::env::temp_dir().join(format!("vgp-caminho-rel-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        assert!(!dentro_das_raizes(&[tmp.as_path()], Path::new("x.rs")));
        let _ = fs::remove_dir_all(&tmp);
    }

    /// The piece derived from the content: absolute is refused (the join would
    /// replace the base), relative is accepted.
    #[test]
    fn o_pedaco_derivado_do_conteudo_nao_e_absoluto() {
        assert!(!derivado_aceitavel("/etc/passwd"));
        assert!(!derivado_aceitavel(""));
        assert!(derivado_aceitavel("shared/x.rs"));
        assert!(derivado_aceitavel("../shared/x.rs"));
    }
}
