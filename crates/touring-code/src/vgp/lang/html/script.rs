//! The inline `<script>` bodies of an html document, re-parsed with the `javascript` grammar
//! (`<script>` text is a `raw_text` node in `tree-sitter-html`; the injection query names
//! `javascript` as its language).
//!
//! Two products, from one walk of each script:
//! - the DOM lookups with a literal argument (`document.getElementById('x')`,
//!   `document.querySelector('#x .y')`) — checked against the document's ids by
//!   [`verificar_lookups`];
//! - the ids a script may CREATE (a string equal to the id, an `id="x"` inside a template string,
//!   a template `<script type="text/template">`), so that neither a lookup nor an `href="#x"` is
//!   reported against an id the page builds at run time. Anything the walk cannot follow
//!   (`el.id = name`, `innerHTML = html`, a script with a syntax error, an unknown `type`) sets
//!   `dinamico`: the lookups stand down and the anchors lose their right to a `deny`.
//!
//! Grammar (`tree-sitter-javascript` 0.25.0, `src/node-types.json`): `call_expression` has the
//! fields `function` and `arguments`; `member_expression` has `object` and `property`;
//! `assignment_expression` has `left` and `right`; a `string` holds `string_fragment`s,
//! `escape_sequence`s and `html_character_reference`s; a `template_string` also holds
//! `template_substitution`s.

use std::collections::HashSet;

use tree_sitter::Node;

use super::Saida;
use super::documento::{Documento, Elemento, ScriptInline};
use super::texto::{ids_no_texto, tem_template};
use crate::vgp::Finding;
use crate::vgp::lang::parse;
use crate::vgp::politica::Alvo;

/// An inline script above this size is not re-parsed (the cost of the `javascript` parse would
/// dominate the hook): inlined data blobs and bundles are not what an edit writes by hand.
pub(super) const TETO_DO_SCRIPT: usize = 512 << 10;

/// The budget of ALL inline scripts of one document.
const TETO_TOTAL_DE_SCRIPTS: usize = 2 << 20;

/// What kind of text a `<script type=…>` holds.
enum Tipo {
    /// Classic or module JavaScript.
    Js,
    /// Data blocks (`application/json`, `importmap`…): not DOM code.
    Dados,
    /// Markup the page instantiates later (`text/template`, `text/html`…).
    Template,
    /// A type the adapter does not know: it may build anything.
    Desconhecido,
}

fn tipo(el: &Elemento) -> Tipo {
    let t = el.valor("type").unwrap_or("").trim().to_ascii_lowercase();
    match t.as_str() {
        ""
        | "module"
        | "text/javascript"
        | "application/javascript"
        | "text/ecmascript"
        | "application/ecmascript"
        | "application/x-javascript"
        | "text/x-javascript"
        | "text/jsx"
        | "text/babel" => Tipo::Js,
        "application/json"
        | "application/ld+json"
        | "importmap"
        | "speculationrules"
        | "text/json" => Tipo::Dados,
        _ if t.contains("template") || t == "text/html" || t.starts_with("text/x-handlebars") => {
            Tipo::Template
        }
        _ => Tipo::Desconhecido,
    }
}

/// One `getElementById` / `querySelector*` call with a literal argument.
pub(super) struct Lookup {
    pub id: String,
    /// 1-based line in the html document.
    pub linha: usize,
    /// Source of the call, for the teaching message (`document.getElementById('x')`).
    pub chamada: String,
}

/// The product of walking every inline script of a document.
#[derive(Default)]
pub(super) struct AnaliseScripts {
    pub lookups: Vec<Lookup>,
    /// Ids a script may create at run time.
    pub criados: HashSet<String>,
    /// Some script builds ids or markup the walk cannot follow.
    pub dinamico: bool,
    /// Why part of the scripts was not measured.
    pub motivos: Vec<String>,
    /// Bytes of script already parsed (against [`TETO_TOTAL_DE_SCRIPTS`]).
    bytes_lidos: usize,
}

/// Walk every inline script of `doc`.
pub(super) fn analisar(doc: &Documento) -> AnaliseScripts {
    let mut out = AnaliseScripts::default();
    for s in &doc.scripts {
        let el = &doc.elementos[s.elemento];
        if el.atributo("src").is_some() {
            continue; // a body next to a `src` is never executed
        }
        match tipo(el) {
            Tipo::Js => analisar_js(s, &mut out),
            Tipo::Dados => {}
            Tipo::Template => registrar_ids_do_texto(&s.texto, &mut out),
            Tipo::Desconhecido => out.dinamico = true,
        }
    }
    out
}

fn registrar_ids_do_texto(texto: &str, out: &mut AnaliseScripts) {
    for id in ids_no_texto(texto) {
        if tem_template(id) {
            out.dinamico = true;
        }
        out.criados.insert(id.to_string());
    }
}

