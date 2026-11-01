//! Owned summaries of TypeScript declarations — type expressions, interfaces, classes, functions —
//! lowered ONCE from the tree-sitter nodes so that a module can be summarised, cached and consulted
//! without keeping its tree (and the lifetimes) alive.
//!
//! Node and field names are those of `tree-sitter-typescript` 0.23.2 / `tree-sitter-javascript` 0.25.0
//! (read from their `grammar.js` / `node-types.json` and confirmed on probe files with the
//! `tree-sitter` CLI, 10/10/2026).

use tree_sitter::Node;

use super::util::{conteudo_string, tem_token, texto};

/// A type expression, reduced to what member checks need.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum TipoExpr {
    /// `Foo`, `ns.Foo`, `Foo<A, B>` — `qual` holds the namespace path (`React` in `React.FC`).
    Ref {
        nome: String,
        qual: Vec<String>,
        args: Vec<TipoExpr>,
    },
    /// `{ a: number; b?: string }`.
    Obj(ObjTipo),
    Uniao(Vec<TipoExpr>),
    Interseccao(Vec<TipoExpr>),
    /// `string`, `number`, `boolean`, `any`, `null`, `undefined`, `void`, …
    Prim(String),
    /// `T[]`.
    Array(Box<TipoExpr>),
    /// A string/number/boolean literal type.
    Literal,
    /// Anything the checks do not model (mapped, conditional, `typeof x`, `keyof`, functions, tuples).
    Outro,
}

/// The members of an object type, interface body or class body.
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct ObjTipo {
    pub membros: Vec<Membro>,
    /// An index signature, a mapped type or a computed key: the member set is NOT closed.
    pub aberto: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Membro {
    pub nome: String,
    pub tipo: Option<TipoExpr>,
}

/// One function parameter.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Param {
    pub tipo: Option<TipoExpr>,
    pub rest: bool,
}

/// A class's `extends`.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Heranca {
    Nenhuma,
    Tipo(TipoExpr),
    /// `extends mixin(Base)`, `extends (cond ? A : B)`: the base is not statically known.
    Opaca,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ClasseDecl {
    pub params: Vec<String>,
    pub estende: Heranca,
    /// Instance members (fields, methods, accessors, parameter properties, `this.x = …`).
    pub membros: Vec<Membro>,
    pub estaticos: Vec<String>,
    /// Computed members, an index signature, `Object.assign(this, …)`: not closed.
    pub aberta: bool,
}

/// A `const`/`let`/`var` with a simple name.
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct VarDecl {
    pub tipo: Option<TipoExpr>,
    /// The parameters when the initializer is a function/arrow (or wrapped by `memo`/`forwardRef`).
    pub funcao: Option<Vec<Param>>,
    /// `memo(...)`/`forwardRef<El, P>(...)`: the callee's last segment and its type arguments.
    pub embrulho: Option<(String, Vec<TipoExpr>)>,
}

/// A top-level declaration the checks can later resolve by name.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Decl {
    Interface {
        params: Vec<String>,
        estende: Vec<TipoExpr>,
        corpo: ObjTipo,
    },
    Alias {
        params: Vec<String>,
        tipo: TipoExpr,
    },
    Classe(ClasseDecl),
    Enum(Vec<String>),
    Funcao {
        params: Vec<Param>,
    },
    Variavel(VarDecl),
    Namespace,
    Outro,
}

// ---------------------------------------------------------------------------------- types

/// Lower a type node (`type_annotation` allowed) to a [`TipoExpr`].
pub(super) fn baixar_tipo(no: Node, src: &str) -> TipoExpr {
    match no.kind() {
        "type_annotation" | "parenthesized_type" | "readonly_type" => {
            primeiro_filho_nomeado(no).map_or(TipoExpr::Outro, |f| baixar_tipo(f, src))
        }
        "predefined_type" => TipoExpr::Prim(texto(src, no).to_string()),
        "type_identifier" | "nested_type_identifier" | "generic_type" => baixar_referencia(no, src),
        "literal_type" => match texto(src, no) {
            "null" | "undefined" => TipoExpr::Prim(texto(src, no).to_string()),
            _ => TipoExpr::Literal,
        },
        _ => baixar_composto(no, src),
    }
}

