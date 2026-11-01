//! Lexical scope analysis for TypeScript / TSX / JavaScript: which value identifiers refer to
//! NOTHING declared in the file.
//!
//! The walk builds a scope tree (program, function, block, loop, `catch`, class, enum, namespace),
//! declares what each scope binds — hoisted `var`s and functions, `let`/`const`/`class`, parameters
//! and their patterns, imports, `declare`d names — and reports every value reference that no
//! enclosing scope binds. Type positions are not walked: they live in another namespace and a
//! value-level check would only misfire there.
//!
//! Over-binding is the safe direction (an extra binding hides a typo, never invents one), so
//! the analysis hoists generously: a `var` anywhere in a function binds in the whole function, a
//! function declared in a block binds in the block AND the function.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use super::decls::nomes_do_padrao;
use super::util::{tem_token, texto};

/// What kind of declaration introduced a binding (the type inference reads it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BindTipo {
    Const,
    Let,
    Var,
    Param,
    Funcao,
    Classe,
    Import,
    Enum,
    Namespace,
    Catch,
    Outro,
}

/// A name bound in a scope, with the node that declares it (declarator, parameter, class…).
#[derive(Clone, Copy)]
pub(super) struct Ligacao<'t> {
    pub tipo: BindTipo,
    pub no: Node<'t>,
}

struct Escopo<'t> {
    pai: Option<usize>,
    /// The function (or program) scope `var` and sloppy function hoisting belong to.
    funcao: usize,
    nomes: HashMap<String, Ligacao<'t>>,
}

/// A value reference that no enclosing scope binds.
pub(super) struct Referencia<'t> {
    pub nome: String,
    pub no: Node<'t>,
    /// A JSX tag name (`<Comp>`, `<ns.Comp>`), not a plain value use.
    pub componente: bool,
}

/// The result of analysing one tree.
pub(super) struct Analise<'t> {
    src: &'t str,
    escopos: Vec<Escopo<'t>>,
    /// scope-creating node id → scope index
    por_no: HashMap<usize, usize>,
    /// References nothing in the file binds.
    pub livres: Vec<Referencia<'t>>,
    /// Names that are the target of a plain assignment anywhere (JS implicit globals).
    pub atribuidos: HashSet<String>,
    /// A `with` statement dissolves static scoping: no free name is proof.
    pub tem_with: bool,
    /// Names the file itself feature-detects (`typeof X`, `window.X`): optional host globals, so a
    /// use of them elsewhere in the file is not proof that they are missing.
    pub detectados: HashSet<String>,
}

const FUNCOES: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "function_expression",
    "function",
    "generator_function",
    "arrow_function",
    "method_definition",
];

/// Subtrees with no value reference a check should judge: types, literals, trivia.
const IGNORAR: &[&str] = &[
    "comment",
    "string",
    "regex",
    "number",
    "this",
    "super",
    "meta_property",
    "import_statement",
    "ambient_declaration",
    "interface_declaration",
    "type_alias_declaration",
    "jsx_closing_element",
    // type positions
    "type_annotation",
    "type_arguments",
    "type_parameters",
    "predefined_type",
    "type_identifier",
    "nested_type_identifier",
    "generic_type",
    "object_type",
    "union_type",
    "intersection_type",
    "array_type",
    "tuple_type",
    "function_type",
    "constructor_type",
    "literal_type",
    "conditional_type",
    "type_query",
    "lookup_type",
    "index_type_query",
    "readonly_type",
    "parenthesized_type",
    "template_literal_type",
    "infer_type",
    "this_type",
    "existential_type",
    "type_predicate",
    "type_predicate_annotation",
    "asserts_annotation",
    "implements_clause",
    "extends_type_clause",
];

/// Analyse `raiz` (the `program` node) of `src`.
pub(super) fn analisar<'t>(raiz: Node<'t>, src: &'t str) -> Analise<'t> {
    let mut a = Analise {
        src,
        escopos: Vec::new(),
        por_no: HashMap::new(),
        livres: Vec::new(),
        atribuidos: HashSet::new(),
        tem_with: false,
        detectados: HashSet::new(),
    };
    let global = a.novo_escopo(None, raiz, true);
    a.declarar_filhos(raiz, global);
    a.hoist_vars(raiz, global);
    a.visitar_filhos(raiz, global);
    a
}

fn nomeados<'t>(no: Node<'t>) -> Vec<Node<'t>> {
    let mut c = no.walk();
    no.named_children(&mut c).collect()
}

impl<'t> Analise<'t> {
    fn novo_escopo(&mut self, pai: Option<usize>, no: Node<'t>, funcao: bool) -> usize {
        let idx = self.escopos.len();
        let fn_idx = if funcao {
            idx
        } else {
            pai.map_or(idx, |p| self.escopos[p].funcao)
        };
        self.escopos.push(Escopo {
            pai,
            funcao: fn_idx,
            nomes: HashMap::new(),
        });
        self.por_no.insert(no.id(), idx);
        idx
    }

