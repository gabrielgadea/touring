//! The Html adapter (VHTML) — layer-1 checks over a proposed HTML document.
//!
//! Two classes (VL0 §4), both measured against the DISK and the document itself, never the
//! network (a CDN URL is `cdn`'s, a stylesheet class is `classe_css`'s):
//! - `recurso`: a local `src` / `href` that does not exist on disk (module `recurso`);
//! - `ancora`: an id referenced by `href="#x"`, `<label for>`, `aria-labelledby` /
//!   `aria-describedby` or an inline `getElementById` / `querySelector('#x')` that no element
//!   carries, and an id declared twice (modules `ancora` and `script`).
//!
//! The adapter MEASURES and reports the [`Alvo`](crate::vgp::politica::Alvo) of each finding;
//! `check` decides the severity.
//! A missing relative file, or an absent id on a page no script can touch, is `Alvo::Projeto`
//! (the disk decides); the shapes that are uncertain for real are `Alvo::Incerto` (a root-relative
//! URL, a page with external or dynamic scripts, a DOM lookup, a duplicate id). Neither ever
//! denies: `recurso` and `ancora` are consultive classes of the policy
//! (`CLASSES_CONSULTIVAS`) — a missing file or id is what the next write creates (the
//! `index.html` goes out before the `style.css` it links), and spec §5.1 reserves the "runs now"
//! `deny` for symbol classes. The only HTML `deny` is a confirmed CDN 404, the `cdn` class's.
//! Whatever the adapter cannot read — a templated URL, a fragment, a syntax error in a script —
//! goes to `nao_medido` with the reason, so a clean answer is never silence.
//!
//! The parse is `tree-sitter-html` through [`super::parse`]; `<script>` bodies are `raw_text`
//! nodes re-parsed with the `javascript` grammar (module `script`); the tree is read once into a
//! `Documento` (module `documento`).

mod ancora;
mod documento;
mod recurso;
mod script;
#[cfg(test)]
mod tests;
mod texto;

use std::collections::HashSet;

use super::{Adaptador, Verificacao, parse};
use crate::vgp::{CheckInput, Finding};
use documento::Documento;

/// Documents above this size are not parsed: the hook budget is p95 ≤ 150 ms and the parse costs
/// about 0.2 s per MiB (measured on the analise tree: 4.7 MiB → 0.85 s, 42 MiB → 4.2 s). Such a file
/// is a generated artifact (a map, a scraped dump), not something an edit proposes.
const TETO_DO_DOCUMENTO: usize = 4 << 20;

/// The `html` adapter registered in `super::REGISTRO`.
pub struct Html;

impl Adaptador for Html {
    fn lang(&self) -> &'static str {
        "html"
    }

    fn verificar(&self, input: &CheckInput) -> Verificacao {
        if input.content.len() > TETO_DO_DOCUMENTO {
            return Verificacao::nao_medido(format!(
                "documento de {} MiB acima do limite de leitura ({} MiB)",
                input.content.len() >> 20,
                TETO_DO_DOCUMENTO >> 20
            ));
        }
        let tree = match parse("html", &input.content) {
            Ok(t) => t,
            Err(motivo) => return Verificacao::nao_medido(motivo),
        };
        let doc = Documento::ler(&input.content, &tree);
        let mut saida = Saida::default();
        if doc.tem_erro {
            saida.nao_medido("html com erro de sintaxe: parte do documento pode não ter sido lida");
        }
        let scripts = script::analisar(&doc);
        for motivo in &scripts.motivos {
            saida.nao_medido(motivo.clone());
        }
        recurso::verificar(input, &doc, &mut saida);
        let presentes: HashSet<&str> = ancora::ids_presentes(&doc, &input.content, &scripts);
        ancora::verificar(&doc, &input.content, &scripts, &presentes, &mut saida);
        script::verificar_lookups(&scripts, &presentes, &mut saida);
        saida.concluir()
    }
}

/// What the checks of one document produce: findings (one per class and name) and the reasons
/// something was not measured.
#[derive(Default)]
struct Saida {
    achados: Vec<Finding>,
    motivos: Vec<String>,
}

impl Saida {
    /// Keep the first finding of a (class, name) — the judge counts presence, not occurrences.
    fn achado(&mut self, f: Finding) {
        if !self
            .achados
            .iter()
            .any(|a| a.classe == f.classe && a.nome == f.nome)
        {
            self.achados.push(f);
        }
    }

    fn nao_medido(&mut self, motivo: impl Into<String>) {
        let motivo = motivo.into();
        if !self.motivos.contains(&motivo) {
            self.motivos.push(motivo);
        }
    }

    fn concluir(self) -> Verificacao {
        Verificacao {
            achados: self.achados,
            nao_medido: (!self.motivos.is_empty()).then(|| self.motivos.join("; ")),
        }
    }
}
