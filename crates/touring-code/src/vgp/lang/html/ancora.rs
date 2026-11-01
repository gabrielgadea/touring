//! Class `ancora` (VL0 §4): an id the document refers to that no element carries, and an id
//! declared twice.
//!
//! The references are the ones the HTML spec resolves INSIDE the document: `<a href="#x">` /
//! `<area>` / SVG `<use href="#x">` (fragment navigation: an element with that `id`, or an `<a
//! name>`), `<label for="x">`, and the id lists of `aria-labelledby` / `aria-describedby`. The
//! inline-script lookups (`getElementById`, `querySelector('#x')`) live in `script.rs`.
//!
//! "Missing" is a claim about the whole page, so the set of ids that exist is deliberately generous:
//! the parse tree's ids, a lexical `id=` scan of the source (it also sees what an error region or a
//! comment hides), `<a name>`, ids inside `<template>`, and everything a script may create. A page
//! whose ids may live elsewhere — a template that extends or includes another, a fragment with no
//! `<html>`/`<body>` — is not measured at all.

use std::collections::{HashMap, HashSet};

use super::Saida;
use super::documento::{Documento, Elemento};
use super::script::AnaliseScripts;
use super::texto::{decodificar_percentual, ids_no_texto, mais_proximos, tem_template};
use crate::vgp::Finding;
use crate::vgp::politica::Alvo;

/// Constructs that pull markup (and ids) from another file: the ids a page refers to may be
/// declared there.
const COMPOSICAO: &[&str] = &[
    "{% extends",
    "{%- extends",
    "{% include",
    "{%- include",
    "{% import",
    "{{>",
    "{{> ",
    "{{#extend",
    "<!--#include",
    "<?php",
    "<%@",
    "<jsp:include",
    "<ng-include",
    "<ui:include",
];

/// A reference to an id of the document.
struct Referencia {
    id: String,
    /// 1-based line of the attribute.
    linha: usize,
    /// `<a href="#x">`, `<label for>`…
    origem: String,
}

/// Why the anchors of this document cannot be judged, if they cannot.
fn nao_mensuravel(doc: &Documento, fonte: &str) -> Option<String> {
    if let Some(m) = COMPOSICAO.iter().find(|m| fonte.contains(**m)) {
        return Some(format!(
            "documento com `{m}`: os ids referenciados podem estar em outro arquivo"
        ));
    }
    if doc.ids.keys().any(|id| tem_template(id)) {
        return Some("ids gerados por template: âncoras não medidas".to_string());
    }
    if !doc.eh_pagina {
        return Some(
            "fragmento HTML (sem `<html>`, `<head>`, `<body>` nem doctype): \
             os ids podem estar na página hospedeira"
                .to_string(),
        );
    }
    None
}

/// The ids that exist or may exist at run time.
pub(super) fn ids_presentes<'a>(
    doc: &'a Documento,
    fonte: &'a str,
    scripts: &'a AnaliseScripts,
) -> HashSet<&'a str> {
    let mut presentes: HashSet<&str> = HashSet::new();
    presentes.extend(doc.ids.keys().map(String::as_str));
    presentes.extend(doc.ids_em_template.iter().map(String::as_str));
    presentes.extend(ids_no_texto(fonte));
    presentes.extend(scripts.criados.iter().map(String::as_str));
    presentes.extend(
        doc.elementos
            .iter()
            .filter(|e| e.tag == "a")
            .filter_map(|e| e.valor("name")),
    );
    presentes
}

/// Report the missing ids and the duplicated ones.
pub(super) fn verificar(
    doc: &Documento,
    fonte: &str,
    scripts: &AnaliseScripts,
    presentes: &HashSet<&str>,
    saida: &mut Saida,
) {
    let (referencias, dinamicas) = referencias(doc);
    if dinamicas > 0 {
        saida.nao_medido(format!(
            "{dinamicas} referência(s) a id com valor de template não medida(s)"
        ));
    }
    let ha_duplicados = doc.ids.values().any(|ls| ls.len() > 1);
    if (!referencias.is_empty() || ha_duplicados)
        && let Some(motivo) = nao_mensuravel(doc, fonte)
    {
        saida.nao_medido(motivo);
        return;
    }
    // o disco decide: o id é do PROJETO e não existe; um script da página (externo, dinâmico ou
    // ilegível) pode criá-lo em tempo de execução, e aí a dúvida é de verdade (`Incerto`)
    let alvo = if scripts.dinamico || doc.tem_script_externo() {
        Alvo::Incerto
    } else {
        Alvo::Projeto
    };
    let mut ordenados: Option<Vec<&str>> = None;
    for (id, grupo) in agrupar(&referencias) {
        let existe = |i: &str| presentes.contains(i);
        if existe(id) || decodificar_percentual(id).is_some_and(|d| existe(&d)) {
            continue;
        }
        let candidatos = ordenados.get_or_insert_with(|| {
            let mut v: Vec<&str> = presentes.iter().copied().collect();
            v.sort_unstable();
            v
        });
        saida.achado(ausente(id, &grupo, alvo, candidatos));
    }
    duplicados(doc, saida);
}

/// A candidate reference found on an element, before it is classified.
struct Candidato<'a> {
    token: &'a str,
    /// Attribute name (`href`, `for`, `aria-labelledby`…).
    atributo: &'a str,
    linha: usize,
    origem: String,
}

