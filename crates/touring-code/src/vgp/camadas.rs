//! The two-layer VGP verdict, composed in ONE place.
//!
//! Layer 1 is `vgp::check_em`; layer 2 is the project's own checker (`touring_pyright::pool`).
//! Until 09/11/2026 the bridge between them — moment the severities are asked at, severity to
//! vgp terms, the receiver class, the merge and the aggregate verdict — was written three times:
//! the CLI (`touring vgp check --camadas 1,2`), the shadow hook (`vgp_l2::mapear` +
//! `sombra_check_em`) and a test. The copies already differed (an `unknown` layer 2 was merged by
//! the shadow and not by the CLI). Every caller now asks this module, so the shadow measures what
//! the CLI answers.
//!
//! The pool itself (which process may spawn a language server, with what budget) stays with the
//! caller: this module is pure over an [`L2Resultado`] already obtained.
//!
//! Not unified here, because each changes a verdict and waits for a pre-registered measurement
//! (J18): the moment rule of layer 2 (`Balde`) still differs from layer 1's `roda_agora` for
//! `.cache/` and for a project rooted under `/tmp` — see
//! `docs/plans/2026-11-09-work-outer/bfce/DECISOES.md`.

use touring_pyright::achado::{Escopo, rebaixar_tmp_na_escrita};
use touring_pyright::pyright::{L2Resultado, Severidade, Verdict, classe_receptora};

use super::politica::verdict_dos;
use super::sombra::{SombraExtra, fundir_findings};
use super::{Alvo, CheckInput, CheckOutput, Contexto, Finding};

/// A layer-2 result in vgp terms, with the severities of the moment it is asked at: WRITTEN under
/// `/tmp`, a finding is a fragment's (the next edit may complete it, as layer 1 rules); about to
/// be EXECUTED, the `/tmp` bucket's verdict stands. `orcamento_ms` is the budget the checker ran
/// under (reported on the line).
pub fn camada2_em_vgp(
    mut l2: L2Resultado,
    input: &CheckInput,
    contexto: Contexto,
    orcamento_ms: u64,
) -> SombraExtra {
    if contexto == Contexto::Escrita {
        rebaixar_tmp_na_escrita(&mut l2.achados, &input.path, &input.project_root);
    }
    SombraExtra {
        findings: l2
            .achados
            .iter()
            .map(|a| {
                let mut f = Finding::new(
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
                .com_receptor(classe_receptora(&a.mensagem));
                f.alvo = alvo_do_escopo(a.escopo);
                f
            })
            .collect(),
        elapsed_ms: l2.elapsed_ms as f64,
        camada2_motivo: (l2.verdict == Verdict::Unknown).then(|| {
            l2.motivo
                .clone()
                .unwrap_or_else(|| "sem motivo declarado".to_string())
        }),
        l2_budget_ms: Some(orcamento_ms),
    }
}

/// Merge layer 2 into layer 1's output: `layer` 2, budget and wall time reported; an `unknown`
/// layer 2 annotates `camada2` and leaves layer 1's answer standing (fail-open, never an implicit
/// allow); otherwise findings dedupe by (nome, linha), the stronger severity wins
/// ([`fundir_findings`]), and the verdict aggregates both layers (deny > warn > allow).
pub fn fundir_camada2(out: &mut CheckOutput, extra: SombraExtra) {
    out.layer = 2;
    out.l2_budget_ms = extra.l2_budget_ms;
    out.elapsed_ms += extra.elapsed_ms;
    if let Some(motivo) = extra.camada2_motivo {
        out.camada2 = Some(format!("unknown: {motivo}"));
        return;
    }
    fundir_findings(&mut out.findings, extra.findings);
    out.verdict = verdict_dos(&out.findings, &out.verdict);
}

/// What a layer-2 finding is about, in layer 1's terms. Only the shadow line's `escopo` reads it
/// (the verdict reads the severity): an external target, a project symbol, or a doubt.
fn alvo_do_escopo(escopo: Escopo) -> Alvo {
    match escopo {
        Escopo::Externo => Alvo::Externo,
        Escopo::Proprio | Escopo::Projeto => Alvo::Projeto,
        Escopo::Forma | Escopo::Tipo | Escopo::Dinamico => Alvo::Incerto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use touring_pyright::achado::L2Achado;

    fn entrada(path: &str) -> CheckInput {
        CheckInput {
            path: path.to_string(),
            content: String::new(),
            lang: "python".into(),
            project_root: "/proj".into(),
            disk: None,
            overlays: HashMap::new(),
        }
    }

    fn achado(nome: &str, severidade: Severidade, escopo: Escopo) -> L2Achado {
        L2Achado {
            classe: "atributo".into(),
            nome: nome.into(),
            linha: 3,
            severidade,
            escopo,
            regra: "reportAttributeAccessIssue".into(),
            mensagem: format!("\"{nome}\" is not a known attribute"),
        }
    }

    fn resultado(achados: Vec<L2Achado>, verdict: Verdict) -> L2Resultado {
        L2Resultado {
            verdict,
            achados,
            elapsed_ms: 7,
            motivo: None,
        }
    }

    fn base() -> CheckOutput {
        CheckOutput {
            verdict: "allow".into(),
            layer: 1,
            lang: "python".into(),
            elapsed_ms: 1.0,
            findings: Vec::new(),
            camada2: None,
            l2_budget_ms: None,
            motivo: None,
        }
    }

    #[test]
    fn a_layer_two_denial_becomes_the_verdict_and_carries_its_target() {
        let l2 = resultado(
            vec![achado("loadz", Severidade::Deny, Escopo::Externo)],
            Verdict::Deny,
        );
        let mut out = base();
        fundir_camada2(
            &mut out,
            camada2_em_vgp(l2, &entrada("/proj/a.py"), Contexto::Escrita, 1000),
        );
        assert_eq!((out.verdict.as_str(), out.layer), ("deny", 2));
        assert_eq!(out.findings[0].alvo, Alvo::Externo);
        assert_eq!(out.l2_budget_ms, Some(1000));
        assert!((out.elapsed_ms - 8.0).abs() < f64::EPSILON);
    }

    /// An unknown layer 2 never changes layer 1's answer, and says why.
    #[test]
    fn an_unknown_layer_two_annotates_and_leaves_layer_one_standing() {
        let mut l2 = resultado(
            vec![achado("x", Severidade::Deny, Escopo::Externo)],
            Verdict::Unknown,
        );
        l2.motivo = Some("orçamento esgotado".into());
        let mut out = base();
        fundir_camada2(
            &mut out,
            camada2_em_vgp(l2, &entrada("/proj/a.py"), Contexto::Escrita, 1000),
        );
        assert_eq!(out.verdict, "allow");
        assert!(out.findings.is_empty());
        assert_eq!(out.camada2.as_deref(), Some("unknown: orçamento esgotado"));
    }

    #[test]
    fn layer_two_scopes_map_onto_layer_one_targets() {
        assert_eq!(alvo_do_escopo(Escopo::Externo), Alvo::Externo);
        assert_eq!(alvo_do_escopo(Escopo::Proprio), Alvo::Projeto);
        assert_eq!(alvo_do_escopo(Escopo::Projeto), Alvo::Projeto);
        for duvida in [Escopo::Forma, Escopo::Tipo, Escopo::Dinamico] {
            assert_eq!(alvo_do_escopo(duvida), Alvo::Incerto);
        }
    }
}
