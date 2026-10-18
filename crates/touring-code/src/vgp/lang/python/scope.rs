//! Pyflakes-grade scope analysis for the `nome` class (NameError estático).
//!
//! Builds the scope tree (module / class / function / lambda / comprehension) with Python's own
//! resolution semantics: a method's chain SKIPS the class-body scope (class-body names are
//! invisible inside methods — V1 spec), comprehensions are isolated (their first iterable is
//! evaluated in the ENCLOSING scope), `global`/`nonlocal` reroute, and forward references inside
//! one scope are legal (a binding counts regardless of position — pyflakes behaviour, and the
//! reason legitimate forward refs in the 300 negatives stay allow).
//!
//! Fail-open: `from x import *`, or a call to exec/eval/globals/locals in the resolution chain,
//! poisons the lookup into `unknown` — dynamic namespaces are never denied.

use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

use super::stdlib_model::PYTHON_BUILTINS;

/// The kind of a scope in the tree (class bodies are skipped from function-like lookups).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeKind {
    #[default]
    /// Module top level.
    Module,
    /// Class body (skipped when a function-like inner scope looks names up).
    Class,
    /// `def` body.
    Function,
    /// `lambda` body.
    Lambda,
    /// Comprehension (isolated; its first iterable is evaluated in the ENCLOSING scope).
    Comprehension,
}

/// How a name got bound (import wins over assignment in pyflakes diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindKind {
    /// `import m` / `from m import x`.
    Import,
    /// `def f`.
    Def,
    /// `class C`.
    Class,
    /// Assignment / for-target / with-as / except-as / walrus.
    Assign,
    /// Function or lambda parameter.
    Param,
}

/// A bound name in a scope.
#[derive(Debug, Clone)]
pub struct Binding {
    /// How it got bound.
    pub kind: BindKind,
    /// 1-based line of the binding.
    pub line: usize,
}

/// A load-context identifier use.
#[derive(Debug, Clone)]
pub struct Use {
    /// The name used.
    pub name: String,
    /// 1-based line of the use.
    pub line: usize,
}

/// One scope in the tree: bindings, load-uses, and the poison flags.
#[derive(Debug, Default)]
pub struct Scope {
    /// Module / Class / Function / Lambda / Comprehension.
    pub kind: ScopeKind,
    /// Parent scope index (None at the module).
    pub parent: Option<usize>,
    /// Names bound here.
    pub bindings: HashMap<String, Binding>,
    /// Names used (load) here.
    pub uses: Vec<Use>,
    /// `global`-declared names (resolve in the module only).
    pub globals: HashSet<String>,
    /// `nonlocal`-declared names (resolve in enclosing non-module scopes only).
    pub nonlocals: HashSet<String>,
    /// `from x import *` — the namespace is dynamic.
    pub has_star_import: bool,
    /// exec/eval/globals/locals called — the namespace is dynamic.
    pub has_exec_escape: bool,
}

impl Scope {
    fn new(kind: ScopeKind, parent: Option<usize>) -> Self {
        Self {
            kind,
            parent,
            ..Default::default()
        }
    }
}

/// The scope tree of a module (index 0 = the module scope).
pub struct ScopeTree {
    /// All scopes, module first; parents reference by index.
    pub scopes: Vec<Scope>,
}

/// One unresolved-or-escaped name use, for the caller to classify.
/// One unresolved-or-escaped name use, for the caller to classify.
#[derive(Debug, Clone)]
pub struct NameIssue {
    /// The name that did not resolve.
    pub nome: String,
    /// 1-based line of the use.
    pub linha: usize,
    /// true when the lookup was poisoned (star import / exec escape) → caller emits unknown/warn.
    pub escaped: bool,
}

impl ScopeTree {
    /// Build the scope tree of a module body. `inline`: `__file__` is NOT defined (python3 -c);
    /// `__name__` always is (it is "__main__" there too).
    /// Build the scope tree of a module body. `inline`: `__file__` is NOT defined (python3 -c).
    pub fn build(root: Node, source: &str, inline: bool) -> Self {
        let mut st = ScopeTree {
            scopes: vec![Scope::new(ScopeKind::Module, None)],
        };
        // dunders sempre presentes em módulo (o `__doc__` ausente virou aviso falso no corpus)
        for dunder in [
            "__name__",
            "__doc__",
            "__package__",
            "__loader__",
            "__spec__",
            "__builtins__",
            "__annotations__",
        ] {
            st.scopes[0].bindings.insert(
                dunder.into(),
                Binding {
                    kind: BindKind::Assign,
                    line: 0,
                },
            );
        }
        if !inline {
            st.scopes[0].bindings.insert(
                "__file__".into(),
                Binding {
                    kind: BindKind::Assign,
                    line: 0,
                },
            );
        }
        st.walk_block(root, source, 0);
        st
    }

    fn add_binding(&mut self, scope: usize, name: &str, kind: BindKind, line: usize) {
        if !name.is_empty() {
            self.scopes[scope]
                .bindings
                .insert(name.to_string(), Binding { kind, line });
        }
    }

