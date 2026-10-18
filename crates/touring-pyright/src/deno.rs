//! VGP layer 2 for a Deno project — `deno lsp`, type-checker diagnostics only.
//!
//! A project is Deno when the nearest project marker above the edited file is a `deno.json[c]`
//! ([`crate::servidor::projeto_deno`]); then TypeScript is checked by Deno's own checker (its bundled
//! `tsc`, with the project's import map, `jsr:`/`npm:` specifiers and ambient types), which `tsserver`
//! from `node_modules` cannot reproduce. The binary is the project's `node_modules/.bin/deno` when the
//! project vendors one, else the `deno` on `PATH` (Deno is installed per machine, not per project — this
//! is the one verifier whose "project's own" is the machine's); a machine without `deno` is an `unknown`
//! that says so, and nothing is installed.
//!
//! `deno lsp` mixes sources in one report: `deno-ts` (the checker), `deno-lint` (style) and `deno`
//! (configuration, import resolution). Only `deno-ts` is read: lint is not what layer 2 is for, and a
//! resolution warning is not an invented name. The server advertises pull diagnostics, so each check
//! is a `textDocument/diagnostic` request (no version to match; see [`crate::lsp_nucleo`]).
//!
//! Not verified here: whether `deno lsp` fetches remote modules (`https:`/`jsr:`/`npm:`) that are not
//! yet in `DENO_DIR` while analysing — it has no offline switch. The fixture has no remote imports.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::lsp_nucleo::{EscolhaDeModo, Inicio, NucleoLsp, Perfil};
use crate::servidor::{Alvo, Ambiente, Edicao, Sessao, Verificado, dirs_ate_a_raiz, tem_extensao};
use crate::ts_diag::NormTs;

/// Cold start of the language server (it caches and type-checks lazily).
const STARTUP_BUDGET: Duration = Duration::from_secs(30);
/// Per-check budget of a warm session.
const DEFAULT_BUDGET: Duration = Duration::from_millis(2000);

static PERFIL: Perfil = Perfil {
    nome: "deno",
    extensao_inline: "ts",
    exige_quiescencia: false,
    language_id,
};

/// The LSP `languageId` Deno expects for a path.
fn language_id(path: &str) -> &'static str {
    if tem_extensao(path, &["tsx"]) {
        "typescriptreact"
    } else if tem_extensao(path, &["jsx"]) {
        "javascriptreact"
    } else if tem_extensao(path, &["js", "mjs", "cjs"]) {
        "javascript"
    } else {
        "typescript"
    }
}

// ── discovery ──────────────────────────────────────────────────────────────

/// A `deno` found for a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Localizado {
    /// The `deno` executable.
    pub(crate) binario: PathBuf,
    /// The Deno workspace root: the highest directory with a `deno.json[c]` between the file and the
    /// project root.
    pub(crate) raiz_deno: PathBuf,
}

fn tem_deno_json(dir: &Path) -> bool {
    ["deno.json", "deno.jsonc"]
        .iter()
        .any(|nome| dir.join(nome).is_file())
}

/// The Deno workspace root of an edit (`None` for a project that is not Deno).
pub(crate) fn raiz_deno(raiz: &Path, arquivo: &str) -> Option<PathBuf> {
    if !crate::servidor::projeto_deno(raiz, arquivo) {
        return None;
    }
    dirs_ate_a_raiz(raiz, arquivo)
        .into_iter()
        .rfind(|dir| tem_deno_json(dir))
}

/// The registry sub-key of an edit: the Deno workspace root (empty when the project is not Deno).
pub(crate) fn subchave(raiz: &Path, arquivo: &str) -> PathBuf {
    raiz_deno(raiz, arquivo).unwrap_or_default()
}

/// Finds the `deno` of a Deno project — or says why there is none.
pub(crate) fn localizar(raiz: &Path, arquivo: &str, amb: &Ambiente) -> Result<Localizado, String> {
    let raiz_deno = raiz_deno(raiz, arquivo).ok_or_else(|| {
        format!(
            "{arquivo} não está num projeto Deno (nenhum deno.json[c] como marcador mais próximo)"
        )
    })?;
    let do_projeto = dirs_ate_a_raiz(raiz, arquivo)
        .into_iter()
        .map(|dir| dir.join("node_modules/.bin/deno"))
        .find(|candidato| candidato.is_file());
    let binario = do_projeto.or_else(|| amb.achar("deno")).ok_or_else(|| {
        "deno não está no PATH nem em node_modules/.bin: o `deno lsp` não pode ser executado e o Touring nunca o instala"
            .to_string()
    })?;
    Ok(Localizado { binario, raiz_deno })
}

// ── the session ────────────────────────────────────────────────────────────

/// A warm `deno lsp` rooted at a Deno workspace.
pub(crate) struct SessaoDeno {
    nucleo: NucleoLsp,
    norm: NormTs,
}

fn capabilities() -> Value {
    json!({
        "textDocument": {
            "publishDiagnostics": {"versionSupport": true},
            "diagnostic": {"dynamicRegistration": false},
        },
        "workspace": {"configuration": true},
    })
}

