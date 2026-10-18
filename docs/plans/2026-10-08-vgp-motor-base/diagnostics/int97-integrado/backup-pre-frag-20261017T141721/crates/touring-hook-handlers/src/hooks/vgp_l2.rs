//! Layer-2 (pooled language-server verifier) glue for the VGP shadow hooks — builds the
//! [`SombraExtra`] that the shared `sombra_check` merges into the shadow line (`camadas: [1, 2]`).
//! It lives here and not in touring-code so the dependency graph stays acyclic:
//! touring-hook-handlers → touring-pyright (a crate extracted from touring-lsp with zero
//! touring-* deps — the lsp-bridge's optional touring-hooks edge is what made this wiring
//! impossible before the split).
//!
//! The verifier follows the input's language (`pool::checar_lang`: pyright, rust-analyzer,
//! tsserver or deno lsp), but only where the project turned layer 2 on: Python by default, every
//! other language through `[vgp] camada2_langs` (`touring_code::vgp::sombra::camada2_ligada`).
//!
//! The pool is process-gated: the daemon calls `habilitar_neste_processo()` at startup, so in the
//! daemon this runs for real; in the ephemeral STANDALONE hook `checar_lang` returns `unknown`
//! with a reason and spawns nothing (fail-open — the shadow line carries `camada2` with the motivo).

use touring_code::vgp::sombra::{SombraExtra, camada2_ligada};
use touring_code::vgp::{CheckInput, Finding};
use touring_pyright::pyright::{Severidade, Verdict};

/// Runs the pooled layer-2 verifier of the input's language over the same input layer 1 judged,
/// mapped into vgp terms. `None` when layer 2 is off for that language in this project — the
/// shadow line then keeps `camadas: [1]`.
pub(crate) fn extra(input: &CheckInput) -> Option<SombraExtra> {
    if !camada2_ligada(&input.lang, &input.project_root) {
        return None;
    }
    let overlays: Vec<(String, String)> = input
        .overlays
        .iter()
        .map(|(origem, texto)| (origem.clone(), texto.clone()))
        .collect();
    let l2 = touring_pyright::pool::checar_lang(
        &input.lang,
        &input.project_root,
        &input.path,
        &input.content,
        input.disk.as_deref(),
        &overlays,
    );
    Some(SombraExtra {
        findings: l2
            .achados
            .iter()
            .map(|a| {
                Finding::new(
                    &a.classe,
                    &a.nome,
                    a.linha as usize,
                    match a.severidade {
                        Severidade::Deny => "deny",
                        Severidade::Warn => "warn",
                    },
                    a.mensagem.clone(),
                    Vec::new(),
                )
            })
            .collect(),
        elapsed_ms: l2.elapsed_ms as f64,
        camada2_motivo: (l2.verdict == Verdict::Unknown).then(|| {
            l2.motivo
                .clone()
                .unwrap_or_else(|| "sem motivo declarado".to_string())
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    fn entrada(lang: &str, raiz: &Path) -> CheckInput {
        CheckInput {
            path: raiz.join("src/a.x").display().to_string(),
            content: "x\n".into(),
            project_root: raiz.to_path_buf(),
            disk: None,
            overlays: HashMap::new(),
            lang: lang.into(),
        }
    }

    fn projeto(toml: Option<&str>) -> tempfile::TempDir {
        let raiz = tempfile::tempdir().unwrap();
        if let Some(texto) = toml {
            std::fs::create_dir_all(raiz.path().join(".touring")).unwrap();
            std::fs::write(raiz.path().join(".touring/touring.toml"), texto).unwrap();
        }
        raiz
    }

    #[test]
    fn sem_declaracao_so_python_tem_camada2() {
        let raiz = projeto(None);
        for lang in ["rust", "typescript", "tsx", "javascript", "html", "css"] {
            assert!(
                extra(&entrada(lang, raiz.path())).is_none(),
                "{lang}: camada 2 desligada por padrão, a linha da sombra fica em [1]"
            );
        }
        let py = extra(&entrada("python", raiz.path())).expect("python tem camada 2 por padrão");
        assert!(
            py.camada2_motivo.is_some(),
            "hook standalone: o pool não está habilitado, a camada 2 diz por que não mediu"
        );
    }

    #[test]
    fn a_lista_declarada_liga_a_linguagem_e_desliga_o_resto() {
        let raiz = projeto(Some("[vgp]\ncamada2_langs = [\"rust\"]\n"));
        let rust = extra(&entrada("rust", raiz.path())).expect("rust declarado");
        let motivo = rust
            .camada2_motivo
            .expect("sem Cargo.toml nem daemon: unknown");
        assert!(
            motivo.contains("rust-analyzer"),
            "o motivo vem do verificador de Rust (checar_lang), não do pyright: {motivo}"
        );
        assert!(
            extra(&entrada("python", raiz.path())).is_none(),
            "a lista declarada substitui o padrão: python fora dela, desligado"
        );
        assert!(extra(&entrada("typescript", raiz.path())).is_none());
    }

    #[test]
    fn linguagem_declarada_sem_verificador_diz_que_nao_mediu() {
        // `html` não tem servidor: ligada à mão, a camada 2 responde unknown com o motivo, nunca vazio
        let raiz = projeto(Some("[vgp]\ncamada2_langs = [\"html\"]\n"));
        let h = extra(&entrada("html", raiz.path())).expect("html declarado");
        assert!(h.findings.is_empty());
        assert!(
            h.camada2_motivo
                .as_deref()
                .is_some_and(|m| m.contains("html")),
            "ausência de verificador não pode virar allow calado: {:?}",
            h.camada2_motivo
        );
    }
}