    fn add_use(&mut self, scope: usize, name: &str, line: usize) {
        if !name.is_empty() {
            self.scopes[scope].uses.push(Use {
                name: name.to_string(),
                line,
            });
        }
    }

    /// Every name use that does NOT resolve, with the poison flag. Resolution order: the scope
    /// chain (skipping class bodies from function-like scopes) → module → builtins.
    /// Every name use that does NOT resolve, with the poison flag.
    pub fn issues(&self) -> Vec<NameIssue> {
        let mut out = Vec::new();
        for (i, scope) in self.scopes.iter().enumerate() {
            for u in &scope.uses {
                match self.resolve(i, &u.name) {
                    Resolve::Found => {}
                    Resolve::Missing => out.push(NameIssue {
                        nome: u.name.clone(),
                        linha: u.line,
                        escaped: false,
                    }),
                    Resolve::Escaped => out.push(NameIssue {
                        nome: u.name.clone(),
                        linha: u.line,
                        escaped: true,
                    }),
                }
            }
        }
        out
    }

    fn resolve(&self, start: usize, name: &str) -> Resolve {
        let scope = &self.scopes[start];
        if scope.globals.contains(name) {
            return self.resolve_in_module(name);
        }
        if scope.nonlocals.contains(name) {
            return self.resolve_in_enclosing(start, name, false);
        }
        let mut cur = Some(start);
        let mut poisoned = false;
        while let Some(i) = cur {
            let s = &self.scopes[i];
            // a function-like scope never sees the class body it is nested in (Python semantics)
            if s.kind == ScopeKind::Class && i != start && starts_function_like(self, start) {
                cur = s.parent;
                continue;
            }
            if s.bindings.contains_key(name) {
                return Resolve::Found;
            }
            poisoned |= s.has_star_import || s.has_exec_escape;
            cur = s.parent;
        }
        if PYTHON_BUILTINS.contains(&name) {
            return Resolve::Found;
        }
        if poisoned {
            return Resolve::Escaped;
        }
        Resolve::Missing
    }

    fn resolve_in_module(&self, name: &str) -> Resolve {
        let m = &self.scopes[0];
        if m.bindings.contains_key(name) || PYTHON_BUILTINS.contains(&name) {
            return Resolve::Found;
        }
        if m.has_star_import || m.has_exec_escape {
            return Resolve::Escaped;
        }
        Resolve::Missing
    }

    fn resolve_in_enclosing(&self, start: usize, name: &str, include_module: bool) -> Resolve {
        let mut cur = self.scopes[start].parent;
        while let Some(i) = cur {
            let s = &self.scopes[i];
            if s.kind == ScopeKind::Module {
                return if include_module
                    || s.bindings.contains_key(name)
                    || PYTHON_BUILTINS.contains(&name)
                {
                    Resolve::Found
                } else {
                    Resolve::Missing
                };
            }
            if s.kind != ScopeKind::Class && s.bindings.contains_key(name) {
                return Resolve::Found;
            }
            cur = s.parent;
        }
        Resolve::Missing
    }

    // ------------------------------------------------------------------ the walk

