//! [`DiscoReal`]: the environment read straight from the filesystem — the ONE place in this
//! crate that touches `std::fs` (a test scans the sources to keep it that way).

use std::path::Path;

use super::Disco;

/// The plain filesystem, as layer 2 (`touring-pyright`) reads it.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiscoReal;

impl Disco for DiscoReal {
    fn nomes(&self, dir: &Path) -> Option<Vec<String>> {
        let entradas = std::fs::read_dir(dir).ok()?;
        Some(
            entradas
                .flatten()
                .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
                .collect(),
        )
    }

    fn e_diretorio(&self, caminho: &Path) -> bool {
        caminho.is_dir()
    }

    fn ler(&self, arquivo: &Path) -> Option<String> {
        std::fs::read_to_string(arquivo).ok()
    }
}
