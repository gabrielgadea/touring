//! Where a class is USED: html `class="…"` and JSX `className="…"` — only the static strings.
//! A dynamic expression, a template placeholder or a token that cannot be a class name is never
//! guessed at (it is skipped), and the classes a document's own script drives by name
//! (`querySelector('.x')`, `classList.add('x')`…) are collected so a JS hook is not mistaken for
//! a class with no definition.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use tree_sitter::{Node, Tree};

use crate::vgp::lang::cdn::documento::{Documento, e_script_js};
use crate::vgp::lang::parse;

/// Text that marks a value as a server/client template placeholder: the real class list is
/// only known after rendering.
const DELIMITADORES_DE_TEMPLATE: [&str; 6] = ["{{", "{%", "{#", "<%", "<?", "${"];

/// `.name` selectors in script text (`$('.card')`, `querySelector("a .b")`): group 1 is the name.
static SELETOR_JS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|[^\w)\]$])\.(-?[_A-Za-z][\w-]*)"#).expect("regex constante")
});

/// `classList.add('a', "b")`-style calls: group 1 is the argument list.
static CLASSLIST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"classList\s*\.\s*(?:add|remove|toggle|contains|replace)\s*\(([^)]*)\)"#)
        .expect("regex constante")
});

/// `getElementsByClassName('a b')`: group 1 is the class list.
static POR_CLASSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"getElementsByClassName\s*\(\s*['"]([^'"]+)['"]"#).expect("regex constante")
});

/// `el.className = 'a b'` / `+=`: group 1 is the class list.
static ATRIBUI_CLASSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"className\s*\+?=\s*['"]([^'"]+)['"]"#).expect("regex constante")
});

/// Any quoted string: group 1 is its content.
static LITERAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"['"]([^'"]*)['"]"#).expect("regex constante"));

/// A script that builds or edits stylesheets at run time — it can define any class.
static INJETA_FOLHA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"createElement\s*\(\s*['"](?:link|style)['"]|insertRule|adoptedStyleSheets|new\s+CSSStyleSheet|\.styleSheets\b"#,
    )
    .expect("regex constante")
});

/// One class name used in the document, with the 1-based line it appears on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Uso {
    pub classe: String,
    pub linha: usize,
}

/// Does `valor` carry a template placeholder?
pub(super) fn tem_placeholder(valor: &str) -> bool {
    DELIMITADORES_DE_TEMPLATE.iter().any(|d| valor.contains(d))
}

/// Can `token` be a plain class name (no markup or template punctuation)?
fn token_de_classe(token: &str) -> bool {
    !token.contains(['{', '}', '$', '<', '>', '%', '(', ')', '=', '"', '\'', '&'])
}

/// The whitespace-separated tokens of a class list, each with its line (`linha_base` is the line
/// the list starts on).
fn tokens_com_linha(valor: &str, linha_base: usize) -> Vec<Uso> {
    let mut usos = Vec::new();
    let mut linha = linha_base;
    let mut inicio: Option<(usize, usize)> = None;
    for (i, c) in valor.char_indices() {
        if c.is_whitespace() {
            if let Some((s, l)) = inicio.take() {
                usos.push(Uso {
                    classe: valor[s..i].to_string(),
                    linha: l,
                });
            }
            if c == '\n' {
                linha += 1;
            }
        } else if inicio.is_none() {
            inicio = Some((i, linha));
        }
    }
    if let Some((s, l)) = inicio {
        usos.push(Uso {
            classe: valor[s..].to_string(),
            linha: l,
        });
    }
    usos.retain(|u| token_de_classe(&u.classe));
    usos
}

/// Every class named by a static `class="…"` of the html document.
pub(super) fn usos_html(doc: &Documento) -> Vec<Uso> {
    let mut usos = Vec::new();
    for elemento in &doc.elementos {
        if let Some(atributo) = elemento.atributo("class")
            && !tem_placeholder(&atributo.valor)
        {
            usos.extend(tokens_com_linha(&atributo.valor, atributo.linha));
        }
    }
    usos
}

fn texto_do_no<'a>(no: Node, fonte: &'a str) -> Option<&'a str> {
    no.utf8_text(fonte.as_bytes()).ok()
}

/// A literal without its delimiters (quotes or backticks).
fn sem_delimitadores(bruto: &str) -> Option<String> {
    bruto
        .get(1..bruto.len().checked_sub(1)?)
        .map(str::to_string)
}

/// The text of a JSX attribute value when it is a static string: `"a b"`, `{"a b"}`, `{'a b'}`
/// or a template literal with no `${}`. Anything else (identifier, ternary, call) is dynamic.
fn valor_estatico(valor: Node, fonte: &str) -> Option<(String, usize)> {
    let alvo = if valor.kind() == "jsx_expression" {
        valor.named_child(0)?
    } else {
        valor
    };
    let bruto = texto_do_no(alvo, fonte)?;
    // a template literal is read as text; its `${}` placeholders are rejected by `tem_placeholder`
    matches!(alvo.kind(), "string" | "template_string")
        .then(|| (sem_delimitadores(bruto), alvo.start_position().row + 1))
        .and_then(|(texto, linha)| Some((texto?, linha)))
}

