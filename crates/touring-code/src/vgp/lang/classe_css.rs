//! Transversal check `classe_css` (VCSS, VL0 §4): a class USED in html `class="…"` or JSX
//! `className="…"` that no LOADED stylesheet defines — with "você quis dizer" from the classes
//! that are defined.
//!
//! It is advisory by policy (§5.2: a class may be a hook for JS), so it never denies; what it
//! must get right is when NOT to speak. It stays silent — `nao_medido` with the reason, never a
//! guess — whenever the sheets cannot all be read or the classes are not all written in sheets:
//!
//! * a sheet that is missing, a preprocessor file (`.scss`…), an unreadable `@import`, or a
//!   remote sheet the CDN cache ([`super::cdn::cache`]) has no classes for;
//! * a utility-class generator or CSS-in-JS library (Tailwind, UnoCSS, styled-components,
//!   emotion…) in the document, its sheets or the package it belongs to;
//! * a template (`{% extends %}`, `<?php`…), a fragment with no `<html>`/`<head>`, or a script
//!   that builds sheets at run time;
//! * no sheet at all (nothing to measure against);
//! * rich-text editor output (an element carries `contenteditable`): the classes the editor
//!   injects are styled by the editor's own sheet, which an exported file does not bring — the
//!   classes that would be reported are withheld, and the withholding is itself reported.
//!
//! Dynamic values (`className={x}`, `class="a {{ b }}"`) are skipped, `js-*` names and classes
//! the document's own script drives by name are hooks, and `[class*="x"]` selectors count as
//! definitions. For html the loaded sheets are the document's own (`<style>`, `<link
//! rel=stylesheet>`, `@import`); for JSX they are every stylesheet the project owns plus the ones
//! the file imports (a component cannot know which global sheet the app loads, so "no sheet of the
//! project defines it" is the claim that stays sound). A sheet served by a CDN is read through the
//! CDN cache: the network resolver ([`super::cdn::resolvedor`]) records the classes of the `.css`
//! files it downloads, so layer 1 stays pure.

mod folhas;
mod projeto;
#[cfg(test)]
mod tests;
mod usos;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use regex::Regex;

use self::folhas::{Folhas, Leitor, Resolucao};
use self::projeto::Limites;
use self::usos::Uso;
use super::cdn::cache::Cache;
use super::cdn::documento::{self, Documento};
use super::{Transversal, Verificacao};
use crate::vgp::politica::Alvo;
use crate::vgp::{CheckInput, Finding};

/// A document that is a template: its sheets can come from another file.
static TEMPLATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"\{%-?\s*(?:extends|include|import|block|load)\b|<\?php|<%[@=-]|@(?:extends|include|section)\s*\("#,
    )
    .expect("regex constante")
});

/// Script `src` fragments of runtime utility-class generators.
const SCRIPTS_DE_GERADOR: [(&str, &str); 4] = [
    ("tailwind", "Tailwind"),
    ("unocss", "UnoCSS"),
    ("windi", "Windi CSS"),
    ("twind", "Twind"),
];

/// How many "did you mean" candidates a finding carries.
const MAX_SUGESTOES: usize = 5;

/// The `classe_css` transversal check.
pub struct ClasseCss;

/// What a stylesheet defines, in the shape the CDN cache stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolhaLida {
    /// Defined class names, sorted.
    pub classes: Vec<String>,
    /// `[class…=…]` patterns: (operator, value).
    pub padroes: Vec<(String, String)>,
    /// `@import` targets, as written.
    pub imports: Vec<String>,
}

/// Read a downloaded stylesheet for the CDN cache; `None` when the grammar cannot run at all.
pub(crate) fn ler_folha(css: &str) -> Option<FolhaLida> {
    let def = folhas::ler_css(css).ok()?;
    let mut classes: Vec<String> = def.classes.into_iter().collect();
    classes.sort();
    Some(FolhaLida {
        classes,
        padroes: def.padroes,
        imports: def.imports,
    })
}

/// Edit distance (Levenshtein) between two class names, by characters.
fn distancia(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut anterior: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut atual = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let troca = anterior[j] + usize::from(ca != cb);
            atual.push(troca.min(anterior[j + 1] + 1).min(atual[j] + 1));
        }
        anterior = atual;
    }
    anterior[b.len()]
}

/// The defined classes closest to `nome` — a typo is a few edits from the class meant.
fn mais_proximas(nome: &str, folhas: &Folhas) -> Vec<String> {
    let limite = (nome.chars().count() / 4).clamp(1, 3);
    let mut candidatas: Vec<(usize, &String)> = folhas
        .classes()
        .filter(|d| d.len().abs_diff(nome.len()) <= limite * 2 && d.as_str() != nome)
        .filter_map(|d| {
            let dist = distancia(nome, d);
            (dist <= limite).then_some((dist, d))
        })
        .collect();
    candidatas.sort();
    candidatas.dedup();
    candidatas.truncate(MAX_SUGESTOES);
    candidatas.into_iter().map(|(_, d)| d.clone()).collect()
}