    fn walk_block(&mut self, node: Node, source: &str, scope: usize) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_stmt(child, source, scope);
        }
    }

    fn walk_stmt(&mut self, node: Node, source: &str, scope: usize) {
        match node.kind() {
            "function_definition" | "decorated_definition" => self.walk_def(node, source, scope),
            "class_definition" => self.walk_class(node, source, scope),
            "import_statement" => self.walk_import(node, source, scope),
            "import_from_statement" => self.walk_import_from(node, source, scope),
            "assignment" => self.walk_assignment(node, source, scope),
            "augmented_assignment" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.bind_or_use_target(left, source, scope, true);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.walk_expr(right, source, scope);
                }
            }
            "for_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.bind_target(left, source, scope, BindKind::Assign);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.walk_expr(right, source, scope);
                }
                self.walk_named(node, source, scope, &["body", "alternative"]);
            }
            "with_statement" => self.walk_named(node, source, scope, &["body"]),
            "with_clause" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.kind() == "with_item" {
                        self.walk_with_item(ch, source, scope);
                    }
                }
            }
            // `from __future__ import annotations` é kind PRÓPRIO — sem braço, `annotations`
            // caía em walk_expr e virava uso sem vínculo (R3 da coordenadora: 9 falsos deny)
            "future_import_statement" => {}
            "global_statement" | "nonlocal_statement" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.kind() == "identifier" {
                        let name = node_text(source, ch);
                        if node.kind() == "global_statement" {
                            self.scopes[scope].globals.insert(name.to_string());
                        } else {
                            self.scopes[scope].nonlocals.insert(name.to_string());
                        }
                    }
                }
            }
            "delete_statement" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.kind() == "identifier" {
                        self.add_use(scope, node_text(source, ch), ch.start_position().row + 1);
                    } else {
                        self.walk_expr(ch, source, scope);
                    }
                }
            }
            "expression_statement" => {
                // a gramática 0.25 embrulha assignments em expression_statement — sem este
                // despacho o ALVO virava uso (e o "nome ausente" era o próprio alvo)
                match node.child(0) {
                    Some(filho) if filho.kind() == "assignment" => {
                        self.walk_assignment(filho, source, scope)
                    }
                    Some(filho) if filho.kind() == "augmented_assignment" => {
                        self.walk_stmt(filho, source, scope)
                    }
                    _ => self.walk_children_exprs(node, source, scope),
                }
            }
            "return_statement" | "assert_statement" | "raise_statement" | "print_statement"
            | "exec_statement" => self.walk_children_exprs(node, source, scope),
            "if_statement" | "while_statement" | "try_statement" | "match_statement"
            | "except_clause" | "finally_clause" | "else_clause" | "elif_clause" => {
                self.walk_except_binding(node, source, scope);
                self.walk_children_exprs_and_blocks(node, source, scope);
            }
            "case_clause" => {
                // o case_pattern é filho POSICIONAL do case_clause (sem field na gramática):
                // child_by_field_name("pattern") devolvia None e a captura nunca era ligada
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    match ch.kind() {
                        "case_pattern" => self.walk_case_pattern(ch, source, scope),
                        "block" => self.walk_block(ch, source, scope),
                        _ if ch.is_named() => self.walk_expr(ch, source, scope), // guard
                        _ => {}
                    }
                }
            }
            _ => self.walk_children_exprs_and_blocks(node, source, scope),
        }
    }

    fn walk_named(&mut self, node: Node, source: &str, scope: usize, _fields: &[&str]) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "with_clause" => self.walk_stmt(ch, source, scope),
                "block" => self.walk_block(ch, source, scope),
                _ => {}
            }
        }
    }

    fn walk_with_item(&mut self, node: Node, source: &str, scope: usize) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "as_pattern" => {
                    if let Some(target) = ch.child(0)
                        && target.kind() != "as_pattern_target"
                    {
                        self.walk_expr(target, source, scope);
                    }
                    let mut cc = ch.walk();
                    for sub in ch.children(&mut cc) {
                        if sub.kind() == "as_pattern_target" {
                            self.bind_target(sub, source, scope, BindKind::Assign);
                        }
                    }
                }
                _ => self.walk_expr(ch, source, scope),
            }
        }
    }

    fn walk_except_binding(&mut self, node: Node, source: &str, scope: usize) {
        if node.kind() != "except_clause" {
            return;
        }
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "as_pattern" {
                let mut cc = ch.walk();
                for sub in ch.children(&mut cc) {
                    match sub.kind() {
                        "as_pattern_target" => {
                            self.bind_target(sub, source, scope, BindKind::Assign)
                        }
                        "identifier" => self.add_use(
                            scope,
                            node_text(source, sub),
                            sub.start_position().row + 1,
                        ),
                        _ => self.walk_expr(sub, source, scope),
                    }
                }
            }
        }
    }

    fn walk_children_exprs(&mut self, node: Node, source: &str, scope: usize) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            self.walk_expr(ch, source, scope);
        }
    }

    fn walk_children_exprs_and_blocks(&mut self, node: Node, source: &str, scope: usize) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "block" => self.walk_block(ch, source, scope),
                "comment" => {}
                "string" => self.walk_interpolacoes(ch, source, scope),
                _ if ch.is_named() => self.walk_stmt_or_expr(ch, source, scope),
                _ => {}
            }
        }
    }

    fn walk_stmt_or_expr(&mut self, node: Node, source: &str, scope: usize) {
        match node.kind() {
            "function_definition"
            | "class_definition"
            | "decorated_definition"
            | "import_statement"
            | "import_from_statement"
            | "assignment"
            | "augmented_assignment"
            | "for_statement"
            | "global_statement"
            | "nonlocal_statement"
            | "with_statement"
            | "delete_statement" => self.walk_stmt(node, source, scope),
            "if_statement" | "while_statement" | "try_statement" | "match_statement"
            | "elif_clause" | "else_clause" => {
                self.walk_children_exprs_and_blocks(node, source, scope)
            }
            "except_clause" | "finally_clause" => {
                self.walk_except_binding(node, source, scope);
                self.walk_children_exprs_and_blocks(node, source, scope);
            }
            "case_clause" => {
                // o case_pattern é filho POSICIONAL do case_clause (sem field na gramática):
                // child_by_field_name("pattern") devolvia None e a captura nunca era ligada
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    match ch.kind() {
                        "case_pattern" => self.walk_case_pattern(ch, source, scope),
                        "block" => self.walk_block(ch, source, scope),
                        _ if ch.is_named() => self.walk_expr(ch, source, scope), // guard
                        _ => {}
                    }
                }
            }
            "with_clause" => self.walk_stmt(node, source, scope),
            "expression_statement" => self.walk_stmt(node, source, scope),
            _ => self.walk_expr(node, source, scope),
        }
    }

    fn walk_def(&mut self, node: Node, source: &str, scope: usize) {
        let (def, decorators) = if node.kind() == "decorated_definition" {
            let mut d = None;
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                match ch.kind() {
                    "decorator" => self.walk_expr(ch, source, scope),
                    "function_definition" | "class_definition" => d = Some(ch),
                    _ => {}
                }
            }
            match d {
                Some(x) => (x, true),
                None => return,
            }
        } else {
            (node, false)
        };
        let _ = decorators;
        if def.kind() == "class_definition" {
            return self.walk_class(def, source, scope);
        }
        let name = def
            .child_by_field_name("name")
            .map(|n| node_text(source, n).to_string())
            .unwrap_or_default();
        self.add_binding(scope, &name, BindKind::Def, def.start_position().row + 1);
        // decorators/defaults/return annotation are evaluated in the ENCLOSING scope
        if let Some(ret) = def.child_by_field_name("return_type") {
            self.walk_expr(ret, source, scope);
        }
        let child = self.push(ScopeKind::Function, scope);
        if let Some(params) = def.child_by_field_name("parameters") {
            self.bind_parameters(params, source, child);
        }
        if let Some(body) = def.child_by_field_name("body") {
            self.walk_block(body, source, child);
        }
    }

    fn walk_class(&mut self, node: Node, source: &str, scope: usize) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(source, n).to_string())
            .unwrap_or_default();
        self.add_binding(scope, &name, BindKind::Class, node.start_position().row + 1);
        // bases and keywords are evaluated in the ENCLOSING scope
        if let Some(sup) = node.child_by_field_name("superclasses") {
            self.walk_expr(sup, source, scope);
        }
        let child = self.push(ScopeKind::Class, scope);
        if let Some(body) = node.child_by_field_name("body") {
            self.walk_block(body, source, child);
        }
    }

    fn push(&mut self, kind: ScopeKind, parent: usize) -> usize {
        self.scopes.push(Scope::new(kind, Some(parent)));
        self.scopes.len() - 1
    }

    fn walk_import(&mut self, node: Node, source: &str, scope: usize) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "dotted_name" => {
                    let first = node_text(source, ch)
                        .split('.')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    self.add_binding(scope, &first, BindKind::Import, ch.start_position().row + 1);
                }
                "aliased_import" => {
                    if let Some(alias) = last_identifier(ch) {
                        self.add_binding(
                            scope,
                            node_text(source, alias),
                            BindKind::Import,
                            alias.start_position().row + 1,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_import_from(&mut self, node: Node, source: &str, scope: usize) {
        // gramática 0.25: o PRIMEIRO dotted_name é o módulo; os seguintes são os nomes importados
        let mut modulo_visto = false;
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "wildcard_import" => self.scopes[scope].has_star_import = true,
                "aliased_import" => {
                    if let Some(alias) = last_identifier(ch) {
                        self.add_binding(
                            scope,
                            node_text(source, alias),
                            BindKind::Import,
                            alias.start_position().row + 1,
                        );
                    }
                }
                "dotted_name" => {
                    if modulo_visto {
                        let last = node_text(source, ch)
                            .split('.')
                            .next_back()
                            .unwrap_or("")
                            .to_string();
                        self.add_binding(
                            scope,
                            &last,
                            BindKind::Import,
                            ch.start_position().row + 1,
                        );
                    } else {
                        modulo_visto = true;
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_assignment(&mut self, node: Node, source: &str, scope: usize) {
        if let Some(ty) = node.child_by_field_name("type") {
            self.walk_expr(ty, source, scope);
        }
        if let Some(right) = node.child_by_field_name("right") {
            if right.kind() == "assignment" {
                // atribuição encadeada `T=E=R=A=0`: a gramática aninha assignments à direita
                self.walk_assignment(right, source, scope);
            } else {
                self.walk_expr(right, source, scope);
            }
        }
        if let Some(left) = node.child_by_field_name("left") {
            self.bind_or_use_target(left, source, scope, false);
        }
    }

    /// An assignment target: plain identifiers BIND; attribute/subscript objects are USES
    /// (`obj.attr = v` uses `obj`); patterns recurse. `aug` = augmented (x += 1 loads AND binds).
    fn bind_or_use_target(&mut self, node: Node, source: &str, scope: usize, aug: bool) {
        match node.kind() {
            "identifier" => {
                if aug {
                    let name = node_text(source, node);
                    self.add_use(scope, name, node.start_position().row + 1);
                    // `x += 1` LÊ antes de escrever: sem vínculo prévio na cadeia, a leitura falha
                    // em runtime (F821) — a própria escrita não pode satisfazê-la
                    if !matches!(self.resolve(scope, name), Resolve::Found) {
                        return;
                    }
                }
                self.add_binding(
                    scope,
                    node_text(source, node),
                    BindKind::Assign,
                    node.start_position().row + 1,
                );
            }
            _ => self.bind_target(node, source, scope, BindKind::Assign),
        }
    }

    fn bind_target(&mut self, node: Node, source: &str, scope: usize, kind: BindKind) {
        match node.kind() {
            "identifier" => self.add_binding(
                scope,
                node_text(source, node),
                kind,
                node.start_position().row + 1,
            ),
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "list"
            | "tuple"
            | "parenthesized_expression"
            | "as_pattern_target" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    self.bind_target(ch, source, scope, kind);
                }
            }
            "attribute" => {
                if let Some(obj) = node.child_by_field_name("object") {
                    self.walk_expr(obj, source, scope);
                }
            }
            "subscript" => {
                if let Some(v) = node.child_by_field_name("value") {
                    self.walk_expr(v, source, scope);
                }
                if let Some(s) = node.child_by_field_name("subscript") {
                    self.walk_expr(s, source, scope);
                }
            }
            _ => {}
        }
    }

    fn bind_parameters(&mut self, node: Node, source: &str, scope: usize) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "identifier" => self.add_binding(
                    scope,
                    node_text(source, ch),
                    BindKind::Param,
                    ch.start_position().row + 1,
                ),
                "typed_parameter" | "typed_default_parameter" => {
                    if let Some(ty) = ch.child_by_field_name("type") {
                        self.walk_expr(ty, source, self.scopes[scope].parent.unwrap_or(scope));
                    }
                    // a parte do parâmetro é o PRIMEIRO filho nomeado; os splats TIPADOS vivem
                    // aqui (`*extra: str` · `**kw: Unpack[X]`) — sem a descida ficavam sem vínculo
                    let mut cc = ch.walk();
                    if let Some(alvo) = ch.children(&mut cc).find(|n| n.is_named()) {
                        match alvo.kind() {
                            "list_splat_pattern" | "dictionary_splat_pattern" => {
                                let mut ci = alvo.walk();
                                for inner in alvo.children(&mut ci) {
                                    if inner.kind() == "identifier" {
                                        self.add_binding(
                                            scope,
                                            node_text(source, inner),
                                            BindKind::Param,
                                            inner.start_position().row + 1,
                                        );
                                    }
                                }
                            }
                            "identifier" => self.add_binding(
                                scope,
                                node_text(source, alvo),
                                BindKind::Param,
                                alvo.start_position().row + 1,
                            ),
                            _ => self.walk_expr(alvo, source, scope),
                        }
                    }
                    if let Some(value) = ch.child_by_field_name("value") {
                        self.walk_expr(value, source, scope);
                    }
                }
                "default_parameter" => {
                    if let Some(name) = ch.child_by_field_name("name") {
                        self.add_binding(
                            scope,
                            node_text(source, name),
                            BindKind::Param,
                            name.start_position().row + 1,
                        );
                    }
                    if let Some(value) = ch.child_by_field_name("value") {
                        self.walk_expr(value, source, scope);
                    }
                }
                "list_splat_pattern" | "dictionary_splat_pattern" => {
                    let mut cc = ch.walk();
                    for sub in ch.children(&mut cc) {
                        if sub.kind() == "identifier" {
                            self.add_binding(
                                scope,
                                node_text(source, sub),
                                BindKind::Param,
                                sub.start_position().row + 1,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Desce nas interpolações de uma f-string (e nas aninhadas em format_spec).
    fn walk_interpolacoes(&mut self, node: Node, source: &str, scope: usize) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "interpolation" {
                if let Some(expr) = ch.child_by_field_name("expression") {
                    self.walk_expr(expr, source, scope);
                }
                // `f"{x:{width}}"`: o format_spec pode conter interpolações aninhadas
                self.walk_interpolacoes(ch, source, scope);
            }
        }
    }

    /// Padrão de um `case`: identificador NU é captura (LIGA); `Cores.RED` (com ponto) é
    /// padrão-valor (USO); literais não são nomes; padrões compostos recursam.
    fn walk_case_pattern(&mut self, node: Node, source: &str, scope: usize) {
        match node.kind() {
            "dotted_name" => {
                let mut c = node.walk();
                let ids: Vec<Node> = node
                    .children(&mut c)
                    .filter(|ch| ch.kind() == "identifier")
                    .collect();
                if ids.len() == 1 {
                    self.add_binding(
                        scope,
                        node_text(source, ids[0]),
                        BindKind::Assign,
                        ids[0].start_position().row + 1,
                    );
                } else {
                    for id in ids {
                        self.add_use(scope, node_text(source, id), id.start_position().row + 1);
                    }
                }
            }
            "as_pattern" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.kind() == "as_pattern_target" {
                        self.bind_target(ch, source, scope, BindKind::Assign);
                    } else if ch.is_named() {
                        self.walk_case_pattern(ch, source, scope);
                    }
                }
            }
            "string"
            | "concatenated_string"
            | "integer"
            | "float"
            | "true"
            | "false"
            | "none"
            | "comment" => {}
            "attribute" => {
                // padrão-valor `mod.CONST`: o objeto é uso
                if let Some(obj) = node.child_by_field_name("object") {
                    self.walk_expr(obj, source, scope);
                }
            }
            _ => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() {
                        self.walk_case_pattern(ch, source, scope);
                    }
                }
            }
        }
    }

    fn walk_expr(&mut self, node: Node, source: &str, scope: usize) {
        match node.kind() {
            "identifier" => self.add_use(
                scope,
                node_text(source, node),
                node.start_position().row + 1,
            ),
            "comment" | "integer" | "float" | "true" | "false" | "none" => {}
            "string" | "concatenated_string" => {
                // f-strings escondem USOS: a interpolação é um nó `interpolation` com campo
                // `expression` (recall medido no corpus: uf/nm_municipio/scripts/tau_val_coverage)
                self.walk_interpolacoes(node, source, scope);
            }
            "named_expression" => {
                // walrus `m := 3`: avalia o valor, liga o nome
                if let Some(value) = node.child_by_field_name("value") {
                    self.walk_expr(value, source, scope);
                }
                if let Some(name) = node.child_by_field_name("name") {
                    self.bind_or_use_target(name, source, scope, false);
                }
            }
            "lambda" => {
                let child = self.push(ScopeKind::Lambda, scope);
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.bind_parameters(params, source, child);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.walk_expr(body, source, child);
                }
            }
            "list_comprehension"
            | "set_comprehension"
            | "generator_expression"
            | "dictionary_comprehension" => {
                self.walk_comprehension(node, source, scope);
            }
            "attribute" => {
                // `obj.attr`: só `obj` é uso de escopo — `attr` é NOME DE ATRIBUTO, não de escopo
                // (era a fonte dos falsos "PASTA"/"run" ausentes)
                if let Some(obj) = node.child_by_field_name("object") {
                    self.walk_expr(obj, source, scope);
                }
            }
            "keyword_argument" => {
                // `f(nome=valor)`: `nome` é NOME DE PARÂMETRO, não de escopo — só o valor conta
                if let Some(value) = node.child_by_field_name("value") {
                    self.walk_expr(value, source, scope);
                }
            }
            "call" => self.walk_call(node, source, scope),
            _ => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() {
                        self.walk_expr(ch, source, scope);
                    }
                }
            }
        }
    }

    fn walk_call(&mut self, node: Node, source: &str, scope: usize) {
        if let Some(f) = node.child_by_field_name("function") {
            if f.kind() == "identifier" {
                let name = node_text(source, f);
                if matches!(
                    name,
                    "exec"
                        | "eval"
                        | "globals"
                        | "locals"
                        | "vars"
                        | "dir"
                        | "compile"
                        | "__import__"
                ) {
                    self.scopes[scope].has_exec_escape = true;
                }
            }
            self.walk_expr(f, source, scope);
        }
        if let Some(args) = node.child_by_field_name("arguments") {
            self.walk_expr(args, source, scope);
        }
    }

    fn content_or<'a>(&self, s: &'a str) -> &'a str {
        s
    }

    fn walk_comprehension(&mut self, node: Node, source: &str, scope: usize) {
        let child = self.push(ScopeKind::Comprehension, scope);
        let mut c = node.walk();
        let mut first_iterable_done = false;
        for ch in node.children(&mut c) {
            match ch.kind() {
                "for_in_clause" => {
                    let (left, right) = for_in_parts(self.content_or(source), ch);
                    if let Some(right) = right {
                        // the FIRST iterable is evaluated in the ENCLOSING scope (Python semantics)
                        if first_iterable_done {
                            self.walk_expr(right, source, child);
                        } else {
                            self.walk_expr(right, source, scope);
                            first_iterable_done = true;
                        }
                    }
                    if let Some(left) = left {
                        self.bind_target(left, source, child, BindKind::Assign);
                    }
                    let mut cc = ch.walk();
                    for sub in ch.children(&mut cc) {
                        if sub.kind() == "if_clause" {
                            self.walk_expr(sub, source, child);
                        }
                    }
                }
                "if_clause" => self.walk_expr(ch, source, child),
                _ => {
                    if ch.is_named() {
                        self.walk_expr(ch, source, child);
                    }
                }
            }
        }
    }
}