/// The `className` / `class` attribute of a `jsx_attribute` node, when its value is static.
fn classname_estatico(no: Node, fonte: &str) -> Option<(String, usize)> {
    let mut cursor = no.walk();
    let mut nome = None;
    let mut valor = None;
    for filho in no.children(&mut cursor) {
        match filho.kind() {
            "property_identifier" | "class" if nome.is_none() => {
                nome = filho.utf8_text(fonte.as_bytes()).ok()
            }
            "string" | "jsx_expression" => valor = Some(filho),
            _ => {}
        }
    }
    if !matches!(nome, Some("className" | "class")) {
        return None;
    }
    valor_estatico(valor?, fonte)
}

/// Parse a JSX-bearing source. The JavaScript grammar reserves `class`, so `<i class="x" />`
/// (Preact, Solid) is a syntax error there while the TSX grammar reads it: when the JavaScript
/// parse reports an error, the TSX parse is used if it is clean.
fn arvore_jsx(lang: &str, fonte: &str) -> Result<Tree, String> {
    let arvore = parse(lang, fonte)?;
    if lang == "javascript"
        && arvore.root_node().has_error()
        && let Ok(alternativa) = parse("tsx", fonte)
        && !alternativa.root_node().has_error()
    {
        return Ok(alternativa);
    }
    Ok(arvore)
}

/// Every class named by a static `className="…"` of a JSX file (`tsx` or `javascript`).
pub(super) fn usos_jsx(lang: &str, fonte: &str) -> Result<Vec<Uso>, String> {
    let arvore = arvore_jsx(lang, fonte)?;
    let mut usos = Vec::new();
    let mut cursor = arvore.walk();
    loop {
        let no = cursor.node();
        if no.kind() == "jsx_attribute"
            && let Some((valor, linha)) = classname_estatico(no, fonte)
            && !tem_placeholder(&valor)
        {
            usos.extend(tokens_com_linha(&valor, linha));
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Ok(usos);
            }
        }
    }
}

/// The relative or package target of every `import '…css'` / `import x from '…css'` of a JS file.
pub(super) fn imports_de_folha(lang: &str, fonte: &str) -> Result<Vec<String>, String> {
    const EXTENSOES: [&str; 7] = [
        ".css", ".scss", ".sass", ".less", ".styl", ".stylus", ".pcss",
    ];
    let arvore = arvore_jsx(lang, fonte)?;
    let mut alvos = Vec::new();
    let mut cursor = arvore.walk();
    loop {
        let no = cursor.node();
        if no.kind() == "import_statement"
            && let Some(origem) = no.child_by_field_name("source")
            && let Ok(bruto) = origem.utf8_text(fonte.as_bytes())
            && let Some(alvo) = bruto.get(1..bruto.len().saturating_sub(1))
            && EXTENSOES
                .iter()
                .any(|e| alvo.split('?').next().unwrap_or(alvo).ends_with(e))
        {
            alvos.push(alvo.to_string());
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Ok(alvos);
            }
        }
    }
}

/// The text of every script of the document that is JavaScript.
fn textos_de_script(doc: &Documento) -> Vec<&str> {
    doc.scripts
        .iter()
        .filter(|b| {
            let tipo = doc
                .elemento_de(b)
                .atributo("type")
                .map(|t| t.valor.trim().to_ascii_lowercase())
                .unwrap_or_default();
            e_script_js(&tipo)
        })
        .map(|b| b.texto.as_str())
        .collect()
}

/// Class names the document's own scripts drive by name: they are JS hooks, not styling gaps.
pub(super) fn classes_do_js(doc: &Documento) -> HashSet<String> {
    let mut nomes = HashSet::new();
    for texto in textos_de_script(doc) {
        for c in SELETOR_JS.captures_iter(texto) {
            nomes.insert(c[1].to_string());
        }
        for c in CLASSLIST.captures_iter(texto) {
            for l in LITERAL.captures_iter(&c[1]) {
                nomes.extend(l[1].split_whitespace().map(str::to_string));
            }
        }
        for c in POR_CLASSE
            .captures_iter(texto)
            .chain(ATRIBUI_CLASSE.captures_iter(texto))
        {
            nomes.extend(c[1].split_whitespace().map(str::to_string));
        }
    }
    nomes
}

/// Does any script of the document build or edit stylesheets at run time?
pub(super) fn injeta_folhas(doc: &Documento) -> bool {
    textos_de_script(doc)
        .iter()
        .any(|t| INJETA_FOLHA.is_match(t))
}