fn analisar_js(s: &ScriptInline, out: &mut AnaliseScripts) {
    if s.texto.len() > TETO_DO_SCRIPT || out.bytes_lidos + s.texto.len() > TETO_TOTAL_DE_SCRIPTS {
        out.dinamico = true;
        out.motivos.push(format!(
            "script inline de {} KiB acima do limite de leitura: chamadas de DOM não medidas",
            s.texto.len() >> 10
        ));
        return;
    }
    out.bytes_lidos += s.texto.len();
    let tree = match parse("javascript", &s.texto) {
        Ok(t) if !t.root_node().has_error() => t,
        Ok(_) => {
            out.dinamico = true;
            out.motivos.push(format!(
                "script inline na linha {} com erro de sintaxe: chamadas de DOM não medidas",
                s.linha_inicial + 1
            ));
            return;
        }
        Err(e) => {
            out.dinamico = true;
            out.motivos.push(format!("script inline não lido: {e}"));
            return;
        }
    };
    let mut ignorados: HashSet<usize> = HashSet::new();
    let mut cur = tree.walk();
    loop {
        let no = cur.node();
        match no.kind() {
            "call_expression" => chamada(&s.texto, no, s.linha_inicial, &mut ignorados, out),
            "assignment_expression" | "augmented_assignment_expression" => {
                atribuicao(&s.texto, no, out)
            }
            "string" | "template_string" if !ignorados.contains(&no.id()) => {
                literal_solto(&s.texto, no, out)
            }
            _ => {}
        }
        if cur.goto_first_child() {
            continue;
        }
        while !cur.goto_next_sibling() {
            if !cur.goto_parent() {
                return;
            }
        }
    }
}

fn texto<'a>(fonte: &'a str, no: Node) -> &'a str {
    &fonte[no.byte_range()]
}

/// `document`, `window.document`, `globalThis.document`, `self.document`.
fn eh_document(fonte: &str, obj: Node) -> bool {
    match obj.kind() {
        "identifier" => texto(fonte, obj) == "document",
        "member_expression" => {
            let prop = obj.child_by_field_name("property");
            let base = obj.child_by_field_name("object");
            matches!((base, prop), (Some(b), Some(p))
                if texto(fonte, p) == "document"
                    && matches!(texto(fonte, b), "window" | "globalThis" | "self"))
        }
        _ => false,
    }
}

/// The text of a string literal with no escape and no interpolation; `None` otherwise (the
/// adapter never guesses what an escaped or interpolated string evaluates to).
fn literal(fonte: &str, no: Node) -> Option<String> {
    let bruto = texto(fonte, no);
    let miolo = match no.kind() {
        "string" => bruto.get(1..bruto.len().checked_sub(1)?)?,
        "template_string" => bruto.get(1..bruto.len().checked_sub(1)?)?,
        _ => return None,
    };
    let com_escape_ou_interpolacao = miolo.contains('\\') || miolo.contains("${");
    (!com_escape_ou_interpolacao).then(|| miolo.to_string())
}

fn argumentos(chamada: Node) -> Vec<Node> {
    let Some(args) = chamada.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut c = args.walk();
    args.named_children(&mut c).collect()
}

fn chamada(
    fonte: &str,
    no: Node,
    linha_inicial: usize,
    ignorados: &mut HashSet<usize>,
    out: &mut AnaliseScripts,
) {
    let Some((obj, nome)) = receptor_e_metodo(fonte, no) else {
        return;
    };
    let args = argumentos(no);
    if matches!(
        nome,
        "getElementById" | "querySelector" | "querySelectorAll"
    ) && eh_document(fonte, obj)
    {
        let linha = linha_inicial + no.start_position().row + 1;
        registrar_lookup(fonte, nome, &args, linha, ignorados, out);
    } else if cria_marcacao_dinamica(fonte, nome, obj, &args) {
        out.dinamico = true;
    }
}

/// `obj.nome(…)`: the receiver and the method name of a call.
fn receptor_e_metodo<'a>(fonte: &'a str, no: Node<'a>) -> Option<(Node<'a>, &'a str)> {
    let f = no.child_by_field_name("function")?;
    if f.kind() != "member_expression" {
        return None;
    }
    let obj = f.child_by_field_name("object")?;
    let prop = f.child_by_field_name("property")?;
    Some((obj, texto(fonte, prop)))
}

/// A lookup with a literal argument: remember each id it requires, and keep its string out of
/// the "created ids" set (it names an id, it does not build one).
fn registrar_lookup(
    fonte: &str,
    nome: &str,
    args: &[Node],
    linha: usize,
    ignorados: &mut HashSet<usize>,
    out: &mut AnaliseScripts,
) {
    let Some(arg) = args.first() else { return };
    let Some(lit) = literal(fonte, *arg) else {
        return;
    };
    ignorados.insert(arg.id());
    let ids = if nome == "getElementById" {
        vec![lit]
    } else {
        ids_do_seletor(&lit)
    };
    let chamada = format!("document.{nome}({})", texto(fonte, *arg));
    out.lookups.extend(ids.into_iter().map(|id| Lookup {
        id,
        linha,
        chamada: chamada.clone(),
    }));
}