/// The last bare `identifier` child of a node — the alias in `import x as y` / `from m import x as y`
/// (tree-sitter-python 0.25 does NOT name an `alias` field; the alias is the trailing identifier).
fn last_identifier(node: Node) -> Option<Node> {
    let mut c = node.walk();
    let mut out = None;
    for ch in node.children(&mut c) {
        if ch.kind() == "identifier" {
            out = Some(ch);
        }
    }
    out
}

/// (left, right) of a `for x in y` clause: fields when present, positional fallback otherwise.
fn for_in_parts<'a>(source: &'a str, node: Node<'a>) -> (Option<Node<'a>>, Option<Node<'a>>) {
    if let (Some(l), Some(r)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
    ) {
        return (Some(l), Some(r));
    }
    let mut c = node.walk();
    let mut idents = node.children(&mut c).filter(|ch| ch.is_named());
    let left = idents.next();
    let right = idents.find(|ch| {
        matches!(
            ch.kind(),
            "identifier" | "call" | "attribute" | "list" | "tuple"
        )
    });
    let _ = source;
    (left, right)
}

fn starts_function_like(st: &ScopeTree, start: usize) -> bool {
    matches!(
        st.scopes[start].kind,
        ScopeKind::Function | ScopeKind::Lambda | ScopeKind::Comprehension
    ) || st.scopes[start]
        .parent
        .map(|p| starts_function_like(st, p))
        .unwrap_or(false)
}