fn achado_de(uso: &Uso, folhas: &Folhas) -> Finding {
    let proximas = mais_proximas(&uso.classe, folhas);
    let dica = match proximas.as_slice() {
        [] => String::new(),
        lista => format!(
            " — você quis dizer {}?",
            lista
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    Finding::candidato(
        "classe_css",
        &uso.classe,
        uso.linha,
        Alvo::Projeto,
        format!(
            "a classe `{}` não é definida por nenhuma folha carregada ({} lida(s)){dica}",
            uso.classe, folhas.lidas
        ),
        proximas,
    )
}

/// The used classes no loaded sheet defines, once each, at the line of the first use.
fn julgar(usos: &[Uso], folhas: &Folhas, ganchos: &HashSet<String>) -> Vec<Finding> {
    let mut vistos = HashSet::new();
    let mut achados = Vec::new();
    for uso in usos {
        if !vistos.insert(uso.classe.as_str()) {
            continue;
        }
        let gancho_por_convencao = uso.classe.starts_with("js-") || uso.classe.starts_with("js_");
        if gancho_por_convencao || ganchos.contains(&uso.classe) || folhas.define(&uso.classe) {
            continue;
        }
        achados.push(achado_de(uso, folhas));
    }
    achados
}

/// Why the loaded sheets are not enough to judge, if so.
fn motivo_de_folhas(folhas: &Folhas, onde: &str) -> Option<String> {
    if !folhas.nao_resolvidas.is_empty() {
        return Some(folhas.nao_resolvidas.join("; "));
    }
    if let Some(gerador) = folhas.gerador {
        return Some(format!(
            "uma folha usa {gerador} (`@tailwind`/`@apply`…): as classes utilitárias são geradas, \
             não definidas"
        ));
    }
    (folhas.lidas == 0).then(|| format!("nenhuma folha {onde}: não há contra o que medir"))
}

/// The absolute path of the checked file (`path` may be relative to the project root).
fn caminho_do_arquivo(input: &CheckInput) -> PathBuf {
    let caminho = Path::new(&input.path);
    if caminho.is_absolute() {
        caminho.to_path_buf()
    } else {
        input.project_root.join(caminho)
    }
}

fn pasta_do_arquivo(input: &CheckInput) -> PathBuf {
    caminho_do_arquivo(input)
        .parent()
        .map_or_else(|| input.project_root.clone(), Path::to_path_buf)
}

fn e_link_de_folha(elemento: &documento::Elemento) -> bool {
    elemento.tag == "link"
        && elemento.atributo("rel").is_some_and(|rel| {
            rel.valor
                .split_whitespace()
                .any(|t| t.eq_ignore_ascii_case("stylesheet"))
        })
}

/// A runtime generator loaded by a `<script src>` of the document.
fn gerador_no_documento(doc: &Documento) -> Option<&'static str> {
    doc.elementos
        .iter()
        .filter(|e| e.tag == "script")
        .filter_map(|e| e.atributo("src"))
        .find_map(|src| {
            let src = src.valor.to_ascii_lowercase();
            SCRIPTS_DE_GERADOR
                .iter()
                .find(|(trecho, _)| src.contains(trecho))
                .map(|(_, nome)| *nome)
        })
}

/// Things about an html document that make "no loaded sheet defines it" unprovable.
fn barreira_html(doc: &Documento, texto: &str, dir: &Path, raiz: &Path) -> Option<String> {
    let mut motivos = Vec::new();
    if TEMPLATE.is_match(texto) {
        motivos.push(
            "o documento é um template (extends/include…): as folhas podem vir de outro arquivo"
                .to_string(),
        );
    }
    if !(doc.tem_html && doc.tem_head) {
        motivos.push(
            "fragmento (sem <html> e <head>): as folhas vêm da página que o inclui".to_string(),
        );
    }
    if usos::injeta_folhas(doc) {
        motivos
            .push("um script do documento cria ou edita folhas em tempo de execução".to_string());
    }
    if let Some(gerador) = gerador_no_documento(doc) {
        motivos.push(format!(
            "{gerador} carregado por <script>: as classes utilitárias são geradas, não definidas"
        ));
    }
    if let Some(gerador) = projeto::gerador_do_pacote(dir, raiz) {
        motivos.push(format!(
            "projeto com {gerador}: as classes utilitárias são geradas, não definidas em folha"
        ));
    }
    (!motivos.is_empty()).then(|| motivos.join("; "))
}

fn carregar_folhas_html(
    doc: &Documento,
    dir: &Path,
    raiz: &Path,
    leitor: &Leitor,
    folhas: &mut Folhas,
) {
    for bloco in &doc.estilos {
        let origem = format!("<style> da linha {}", bloco.linha);
        folhas.adicionar_texto(&bloco.texto, &origem, dir, raiz, leitor);
    }
    for elemento in doc.elementos.iter().filter(|e| e_link_de_folha(e)) {
        if let Some(href) = elemento.atributo("href") {
            let origem = format!("linha {}", href.linha);
            folhas.adicionar_href(&href.valor, &origem, dir, raiz, leitor);
        }
    }
}

fn medir_html(input: &CheckInput, leitor: &Leitor) -> Verificacao {
    if !input.content.contains("class") {
        return Verificacao::medido(Vec::new());
    }
    let doc = match documento::ler(&input.content) {
        Ok(d) => d,
        Err(e) => return Verificacao::nao_medido(format!("classe_css: {e}")),
    };
    let usos = usos::usos_html(&doc);
    if usos.is_empty() {
        return Verificacao::medido(Vec::new());
    }
    let dir = pasta_do_arquivo(input);
    let raiz = input.project_root.as_path();
    if let Some(motivo) = barreira_html(&doc, &input.content, &dir, raiz) {
        return Verificacao::nao_medido(format!("classe_css: {motivo}"));
    }
    let mut folhas = Folhas::default();
    carregar_folhas_html(&doc, &dir, raiz, leitor, &mut folhas);
    let onde = "carregada pelo documento (nem <style> nem <link rel=stylesheet>)";
    if let Some(motivo) = motivo_de_folhas(&folhas, onde) {
        return Verificacao::nao_medido(format!("classe_css: {motivo}"));
    }
    let ganchos = usos::classes_do_js(&doc);
    let achados = julgar(&usos, &folhas, &ganchos);
    if !achados.is_empty() && e_saida_de_editor(&doc) {
        return Verificacao::nao_medido(motivo_de_editor(&achados));
    }
    Verificacao::medido(achados)
}

/// Does the document come out of a rich-text editor (the SEI editor, CKEditor…)? Those editors
/// mark their elements `contenteditable` (any value, any case) and style the classes they inject
/// with their OWN sheet, which an exported file does not carry. The test is on the whole
/// document: an editable island is not isolated from the rest of the page.
fn e_saida_de_editor(doc: &Documento) -> bool {
    doc.elementos
        .iter()
        .any(|e| e.atributo("contenteditable").is_some())
}

/// The abstention, as a number: how many undefined classes were withheld, and the first ones.
fn motivo_de_editor(achados: &[Finding]) -> String {
    let nomes: Vec<&str> = achados.iter().take(3).map(|f| f.nome.as_str()).collect();
    let mais = if achados.len() > nomes.len() {
        ", …"
    } else {
        ""
    };
    format!(
        "classe_css: classes de saída de editor rico (contenteditable): a folha do editor não está no arquivo ({} classe(s) sem definição não avaliada(s): {}{mais})",
        achados.len(),
        nomes.join(", ")
    )
}

/// The sheets of the project's entry `index.html`: its `<style>`, its links, and a generator
/// script as the reason nothing can be proved.
fn carregar_html_de_entrada(caminho: &Path, raiz: &Path, leitor: &Leitor, folhas: &mut Folhas) {
    let Ok(texto) = leitor.ler(caminho) else {
        return;
    };
    let Ok(doc) = documento::ler(&texto) else {
        return;
    };
    let dir = caminho.parent().unwrap_or(raiz);
    if let Some(gerador) = gerador_no_documento(&doc) {
        folhas.nao_resolvidas.push(format!(
            "{} carrega {gerador} por <script>: as classes utilitárias são geradas",
            caminho.display()
        ));
    }
    carregar_folhas_html(&doc, dir, raiz, leitor, folhas);
}

/// Where a JS `import` of a stylesheet points, or why it cannot be read.
fn resolver_import(
    alvo: &str,
    dir: &Path,
    raiz: &Path,
    leitor: &Leitor,
) -> Result<PathBuf, String> {
    if alvo.starts_with('.') || alvo.starts_with('/') {
        return match folhas::resolver(alvo, dir, raiz, leitor) {
            Resolucao::Local(p) => Ok(p),
            Resolucao::Remota => Err("remota".to_string()),
            Resolucao::Ilegivel(motivo) => Err(motivo),
        };
    }
    if let Some(motivo) = folhas::recusa_previa(alvo) {
        return Err(motivo);
    }
    projeto::resolver_pacote(alvo, dir, raiz)
        .ok_or_else(|| format!("pacote não instalado (node_modules/{alvo})"))
}

/// Add the sheet a JS file imports (`import './x.css'`, `import 'pkg/x.css'`). A CSS module is
/// scoped, so it is only checked for existence — its classes are not global.
fn importar_folha(alvo: &str, dir: &Path, raiz: &Path, leitor: &Leitor, folhas: &mut Folhas) {
    match resolver_import(alvo, dir, raiz, leitor) {
        Ok(_) if alvo.contains(".module.") => folhas.conta_sem_absorver(),
        Ok(caminho) => folhas.adicionar_arquivo(&caminho, raiz, leitor),
        Err(motivo) => folhas
            .nao_resolvidas
            .push(format!("import `{alvo}` não resolvido: {motivo}")),
    }
}

/// The sheets a JSX file can see: every stylesheet the project owns, the entry `index.html`'s,
/// and the ones the file itself imports. `Err` when the project cannot be walked completely.
fn carregar_folhas_jsx(input: &CheckInput, dir: &Path, leitor: &Leitor) -> Result<Folhas, String> {
    let raiz = input.project_root.as_path();
    let mut folhas = Folhas::default();
    for folha in projeto::folhas_do_projeto(raiz, Limites::default())? {
        folhas.adicionar_arquivo(&folha, raiz, leitor);
    }
    for html in projeto::html_de_entrada(raiz) {
        carregar_html_de_entrada(&html, raiz, leitor, &mut folhas);
    }
    match usos::imports_de_folha(&input.lang, &input.content) {
        Ok(alvos) => {
            for alvo in alvos {
                importar_folha(&alvo, dir, raiz, leitor, &mut folhas);
            }
        }
        Err(e) => folhas.nao_resolvidas.push(e),
    }
    Ok(folhas)
}

fn medir_jsx(input: &CheckInput, leitor: &Leitor) -> Verificacao {
    if !input.content.contains("lass") {
        return Verificacao::medido(Vec::new());
    }
    let usos = match usos::usos_jsx(&input.lang, &input.content) {
        Ok(u) => u,
        Err(e) => return Verificacao::nao_medido(format!("classe_css: {e}")),
    };
    if usos.is_empty() {
        return Verificacao::medido(Vec::new());
    }
    let dir = pasta_do_arquivo(input);
    if let Some(gerador) = projeto::gerador_do_pacote(&dir, &input.project_root) {
        return Verificacao::nao_medido(format!(
            "classe_css: projeto com {gerador}: as classes utilitárias são geradas, não definidas em folha"
        ));
    }
    let folhas = match carregar_folhas_jsx(input, &dir, leitor) {
        Ok(f) => f,
        Err(e) => return Verificacao::nao_medido(format!("classe_css: {e}")),
    };
    if let Some(motivo) = motivo_de_folhas(&folhas, "CSS no projeto") {
        return Verificacao::nao_medido(format!("classe_css: {motivo}"));
    }
    Verificacao::medido(julgar(&usos, &folhas, &HashSet::new()))
}

impl ClasseCss {
    fn medir_com_leitor(&self, input: &CheckInput, leitor: &Leitor) -> Verificacao {
        match input.lang.as_str() {
            "html" => medir_html(input, leitor),
            "tsx" | "javascript" => medir_jsx(input, leitor),
            _ => Verificacao::medido(Vec::new()),
        }
    }

    /// Judge the classes `input` uses against the sheets it loads. Findings are `classe_css`
    /// ([`Alvo::Projeto`], advisory by policy); whatever could not be read or proved comes back
    /// in [`Verificacao::nao_medido`] with its reason — never as a silent `allow`. Remote sheets
    /// are read from the default CDN cache.
    pub fn medir(&self, input: &CheckInput) -> Verificacao {
        self.medir_com_leitor(input, &Leitor::novo(&input.overlays))
    }

    /// [`ClasseCss::medir`] against an explicit CDN cache (tests, tools).
    pub fn medir_com_cache(&self, input: &CheckInput, cache: Arc<Cache>) -> Verificacao {
        self.medir_com_leitor(input, &Leitor::com_cache(&input.overlays, cache))
    }
}

impl Transversal for ClasseCss {
    fn nome(&self) -> &'static str {
        "classe_css"
    }

    fn langs(&self) -> &'static [&'static str] {
        &["html", "tsx", "javascript"]
    }

    /// [`ClasseCss::medir`]: the findings and the reason for whatever stayed unmeasured.
    fn verificar(&self, input: &CheckInput) -> Verificacao {
        self.medir(input)
    }
}