/// `setAttribute('id', x)`, `insertAdjacentHTML(pos, x)`, `document.write(x)` with a non-literal
/// `x`: the script builds ids the walk cannot follow.
fn cria_marcacao_dinamica(fonte: &str, nome: &str, obj: Node, args: &[Node]) -> bool {
    let nao_literal = |i: usize| args.get(i).is_some_and(|a| literal(fonte, *a).is_none());
    match nome {
        "setAttribute" => {
            args.first().and_then(|a| literal(fonte, *a)).as_deref() == Some("id") && nao_literal(1)
        }
        "insertAdjacentHTML" => nao_literal(1),
        "write" | "writeln" => eh_document(fonte, obj) && (0..args.len()).any(nao_literal),
        _ => false,
    }
}

/// `el.id = x`, `el.innerHTML = x` (and `+=`): fine with a literal on the right, dynamic otherwise.
fn atribuicao(fonte: &str, no: Node, out: &mut AnaliseScripts) {
    let (Some(esq), Some(dir)) = (
        no.child_by_field_name("left"),
        no.child_by_field_name("right"),
    ) else {
        return;
    };
    if esq.kind() != "member_expression" {
        return;
    }
    let Some(prop) = esq.child_by_field_name("property") else {
        return;
    };
    let cria = matches!(texto(fonte, prop), "id" | "innerHTML" | "outerHTML");
    if cria && literal(fonte, dir).is_none() {
        out.dinamico = true;
    }
}

/// A string that is not a lookup argument: it may BE an id (`el.id = 'x'`, `{ id: 'x' }`) or hold
/// markup with ids (`'<div id="x">'`). Over-approximating what a script can create is the safe side.
fn literal_solto(fonte: &str, no: Node, out: &mut AnaliseScripts) {
    let bruto = texto(fonte, no);
    let Some(miolo) = bruto.get(1..bruto.len().saturating_sub(1)) else {
        return;
    };
    if !miolo.is_empty() && miolo.len() <= 200 && !miolo.contains(char::is_whitespace) {
        out.criados.insert(miolo.to_string());
    }
    registrar_ids_do_texto(miolo, out);
}

/// The ids a CSS selector REQUIRES in the document: `#a` of `#a .b > c`, never those inside
/// `:not(…)`/`:is(…)`/`[…]`, and none for a selector list (`#a, #b` matches with either) or one
/// with escapes.
fn ids_do_seletor(sel: &str) -> Vec<String> {
    let cs: Vec<char> = sel.chars().collect();
    let Some(topo) = posicoes_de_topo(&cs) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for (i, _) in cs
        .iter()
        .enumerate()
        .filter(|(i, c)| **c == '#' && topo[*i])
    {
        let id: String = cs[i + 1..]
            .iter()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
            .collect();
        if id.chars().next().is_some_and(|c| !c.is_ascii_digit()) {
            ids.push(id);
        }
    }
    ids
}

/// For each character of a selector: is it at the top level (outside parentheses, brackets and
/// quotes)? `None` when the selector is a list (`,` at the top level) or has escapes.
fn posicoes_de_topo(cs: &[char]) -> Option<Vec<bool>> {
    let (mut paren, mut colchete) = (0usize, 0usize);
    let mut aspa: Option<char> = None;
    let mut topo = Vec::with_capacity(cs.len());
    for &c in cs {
        let no_topo = paren == 0 && colchete == 0 && aspa.is_none();
        topo.push(no_topo);
        if let Some(q) = aspa {
            if c == q {
                aspa = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => aspa = Some(c),
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            '[' => colchete += 1,
            ']' => colchete = colchete.saturating_sub(1),
            ',' if no_topo => return None,
            '\\' => return None,
            _ => {}
        }
    }
    Some(topo)
}

/// Report every lookup whose id no element of the document carries and no script creates.
/// A script that builds ids dynamically makes the whole class unmeasurable.
pub(super) fn verificar_lookups(
    scripts: &AnaliseScripts,
    presentes: &HashSet<&str>,
    saida: &mut Saida,
) {
    if scripts.dinamico {
        if !scripts.lookups.is_empty() {
            saida.nao_medido(
                "script cria ids ou marcação por dados dinâmicos: chamadas de DOM não medidas",
            );
        }
        return;
    }
    for l in &scripts.lookups {
        if presentes.contains(l.id.as_str()) {
            continue;
        }
        saida.achado(Finding::candidato(
            "ancora",
            &l.id,
            l.linha,
            Alvo::Incerto,
            format!(
                "`{}` procura o id `{}`, que nenhum elemento do documento tem",
                l.chamada, l.id
            ),
            Vec::new(),
        ));
    }
}