/// `Foo`, `ns.Foo`, `Foo<A>`, `ns.Foo<A>`.
fn baixar_referencia(no: Node, src: &str) -> TipoExpr {
    match no.kind() {
        "type_identifier" => TipoExpr::Ref {
            nome: texto(src, no).to_string(),
            qual: Vec::new(),
            args: Vec::new(),
        },
        "nested_type_identifier" => referencia_qualificada(no, src, Vec::new()),
        _ => {
            let args = no
                .child_by_field_name("type_arguments")
                .map(|a| nomeados(a).map(|t| baixar_tipo(t, src)).collect())
                .unwrap_or_default();
            match no.child_by_field_name("name") {
                Some(n) if n.kind() == "nested_type_identifier" => {
                    referencia_qualificada(n, src, args)
                }
                Some(n) => TipoExpr::Ref {
                    nome: texto(src, n).to_string(),
                    qual: Vec::new(),
                    args,
                },
                None => TipoExpr::Outro,
            }
        }
    }
}

/// Object, union, intersection and array types.
fn baixar_composto(no: Node, src: &str) -> TipoExpr {
    match no.kind() {
        "object_type" => TipoExpr::Obj(baixar_corpo(no, src)),
        "union_type" => TipoExpr::Uniao(membros_achatados(no, src)),
        "intersection_type" => TipoExpr::Interseccao(membros_achatados(no, src)),
        "array_type" => primeiro_filho_nomeado(no).map_or(TipoExpr::Outro, |f| {
            TipoExpr::Array(Box::new(baixar_tipo(f, src)))
        }),
        _ => TipoExpr::Outro,
    }
}

/// The members of `A | B | C` (or `A & B & C`): the grammar nests the operator left to right, so a
/// child of the SAME kind is flattened into the list (a parenthesised group stays one member).
fn membros_achatados(no: Node, src: &str) -> Vec<TipoExpr> {
    let mut out = Vec::new();
    for filho in nomeados(no) {
        if filho.kind() == no.kind() {
            out.extend(membros_achatados(filho, src));
        } else {
            out.push(baixar_tipo(filho, src));
        }
    }
    out
}

fn referencia_qualificada(no: Node, src: &str, args: Vec<TipoExpr>) -> TipoExpr {
    let nome = no
        .child_by_field_name("name")
        .map(|n| texto(src, n).to_string())
        .unwrap_or_default();
    let qual = no
        .child_by_field_name("module")
        .map(|m| texto(src, m).split('.').map(str::to_string).collect())
        .unwrap_or_default();
    TipoExpr::Ref { nome, qual, args }
}

/// Named, non-comment children.
fn nomeados<'t>(no: Node<'t>) -> impl Iterator<Item = Node<'t>> {
    let total = u32::try_from(no.named_child_count()).unwrap_or(0);
    let mut v = Vec::with_capacity(total as usize);
    for i in 0..total {
        if let Some(f) = no.named_child(i).filter(|f| f.kind() != "comment") {
            v.push(f);
        }
    }
    v.into_iter()
}

fn primeiro_filho_nomeado(no: Node) -> Option<Node> {
    nomeados(no).next()
}

/// The name of a member node (`property_identifier`, string, number); `None` when computed.
fn nome_do_membro(no: Node, src: &str) -> Option<String> {
    let n = no.child_by_field_name("name")?;
    match n.kind() {
        "property_identifier" | "private_property_identifier" | "identifier" | "number" => {
            Some(texto(src, n).to_string())
        }
        "string" => conteudo_string(src, n),
        _ => None,
    }
}

