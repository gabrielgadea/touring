//! C4 (12/11/2026) — the project as an adapter sees it: the OVERLAYS of the entry laid over the
//! disk. One module answers "does it exist / is it a file / is it a directory / what does it say",
//! and with it the two rules each adapter used to carry its own copy of.
//!
//! **Precedence.** The overlay wins over the disk. It is the file at the entry's instant — what the
//! entry's other files WILL say — so the disk is only asked about what no overlay covers. The disk
//! is touched through [`caminho`] (containment is its operation), and only inside the roots the
//! caller passes; an overlay does not depend on the roots: the CALLER declared it, the analyzed
//! content did not choose it. An overlay has no size cap either — the cap is the disk's.
//!
//! **The key.** An overlay key is the lexically NORMAL absolute path of the file (`/p/src/a.rs`,
//! no `.`/`..`/`//`), and a lookup normalizes the path it receives: `./a.rs` and `sub/../a.rs` are
//! the file `a.rs`. Until this module each of five adapters spelled the key its own way — three
//! normalized in the caller, two looked the path up as it was written (a `#[path = "./x.rs"]`
//! missed the overlay for `x.rs` and the module was reported "declared without a file"). Only the
//! KEY is normalized here: the path handed to the disk is the caller's, unchanged.
//!
//! **Directories.** A directory exists when the disk says so, or when an overlay lies under it
//! (the files an entry creates in a new directory make the directory): the TypeScript rule, now
//! everybody's — the HTML adapter dropped `/assets/a.png` because `assets` was not on the disk.
//!
//! There is no "the overlay deletes the file": the map cannot say it, and no adapter asked.
//!
//! The empty map — what the hooks pass — costs one `is_empty()` per question: nothing is
//! normalized and nothing is allocated.

use std::collections::HashMap;
use std::path::Path;

use super::caminho::{self, Recusa, Sonda};

/// The overlays of an entry (path → content) over the disk. Copy: it only borrows the map.
#[derive(Clone, Copy)]
pub struct Vista<'a> {
    sobrepostos: &'a HashMap<String, String>,
}

/// The key of `p` in the overlay map: the one rule (see the module doc).
fn chave(p: &Path) -> String {
    caminho::normaliza(p).to_string_lossy().into_owned()
}

impl<'a> Vista<'a> {
    /// A view over `sobrepostos` (path → content, keyed by the NORMAL absolute path).
    pub fn nova(sobrepostos: &'a HashMap<String, String>) -> Self {
        Self { sobrepostos }
    }

    /// The overlay at `p`, alone — what the entry says the file is, without asking the disk.
    pub fn sobreposto(&self, p: &Path) -> Option<&'a str> {
        if self.sobrepostos.is_empty() {
            return None;
        }
        self.sobrepostos.get(&chave(p)).map(String::as_str)
    }

    /// Does some overlay lie UNDER `p` — is `p` a directory only an overlay creates (or fills)?
    fn cobre_diretorio(&self, p: &Path) -> bool {
        if self.sobrepostos.is_empty() {
            return false;
        }
        let mut prefixo = chave(p);
        if prefixo.is_empty() {
            return false; // `.` is nothing an absolute key lies under
        }
        if !prefixo.ends_with('/') {
            prefixo.push('/');
        }
        self.sobrepostos.keys().any(|k| k.starts_with(&prefixo))
    }

    /// What `p` is: a file when an overlay is there, else what the disk says INSIDE `raizes`
    /// (something that exists outside them is [`Sonda::Fora`], not looked at any further).
    pub fn sondar<P: AsRef<Path>>(&self, raizes: &[P], p: &Path) -> Sonda {
        if self.sobreposto(p).is_some() {
            return Sonda::Arquivo;
        }
        caminho::sondar(raizes, p)
    }

    /// Is `p` a file — in the overlays, or on the disk inside `raizes`?
    pub fn e_arquivo<P: AsRef<Path>>(&self, raizes: &[P], p: &Path) -> bool {
        self.sondar(raizes, p) == Sonda::Arquivo
    }

    /// Is `p` a directory — on the disk inside `raizes`, or one an overlay lies under?
    pub fn e_diretorio<P: AsRef<Path>>(&self, raizes: &[P], p: &Path) -> bool {
        self.cobre_diretorio(p) || caminho::e_diretorio(raizes, p)
    }

    /// Does `p` exist — a file or a directory of the overlays, or anything on the disk inside
    /// `raizes`?
    pub fn existe<P: AsRef<Path>>(&self, raizes: &[P], p: &Path) -> bool {
        self.sobreposto(p).is_some() || self.cobre_diretorio(p) || caminho::existe(raizes, p)
    }

    /// The text at `p`: the overlay, else the file on the disk inside `raizes` — refused over
    /// `limite` bytes when the caller caps the DISK read (`None`: no cap).
    pub fn ler<P: AsRef<Path>>(
        &self,
        raizes: &[P],
        p: &Path,
        limite: Option<u64>,
    ) -> Result<String, Recusa> {
        if let Some(texto) = self.sobreposto(p) {
            return Ok(texto.to_string());
        }
        match limite {
            Some(limite) => caminho::ler_contido_ate(raizes, p, limite),
            None => caminho::ler_contido(raizes, p),
        }
    }

    /// Every overlay (key, text) except the one at `p` — the entry's siblings.
    pub fn outros(&self, p: &Path) -> impl Iterator<Item = (&'a str, &'a str)> + use<'a> {
        let sobrepostos = self.sobrepostos;
        let eu = if sobrepostos.is_empty() {
            String::new()
        } else {
            chave(p)
        };
        sobrepostos
            .iter()
            .filter(move |(k, _)| k.as_str() != eu)
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[cfg(test)]
mod tests;
