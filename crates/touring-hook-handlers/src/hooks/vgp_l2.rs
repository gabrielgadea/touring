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
use touring_code::vgp::{CheckInput, Contexto, Finding};
use touring_pyright::pyright::{L2Resultado, Severidade, Verdict, classe_receptora};

/// Runs the pooled layer-2 verifier of the input's language over the same input layer 1 judged,
/// mapped into vgp terms. `None` when layer 2 is off for that language in this project — the
/// shadow line then keeps `camadas: [1]`. The verdict of a file being WRITTEN
/// ([`Contexto::Escrita`]); a file about to be run asks [`extra_em`].
pub(crate) fn extra(input: &CheckInput) -> Option<SombraExtra> {
    extra_em(input, Contexto::Escrita)
}

/// [`extra`] at an explicit moment: layer 2 follows the same rule as layer 1 — a fragment written
/// under `/tmp` is judged as a project file (only an external target denies), the same file about
/// to be EXECUTED keeps the `/tmp` bucket's verdict.
pub(crate) fn extra_em(input: &CheckInput, contexto: Contexto) -> Option<SombraExtra> {
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
    Some(mapear(l2, input, contexto))
}

/// A layer-2 result in vgp terms, with the severities of the moment it is asked at: written under
/// `/tmp`, a finding is a fragment's ([`touring_pyright::achado::rebaixar_tmp_na_escrita`]);
/// executed, the `/tmp` bucket's verdict stands. Pure — the pool is the caller's.
fn mapear(mut l2: L2Resultado, input: &CheckInput, contexto: Contexto) -> SombraExtra {
    if contexto == Contexto::Escrita {
        touring_pyright::achado::rebaixar_tmp_na_escrita(
            &mut l2.achados,
            &input.path,
            &input.project_root,
        );
    }
    SombraExtra {
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
                // the receiver class the checker named: layer 1's closed-type finding at the same
                // (name, line) is corroborated by it in `fundir_findings` (P0B-d)
                .com_receptor(classe_receptora(&a.mensagem))
            })
            .collect(),
        elapsed_ms: l2.elapsed_ms as f64,
        camada2_motivo: (l2.verdict == Verdict::Unknown).then(|| {
            l2.motivo
                .clone()
                .unwrap_or_else(|| "sem motivo declarado".to_string())
        }),
        l2_budget_ms: Some(touring_pyright::pyright::l2_budget_ms()),
    }
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

    /// FRAG: um achado de camada 2 por escopo, com a severidade que o balde `/tmp` lhe dá.
    fn l2_do_tmp() -> L2Resultado {
        use touring_pyright::achado::{Balde, severidade};
        use touring_pyright::pyright::{Escopo, L2Achado};
        let achados = [
            Escopo::Proprio,
            Escopo::Projeto,
            Escopo::Externo,
            Escopo::Forma,
            Escopo::Dinamico,
        ]
        .into_iter()
        .map(|escopo| L2Achado {
            classe: "nome".into(),
            nome: format!("{escopo:?}").to_lowercase(),
            linha: 1,
            escopo,
            severidade: severidade(Balde::Tmp, escopo),
            regra: "reportUndefinedVariable".into(),
            mensagem: "m".into(),
        })
        .collect();
        L2Resultado::from_achados(achados, 3)
    }

    fn severidades(extra: &SombraExtra) -> Vec<(&str, &str)> {
        extra
            .findings
            .iter()
            .map(|f| (f.nome.as_str(), f.severidade.as_str()))
            .collect()
    }

    #[test]
    fn a_camada_2_segue_o_momento_na_escrita_o_fragmento_em_tmp_so_avisa_na_execucao_nega() {
        let raiz = std::path::Path::new("/home/u/proj");
        let mut tmp = entrada("python", raiz);
        tmp.path = "/tmp/frag.py".into();
        let escrita = mapear(l2_do_tmp(), &tmp, Contexto::Escrita);
        assert_eq!(
            severidades(&escrita),
            [
                ("proprio", "warn"),
                ("projeto", "warn"),
                ("externo", "deny"),
                ("forma", "warn"),
                ("dinamico", "warn")
            ],
            "escrita em /tmp: só o alvo EXTERNO nega"
        );
        let execucao = mapear(l2_do_tmp(), &tmp, Contexto::Execucao);
        assert_eq!(
            severidades(&execucao),
            [
                ("proprio", "deny"),
                ("projeto", "deny"),
                ("externo", "deny"),
                ("forma", "warn"),
                ("dinamico", "warn")
            ],
            "execução: o balde /tmp inteiro volta"
        );
        // o inline roda agora em qualquer momento: a escrita não o rebaixa
        let mut inline = entrada("python", raiz);
        inline.path = "<string>".into();
        let achados = l2_do_tmp(); // severidades do /tmp == as do inline
        let escrita_inline = mapear(achados, &inline, Contexto::Escrita);
        assert_eq!(severidades(&escrita_inline)[0], ("proprio", "deny"));
    }

    // ---- P0B-d: "tipo fechado" corroborado pelas duas camadas ------------------------------------

    /// A pyright `Cannot access attribute` as layer 2 reduces it: `Escopo::Tipo`, hence `warn`.
    fn achado_de_membro(
        nome: &str,
        linha: u32,
        classe: &str,
    ) -> touring_pyright::pyright::L2Achado {
        use touring_pyright::achado::{Balde, severidade};
        use touring_pyright::pyright::Escopo;
        touring_pyright::pyright::L2Achado {
            classe: "membro".into(),
            nome: nome.into(),
            linha,
            escopo: Escopo::Tipo,
            severidade: severidade(Balde::Arquivo, Escopo::Tipo),
            regra: "reportAttributeAccessIssue".into(),
            mensagem: format!(
                "Cannot access attribute \"{nome}\" for class \"{classe}\"\n  Attribute \"{nome}\" is unknown"
            ),
        }
    }

    /// Layer 1 on `fonte`, layer 2's diagnostic through the REAL glue (`mapear` reads the receiver
    /// class off the checker's message), then the merge — what the shadow hook and the CLI do.
    fn camadas_1_e_2(
        fonte: &str,
        nome: &str,
        linha: u32,
        classe: &str,
    ) -> touring_code::vgp::CheckOutput {
        camadas_1_e_2_em(
            "/home/u/proj/a.py",
            Contexto::Escrita,
            fonte,
            nome,
            linha,
            classe,
        )
    }

    /// [`camadas_1_e_2`] at an explicit file and moment (both layers get the same `contexto`, as in
    /// `sombra_check_em`), so FRAG's write-time `/tmp` rule applies to each layer's own severities.
    fn camadas_1_e_2_em(
        path: &str,
        contexto: Contexto,
        fonte: &str,
        nome: &str,
        linha: u32,
        classe: &str,
    ) -> touring_code::vgp::CheckOutput {
        let mut input = entrada("python", std::path::Path::new("/home/u/proj"));
        input.path = path.into();
        input.content = fonte.into();
        let mut out = touring_code::vgp::check_em(&input, contexto);
        let l2 = L2Resultado::from_achados(vec![achado_de_membro(nome, linha, classe)], 3);
        let extra = mapear(l2, &input, contexto);
        touring_code::vgp::sombra::fundir_findings(&mut out.findings, extra.findings);
        out.verdict = touring_code::vgp::politica::verdict_dos(&out.findings, &out.verdict);
        out
    }

    #[test]
    fn p0bd_a_corroboracao_nega_em_todo_balde_e_momento_a_politica_do_frag_nao_a_mascara() {
        // d94f8090d150 lives in `/tmp/eleitoral_probe/causa_raiz.py`. FRAG downgrades a WRITE under
        // `/tmp` to a warning for each layer's OWN severity (a fragment is no error until it runs), but
        // the corroborated closed-type finding is neither a project symbol the next edit may create
        // nor a severity of either layer: it denies in every bucket and at either moment. The case
        // outside `/tmp` and the EXECUTION moment are the ones FRAG's own tests do not cover.
        for (path, contexto) in [
            ("/home/u/proj/a.py", Contexto::Escrita),
            ("/tmp/eleitoral_probe/causa_raiz.py", Contexto::Escrita),
            ("/tmp/eleitoral_probe/causa_raiz.py", Contexto::Execucao),
            ("<string>", Contexto::Escrita),
        ] {
            // each layer alone only warns — nothing but the corroboration can have made it a deny
            let so_l1 = {
                let mut input = entrada("python", std::path::Path::new("/home/u/proj"));
                input.path = path.into();
                input.content = FONTE_STAT.into();
                touring_code::vgp::check_em(&input, contexto)
            };
            assert_eq!(so_l1.verdict, "warn", "{path} {contexto:?}: L1 sozinha");
            let out = camadas_1_e_2_em(path, contexto, FONTE_STAT, "st_m_size", 3, "stat_result");
            assert_eq!(
                out.verdict, "deny",
                "{path} {contexto:?}: {:?}",
                out.findings
            );
            // …and the class check holds at every bucket and moment too
            let out = camadas_1_e_2_em(path, contexto, FONTE_STAT, "st_m_size", 3, "Foo");
            assert_eq!(out.verdict, "warn", "{path} {contexto:?}: classe diferente");
        }
    }

    const FONTE_STAT: &str =
        "from pathlib import Path\nst = Path('x').stat()\nprint(st.st_m_size)\n";

    #[test]
    fn p0bd_as_duas_camadas_pelo_encaixe_real_negam_o_membro_de_tipo_fechado() {
        // d94f8090d150: `st.st_m_size` raises at run time — layer 1 knows the closed type, layer 2
        // names it (qualified or not): together they deny
        for classe in ["stat_result", "os.stat_result"] {
            let out = camadas_1_e_2(FONTE_STAT, "st_m_size", 3, classe);
            assert_eq!(out.verdict, "deny", "{classe}: {:?}", out.findings);
            assert_eq!(out.findings.len(), 1, "{classe}: {:?}", out.findings);
            assert_eq!(out.findings[0].severidade, "deny", "{classe}");
        }
        // another receiver class: layer 2 talks about an object layer 1 never typed → still a warning
        let out = camadas_1_e_2(FONTE_STAT, "st_m_size", 3, "Foo");
        assert_eq!(out.verdict, "warn", "{:?}", out.findings);
        // 0893b1f5187f: a `str` receiver is not a closed type, whatever the checker says
        let out = camadas_1_e_2("x = 'abc'\nx.setdefault(1)\n", "setdefault", 2, "str");
        assert_eq!(out.verdict, "warn", "{:?}", out.findings);
    }

    #[test]
    fn p0bd_mapear_carrega_a_classe_receptora_so_de_diagnostico_de_membro() {
        use touring_pyright::pyright::{Escopo, L2Achado};
        let input = entrada("python", std::path::Path::new("/home/u/proj"));
        let modulo = L2Achado {
            classe: "atributo".into(),
            nome: "pathx".into(),
            linha: 1,
            escopo: Escopo::Externo,
            severidade: Severidade::Deny,
            regra: "reportAttributeAccessIssue".into(),
            mensagem: "\"pathx\" is not a known attribute of module \"os\"".into(),
        };
        let l2 = L2Resultado::from_achados(
            vec![achado_de_membro("st_m_size", 2, "os.stat_result"), modulo],
            3,
        );
        let extra = mapear(l2, &input, Contexto::Escrita);
        let receptores: Vec<(&str, Option<&str>)> = extra
            .findings
            .iter()
            .map(|f| (f.nome.as_str(), f.receptor.as_deref()))
            .collect();
        assert_eq!(
            receptores,
            [("st_m_size", Some("os.stat_result")), ("pathx", None)]
        );
    }
}