#[derive(Debug, PartialEq, Eq)]
enum Resolve {
    Found,
    Missing,
    Escaped,
}

/// `fn node_text` local para não depender do módulo ast (o motor é puro e self-contained).
fn node_text<'a>(source: &'a str, node: Node) -> &'a str {
    &source[node.byte_range()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::languages::Lang;
    use crate::ast::parser::parse_thread_local;

    fn issues_of(src: &str, inline: bool) -> Vec<NameIssue> {
        let tree = parse_thread_local(src, Lang::Python).expect("parse");
        ScopeTree::build(tree.root_node(), src, inline).issues()
    }

    fn denies(src: &str, inline: bool) -> Vec<String> {
        let mut v: Vec<String> = issues_of(src, inline)
            .into_iter()
            .filter(|i| !i.escaped)
            .map(|i| i.nome)
            .collect();
        v.sort();
        v
    }

    #[test]
    fn nome_usado_sem_definicao_nega() {
        assert_eq!(denies("print(cp.PASTA)\n", true), vec!["cp"]);
    }

    #[test]
    fn import_simples_vincula() {
        assert_eq!(
            denies("import subprocess\nsubprocess.run(['x'])\n", true),
            Vec::<String>::new()
        );
    }

    #[test]
    fn nome_de_funcao_ausente_em_tupla_nega() {
        let src =
            "def test_ok():\n    pass\n\nt = (test_ok, test_a_fallback_counts_as_unmeasured)\n";
        assert_eq!(
            denies(src, false),
            vec!["test_a_fallback_counts_as_unmeasured"]
        );
    }

    #[test]
    fn forward_reference_no_mesmo_escopo_e_legitima() {
        let src = "def f():\n    return g()\ndef g():\n    return 1\n";
        assert_eq!(denies(src, false), Vec::<String>::new());
    }

    #[test]
    fn classe_invisivel_dentro_do_metodo_mas_nome_da_classe_visivel() {
        // o nome da CLASSE resolve pelo módulo; atributo do corpo da classe, não (semântica Python)
        let ok = "class A:\n    def m(self):\n        return A()\n";
        assert_eq!(denies(ok, false), Vec::<String>::new());
        let bad = "class A:\n    x = 1\n    def m(self):\n        return x\n";
        assert_eq!(denies(bad, false), vec!["x"]);
    }

    #[test]
    fn dunder_file_inline_nega_e_em_arquivo_nao() {
        assert_eq!(denies("print(__file__)\n", true), vec!["__file__"]);
        assert_eq!(denies("print(__file__)\n", false), Vec::<String>::new());
        assert_eq!(denies("print(__name__)\n", true), Vec::<String>::new());
    }

    #[test]
    fn comprehension_isolada_e_primeiro_iteravel_no_escopo_externo() {
        assert_eq!(
            denies("xs = [1]\nys = [x for x in xs]\n", false),
            Vec::<String>::new()
        );
        assert_eq!(denies("ys = [y for x in xs]\n", false), vec!["xs", "y"]);
    }

    #[test]
    fn star_import_e_exec_escape_viram_desconhecido_nunca_deny() {
        let star = issues_of("from m import *\nprint(nome_qualquer)\n", false);
        assert_eq!(star.len(), 1);
        assert!(star[0].escaped);
        let exec = issues_of("exec('x = 1')\nprint(x)\n", false);
        assert!(exec.iter().all(|i| i.escaped));
    }

    #[test]
    fn aug_assign_carrega_e_vincula() {
        assert_eq!(denies("x = 1\nx += 1\n", false), Vec::<String>::new());
        assert_eq!(denies("x += 1\n", false), vec!["x"]);
    }

    #[test]
    fn anotacoes_usam_nomes() {
        assert_eq!(
            denies("def f(x: ClsInexistente):\n    return x\n", false),
            vec!["ClsInexistente"]
        );
        assert_eq!(
            denies("def f(x: int):\n    return x\n", false),
            Vec::<String>::new()
        );
    }

    #[test]
    fn fechamento_enxerga_escopo_externo_de_funcao() {
        let src = "def f():\n    y = 2\n    def g():\n        return y\n    return g\n";
        assert_eq!(denies(src, false), Vec::<String>::new());
    }

    #[test]
    fn global_redireciona_para_o_modulo() {
        let src = "g = 1\ndef f():\n    global g\n    g = 2\nf()\n";
        assert_eq!(denies(src, false), Vec::<String>::new());
    }
}

