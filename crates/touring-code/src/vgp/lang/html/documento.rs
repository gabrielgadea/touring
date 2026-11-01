//! What the html adapter reads out of the parse tree, once: every start tag with its attributes
//! (names lowercased, character references decoded), the inline script bodies, and the ids.
//!
//! Grammar (`tree-sitter-html` 0.23.2, `src/grammar.json`): `start_tag` / `self_closing_tag` hold a
//! `tag_name` and `attribute`s; an `attribute` is an `attribute_name` plus, optionally, an
//! `attribute_value` or a `quoted_attribute_value` (which wraps an `attribute_value`, absent when
//! the quotes are empty). `<script>` / `<style>` bodies are `raw_text` children of
//! `script_element` / `style_element`. The grammar declares no fields.

use std::collections::{HashMap, HashSet};

use tree_sitter::{Node, Tree};

use super::texto::decodificar_entidades;

/// One attribute of a start tag.
pub(super) struct Atributo {
    /// Lowercased name (`href`, `xlink:href`, `aria-labelledby`…).
    pub nome: String,
    /// Value with character references decoded; `None` for a bare attribute (`<input disabled>`).
    pub valor: Option<String>,
    /// 1-based line of the attribute.
    pub linha: usize,
}

/// One start tag (or self-closing tag).
pub(super) struct Elemento {
    /// Lowercased tag name.
    pub tag: String,
    pub atributos: Vec<Atributo>,
    /// 1-based line of the tag.
    pub linha: usize,
}

impl Elemento {
    /// The attribute called `nome` (already lowercase), if the tag has it.
    pub fn atributo(&self, nome: &str) -> Option<&Atributo> {
        self.atributos.iter().find(|a| a.nome == nome)
    }

    /// The value of the attribute called `nome`, if it has one.
    pub fn valor(&self, nome: &str) -> Option<&str> {
        self.atributo(nome).and_then(|a| a.valor.as_deref())
    }
}

/// The body of an inline `<script>` (one with no `src` still has its element in `elementos`).
pub(super) struct ScriptInline {
    /// Index of the `<script>` start tag in [`Documento::elementos`].
    pub elemento: usize,
    pub texto: String,
    /// 0-based line of the first byte of the body.
    pub linha_inicial: usize,
}

/// Everything the checks need from one parsed document.
pub(super) struct Documento {
    pub elementos: Vec<Elemento>,
    pub scripts: Vec<ScriptInline>,
    /// `id` value → lines of the elements that carry it (more than one = a duplicate).
    pub ids: HashMap<String, Vec<usize>>,
    /// Ids of elements inside a `<template>`: inert content, outside the duplicate check but a
    /// valid target once instantiated.
    pub ids_em_template: HashSet<String>,
    /// The tree has a MISSING node or an ERROR that swallowed markup (`<`): part of the tags may
    /// not have been read. An ERROR made of text only — a bare `>` in `a -> b`, which the grammar's
    /// `text` token excludes — leaves every element intact and does not count (measured: 1 797 of
    /// the 2 467 documents with a tree error in the analise tree).
    pub tem_erro: bool,
    /// The document has `<html>`, `<head>`, `<body>` or a doctype — it is a page, not a fragment.
    pub eh_pagina: bool,
}

impl Documento {
    /// Walk the tree once (iteratively — a deeply nested document must not overflow the stack).
    pub fn ler(fonte: &str, tree: &Tree) -> Self {
        let mut doc = Self {
            elementos: Vec::new(),
            scripts: Vec::new(),
            ids: HashMap::new(),
            ids_em_template: HashSet::new(),
            tem_erro: false,
            eh_pagina: false,
        };
        let mut cur = tree.walk();
        loop {
            let no = cur.node();
            if !doc.tem_erro && erro_que_esconde_marcacao(fonte, no) {
                doc.tem_erro = true;
            }
            match no.kind() {
                "start_tag" | "self_closing_tag" if !no.is_missing() => doc.elemento(fonte, no),
                "raw_text" => doc.script(fonte, no),
                "doctype" => doc.eh_pagina = true,
                _ => {}
            }
            if cur.goto_first_child() {
                continue;
            }
            while !cur.goto_next_sibling() {
                if !cur.goto_parent() {
                    return doc;
                }
            }
        }
    }