/// Lower an `object_type` / `interface_body` / `class_body` to its members.
pub(super) fn baixar_corpo(no: Node, src: &str) -> ObjTipo {
    let mut corpo = ObjTipo::default();
    for m in nomeados(no) {
        match m.kind() {
            "property_signature"
            | "method_signature"
            | "public_field_definition"
            | "method_definition"
            | "abstract_method_signature" => match nome_do_membro(m, src) {
                Some(nome) => {
                    let tipo = m.child_by_field_name("type").map(|t| baixar_tipo(t, src));
                    corpo.membros.push(Membro { nome, tipo });
                }
                None => corpo.aberto = true,
            },
            "index_signature" => corpo.aberto = true,
            _ => {}
        }
    }
    corpo
}

// ---------------------------------------------------------------------------------- functions

/// The parameters of a `formal_parameters` node.
pub(super) fn baixar_params(no: Node, src: &str) -> Vec<Param> {
    nomeados(no)
        .filter(|p| p.kind() != "decorator")
        .map(|p| {
            let alvo = p.child_by_field_name("pattern").unwrap_or(p);
            let rest = alvo.kind() == "rest_pattern" || p.kind() == "rest_pattern";
            let tipo = p.child_by_field_name("type").map(|t| baixar_tipo(t, src));
            Param { tipo, rest }
        })
        .collect()
}

/// Names declared by a `type_parameters` node.
pub(super) fn nomes_de_tipo_params(no: Option<Node>, src: &str) -> Vec<String> {
    let Some(tp) = no else { return Vec::new() };
    nomeados(tp)
        .filter_map(|p| p.child_by_field_name("name"))
        .map(|n| texto(src, n).to_string())
        .collect()
}

// ---------------------------------------------------------------------------------- declarations

/// Lower the declaration `no` (a statement or the `declaration` of an `export`) to (local name, Decl)
/// pairs. Pattern declarations (`const { a } = x`) yield one `Outro` per bound name.
pub(super) fn baixar_decl(no: Node, src: &str) -> Vec<(String, Decl)> {
    match no.kind() {
        "function_declaration" | "generator_function_declaration" | "function_signature" => {
            nome_simples(no, src, baixar_funcao(no, src))
        }
        "class_declaration" | "abstract_class_declaration" | "class" => {
            nome_simples(no, src, baixar_classe(no, src))
        }
        "interface_declaration" => nome_simples(no, src, baixar_interface(no, src)),
        "type_alias_declaration" => nome_simples(no, src, baixar_alias(no, src)),
        "enum_declaration" => nome_simples(no, src, baixar_enum(no, src)),
        "lexical_declaration" | "variable_declaration" => nomeados(no)
            .filter(|d| d.kind() == "variable_declarator")
            .flat_map(|d| baixar_declarador(d, src))
            .collect(),
        "internal_module" | "module" => nome_de_namespace(no, src),
        "ambient_declaration" => nomeados(no).flat_map(|f| baixar_decl(f, src)).collect(),
        _ => Vec::new(),
    }
}

fn baixar_funcao(no: Node, src: &str) -> Decl {
    let params = no
        .child_by_field_name("parameters")
        .map(|p| baixar_params(p, src))
        .unwrap_or_default();
    Decl::Funcao { params }
}

fn baixar_alias(no: Node, src: &str) -> Decl {
    let tipo = no
        .child_by_field_name("value")
        .map_or(TipoExpr::Outro, |v| baixar_tipo(v, src));
    let params = nomes_de_tipo_params(no.child_by_field_name("type_parameters"), src);
    Decl::Alias { params, tipo }
}

fn baixar_interface(no: Node, src: &str) -> Decl {
    let estende = nomeados(no)
        .filter(|f| f.kind() == "extends_type_clause")
        .flat_map(|c| {
            let mut cur = c.walk();
            c.children_by_field_name("type", &mut cur)
                .collect::<Vec<_>>()
        })
        .map(|t| baixar_tipo(t, src))
        .collect();
    let corpo = no
        .child_by_field_name("body")
        .map(|b| baixar_corpo(b, src))
        .unwrap_or_default();
    let params = nomes_de_tipo_params(no.child_by_field_name("type_parameters"), src);
    Decl::Interface {
        params,
        estende,
        corpo,
    }
}