#[cfg(test)]
mod r2_constructs_tests {
    use crate::ast::languages::Lang;
    use crate::ast::parser::parse_thread_local;
    use crate::vgp::lang::python::scope::ScopeTree;

    fn issues(src: &str) -> Vec<String> {
        let tree = parse_thread_local(src, Lang::Python).expect("parse");
        let mut v: Vec<String> = ScopeTree::build(tree.root_node(), src, true)
            .issues()
            .iter()
            .map(|i| i.nome.clone())
            .collect();
        v.sort();
        v
    }

    // R2 da coordenadora — um construto ligador de nome por teste (medidos nos snaps do corpus)

    #[test]
    fn splat_tipado_liga_o_nome() {
        // falso deny medido (712b75c8c772): `def f(*extra: str)` — typed_parameter embrulha o
        // list_splat_pattern; sem descida, `extra` ficava sem vínculo
        let src = "def f(*extra: str) -> int:\n    print(*extra)\n    return len(extra)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn with_as_liga_o_alvo() {
        let src = "with open('x') as zf:\n    print(zf)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn atribuicao_encadeada_liga_todos_os_alvos() {
        // T=E=R=A=0: a gramática aninha assignments à DIREITA; sem recursão, E/R/A ficam sem vínculo
        let src = "T=E=R=A=0\nprint(T, E, R, A)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn splat_args_e_kwargs_ligam() {
        let src = "def f(*args, **kw):\n    print(args, kw)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn walrus_liga_o_nome() {
        let src = "if (m := 3) > 2:\n    print(m)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn future_import_nao_vira_uso() {
        // R3: `from __future__ import annotations` é future_import_statement — kind próprio;
        // sem braço, caía em walk_expr e `annotations` virava uso sem vínculo
        let src = "from __future__ import annotations\ndef f(x: int) -> str:\n    return str(x)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn match_captura_liga_e_valor_pontuado_e_uso() {
        // case ('load', path): `path` (identificador nu) LIGA; `Colors.RED` (com ponto) seria USO
        let src = "comando = ('load', 'p')\nmatch comando:\n    case ('load', path):\n        print(path)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn alvo_de_comprehension_liga_no_escopo_isolado() {
        let src = "xs = [1, 2]\nys = [x for x in xs if x > 0]\nprint(ys)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn uso_dentro_de_fstring_conta() {
        // recall medido no corpus (uf/nm_municipio/scripts/tau_val_coverage): a interpolação de
        // f-string é um nó `interpolation` com campo `expression` — sem descida, o uso era invisível
        let src = "u = {'uf': 1}\nprint(f'{u[uf]}')\n";
        assert_eq!(issues(src), vec!["uf".to_string()]);
    }