/// What Deno asks through `workspace/configuration` (section `deno`) and `initializationOptions`.
fn opcoes() -> Value {
    json!({"enable": true, "lint": false, "unstable": false})
}

/// Starts the project's `deno lsp` (the spawner of [`crate::servidor::Servidor::DenoLsp`]).
pub(crate) fn abrir(alvo: &Alvo<'_>, amb: &Ambiente) -> Result<Box<dyn Sessao>, String> {
    let achado = localizar(alvo.raiz, alvo.arquivo, amb)?;
    let mut comando = Command::new(&achado.binario);
    comando
        .arg("lsp")
        .current_dir(&achado.raiz_deno)
        .env("DENO_NO_UPDATE_CHECK", "1");
    let mut sections = Map::new();
    sections.insert("deno".into(), opcoes());
    let inicio = Inicio {
        comando,
        sections,
        raiz_lsp: &achado.raiz_deno,
        capabilities: capabilities(),
        init_options: opcoes(),
        startup: STARTUP_BUDGET,
        modo: EscolhaDeModo::Anunciado,
    };
    let nucleo = NucleoLsp::iniciar(inicio, alvo.raiz.to_path_buf(), DEFAULT_BUDGET, &PERFIL)
        .map_err(|e| {
            format!(
                "deno lsp não iniciou em {}: {e}",
                achado.raiz_deno.display()
            )
        })?;
    Ok(Box::new(SessaoDeno {
        nucleo,
        norm: NormTs {
            fonte: Some("deno-ts"),
        },
    }))
}

impl Sessao for SessaoDeno {
    fn verificar(&mut self, edicao: &Edicao<'_>, prazo: Option<Duration>) -> Verificado {
        match prazo {
            None => self.nucleo.verificar(&self.norm, edicao),
            Some(prazo) => self.nucleo.verificar_ate(&self.norm, edicao, prazo),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Arvore;

    #[test]
    fn language_ids_follow_the_extension() {
        for (path, id) in [
            ("a.ts", "typescript"),
            ("a.mts", "typescript"),
            ("a.tsx", "typescriptreact"),
            ("a.js", "javascript"),
            ("a.mjs", "javascript"),
            ("a.jsx", "javascriptreact"),
        ] {
            assert_eq!(language_id(path), id, "{path}");
        }
    }

    #[test]
    fn the_deno_root_is_the_highest_deno_json_between_the_file_and_the_project_root() {
        let t = Arvore::nova("deno_root");
        t.arquivo("deno.json", "{\"workspace\": [\"./pkg\"]}");
        t.arquivo("pkg/deno.jsonc", "{}");
        t.arquivo("pkg/src/a.ts", "");
        assert_eq!(raiz_deno(&t.raiz, "pkg/src/a.ts"), Some(t.raiz.clone()));
        let solto = Arvore::nova("deno_root2");
        solto.arquivo("app/deno.json", "{}");
        assert_eq!(
            raiz_deno(&solto.raiz, "app/main.ts"),
            Some(solto.raiz.join("app"))
        );
        assert_eq!(
            raiz_deno(&solto.raiz, "outro/x.ts"),
            None,
            "no deno.json above: not a Deno project"
        );
        assert_eq!(subchave(&solto.raiz, "outro/x.ts"), PathBuf::new());
    }

    #[test]
    fn a_node_project_is_not_a_deno_project_even_with_a_deno_json_farther_up() {
        let t = Arvore::nova("deno_node");
        t.arquivo("deno.json", "{}");
        t.arquivo("web/package.json", "{}");
        let motivo =
            localizar(&t.raiz, "web/src/a.ts", &Ambiente::com_path(Vec::new())).unwrap_err();
        assert!(motivo.contains("não está num projeto Deno"), "{motivo}");
    }

    #[cfg(unix)]
    #[test]
    fn the_project_binary_wins_over_path_and_a_missing_deno_is_an_absent_verifier() {
        let t = Arvore::nova("deno_bin");
        t.arquivo("deno.json", "{}");
        t.arquivo("src/a.ts", "");
        let global = t.executavel("fakebin/deno", "#!/bin/sh\n");
        let amb = Ambiente::com_path(vec![t.raiz.join("fakebin")]);
        assert_eq!(
            localizar(&t.raiz, "src/a.ts", &amb).unwrap().binario,
            global
        );
        let local = t.executavel("node_modules/.bin/deno", "#!/bin/sh\n");
        assert_eq!(
            localizar(&t.raiz, "src/a.ts", &amb).unwrap().binario,
            local,
            "the project's wins"
        );
        let t2 = Arvore::nova("deno_nobin");
        t2.arquivo("deno.json", "{}");
        let motivo = localizar(&t2.raiz, "a.ts", &Ambiente::com_path(Vec::new())).unwrap_err();
        assert!(
            motivo.contains("deno não está no PATH") && motivo.contains("nunca o instala"),
            "{motivo}"
        );
    }
}
