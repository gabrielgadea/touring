//! JSDoc types for plain JavaScript: `/** @type {T} */` on a declaration, `@param {T} name` on a
//! function, `@typedef {Object} Name` with `@property` lines.
//!
//! JSDoc type syntax is almost TypeScript's: the text of a tag is wrapped as `type __T = <text>;`
//! and lowered by the TypeScript grammar. Anything the grammar does not accept (`function(string)`,
//! `?T`, `Object.<K, V>`) is `None` — fail-closed: no type, no check.

use tree_sitter::Node;

use super::decls::{Decl, Membro, ObjTipo, TipoExpr, baixar_tipo};
use super::util::texto;
use crate::vgp::lang::parse;

/// One `@tag {type} rest` of a JSDoc block.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Tag {
    pub nome: String,
    pub tipo: Option<String>,
    pub resto: String,
}

/// The tags of a `/** … */` comment.
pub(super) fn tags_do_bloco(comentario: &str) -> Vec<Tag> {
    let corpo = comentario
        .trim()
        .trim_start_matches("/**")
        .trim_end_matches("*/");
    let mut tags: Vec<Tag> = Vec::new();
    for linha in corpo.lines() {
        let l = linha.trim().trim_start_matches('*').trim();
        if let Some(depois) = l.strip_prefix('@') {
            tags.push(tag_da_linha(depois));
        }
    }
    tags
}

fn tag_da_linha(depois_do_arroba: &str) -> Tag {
    let fim_do_nome = depois_do_arroba
        .find(|c: char| c.is_whitespace() || c == '{')
        .unwrap_or(depois_do_arroba.len());
    let nome = depois_do_arroba[..fim_do_nome].to_string();
    let resto = depois_do_arroba[fim_do_nome..].trim_start();
    match resto.strip_prefix('{').and_then(chaves_balanceadas) {
        Some((tipo, depois)) => Tag {
            nome,
            tipo: Some(tipo.trim().to_string()),
            resto: depois.trim().to_string(),
        },
        None => Tag {
            nome,
            tipo: None,
            resto: resto.trim().to_string(),
        },
    }
}