    #[test]
    fn nome_definido_em_fstring_nao_nega() {
        let src = "nome = 1\nprint(f'{nome} e {nome}x')\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }
}

#[cfg(test)]
mod except_binding_tests {
    use crate::ast::languages::Lang;
    use crate::ast::parser::parse_thread_local;
    use crate::vgp::lang::python::scope::ScopeTree;

    fn issues(src: &str) -> Vec<String> {
        let tree = parse_thread_local(src, Lang::Python).expect("parse");
        ScopeTree::build(tree.root_node(), src, true)
            .issues()
            .iter()
            .map(|i| i.nome.clone())
            .collect()
    }

    #[test]
    fn except_com_as_liga_o_nome_no_bloco() {
        // falso deny medido no corpus V0d (a6666ab2bf3a): `except Exception as e` não ligava `e`
        // — bind_target recebia o wrapper as_pattern_target e caía no catch-all sem ligar nada.
        let src =
            "try:\n    pass\nexcept Exception as e:\n    print(e)\n    print(type(e).__name__)\n";
        assert_eq!(issues(src), Vec::<String>::new());
    }

    #[test]
    fn nome_realmente_indefinido_no_except_segue_negado() {
        let src = "try:\n    pass\nexcept Exception as e:\n    print(indefinido)\n";
        assert_eq!(issues(src), vec!["indefinido".to_string()]);
    }
}

#[cfg(test)]
mod sexp_probe {
    use crate::ast::languages::Lang;
    use crate::ast::parser::parse_thread_local;

    #[test]
    fn dump_except_sexp() {
        let src = "T=E=R=A=0\nr = subprocess_run = None\nwith open('x') as zf, open('y') as h:\n    pass\ndef f(*args, **kw):\n    pass\nif (m := 3) > 2:\n    pass\nfrom __future__ import annotations\nmatch comando():\n    case ('load', path):\n        print(path)\n";
        let tree = parse_thread_local(src, Lang::Python).unwrap();
        println!("SEXP: {}", tree.root_node().to_sexp());
    }
}