    fn declarar(&mut self, esc: usize, nome: &str, tipo: BindTipo, no: Node<'t>) {
        if nome.is_empty() {
            return;
        }
        self.escopos[esc]
            .nomes
            .entry(nome.to_string())
            .or_insert(Ligacao { tipo, no });
    }

    fn declarar_padrao(&mut self, esc: usize, padrao: Node<'t>, tipo: BindTipo, no: Node<'t>) {
        for n in nomes_do_padrao(padrao, self.src) {
            self.declarar(esc, &n, tipo, no);
        }
    }

    /// The binding `nome` visible from the scope `esc`.
    fn procurar(&self, esc: usize, nome: &str) -> Option<Ligacao<'t>> {
        let mut atual = Some(esc);
        while let Some(i) = atual {
            if let Some(l) = self.escopos[i].nomes.get(nome) {
                return Some(*l);
            }
            atual = self.escopos[i].pai;
        }
        None
    }

    /// The innermost scope that contains `no`.
    pub fn escopo_de(&self, no: Node<'t>) -> usize {
        let mut atual = Some(no);
        while let Some(n) = atual {
            if let Some(i) = self.por_no.get(&n.id()) {
                return *i;
            }
            atual = n.parent();
        }
        0
    }

    /// The binding `nome` as seen from `no`.
    pub fn resolver(&self, no: Node<'t>, nome: &str) -> Option<Ligacao<'t>> {
        self.procurar(self.escopo_de(no), nome)
    }

    /// The binding `nome` as seen from `no`, only if it lives in the module's top-level scope —
    /// the scope whose declarations the module summary describes (a local `const f` that shadows a
    /// top-level `f` is a different function).
    pub fn resolver_no_topo(&self, no: Node<'t>, nome: &str) -> Option<Ligacao<'t>> {
        let mut atual = Some(self.escopo_de(no));
        while let Some(i) = atual {
            if let Some(l) = self.escopos[i].nomes.get(nome) {
                return (i == 0).then_some(*l);
            }
            atual = self.escopos[i].pai;
        }
        None
    }

    // ------------------------------------------------------------------ declaring

    /// Declare the statements directly under `pai` (a program, a block, a switch body).
    fn declarar_filhos(&mut self, pai: Node<'t>, esc: usize) {
        for stmt in nomeados(pai) {
            self.declarar_stmt(stmt, esc);
        }
    }

    fn declarar_stmt(&mut self, stmt: Node<'t>, esc: usize) {
        let src = self.src;
        let fn_esc = self.escopos[esc].funcao;
        match stmt.kind() {
            "lexical_declaration" => {
                let tipo = if tem_token(stmt, "const") {
                    BindTipo::Const
                } else {
                    BindTipo::Let
                };
                self.declarar_declaradores(stmt, esc, tipo);
            }
            "variable_declaration" => {
                self.declarar_declaradores(stmt, esc, BindTipo::Var);
                self.declarar_declaradores(stmt, fn_esc, BindTipo::Var);
            }
            "function_declaration" | "generator_function_declaration" | "function_signature" => {
                if let Some(n) = stmt.child_by_field_name("name") {
                    self.declarar(esc, texto(src, n), BindTipo::Funcao, stmt);
                    self.declarar(fn_esc, texto(src, n), BindTipo::Funcao, stmt);
                }
            }
            "class_declaration" | "abstract_class_declaration" => {
                self.declarar_nome(stmt, esc, BindTipo::Classe)
            }
            "enum_declaration" => self.declarar_nome(stmt, esc, BindTipo::Enum),
            "interface_declaration" | "type_alias_declaration" => {
                self.declarar_nome(stmt, esc, BindTipo::Outro)
            }
            _ => self.declarar_stmt_de_modulo(stmt, esc),
        }
    }

    /// Namespaces, imports, exports and `declare` blocks.
    fn declarar_stmt_de_modulo(&mut self, stmt: Node<'t>, esc: usize) {
        match stmt.kind() {
            "internal_module" | "module" => self.declarar_namespace(stmt, esc),
            "expression_statement" => {
                if let Some(e) = stmt.named_child(0)
                    && matches!(e.kind(), "internal_module" | "module")
                {
                    self.declarar_namespace(e, esc);
                }
            }
            "import_statement" => self.declarar_import(stmt, esc),
            "import_alias" => {
                if let Some(n) = stmt.named_child(0) {
                    self.declarar(esc, texto(self.src, n), BindTipo::Outro, stmt);
                }
            }
            "export_statement" => self.declarar_export(stmt, esc),
            "ambient_declaration" => self.declarar_ambiente(stmt, esc),
            _ => {}
        }
    }

    fn declarar_nome(&mut self, decl: Node<'t>, esc: usize, tipo: BindTipo) {
        if let Some(n) = decl.child_by_field_name("name") {
            self.declarar(esc, texto(self.src, n), tipo, decl);
        }
    }

    fn declarar_declaradores(&mut self, decl: Node<'t>, esc: usize, tipo: BindTipo) {
        for d in nomeados(decl)
            .into_iter()
            .filter(|d| d.kind() == "variable_declarator")
        {
            if let Some(nome) = d.child_by_field_name("name") {
                self.declarar_padrao(esc, nome, tipo, d);
            }
        }
    }

    fn declarar_namespace(&mut self, modulo: Node<'t>, esc: usize) {
        if let Some(n) = modulo.child_by_field_name("name")
            && matches!(n.kind(), "identifier" | "nested_identifier")
        {
            let primeiro = texto(self.src, n)
                .split('.')
                .next()
                .unwrap_or("")
                .to_string();
            self.declarar(esc, &primeiro, BindTipo::Namespace, modulo);
        }
    }

    fn declarar_import(&mut self, imp: Node<'t>, esc: usize) {
        let src = self.src;
        for clausula in nomeados(imp) {
            match clausula.kind() {
                "import_clause" => {
                    for f in nomeados(clausula) {
                        self.declarar_clausula_de_import(f, esc, imp);
                    }
                }
                "import_require_clause" => {
                    if let Some(id) = clausula.named_child(0) {
                        self.declarar(esc, texto(src, id), BindTipo::Import, imp);
                    }
                }
                _ => {}
            }
        }
    }

    fn declarar_clausula_de_import(&mut self, f: Node<'t>, esc: usize, imp: Node<'t>) {
        let src = self.src;
        match f.kind() {
            "identifier" => self.declarar(esc, texto(src, f), BindTipo::Import, imp),
            "namespace_import" => {
                if let Some(n) = f.named_child(0) {
                    self.declarar(esc, texto(src, n), BindTipo::Import, imp);
                }
            }
            "named_imports" => {
                for s in nomeados(f)
                    .into_iter()
                    .filter(|s| s.kind() == "import_specifier")
                {
                    let local = s
                        .child_by_field_name("alias")
                        .or_else(|| s.child_by_field_name("name"));
                    if let Some(l) = local {
                        self.declarar(esc, texto(src, l), BindTipo::Import, imp);
                    }
                }
            }
            _ => {}
        }
    }

    fn declarar_export(&mut self, exp: Node<'t>, esc: usize) {
        if let Some(d) = exp.child_by_field_name("declaration") {
            self.declarar_stmt(d, esc);
        }
        for f in nomeados(exp)
            .into_iter()
            .filter(|f| f.kind() == "import_alias")
        {
            self.declarar_stmt(f, esc);
        }
    }

    /// `declare const x`, `declare function f()`, `declare class C`, `declare namespace N`, and
    /// `declare global { … }` (whose names land in the GLOBAL scope).
    fn declarar_ambiente(&mut self, amb: Node<'t>, esc: usize) {
        for f in nomeados(amb) {
            if f.kind() == "statement_block" {
                self.declarar_filhos(f, 0); // `declare global { var X }`
            } else {
                self.declarar_stmt(f, esc);
            }
        }
    }

    /// `var` declarations and block-level functions anywhere in `no` bind in the function scope
    /// `esc_fn` (nested functions and classes keep their own).
    fn hoist_vars(&mut self, no: Node<'t>, esc_fn: usize) {
        let mut pilha = nomeados(no);
        while let Some(n) = pilha.pop() {
            match n.kind() {
                k if FUNCOES.contains(&k) || k == "class_declaration" || k == "class" => {
                    if matches!(k, "function_declaration" | "generator_function_declaration")
                        && let Some(nome) = n.child_by_field_name("name")
                    {
                        self.declarar(esc_fn, texto(self.src, nome), BindTipo::Funcao, n);
                    }
                    continue; // its body is another function scope
                }
                "variable_declaration" => self.declarar_declaradores(n, esc_fn, BindTipo::Var),
                "for_in_statement" if tem_token(n, "var") => {
                    if let Some(l) = n.child_by_field_name("left") {
                        self.declarar_padrao(esc_fn, l, BindTipo::Var, n);
                    }
                }
                _ => {}
            }
            pilha.extend(nomeados(n));
        }
    }

    // ------------------------------------------------------------------ visiting

    fn visitar_filhos(&mut self, no: Node<'t>, esc: usize) {
        for f in nomeados(no) {
            self.visitar(f, esc);
        }
    }

    fn visitar(&mut self, no: Node<'t>, esc: usize) {
        let k = no.kind();
        if IGNORAR.contains(&k) {
            return;
        }
        if FUNCOES.contains(&k) {
            return self.visitar_funcao(no, esc);
        }
        if !self.visitar_com_escopo(k, no, esc) {
            self.visitar_expressao(k, no, esc);
        }
    }

    /// The constructs that open a scope of their own; `false` if `k` is none of them.
    fn visitar_com_escopo(&mut self, k: &str, no: Node<'t>, esc: usize) -> bool {
        match k {
            "statement_block" => self.visitar_bloco(no, esc),
            "for_statement" => self.visitar_for(no, esc),
            "for_in_statement" => self.visitar_for_in(no, esc),
            "catch_clause" => self.visitar_catch(no, esc),
            "class_declaration" | "class" | "abstract_class_declaration" => {
                self.visitar_classe(no, esc)
            }
            "switch_statement" => self.visitar_switch(no, esc),
            "enum_declaration" => self.visitar_enum(no, esc),
            "internal_module" | "module" => self.visitar_namespace(no, esc),
            _ => return false,
        }
        true
    }

    /// Expressions and statements that bind nothing themselves.
    fn visitar_expressao(&mut self, k: &str, no: Node<'t>, esc: usize) {
        match k {
            "identifier" | "shorthand_property_identifier" => self.referencia(no, esc, false),
            "export_statement" => self.visitar_export(no, esc),
            "member_expression" => self.visitar_objeto_do_membro(no, esc),
            "pair" => self.visitar_par(no, esc),
            "variable_declarator" => self.visitar_declarador(no, esc),
            "assignment_expression" => self.visitar_atribuicao(no, esc),
            "augmented_assignment_expression" | "update_expression" => {
                self.visitar_modificacao(no, esc)
            }
            "unary_expression" => self.visitar_unario(no, esc),
            "jsx_opening_element" | "jsx_self_closing_element" => self.visitar_jsx(no, esc),
            "as_expression" | "satisfies_expression" | "non_null_expression" | "type_assertion" => {
                self.visitar_so_a_expressao(no, esc);
            }
            "with_statement" => {
                self.tem_with = true;
                self.visitar_filhos(no, esc);
            }
            _ => self.visitar_filhos(no, esc),
        }
    }

    /// `a.b`: only the object is a reference; the property name is not. `window.b` (and the other
    /// global objects) also tells that the file treats `b` as a property of the host.
    fn visitar_objeto_do_membro(&mut self, no: Node<'t>, esc: usize) {
        let Some(o) = no.child_by_field_name("object") else {
            return;
        };
        if o.kind() == "identifier"
            && matches!(
                texto(self.src, o),
                "window" | "globalThis" | "self" | "global"
            )
            && let Some(p) = no.child_by_field_name("property")
        {
            self.detectados.insert(texto(self.src, p).to_string());
        }
        self.visitar(o, esc);
    }

    /// `x as T` / `x satisfies T` / `<T>x` / `x!`: only the expression side — the other child is a type.
    fn visitar_so_a_expressao(&mut self, no: Node<'t>, esc: usize) {
        for f in nomeados(no)
            .into_iter()
            .filter(|f| !IGNORAR.contains(&f.kind()))
        {
            self.visitar(f, esc);
        }
    }

    /// `x += 1` / `x++`: the target is reassigned (it keeps being a reference too).
    fn visitar_modificacao(&mut self, no: Node<'t>, esc: usize) {
        let alvo = no
            .child_by_field_name("left")
            .or_else(|| no.child_by_field_name("argument"));
        if let Some(a) = alvo
            && a.kind() == "identifier"
        {
            self.atribuidos.insert(texto(self.src, a).to_string());
        }
        self.visitar_filhos(no, esc);
    }

    fn referencia(&mut self, no: Node<'t>, esc: usize, componente: bool) {
        let nome = texto(self.src, no);
        if self.procurar(esc, nome).is_none() {
            self.livres.push(Referencia {
                nome: nome.to_string(),
                no,
                componente,
            });
        }
    }

    fn visitar_funcao(&mut self, f: Node<'t>, esc: usize) {
        let src = self.src;
        let fesc = self.novo_escopo(Some(esc), f, true);
        if matches!(
            f.kind(),
            "function_expression" | "function" | "generator_function"
        ) && let Some(n) = f.child_by_field_name("name")
        {
            self.declarar(fesc, texto(src, n), BindTipo::Funcao, f);
        }
        if f.kind() != "arrow_function" {
            self.declarar(fesc, "arguments", BindTipo::Outro, f);
        }
        let params = f
            .child_by_field_name("parameters")
            .map(nomeados)
            .unwrap_or_default();
        for p in &params {
            let alvo = p.child_by_field_name("pattern").unwrap_or(*p);
            // only a plain identifier has the parameter's own type; `({ a }: P)` binds members of it
            let tipo = if alvo.kind() == "identifier" {
                BindTipo::Param
            } else {
                BindTipo::Outro
            };
            self.declarar_padrao(fesc, alvo, tipo, *p);
        }
        if let Some(unico) = f.child_by_field_name("parameter") {
            self.declarar_padrao(fesc, unico, BindTipo::Param, unico);
        }
        for d in nomeados(f).into_iter().filter(|d| d.kind() == "decorator") {
            self.visitar(d, esc);
        }
        if let Some(n) = f.child_by_field_name("name")
            && n.kind() == "computed_property_name"
        {
            self.visitar(n, esc);
        }
        for p in params {
            self.visitar_parametro(p, fesc);
        }
        if let Some(corpo) = f.child_by_field_name("body") {
            if corpo.kind() == "statement_block" {
                self.por_no.insert(corpo.id(), fesc);
                self.declarar_filhos(corpo, fesc);
                self.hoist_vars(corpo, fesc);
                self.visitar_filhos(corpo, fesc);
            } else {
                self.visitar(corpo, fesc);
            }
        }
    }

    /// A parameter: its default value and the defaults/computed keys inside its pattern.
    fn visitar_parametro(&mut self, p: Node<'t>, esc: usize) {
        match p.child_by_field_name("pattern") {
            Some(padrao) => {
                self.visitar_padrao_decl(padrao, esc);
                if let Some(v) = p.child_by_field_name("value") {
                    self.visitar(v, esc);
                }
            }
            None => self.visitar_padrao_decl(p, esc),
        }
        for d in nomeados(p).into_iter().filter(|d| d.kind() == "decorator") {
            self.visitar(d, esc);
        }
    }

    /// Walk a DECLARING pattern: names are bindings, but defaults and computed keys are expressions.
    fn visitar_padrao_decl(&mut self, p: Node<'t>, esc: usize) {
        match p.kind() {
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(l) = p.child_by_field_name("left") {
                    self.visitar_padrao_decl(l, esc);
                }
                if let Some(r) = p.child_by_field_name("right") {
                    self.visitar(r, esc);
                }
            }
            "pair_pattern" => {
                if let Some(k) = p.child_by_field_name("key")
                    && k.kind() == "computed_property_name"
                {
                    self.visitar(k, esc);
                }
                if let Some(v) = p.child_by_field_name("value") {
                    self.visitar_padrao_decl(v, esc);
                }
            }
            "object_pattern" | "array_pattern" | "rest_pattern" => {
                for f in nomeados(p) {
                    self.visitar_padrao_decl(f, esc);
                }
            }
            _ => {}
        }
    }

    /// Walk an ASSIGNMENT target: its names are references.
    fn visitar_alvo(&mut self, p: Node<'t>, esc: usize) {
        match p.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                self.atribuidos.insert(texto(self.src, p).to_string());
                self.referencia(p, esc, false);
            }
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(l) = p.child_by_field_name("left") {
                    self.visitar_alvo(l, esc);
                }
                if let Some(r) = p.child_by_field_name("right") {
                    self.visitar(r, esc);
                }
            }
            "pair_pattern" => {
                if let Some(v) = p.child_by_field_name("value") {
                    self.visitar_alvo(v, esc);
                }
            }
            "object_pattern" | "array_pattern" | "rest_pattern" => {
                for f in nomeados(p) {
                    self.visitar_alvo(f, esc);
                }
            }
            _ => self.visitar(p, esc),
        }
    }

    fn visitar_bloco(&mut self, b: Node<'t>, esc: usize) {
        let bes = self.novo_escopo(Some(esc), b, false);
        self.declarar_filhos(b, bes);
        self.visitar_filhos(b, bes);
    }

    fn visitar_for(&mut self, f: Node<'t>, esc: usize) {
        let fes = self.novo_escopo(Some(esc), f, false);
        if let Some(ini) = f.child_by_field_name("initializer") {
            self.declarar_stmt(ini, fes);
        }
        self.visitar_filhos(f, fes);
    }

    fn visitar_for_in(&mut self, f: Node<'t>, esc: usize) {
        let fes = self.novo_escopo(Some(esc), f, false);
        let esquerda = f.child_by_field_name("left");
        let declara = tem_token(f, "const") || tem_token(f, "let") || tem_token(f, "var");
        if let Some(l) = esquerda {
            if declara {
                let tipo = if tem_token(f, "const") {
                    BindTipo::Const
                } else {
                    BindTipo::Let
                };
                self.declarar_padrao(fes, l, tipo, f);
                self.visitar_padrao_decl(l, fes);
            } else {
                self.visitar_alvo(l, fes);
            }
        }
        for no in [
            f.child_by_field_name("right"),
            f.child_by_field_name("body"),
        ]
        .into_iter()
        .flatten()
        {
            self.visitar(no, fes);
        }
    }

    fn visitar_catch(&mut self, c: Node<'t>, esc: usize) {
        let ces = self.novo_escopo(Some(esc), c, false);
        if let Some(p) = c.child_by_field_name("parameter") {
            self.declarar_padrao(ces, p, BindTipo::Catch, c);
            self.visitar_padrao_decl(p, ces);
        }
        if let Some(corpo) = c.child_by_field_name("body") {
            self.visitar(corpo, ces);
        }
    }

    fn visitar_classe(&mut self, c: Node<'t>, esc: usize) {
        let ces = self.novo_escopo(Some(esc), c, false);
        if let Some(n) = c.child_by_field_name("name") {
            self.declarar(ces, texto(self.src, n), BindTipo::Classe, c);
        }
        for f in nomeados(c) {
            match f.kind() {
                "decorator" | "class_heritage" => self.visitar(f, esc),
                _ => {}
            }
        }
        let Some(corpo) = c.child_by_field_name("body") else {
            return;
        };
        for m in nomeados(corpo) {
            self.visitar_membro_de_classe(m, ces);
        }
    }

    fn visitar_membro_de_classe(&mut self, m: Node<'t>, ces: usize) {
        match m.kind() {
            "method_definition" => self.visitar_funcao(m, ces),
            "public_field_definition" | "field_definition" => {
                if let Some(n) = m.child_by_field_name("name")
                    && n.kind() == "computed_property_name"
                {
                    self.visitar(n, ces);
                }
                for d in nomeados(m).into_iter().filter(|d| d.kind() == "decorator") {
                    self.visitar(d, ces);
                }
                let valor = m
                    .child_by_field_name("value")
                    .or_else(|| m.child_by_field_name("property"));
                if let Some(v) = valor {
                    self.visitar(v, ces);
                }
            }
            "class_static_block" => {
                let fes = self.novo_escopo(Some(ces), m, true);
                if let Some(b) = m.child_by_field_name("body") {
                    self.declarar_filhos(b, fes);
                    self.hoist_vars(b, fes);
                    self.visitar_filhos(b, fes);
                }
            }
            _ => {}
        }
    }

    fn visitar_switch(&mut self, s: Node<'t>, esc: usize) {
        if let Some(v) = s.child_by_field_name("value") {
            self.visitar(v, esc);
        }
        let Some(corpo) = s.child_by_field_name("body") else {
            return;
        };
        let ses = self.novo_escopo(Some(esc), s, false);
        for caso in nomeados(corpo) {
            self.declarar_filhos(caso, ses);
        }
        for caso in nomeados(corpo) {
            self.visitar_filhos(caso, ses);
        }
    }

    fn visitar_enum(&mut self, e: Node<'t>, esc: usize) {
        let ees = self.novo_escopo(Some(esc), e, false);
        let Some(corpo) = e.child_by_field_name("body") else {
            return;
        };
        for m in nomeados(corpo) {
            let nome = if m.kind() == "enum_assignment" {
                m.child_by_field_name("name")
            } else {
                Some(m)
            };
            if let Some(n) = nome {
                self.declarar(ees, texto(self.src, n), BindTipo::Outro, m);
            }
        }
        for m in nomeados(corpo)
            .into_iter()
            .filter(|m| m.kind() == "enum_assignment")
        {
            if let Some(v) = m.child_by_field_name("value") {
                self.visitar(v, ees);
            }
        }
    }

    fn visitar_namespace(&mut self, m: Node<'t>, esc: usize) {
        let Some(corpo) = m.child_by_field_name("body") else {
            return;
        };
        let mes = self.novo_escopo(Some(esc), m, true);
        self.declarar_filhos(corpo, mes);
        self.hoist_vars(corpo, mes);
        self.visitar_filhos(corpo, mes);
    }

    fn visitar_export(&mut self, e: Node<'t>, esc: usize) {
        let com_origem = e.child_by_field_name("source").is_some();
        for f in nomeados(e) {
            match f.kind() {
                "export_clause" => {
                    if !com_origem {
                        for s in nomeados(f) {
                            if let Some(n) = s.child_by_field_name("name") {
                                self.referencia(n, esc, false);
                            }
                        }
                    }
                }
                "namespace_export" => {}
                _ => self.visitar(f, esc),
            }
        }
    }

    fn visitar_par(&mut self, p: Node<'t>, esc: usize) {
        if let Some(k) = p.child_by_field_name("key")
            && k.kind() == "computed_property_name"
        {
            self.visitar(k, esc);
        }
        if let Some(v) = p.child_by_field_name("value") {
            self.visitar(v, esc);
        }
    }

    fn visitar_declarador(&mut self, d: Node<'t>, esc: usize) {
        if let Some(n) = d.child_by_field_name("name") {
            self.visitar_padrao_decl(n, esc);
        }
        if let Some(v) = d.child_by_field_name("value") {
            self.visitar(v, esc);
        }
    }

    fn visitar_atribuicao(&mut self, a: Node<'t>, esc: usize) {
        if let Some(l) = a.child_by_field_name("left") {
            self.visitar_alvo(l, esc);
        }
        if let Some(r) = a.child_by_field_name("right") {
            self.visitar(r, esc);
        }
    }

    /// `typeof undeclared` is legal and yields `"undefined"` — the operand is not judged.
    fn visitar_unario(&mut self, u: Node<'t>, esc: usize) {
        let eh_typeof = u
            .child_by_field_name("operator")
            .is_some_and(|o| texto(self.src, o) == "typeof");
        let operando = u.child_by_field_name("argument");
        match operando {
            Some(a) if eh_typeof && a.kind() == "identifier" => {
                // the file tests for the name: it is an optional host global, not a missing one
                self.detectados.insert(texto(self.src, a).to_string());
            }
            Some(a) => self.visitar(a, esc),
            None => {}
        }
    }

    fn visitar_jsx(&mut self, el: Node<'t>, esc: usize) {
        if let Some(nome) = el.child_by_field_name("name") {
            self.visitar_nome_jsx(nome, esc);
        }
        let mut c = el.walk();
        let atributos: Vec<Node<'t>> = el.children_by_field_name("attribute", &mut c).collect();
        for a in atributos {
            match a.kind() {
                "jsx_attribute" => {
                    for v in nomeados(a)
                        .into_iter()
                        .filter(|v| v.kind() == "jsx_expression")
                    {
                        self.visitar(v, esc);
                    }
                }
                _ => self.visitar(a, esc),
            }
        }
    }

    /// `<Comp>` and `<ns.Comp>` reference a value; `<div>` is an intrinsic tag.
    fn visitar_nome_jsx(&mut self, nome: Node<'t>, esc: usize) {
        match nome.kind() {
            "identifier" => {
                let t = texto(self.src, nome);
                if t.chars().next().is_some_and(char::is_uppercase) {
                    self.referencia(nome, esc, true);
                }
            }
            "member_expression" => {
                let mut raiz = nome;
                while let Some(o) = raiz.child_by_field_name("object") {
                    raiz = o;
                }
                if raiz.kind() == "identifier" {
                    self.referencia(raiz, esc, true);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vgp::lang::parse;

    fn livres(lang: &str, src: &str) -> Vec<String> {
        let arvore = parse(lang, src).expect("parse");
        assert!(
            !arvore.root_node().has_error(),
            "fixture com erro de sintaxe: {src}"
        );
        let a = analisar(arvore.root_node(), src);
        let mut v: Vec<String> = a.livres.iter().map(|r| r.nome.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    fn nenhum(lang: &str, src: &str) {
        assert_eq!(livres(lang, src), Vec::<String>::new(), "{src}");
    }

    #[test]
    fn nome_ausente_em_expressao_e_livre() {
        assert_eq!(livres("typescript", "vgpAusente;"), vec!["vgpAusente"]);
        assert_eq!(livres("javascript", "foo(bar);"), vec!["bar", "foo"]);
    }

    #[test]
    fn declaracoes_basicas_ligam() {
        nenhum(
            "typescript",
            "const a = 1; let b = a; var c = b; function f() {} class K {} enum E { A } f(K, E, c);",
        );
        nenhum(
            "typescript",
            "import d, { x as y } from 'm'; import * as ns from 'n'; d; y; ns;",
        );
        nenhum(
            "typescript",
            "export const q = 1; export function g() {} export default class Z {} g; Z; q;",
        );
    }

    #[test]
    fn hoisting_de_var_e_de_funcao() {
        nenhum(
            "javascript",
            "function f() { if (1) { var x = 1; } return x; } usar(); function usar() {}",
        );
        nenhum("javascript", "g(); function g() {}");
        nenhum("javascript", "{ function h() {} } h();");
        // o `var` de um `for…in` também é içado para a função (e para o módulo)
        nenhum("javascript", "const o = {}; for (var k in o) {} k;");
    }

    #[test]
    fn parametros_padroes_e_defaults() {
        nenhum(
            "typescript",
            "function f({ a, b: { c }, ...r }: P, [d, e = 1]: Q, g = a) { return a + c + r + d + e + g; }",
        );
        nenhum(
            "javascript",
            "const f = ({ a = 1, b: { c = a } }, [d, ...e]) => a + c + d + e;",
        );
        nenhum("typescript", "const f = x => x + 1; f(1);");
        assert_eq!(
            livres("typescript", "function f(a = naoExiste) {}"),
            vec!["naoExiste"]
        );
    }

    #[test]
    fn escopos_de_bloco_for_catch_switch() {
        nenhum(
            "typescript",
            "const ks = [1]; const o = {}; for (let i = 0; i < 3; i++) { i; } for (const k of ks) { k; } for (const x in o) { x }",
        );
        assert_eq!(livres("typescript", "{ let a = 1; } a;"), vec!["a"]);
        assert_eq!(livres("typescript", "for (let i = 0;;) {} i;"), vec!["i"]);
        nenhum(
            "typescript",
            "try { } catch (e) { e } try { } catch ({ message }) { message } try {} catch { }",
        );
        nenhum(
            "javascript",
            "const v = 1; switch (v) { case 1: let z = 1; z; break; default: z; }",
        );
    }

    #[test]
    fn classe_enum_namespace() {
        nenhum(
            "typescript",
            "class Base {} class K extends Base { x = 1; static s = K; m() { return K; } } ",
        );
        assert_eq!(
            livres("typescript", "class K extends Base {}"),
            vec!["Base"]
        );
        nenhum(
            "typescript",
            "enum E { A = 1, B = A + 1 } namespace N { export const v = 1; v; } N;",
        );
        nenhum(
            "typescript",
            "declare const dd: number; declare function df(): void; dd; df();",
        );
        nenhum("typescript", "declare global { var G: number } G;");
    }

    #[test]
    fn expressao_de_funcao_enxerga_o_proprio_nome() {
        nenhum(
            "javascript",
            "const f = function nome() { return nome; }; f;",
        );
        nenhum(
            "javascript",
            "const C = class Interna { m() { return Interna } }; C;",
        );
    }

    #[test]
    fn posicoes_que_nao_sao_referencia() {
        nenhum(
            "typescript",
            "const o = { a: 1, b() {}, get c() { return 1 } }; o.a; o.b; label: while (true) { break label; }",
        );
        nenhum(
            "typescript",
            "import { a as b } from 'm'; export { b as c }; export { z } from './z';",
        );
        nenhum(
            "typescript",
            "type T = Foo<Bar>; interface I extends J { p: typeof q } let v: Baz = null as unknown as Qux;",
        );
        nenhum("typescript", "typeof naoDeclarada === 'undefined';");
        assert_eq!(
            livres("typescript", "const o = { shorthand };"),
            vec!["shorthand"]
        );
        assert_eq!(
            livres("typescript", "const o = { [chave]: 1 };"),
            vec!["chave"]
        );
    }

    #[test]
    fn atribuicao_de_desestruturacao_referencia() {
        assert_eq!(
            livres("javascript", "({ s1, k: s2 } = obj);"),
            vec!["obj", "s1", "s2"]
        );
        nenhum(
            "javascript",
            "let s1, s2; ({ s1, k: s2 } = { s1: 1, k: 2 });",
        );
    }

    #[test]
    fn jsx_componente_e_intrinseco() {
        let l = livres(
            "tsx",
            "const A = () => <div className=\"x\"><Foo bar={baz} /></div>;",
        );
        assert_eq!(l, vec!["Foo", "baz"]);
        nenhum(
            "tsx",
            "import Foo from './foo'; const A = () => <Foo.Bar prop={1}>{[1].map(i => <Foo key={i} />)}</Foo.Bar>;",
        );
        assert_eq!(livres("tsx", "const A = () => <ns.Comp />;"), vec!["ns"]);
        nenhum("javascript", "const A = () => <div>texto</div>;");
    }

    #[test]
    fn with_e_registrado() {
        let src = "with (obj) { x; }";
        let arvore = parse("javascript", src).expect("parse");
        assert!(analisar(arvore.root_node(), src).tem_with);
    }

    #[test]
    fn atribuidos_guarda_globais_implicitos() {
        let src = "implicito = 1; function f() { implicito2 += 1; }";
        let arvore = parse("javascript", src).expect("parse");
        let a = analisar(arvore.root_node(), src);
        assert!(a.atribuidos.contains("implicito"));
    }

    #[test]
    fn typeof_e_propriedade_de_window_marcam_nomes_detectados() {
        let src =
            "if (typeof A !== 'undefined') { A; } if (window.B) { B; } globalThis.C; self.E; D;";
        let arvore = parse("javascript", src).expect("parse");
        let a = analisar(arvore.root_node(), src);
        for n in ["A", "B", "C", "E"] {
            assert!(a.detectados.contains(n), "{n} em {:?}", a.detectados);
        }
        assert!(!a.detectados.contains("D"));
        // detectar não é declarar: o nome segue livre, só deixa de ser prova de ausência
        assert!(a.livres.iter().any(|r| r.nome == "A"));
    }

    #[test]
    fn resolver_devolve_a_ligacao_com_o_tipo() {
        let src = "const a = 1; function f(p) { let q = 2; return a + p + q; }";
        let arvore = parse("javascript", src).expect("parse");
        let a = analisar(arvore.root_node(), src);
        // o `p` do `return`
        let p = arvore
            .root_node()
            .descendant_for_byte_range(
                src.rfind("p +").expect("p"),
                src.rfind("p +").expect("p") + 1,
            )
            .expect("nó");
        assert_eq!(a.resolver(p, "p").map(|l| l.tipo), Some(BindTipo::Param));
        assert_eq!(a.resolver(p, "q").map(|l| l.tipo), Some(BindTipo::Let));
        assert_eq!(a.resolver(p, "a").map(|l| l.tipo), Some(BindTipo::Const));
        assert!(a.resolver(p, "zz").is_none());
    }
}