/// `inner} rest` (after the opening brace) → (`inner`, `rest`), honouring nested braces.
fn chaves_balanceadas(depois_da_abertura: &str) -> Option<(&str, &str)> {
    let mut nivel = 1usize;
    for (i, c) in depois_da_abertura.char_indices() {
        match c {
            '{' => nivel += 1,
            '}' => {
                nivel -= 1;
                if nivel == 0 {
                    return Some((&depois_da_abertura[..i], &depois_da_abertura[i + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// JSDoc-only type syntax the TypeScript grammar may still swallow with another meaning: the
/// nullable/non-nullable prefixes, the `*` wildcard, `...T` rest, `T=` optional, closure `function(…)`.
fn e_jsdocismo(t: &str) -> bool {
    t.starts_with(['?', '!', '*'])
        || t.starts_with("...")
        || t.ends_with('=')
        || t.starts_with("function")
        || (t.contains('\n') && t.contains("function"))
}

/// Lower a JSDoc type text to a [`TipoExpr`] (`None` when the TypeScript grammar rejects it).
pub(super) fn tipo_do_texto(t: &str) -> Option<TipoExpr> {
    if t.is_empty() || e_jsdocismo(t) {
        return None;
    }
    let fonte = format!("type __T = {t};");
    let arvore = parse("typescript", &fonte).ok()?;
    if arvore.root_node().has_error() {
        return None;
    }
    let alias = arvore.root_node().named_child(0)?;
    let valor = alias.child_by_field_name("value")?;
    Some(baixar_tipo(valor, &fonte))
}

/// The JSDoc block that ends right before the statement owning `no` (whitespace only between).
pub(super) fn bloco_anterior<'s>(no: Node, src: &'s str) -> Option<&'s str> {
    let alvo = match no.parent() {
        Some(p) if p.kind() == "export_statement" => p,
        _ => no,
    };
    let anterior = alvo.prev_sibling()?;
    if anterior.kind() != "comment" {
        return None;
    }
    let t = texto(src, anterior);
    let intervalo = src.get(anterior.end_byte()..alvo.start_byte())?;
    (t.starts_with("/**") && intervalo.trim().is_empty()).then_some(t)
}

/// `/** @type {T} */` before the declaration statement `decl`.
pub(super) fn tipo_da_declaracao(decl: Node, src: &str) -> Option<TipoExpr> {
    let bloco = bloco_anterior(decl, src)?;
    let t = tags_do_bloco(bloco)
        .into_iter()
        .find(|t| t.nome == "type")?;
    tipo_do_texto(t.tipo.as_deref()?)
}

/// `@param {T} nome` documented for the parameter `nome` of the function that encloses `param`.
pub(super) fn tipo_do_parametro(param: Node, nome: &str, src: &str) -> Option<TipoExpr> {
    let mut f = param.parent();
    while let Some(n) = f {
        if is_funcao(n.kind()) {
            break;
        }
        f = n.parent();
    }
    let funcao = f?;
    // the comment sits before the function, or before the declaration that holds it
    let mut dono = funcao;
    for _ in 0..4 {
        if let Some(bloco) = bloco_anterior(dono, src) {
            return tags_do_bloco(bloco)
                .into_iter()
                .filter(|t| t.nome == "param" || t.nome == "arg" || t.nome == "argument")
                .find(|t| nome_do_param(&t.resto) == nome)
                .and_then(|t| tipo_do_texto(t.tipo.as_deref()?));
        }
        dono = dono.parent()?;
    }
    None
}

fn is_funcao(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration"
    )
}

/// `[name=default] description` / `name description` → `name` (dotted sub-properties are not params).
fn nome_do_param(resto: &str) -> &str {
    let primeiro = resto.split_whitespace().next().unwrap_or("");
    let sem_colchetes = primeiro.trim_start_matches('[').trim_end_matches(']');
    let nome = sem_colchetes.split('=').next().unwrap_or("");
    if nome.contains('.') { "" } else { nome }
}

/// `@typedef` blocks at the top level of the file ⇒ declarations by name.
pub(super) fn typedefs_do_arquivo(raiz: Node, src: &str) -> Vec<(String, Decl)> {
    let mut out = Vec::new();
    let mut c = raiz.walk();
    for no in raiz
        .named_children(&mut c)
        .filter(|n| n.kind() == "comment")
    {
        let t = texto(src, no);
        if !t.starts_with("/**") {
            continue;
        }
        let tags = tags_do_bloco(t);
        for (i, td) in tags.iter().enumerate().filter(|(_, t)| t.nome == "typedef") {
            if let Some(par) = typedef_decl(td, &tags[i + 1..]) {
                out.push(par);
            }
        }
    }
    out
}

fn typedef_decl(td: &Tag, seguintes: &[Tag]) -> Option<(String, Decl)> {
    let nome = td.resto.split_whitespace().next()?.to_string();
    let tipo = td.tipo.as_deref()?;
    if tipo == "Object" || tipo == "object" {
        let mut corpo = ObjTipo::default();
        let props = seguintes
            .iter()
            .take_while(|t| t.nome != "typedef")
            .filter(|t| t.nome == "property" || t.nome == "prop");
        for p in props {
            let n = nome_do_param(&p.resto);
            if !n.is_empty() {
                corpo.membros.push(Membro {
                    nome: n.to_string(),
                    tipo: p.tipo.as_deref().and_then(tipo_do_texto),
                });
            }
        }
        corpo.aberto = corpo.membros.is_empty();
        return Some((
            nome,
            Decl::Interface {
                params: Vec::new(),
                estende: Vec::new(),
                corpo,
            },
        ));
    }
    tipo_do_texto(tipo).map(|t| {
        (
            nome,
            Decl::Alias {
                params: Vec::new(),
                tipo: t,
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_com_tipo_aninhado_e_continuacao() {
        let tags = tags_do_bloco(
            "/**\n * Descrição.\n * @param {{ a: number, b: { c: string } }} opts as opções\n * @returns {Promise<void>}\n */",
        );
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].nome, "param");
        assert_eq!(
            tags[0].tipo.as_deref(),
            Some("{ a: number, b: { c: string } }")
        );
        assert_eq!(tags[0].resto, "opts as opções");
        assert_eq!(tags[1].tipo.as_deref(), Some("Promise<void>"));
    }

    #[test]
    fn tipo_do_texto_aceita_ts_e_rejeita_jsdocismos() {
        assert!(matches!(
            tipo_do_texto("{ a: number }"),
            Some(TipoExpr::Obj(_))
        ));
        assert!(matches!(tipo_do_texto("Foo"), Some(TipoExpr::Ref { .. })));
        assert!(matches!(
            tipo_do_texto("string[]"),
            Some(TipoExpr::Array(_))
        ));
        assert!(tipo_do_texto("function(string): number").is_none());
        assert!(tipo_do_texto("?Foo").is_none());
        assert!(tipo_do_texto("").is_none());
    }

    #[test]
    fn nome_do_param_tira_colchetes_default_e_ignora_subpropriedade() {
        assert_eq!(nome_do_param("opts as opções"), "opts");
        assert_eq!(nome_do_param("[opts=1] opcional"), "opts");
        assert_eq!(nome_do_param("opts.sub documenta a subpropriedade"), "");
    }

    fn arvore(src: &str) -> tree_sitter::Tree {
        parse("javascript", src).expect("parse")
    }

    #[test]
    fn tipo_da_declaracao_com_export_e_comentario_colado() {
        let src = "/** @type {{ a: number }} */\nconst x = { a: 1 };\n/** @type {Foo} */\nexport const y = 1;\n// comum\nconst z = 1;\n";
        let t = arvore(src);
        let raiz = t.root_node();
        let mut c = raiz.walk();
        let decls: Vec<_> = raiz
            .named_children(&mut c)
            .filter(|n| n.kind() != "comment")
            .collect();
        assert!(matches!(
            tipo_da_declaracao(decls[0], src),
            Some(TipoExpr::Obj(_))
        ));
        assert!(matches!(
            tipo_da_declaracao(decls[1], src),
            Some(TipoExpr::Ref { .. })
        ));
        assert!(
            tipo_da_declaracao(decls[2], src).is_none(),
            "comentário comum não é JSDoc"
        );
    }

    #[test]
    fn tipo_do_parametro_vem_do_param_da_funcao() {
        let src = "/**\n * @param {Foo} a\n * @param {string} b\n */\nfunction f(a, b) { return a.x + b.length; }\n";
        let t = arvore(src);
        // o `a` dentro dos parâmetros
        let pos = src.find("(a, b)").expect("params") + 1;
        let param = t
            .root_node()
            .descendant_for_byte_range(pos, pos + 1)
            .expect("nó");
        let tipo = tipo_do_parametro(param, "a", src);
        assert!(
            matches!(tipo, Some(TipoExpr::Ref { ref nome, .. }) if nome == "Foo"),
            "{tipo:?}"
        );
        assert!(
            matches!(tipo_do_parametro(param, "b", src), Some(TipoExpr::Prim(ref p)) if p == "string")
        );
        assert!(tipo_do_parametro(param, "c", src).is_none());
    }

    #[test]
    fn typedef_object_com_properties_vira_interface_fechada() {
        let src = "/**\n * @typedef {Object} Opcoes\n * @property {number} largura\n * @property {string} [titulo]\n */\n\n/** @typedef {{ x: number }} Ponto */\n";
        let t = arvore(src);
        let defs = typedefs_do_arquivo(t.root_node(), src);
        assert_eq!(defs.len(), 2);
        match &defs[0] {
            (n, Decl::Interface { corpo, .. }) => {
                assert_eq!(n, "Opcoes");
                let nomes: Vec<&str> = corpo.membros.iter().map(|m| m.nome.as_str()).collect();
                assert_eq!(nomes, vec!["largura", "titulo"]);
                assert!(!corpo.aberto);
            }
            outro => panic!("{outro:?}"),
        }
        assert!(matches!(&defs[1], (n, Decl::Alias { .. }) if n == "Ponto"));
    }

    #[test]
    fn typedef_object_sem_properties_fica_aberto() {
        let src = "/** @typedef {Object} Solto */\n";
        let t = arvore(src);
        let defs = typedefs_do_arquivo(t.root_node(), src);
        assert!(matches!(&defs[0], (_, Decl::Interface { corpo, .. }) if corpo.aberto));
    }
}