fn baixar_enum(no: Node, src: &str) -> Decl {
    let membros = no
        .child_by_field_name("body")
        .map(|b| {
            nomeados(b)
                .filter_map(|m| match m.kind() {
                    "property_identifier" => Some(texto(src, m).to_string()),
                    "enum_assignment" => nome_do_membro(m, src),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Decl::Enum(membros)
}

/// `namespace A {}` / `namespace A.B {}` bind `A`; `declare module "x" {}` binds nothing.
fn nome_de_namespace(no: Node, src: &str) -> Vec<(String, Decl)> {
    match no.child_by_field_name("name") {
        Some(n) if matches!(n.kind(), "identifier" | "nested_identifier") => {
            let primeiro = texto(src, n).split('.').next().unwrap_or("").to_string();
            vec![(primeiro, Decl::Namespace)]
        }
        _ => Vec::new(),
    }
}

fn nome_simples(no: Node, src: &str, decl: Decl) -> Vec<(String, Decl)> {
    match no.child_by_field_name("name") {
        Some(n) => vec![(texto(src, n).to_string(), decl)],
        None => Vec::new(),
    }
}

/// `x = …` / `x: T = …` → a [`VarDecl`]; a destructuring pattern → `Outro` per bound name.
fn baixar_declarador(d: Node, src: &str) -> Vec<(String, Decl)> {
    let Some(nome) = d.child_by_field_name("name") else {
        return Vec::new();
    };
    if nome.kind() != "identifier" {
        return nomes_do_padrao(nome, src)
            .into_iter()
            .map(|n| (n, Decl::Outro))
            .collect();
    }
    let mut v = VarDecl {
        tipo: d.child_by_field_name("type").map(|t| baixar_tipo(t, src)),
        ..VarDecl::default()
    };
    if let Some(valor) = d.child_by_field_name("value") {
        preencher_inicializador(valor, src, &mut v);
    }
    vec![(texto(src, nome).to_string(), Decl::Variavel(v))]
}

const KINDS_FUNCAO: &[&str] = &[
    "arrow_function",
    "function_expression",
    "function",
    "generator_function",
];

fn preencher_inicializador(valor: Node, src: &str, v: &mut VarDecl) {
    if KINDS_FUNCAO.contains(&valor.kind()) {
        v.funcao = Some(params_da_funcao(valor, src));
        return;
    }
    if valor.kind() != "call_expression" {
        return;
    }
    let Some(callee) = valor.child_by_field_name("function") else {
        return;
    };
    let ultimo = texto(src, callee)
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_string();
    if ultimo != "memo" && ultimo != "forwardRef" {
        return;
    }
    let targs = valor
        .child_by_field_name("type_arguments")
        .map(|a| nomeados(a).map(|t| baixar_tipo(t, src)).collect())
        .unwrap_or_default();
    v.embrulho = Some((ultimo, targs));
    if let Some(args) = valor.child_by_field_name("arguments")
        && let Some(f) = nomeados(args).find(|a| KINDS_FUNCAO.contains(&a.kind()))
    {
        v.funcao = Some(params_da_funcao(f, src));
    }
}

/// Parameters of a function-like node: `parameters` (formal list) or the arrow's single `parameter`.
pub(super) fn params_da_funcao(f: Node, src: &str) -> Vec<Param> {
    if let Some(p) = f.child_by_field_name("parameters") {
        return baixar_params(p, src);
    }
    if f.child_by_field_name("parameter").is_some() {
        return vec![Param {
            tipo: None,
            rest: false,
        }];
    }
    Vec::new()
}

/// Names bound by a binding pattern (identifier / object / array pattern, with defaults and rest).
pub(super) fn nomes_do_padrao(no: Node, src: &str) -> Vec<String> {
    let mut out = Vec::new();
    coletar_padrao(no, src, &mut out);
    out
}

fn coletar_padrao(no: Node, src: &str, out: &mut Vec<String>) {
    match no.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => {
            out.push(texto(src, no).to_string())
        }
        "pair_pattern" => {
            if let Some(v) = no.child_by_field_name("value") {
                coletar_padrao(v, src, out);
            }
        }
        "assignment_pattern" | "object_assignment_pattern" => {
            if let Some(l) = no.child_by_field_name("left") {
                coletar_padrao(l, src, out);
            }
        }
        "object_pattern" | "array_pattern" | "rest_pattern" => {
            for f in nomeados(no) {
                coletar_padrao(f, src, out);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------- classes

/// Lower a class declaration/expression.
pub(super) fn baixar_classe(no: Node, src: &str) -> Decl {
    let params = nomes_de_tipo_params(no.child_by_field_name("type_parameters"), src);
    let estende = heranca_da_classe(no, src);
    let mut membros: Vec<Membro> = Vec::new();
    let mut estaticos: Vec<String> = Vec::new();
    let mut aberta = false;
    if let Some(corpo) = no.child_by_field_name("body") {
        for m in nomeados(corpo) {
            registrar_membro_de_classe(m, src, &mut membros, &mut estaticos, &mut aberta);
        }
        // `this.x = …` anywhere in the body defines x (JS classes declare no fields; TS accepts it too)
        coletar_atribuicoes_em_this(corpo, src, &mut membros, &mut aberta);
    }
    Decl::Classe(ClasseDecl {
        params,
        estende,
        membros,
        estaticos,
        aberta,
    })
}

fn registrar_membro_de_classe(
    m: Node,
    src: &str,
    membros: &mut Vec<Membro>,
    estaticos: &mut Vec<String>,
    aberta: &mut bool,
) {
    match m.kind() {
        "public_field_definition"
        | "method_definition"
        | "abstract_method_signature"
        | "method_signature" => {
            let Some(nome) = nome_do_membro(m, src) else {
                *aberta = true;
                return;
            };
            if tem_token(m, "static") {
                estaticos.push(nome);
                return;
            }
            if nome == "constructor" {
                propriedades_de_parametro(m, src, membros);
                return;
            }
            let tipo = m.child_by_field_name("type").map(|t| baixar_tipo(t, src));
            membros.push(Membro { nome, tipo });
        }
        "index_signature" => *aberta = true,
        _ => {}
    }
}

/// `constructor(private a: A, readonly b = 1)` declares `a` and `b`.
fn propriedades_de_parametro(ctor: Node, src: &str, membros: &mut Vec<Membro>) {
    let Some(params) = ctor.child_by_field_name("parameters") else {
        return;
    };
    for p in nomeados(params) {
        let modificador =
            nomeados(p).any(|f| f.kind() == "accessibility_modifier") || tem_token(p, "readonly");
        if !modificador {
            continue;
        }
        if let Some(nome) = p.child_by_field_name("pattern")
            && nome.kind() == "identifier"
        {
            let tipo = p.child_by_field_name("type").map(|t| baixar_tipo(t, src));
            membros.push(Membro {
                nome: texto(src, nome).to_string(),
                tipo,
            });
        }
    }
}

fn heranca_da_classe(no: Node, src: &str) -> Heranca {
    let Some(h) = nomeados(no).find(|f| f.kind() == "class_heritage") else {
        return Heranca::Nenhuma;
    };
    // TS: class_heritage > extends_clause(value, type_arguments); JS: class_heritage > <expression>
    let (valor, targs) = match nomeados(h).find(|f| f.kind() == "extends_clause") {
        Some(e) => (
            e.child_by_field_name("value"),
            e.child_by_field_name("type_arguments"),
        ),
        None => (nomeados(h).next(), None),
    };
    let Some(v) = valor else {
        return Heranca::Nenhuma;
    };
    let args: Vec<TipoExpr> = targs
        .map(|a| nomeados(a).map(|t| baixar_tipo(t, src)).collect())
        .unwrap_or_default();
    match v.kind() {
        "identifier" | "member_expression" => heranca_nomeada(texto(src, v), args),
        _ => Heranca::Opaca,
    }
}

/// `Base` / `ns.Base` as a type reference; a dotted chain with anything but plain names is opaque.
fn heranca_nomeada(escrito: &str, args: Vec<TipoExpr>) -> Heranca {
    let eh_nome = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    };
    let mut partes: Vec<String> = escrito.split('.').map(str::to_string).collect();
    let nome = partes.pop().unwrap_or_default();
    if eh_nome(&nome) && partes.iter().all(|p| eh_nome(p)) {
        Heranca::Tipo(TipoExpr::Ref {
            nome,
            qual: partes,
            args,
        })
    } else {
        Heranca::Opaca
    }
}

/// Collect `this.NAME = …` targets under `no`, not entering nested non-arrow functions or classes.
fn coletar_atribuicoes_em_this(no: Node, src: &str, membros: &mut Vec<Membro>, aberta: &mut bool) {
    let mut pilha = vec![no];
    while let Some(n) = pilha.pop() {
        match n.kind() {
            "assignment_expression" | "augmented_assignment_expression" => {
                if let Some(l) = n.child_by_field_name("left") {
                    registrar_alvo_em_this(l, src, membros, aberta);
                }
            }
            "call_expression" => {
                let t = texto(src, n);
                if t.starts_with("Object.assign(this")
                    || t.starts_with("Object.defineProperty(this")
                {
                    *aberta = true;
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        pilha.extend(n.named_children(&mut c));
    }
}

fn registrar_alvo_em_this(l: Node, src: &str, membros: &mut Vec<Membro>, aberta: &mut bool) {
    match l.kind() {
        "member_expression" => {
            if l.child_by_field_name("object")
                .is_some_and(|o| o.kind() == "this")
                && let Some(p) = l.child_by_field_name("property")
            {
                membros.push(Membro {
                    nome: texto(src, p).to_string(),
                    tipo: None,
                });
            }
        }
        "subscript_expression"
            if l.child_by_field_name("object")
                .is_some_and(|o| o.kind() == "this") =>
        {
            *aberta = true;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vgp::lang::parse;

    fn decls(lang: &str, src: &str) -> Vec<(String, Decl)> {
        let tree = parse(lang, src).expect("parse");
        let root = tree.root_node();
        let mut c = root.walk();
        root.named_children(&mut c)
            .flat_map(|n| baixar_decl(n, src))
            .collect()
    }

    fn membros(d: &Decl) -> Vec<&str> {
        match d {
            Decl::Interface { corpo, .. } => {
                corpo.membros.iter().map(|m| m.nome.as_str()).collect()
            }
            Decl::Classe(c) => c.membros.iter().map(|m| m.nome.as_str()).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn interface_com_extends_e_corpo() {
        let ds = decls(
            "typescript",
            "interface A<T> extends B, C<string> { x: number; y?(): void; 'k-z': string }",
        );
        let (nome, d) = &ds[0];
        assert_eq!(nome, "A");
        assert_eq!(membros(d), vec!["x", "y", "k-z"]);
        let Decl::Interface {
            estende, params, ..
        } = d
        else {
            panic!("não é interface")
        };
        assert_eq!(params, &vec!["T".to_string()]);
        assert_eq!(estende.len(), 2);
    }

    #[test]
    fn assinatura_de_indice_abre_o_conjunto() {
        let ds = decls(
            "typescript",
            "interface A { [k: string]: unknown; x: number }",
        );
        let Decl::Interface { corpo, .. } = &ds[0].1 else {
            panic!()
        };
        assert!(corpo.aberto);
    }

    #[test]
    fn classe_com_campos_metodos_estaticos_e_propriedades_de_parametro() {
        let ds = decls(
            "typescript",
            "class K extends Base<T> { a: number = 1; static s = 2; get g() { return 1 } m() { this.z = 3 } constructor(private p: string, readonly q = 1, plain: number) { super() } }",
        );
        let Decl::Classe(c) = &ds[0].1 else { panic!() };
        let nomes: Vec<&str> = c.membros.iter().map(|m| m.nome.as_str()).collect();
        for esperado in ["a", "g", "m", "z", "p", "q"] {
            assert!(nomes.contains(&esperado), "{esperado} em {nomes:?}");
        }
        assert!(
            !nomes.contains(&"plain"),
            "parâmetro simples não é propriedade"
        );
        assert_eq!(c.estaticos, vec!["s".to_string()]);
        assert!(matches!(&c.estende, Heranca::Tipo(TipoExpr::Ref { nome, .. }) if nome == "Base"));
        assert!(!c.aberta);
    }

    #[test]
    fn classe_js_sem_campos_recebe_os_atribuidos_em_this() {
        let ds = decls(
            "javascript",
            "class K extends Base { constructor() { super(); this.a = 1; } m() { this.b = 2 } }",
        );
        let Decl::Classe(c) = &ds[0].1 else { panic!() };
        let nomes: Vec<&str> = c.membros.iter().map(|m| m.nome.as_str()).collect();
        assert!(nomes.contains(&"a") && nomes.contains(&"b") && !nomes.contains(&"constructor"));
    }

    #[test]
    fn object_assign_em_this_abre_a_classe() {
        let ds = decls(
            "javascript",
            "class K { constructor(o) { Object.assign(this, o) } }",
        );
        let Decl::Classe(c) = &ds[0].1 else { panic!() };
        assert!(c.aberta);
    }

    #[test]
    fn mixin_em_extends_e_opaco() {
        let ds = decls("javascript", "class K extends mixin(Base) {}");
        let Decl::Classe(c) = &ds[0].1 else { panic!() };
        assert_eq!(c.estende, Heranca::Opaca);
    }

    #[test]
    fn variavel_com_arrow_e_embrulho_de_memo() {
        let ds = decls(
            "tsx",
            "const A = (p: Props) => null; const B = React.memo((p: Props) => null); const C: FC<Props> = () => null;",
        );
        let Decl::Variavel(a) = &ds[0].1 else {
            panic!()
        };
        assert_eq!(a.funcao.as_ref().map(Vec::len), Some(1));
        let Decl::Variavel(b) = &ds[1].1 else {
            panic!()
        };
        assert_eq!(b.embrulho.as_ref().map(|(n, _)| n.as_str()), Some("memo"));
        assert_eq!(b.funcao.as_ref().map(Vec::len), Some(1));
        let Decl::Variavel(c) = &ds[2].1 else {
            panic!()
        };
        assert!(matches!(&c.tipo, Some(TipoExpr::Ref { nome, .. }) if nome == "FC"));
    }

    #[test]
    fn tipos_basicos_sao_baixados() {
        let ds = decls(
            "typescript",
            "type T = { a: number } | (Foo & Bar)[] | ns.X<string> | null;",
        );
        let Decl::Alias { tipo, .. } = &ds[0].1 else {
            panic!()
        };
        let TipoExpr::Uniao(u) = tipo else {
            panic!("{tipo:?}")
        };
        assert!(matches!(&u[0], TipoExpr::Obj(_)));
        assert!(matches!(&u[1], TipoExpr::Array(_)));
        assert!(
            matches!(&u[2], TipoExpr::Ref { nome, qual, args } if nome == "X" && qual == &vec!["ns".to_string()] && args.len() == 1)
        );
        assert_eq!(u[3], TipoExpr::Prim("null".into()));
    }

    #[test]
    fn padrao_de_desestruturacao_nomeia_tudo() {
        let src = "const { a, b: { c }, ...r } = x; const [d, e = 1, ...f] = y;";
        let ds = decls("typescript", src);
        let nomes: Vec<&str> = ds.iter().map(|(n, _)| n.as_str()).collect();
        for n in ["a", "c", "r", "d", "e", "f"] {
            assert!(nomes.contains(&n), "{n} em {nomes:?}");
        }
    }
}