    fn elemento(&mut self, fonte: &str, no: Node) {
        let mut tag = String::new();
        let mut atributos = Vec::new();
        let mut c = no.walk();
        for filho in no.children(&mut c) {
            match filho.kind() {
                "tag_name" => tag = texto(fonte, filho).to_ascii_lowercase(),
                "attribute" => atributos.extend(atributo(fonte, filho)),
                _ => {}
            }
        }
        if tag.is_empty() {
            return;
        }
        let linha = no.start_position().row + 1;
        if matches!(tag.as_str(), "html" | "head" | "body") {
            self.eh_pagina = true;
        }
        self.registrar_id(fonte, no, &atributos);
        self.elementos.push(Elemento {
            tag,
            atributos,
            linha,
        });
    }

    /// Record the `id` of a start tag: live ids are counted (for duplicates), those inside a
    /// `<template>` are only remembered as valid targets.
    fn registrar_id(&mut self, fonte: &str, no: Node, atributos: &[Atributo]) {
        let Some(a) = atributos.iter().find(|a| a.nome == "id") else {
            return;
        };
        let Some(id) = a.valor.as_deref().filter(|v| !v.is_empty()) else {
            return;
        };
        if dentro_de_template(fonte, no) {
            self.ids_em_template.insert(id.to_string());
        } else {
            self.ids.entry(id.to_string()).or_default().push(a.linha);
        }
    }

    /// The body of a `<script>` (the `raw_text` right after its start tag, already pushed).
    fn script(&mut self, fonte: &str, no: Node) {
        if no.parent().map(|p| p.kind()) != Some("script_element") || self.elementos.is_empty() {
            return;
        }
        self.scripts.push(ScriptInline {
            elemento: self.elementos.len() - 1,
            texto: texto(fonte, no).to_string(),
            linha_inicial: no.start_position().row,
        });
    }

    /// Is there any `<script src=…>` (local or remote)? Whatever it runs may create ids.
    pub fn tem_script_externo(&self) -> bool {
        self.elementos
            .iter()
            .any(|e| e.tag == "script" && e.atributo("src").is_some())
    }
}

/// A MISSING node, or an ERROR whose text holds a `<`: tag markup the parser could not place.
fn erro_que_esconde_marcacao(fonte: &str, no: Node) -> bool {
    no.is_missing() || (no.is_error() && texto(fonte, no).contains('<'))
}

/// Is the start tag `no` inside a `<template>` element? (Its own element — the parent of the
/// start tag — does not count: `<template id="row">` is a live element.)
fn dentro_de_template(fonte: &str, no: Node) -> bool {
    let mut atual = no.parent().and_then(|p| p.parent());
    while let Some(pai) = atual {
        if pai.kind() == "element" && primeira_tag(fonte, pai) == Some("template") {
            return true;
        }
        atual = pai.parent();
    }
    false
}

/// The (case-preserved) tag name of an `element` node's own start tag.
fn primeira_tag<'a>(fonte: &'a str, elemento: Node) -> Option<&'a str> {
    let mut c = elemento.walk();
    let start = elemento
        .children(&mut c)
        .find(|f| f.kind() == "start_tag")?;
    let mut c2 = start.walk();
    let nome = start.children(&mut c2).find(|f| f.kind() == "tag_name")?;
    Some(texto(fonte, nome))
}

fn texto<'a>(fonte: &'a str, no: Node) -> &'a str {
    &fonte[no.byte_range()]
}

/// One `attribute` node; `None` when the parser could not read it whole.
fn atributo(fonte: &str, no: Node) -> Option<Atributo> {
    if no.has_error() || no.is_missing() {
        return None;
    }
    let mut nome = None;
    let mut valor = None;
    let mut c = no.walk();
    for filho in no.children(&mut c) {
        match filho.kind() {
            "attribute_name" => nome = Some(texto(fonte, filho).to_ascii_lowercase()),
            "attribute_value" => valor = Some(texto(fonte, filho)),
            "quoted_attribute_value" => valor = Some(valor_entre_aspas(fonte, filho)),
            _ => {}
        }
    }
    Some(Atributo {
        nome: nome?,
        valor: valor.map(|v| decodificar_entidades(v).into_owned()),
        linha: no.start_position().row + 1,
    })
}

/// The inner `attribute_value` of a quoted value; empty quotes have none.
fn valor_entre_aspas<'a>(fonte: &'a str, no: Node) -> &'a str {
    let mut c = no.walk();
    no.children(&mut c)
        .find(|f| f.kind() == "attribute_value")
        .map_or("", |f| texto(fonte, f))
}