/// `href="#x"` of `<a>`/`<area>`/`<use>`, `for="x"` of `<label>`, the ids of `aria-labelledby` and
/// `aria-describedby` — plus how many carried a template marker (skipped).
fn referencias(doc: &Documento) -> (Vec<Referencia>, usize) {
    let mut out = Vec::new();
    let mut dinamicas = 0;
    let base_externa = tem_base_externa(doc);
    for el in &doc.elementos {
        for c in candidatos_do_elemento(el, base_externa) {
            match classificar(&c) {
                Token::Id => out.push(Referencia {
                    id: c.token.to_string(),
                    linha: c.linha,
                    origem: c.origem,
                }),
                Token::Dinamico => dinamicas += 1,
                Token::Ignorado => {}
            }
        }
    }
    (out, dinamicas)
}

fn candidatos_do_elemento(el: &Elemento, base_externa: bool) -> Vec<Candidato<'_>> {
    let mut out = Vec::new();
    if !base_externa && matches!(el.tag.as_str(), "a" | "area" | "use") {
        for nome in ["href", "xlink:href"] {
            let Some(a) = el.atributo(nome) else { continue };
            let fragmento = a.valor.as_deref().and_then(|v| v.trim().strip_prefix('#'));
            if let Some(token) = fragmento {
                out.push(Candidato {
                    token,
                    atributo: nome,
                    linha: a.linha,
                    origem: format!("<{} {nome}=\"#…\">", el.tag),
                });
            }
        }
    }
    if el.tag == "label"
        && let Some(a) = el.atributo("for")
        && let Some(v) = a.valor.as_deref()
    {
        out.push(Candidato {
            token: v.trim(),
            atributo: "for",
            linha: a.linha,
            origem: "<label for>".to_string(),
        });
    }
    for nome in ["aria-labelledby", "aria-describedby"] {
        let Some(a) = el.atributo(nome) else { continue };
        for token in a.valor.as_deref().unwrap_or("").split_ascii_whitespace() {
            out.push(Candidato {
                token,
                atributo: nome,
                linha: a.linha,
                origem: nome.to_string(),
            });
        }
    }
    out
}

/// What a fragment or id token is.
enum Token {
    Id,
    Dinamico,
    Ignorado,
}

/// `#` / `#top` / `#!/route` / `#/route` / `#:~:text=…` navigate somewhere that is not an id.
fn classificar(c: &Candidato) -> Token {
    if c.token.is_empty() {
        return Token::Ignorado;
    }
    if tem_template(c.token) {
        return Token::Dinamico;
    }
    let navegacao = c.atributo.ends_with("href");
    let especial = c.token.eq_ignore_ascii_case("top")
        || c.token.starts_with(['!', '/'])
        || c.token.contains(":~:");
    if navegacao && especial {
        Token::Ignorado
    } else {
        Token::Id
    }
}

/// With a `<base href>` that is not `#…`, `href="#x"` resolves against the base — another document.
fn tem_base_externa(doc: &Documento) -> bool {
    doc.elementos
        .iter()
        .any(|e| e.tag == "base" && e.valor("href").is_some_and(|v| !v.trim().is_empty()))
}

/// Group the references by id, in order of first appearance.
fn agrupar(referencias: &[Referencia]) -> Vec<(&str, Vec<&Referencia>)> {
    let mut indice: HashMap<&str, usize> = HashMap::new();
    let mut grupos: Vec<(&str, Vec<&Referencia>)> = Vec::new();
    for r in referencias {
        let i = *indice.entry(r.id.as_str()).or_insert_with(|| {
            grupos.push((r.id.as_str(), Vec::new()));
            grupos.len() - 1
        });
        grupos[i].1.push(r);
    }
    grupos
}

/// `candidatos` is the sorted list of ids that exist (sorted so the suggestions are deterministic).
/// `alvo` is `Projeto` when the disk decides (the id is the project's and is absent) and
/// `Incerto` when a script of the page may create it at run time — which the suggestion says.
/// Whether this class may ever `deny` is the POLICY's call (`CLASSES_CONSULTIVAS`), not a label.
fn ausente(id: &str, grupo: &[&Referencia], alvo: Alvo, candidatos: &[&str]) -> Finding {
    let primeira = grupo[0];
    let reais = mais_proximos(id, candidatos.iter().copied(), 5);
    let mut sugestao = format!(
        "id `{id}` ausente no documento (referenciado por {})",
        primeira.origem
    );
    if grupo.len() > 1 {
        sugestao.push_str(&format!(" e por mais {} referência(s)", grupo.len() - 1));
    }
    if alvo == Alvo::Incerto {
        sugestao.push_str(
            "; um script da página (externo ou dinâmico) pode criá-lo em tempo de execução",
        );
    }
    if let Some(proximo) = reais.first() {
        sugestao.push_str(&format!(". Você quis dizer `{proximo}`?"));
    }
    Finding::candidato("ancora", id, primeira.linha, alvo, sugestao, reais)
}

/// One finding per id that two elements carry (the second occurrence's line). `Incerto`: a
/// duplicate is a lint, never an error the motor may deny.
fn duplicados(doc: &Documento, saida: &mut Saida) {
    let mut repetidos: Vec<(&String, &Vec<usize>)> =
        doc.ids.iter().filter(|(_, ls)| ls.len() > 1).collect();
    repetidos.sort_by_key(|(id, ls)| (ls[0], id.as_str()));
    for (id, linhas) in repetidos {
        let onde: Vec<String> = linhas.iter().take(4).map(|l| l.to_string()).collect();
        saida.achado(Finding::candidato(
            "ancora",
            id,
            linhas[1],
            Alvo::Incerto,
            format!(
                "id `{id}` duplicado: {} elementos o declaram (linhas {}{}); só o primeiro é \
                 alvo de âncora, de `label for` e de `getElementById`",
                linhas.len(),
                onde.join(", "),
                if linhas.len() > 4 { ", …" } else { "" }
            ),
            Vec::new(),
        ));
    }
}
