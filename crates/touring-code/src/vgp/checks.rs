//! The five V1 checks over the proposed Python AST, plus the delta assembly.
//!
//! Deny discipline (the ≤1 % false-deny ruler): a finding is denied only when the WHOLE chain
//! closed — the receiver's type is proven (constructor / annotation / project return annotation /
//! closed builtin table), the member set is complete (no `__getattr__`, no unknown base, no
//! rebinding to an unknown source) and the name is provably absent. Every gap answers `unknown`
//! or `warn`, never `deny`. The refusal TEACHES: `reais` carries the real names, `sugestao` the
//! closest one.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use tree_sitter::Node;

use crate::ast::languages::Lang;
use crate::ast::parser::parse_thread_local;

use super::modules::{ModuleInfo, Resolver, Tri, extract_module_info, resolve_relative};
use super::scope::ScopeTree;
use super::stdlib_model::{builtin_type_members, call_result_type, stdlib_module_members};
use super::{CheckInput, Finding};

/// Parse failure of the proposed content is the one `unknown` verdict — fail-open, never deny.
pub fn check_python(input: &CheckInput) -> (String, Vec<Finding>) {
    let inline = super::is_inline(&input.path);
    if parse_thread_local(&input.content, Lang::Python).is_err() {
        return ("unknown".into(), Vec::new());
    }
    let mut findings = run_checks(
        &input.path,
        &input.content,
        &input.project_root,
        &input.overlays,
        inline,
    );

    if let Some(disk) = &input.disk {
        let disco = if parse_thread_local(disk, Lang::Python).is_ok() {
            run_checks(
                &input.path,
                disk,
                &input.project_root,
                &input.overlays,
                inline,
            )
        } else {
            Vec::new()
        };
        let ja: HashSet<(String, String)> = disco.into_iter().map(|f| (f.classe, f.nome)).collect();
        // delta: o hook só nega o que a edição INTRODUZ; o pré-existente informa (warn), nunca nega
        for f in findings.iter_mut() {
            if f.severidade == "deny" && ja.contains(&(f.classe.clone(), f.nome.clone())) {
                f.severidade = "warn".into();
                f.sugestao = format!("pré-existente (já estava no disco): {}", f.sugestao);
            }
        }
    }

    let verdict = if findings.iter().any(|f| f.severidade == "deny") {
        "deny"
    } else if findings.iter().any(|f| f.severidade == "warn") {
        "warn"
    } else {
        "allow"
    };
    (verdict.into(), findings)
}

/// Everything computed on one module's content (proposed or disk), shareable with the delta.
fn run_checks(
    path: &str,
    content: &str,
    root: &std::path::Path,
    overlays: &HashMap<String, String>,
    inline: bool,
) -> Vec<Finding> {
    let Some(tree) = parse_thread_local(content, Lang::Python).ok() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let res = Resolver::new(root, overlays);
    let proprio = extract_module_info(content, tree.root_node());

    // (1) nome — análise de escopo
    let st = ScopeTree::build(tree.root_node(), content, inline);
    let sev_nome = if inline || path.starts_with("/tmp/") {
        "deny"
    } else {
        "warn"
    };
    for issue in st.issues() {
        if issue.escaped {
            // dinamico (P4): import * ou exec no caminho — warn sempre, nunca deny
            out.push(Finding::new(
                "nome",
                &issue.nome,
                issue.linha,
                "warn",
                "nome possivelmente dinâmico (import * / exec) — não resolúvel estaticamente"
                    .into(),
                Vec::new(),
            ));
        } else {
            out.push(Finding::new(
                "nome",
                &issue.nome,
                issue.linha,
                sev_nome,
                "nome usado sem definição em nenhum escopo".into(),
                Vec::new(),
            ));
        }
    }

    // (2) import + (3) atributo de módulo + (4) membro de instância + (5) kwarg
    let mut walker = Checks {
        path,
        content,
        res: &res,
        proprio: &proprio,
        out: &mut out,
        root: tree.root_node(),
        em_curso: std::cell::RefCell::new(HashSet::new()),
        // P2 (medida pela coordenadora em 09/10): /tmp e inline negam; em arquivo do projeto só
        // alvo EXTERNO nega (stdlib/builtin), alvo do projeto vira warn — recall conta os dois
        deny_natural: inline || path.starts_with("/tmp/"),
    };
    walker.walk(tree.root_node());
    out
}

struct Checks<'a, 't> {
    path: &'a str,
    content: &'a str,
    res: &'a Resolver<'a>,
    proprio: &'a ModuleInfo,
    out: &'a mut Vec<Finding>,
    deny_natural: bool,
    /// A árvore já parseada do módulo — a inferência anda nela, nunca re-parseia.
    root: Node<'t>,
    /// Nomes cuja inferência está EM CURSO neste momento — guarda de ciclo (`line = line.strip()`).
    em_curso: std::cell::RefCell<HashSet<String>>,
}

/// The proven type of a receiver — project class, stdlib module, or a closed builtin type.
#[derive(Debug, Clone)]
enum Tipo {
    Modulo(String, Rc<ModuleInfo>),
    /// Closed-table stdlib module (`unicodedata`, `math`): EXTERNAL target — deny even in files.
    Stdlib(String),
    Classe(String, Rc<ModuleInfo>),
    Builtin(&'static str),
    /// A function VALUE (imported): attribute reads beyond dunders fail at runtime.
    Funcao,
}

impl<'a, 't> Checks<'a, 't> {
    fn walk(&mut self, node: Node) {
        match node.kind() {
            "import_from_statement" => self.check_import_from(node),
            "attribute" => self.check_attribute(node),
            "call" => self.check_call(node),
            _ => {}
        }
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.is_named() {
                self.walk(ch);
            }
        }
    }

    // ------------------------------------------------------------------ (2) from M import N

    fn check_import_from(&mut self, node: Node) {
        let module_txt = node
            .child_by_field_name("module_name")
            .map(|m| text(self.content, m).to_string())
            .unwrap_or_default();
        let rel_level = module_txt.chars().take_while(|c| *c == '.').count();
        let dotted = if rel_level > 0 {
            match resolve_relative(
                self.path,
                rel_level,
                module_txt.trim_start_matches('.'),
                self.res_root(),
            ) {
                Some(d) => d,
                None => return, // inline com import relativo: irresolúvel → unknown
            }
        } else {
            module_txt.clone()
        };
        if dotted.is_empty() {
            return;
        }
        let Some(modulo) = self.res.resolve_module(&dotted, self.path) else {
            return; // módulo irresolúvel → unknown
        };
        // gramática 0.25: os nomes importados vivem no campo `name` — `dotted_name` (import
        // simples) ou `aliased_import` (com alias); o MÓDULO também é dotted_name, mas no campo
        // `module_name`. A iteração é por campo: um match por kind sobre todos os filhos pulava o
        // import sem alias (bug medido no baseline v2 — recall inline 0.37) em silêncio.
        let mut c = node.walk();
        for ch in node.children_by_field_name("name", &mut c) {
            let nome = match ch.kind() {
                "dotted_name" => Some(text(self.content, ch).to_string()),
                "aliased_import" => ch
                    .child_by_field_name("name")
                    .map(|n| text(self.content, n).to_string()),
                _ => None, // wildcard_import e demais formas não são um nome checável
            };
            let Some(nome) = nome else { continue };
            if nome == "*" {
                continue;
            }
            match self.module_has(&dotted, &modulo, &nome) {
                Tri::Yes | Tri::Unknown => {}
                Tri::No => {
                    let reais = reals_of_module(&modulo);
                    out_find(
                        self.out,
                        "import",
                        &nome,
                        ch.start_position().row + 1,
                        &format!(
                            "`from {dotted} import {nome}` — `{nome}` não é ligação de topo nem submódulo de `{dotted}`"
                        ),
                        reais,
                        self.sev(stdlib_module_members(&dotted).is_some()),
                    );
                }
            }
        }
    }

    /// `N` ∈ module: top-level binding, or a submodule file, or __all__-listed.
    fn module_has(&mut self, dotted: &str, modulo: &ModuleInfo, nome: &str) -> Tri {
        if modulo.top_names.contains(nome) {
            return Tri::Yes;
        }
        if modulo.has_getattr || modulo.has_star {
            return Tri::Unknown;
        }
        // submódulo ANTES do `__all__`: `from pkg import submod` é legítimo mesmo fora do
        // __all__ (que só governa `import *`) — a ordem anterior condenava defusedxml.ElementTree
        if self
            .res
            .module_exists(&format!("{dotted}.{nome}"), self.path)
        {
            return Tri::Yes;
        }
        if let Some(all) = &modulo.dunder_all {
            return if all.contains(nome) {
                Tri::Yes
            } else {
                Tri::No
            };
        }
        Tri::No
    }

    // ------------------------------------------------------------------ (3)/(4) obj.attr

    fn check_attribute(&mut self, node: Node) {
        let Some(obj) = node.child_by_field_name("object") else {
            return;
        };
        let Some(attr) = node.child_by_field_name("attribute") else {
            return;
        };
        let nome = text(self.content, attr).to_string();
        let linha = attr.start_position().row + 1;
        match self.tipo_de(obj) {
            Some(Tipo::Modulo(dotted, info)) => {
                if self.module_has(&dotted, &info, &nome) == Tri::No {
                    let reais = reals_of_module(&info);
                    out_find(
                        self.out,
                        "atributo",
                        &nome,
                        linha,
                        &format!("módulo `{dotted}` não tem atributo `{nome}`"),
                        reais,
                        self.sev(false),
                    );
                }
            }
            Some(Tipo::Stdlib(dotted)) => {
                if let Some(members) = stdlib_module_members(&dotted)
                    && !members.contains(&nome.as_str())
                {
                    out_find(
                        self.out,
                        "atributo",
                        &nome,
                        linha,
                        &format!("módulo stdlib `{dotted}` não tem atributo `{nome}`"),
                        members.iter().map(|s| s.to_string()).collect(),
                        "deny",
                    );
                }
            }
            Some(Tipo::Funcao) => {
                // função importada usada como receptor: dunders são dinâmicos (warn); qualquer
                // atributo solto falha em runtime (`sfe.slides_fisica_eleitoral`, corpus)
                if nome.starts_with("__") {
                    out_find(
                        self.out,
                        "atributo",
                        &nome,
                        linha,
                        &format!("atributo dunder `{nome}` em função — possivelmente dinâmico"),
                        Vec::new(),
                        "warn",
                    );
                } else {
                    out_find(
                        self.out,
                        "atributo",
                        &nome,
                        linha,
                        &format!("valor-função não tem atributo `{nome}`"),
                        Vec::new(),
                        self.sev(false),
                    );
                }
            }
            Some(Tipo::Classe(cls, modulo)) => self.check_membro(&cls, &modulo, &nome, linha, obj),
            Some(Tipo::Builtin(t)) => {
                if let Some(members) = builtin_type_members(t)
                    && !members.contains(&nome.as_str())
                {
                    out_find(
                        self.out,
                        "atributo",
                        &nome,
                        linha,
                        &format!("`{t}` não tem atributo `{nome}`"),
                        members.iter().map(|s| s.to_string()).collect(),
                        "warn",
                    );
                }
            }
            None => {}
        }
    }

    /// Member of a concrete project class — the deny only when the member set CLOSED.
    fn check_membro(
        &mut self,
        cls: &str,
        modulo: &Rc<ModuleInfo>,
        nome: &str,
        linha: usize,
        _obj: Node,
    ) {
        let Some((membros, completos)) = self.members_of(cls, modulo, 0) else {
            return;
        };
        if !completos {
            return; // base externa/dinâmica ou __getattr__ no caminho → unknown
        }
        if nome == "__dict__" {
            if let Some(ci) = modulo.classes.get(cls)
                && ci.slots.is_some()
            {
                out_find(
                    self.out,
                    "membro",
                    nome,
                    linha,
                    &format!("`{cls}` define __slots__ — instância não tem `__dict__`"),
                    membros.iter().take(8).cloned().collect(),
                    "warn",
                );
            }
            return;
        }
        if !membros.contains(nome) {
            out_find(
                self.out,
                "membro",
                nome,
                linha,
                &format!("instância de `{cls}` não tem membro `{nome}`"),
                membros.iter().take(8).cloned().collect(),
                self.sev(false),
            );
        }
    }

    /// (members, complete) of a project class, closing over project bases recursively.
    fn members_of(
        &self,
        cls: &str,
        modulo: &Rc<ModuleInfo>,
        depth: usize,
    ) -> Option<(HashSet<String>, bool)> {
        if depth > 6 {
            return None;
        }
        let ci = modulo.classes.get(cls)?;
        if ci.has_getattr_instance {
            return Some((HashSet::new(), false));
        }
        let mut membros = ci.own_members();
        let mut completos = true;
        for base in &ci.bases {
            let base_name = base.rsplit('.').next().unwrap_or(base);
            if is_builtin_base(base_name) {
                continue;
            }
            let Some((bmod_name, bmod)) = self.resolve_class_module(base_name, modulo) else {
                completos = false;
                continue;
            };
            match self.members_of(&bmod_name, &bmod, depth + 1) {
                Some((bm, bc)) => {
                    membros.extend(bm);
                    completos &= bc;
                }
                None => completos = false,
            }
        }
        Some((membros, completos))
    }

    /// A class name as written → (name, its defining module): own module first, then imports.
    fn resolve_class_module(
        &self,
        cls: &str,
        modulo: &ModuleInfo,
    ) -> Option<(String, Rc<ModuleInfo>)> {
        if modulo.classes.contains_key(cls) {
            return Some((cls.to_string(), Rc::new(modulo.clone())));
        }
        // com alias (`from m import Cls as D`), o lookup na tabela de m é pelo nome ORIGINAL
        let (from, original) = modulo.imports.get(cls)?;
        let m = self.res.resolve_module(from, self.path)?;
        if m.classes.contains_key(original) {
            Some((original.clone(), m))
        } else {
            None
        }
    }

    /// `nome` é função em `modulo`, seguindo RE-EXPORTS (`from m import f` onde m a importa
    /// de m2): devolve (nome_real, módulo_real, assinatura). Profundidade limitada como members_of.
    fn resolve_funcao(
        &self,
        nome: &str,
        modulo: &Rc<ModuleInfo>,
        depth: usize,
    ) -> Option<(String, Rc<ModuleInfo>, super::modules::FuncSig)> {
        if depth > 4 {
            return None;
        }
        if let Some(sig) = modulo.functions.get(nome) {
            return Some((nome.to_string(), modulo.clone(), sig.clone()));
        }
        let (m2, nome2) = modulo.imports.get(nome)?;
        let m2 = self.res.resolve_module(m2, self.path)?;
        self.resolve_funcao(nome2, &m2, depth + 1)
    }

    // ------------------------------------------------------------------ (5) kwarg

    fn check_call(&mut self, node: Node) {
        let Some(f) = node.child_by_field_name("function") else {
            return;
        };
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let mut kwargs: Vec<(String, usize)> = Vec::new();
        let mut c = args.walk();
        for ch in args.children(&mut c) {
            if ch.kind() == "keyword_argument"
                && let Some(n) = ch.child_by_field_name("name")
            {
                kwargs.push((
                    text(self.content, n).to_string(),
                    n.start_position().row + 1,
                ));
            }
        }
        if kwargs.is_empty() {
            return;
        }
        let Some(sig) = self.sig_de(f) else { return };
        if sig.has_kwargs {
            return;
        }
        for (kw, linha) in kwargs {
            if !sig.params.contains(&kw) {
                out_find(
                    self.out,
                    "kwarg",
                    &kw,
                    linha,
                    "argumento-nome inexistente na assinatura",
                    sig.params.iter().take(10).cloned().collect(),
                    self.sev(false),
                );
            }
        }
    }

    fn sig_de(&self, f: Node) -> Option<super::modules::FuncSig> {
        match f.kind() {
            "identifier" => {
                let nome = text(self.content, f);
                if let Some(sig) = self.proprio.functions.get(nome) {
                    return Some(sig.clone());
                }
                let (from, original) = self.proprio.imports.get(nome)?;
                let m = self.res.resolve_module(from, self.path)?;
                if let Some((_, _, sig)) = self.resolve_funcao(original, &m, 0) {
                    return Some(sig);
                }
                m.classes.get(original).map(|ci| ci.init.clone())
            }
            "attribute" => {
                let obj = f.child_by_field_name("object")?;
                let attr = f.child_by_field_name("attribute")?;
                let nome = text(self.content, attr);
                match self.tipo_de(obj)? {
                    Tipo::Modulo(_, info) => info.functions.get(nome).cloned(),
                    // método: sig não modelada ainda → unknown (nunca deny)
                    Tipo::Classe(_, _) => None,
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------ type inference

    /// The proven type of a receiver expression — only certainties.
    fn tipo_de(&self, node: Node) -> Option<Tipo> {
        match node.kind() {
            "identifier" => self.tipo_do_nome(text(self.content, node)),
            "call" => self.tipo_da_chamada(node),
            "string" | "concatenated_string" => Some(Tipo::Builtin("str")),
            "integer" => Some(Tipo::Builtin("int")),
            "float" => Some(Tipo::Builtin("float")),
            "list" => Some(Tipo::Builtin("list")),
            "dictionary" | "set" => Some(Tipo::Builtin("dict")),
            "none" => None,
            "binary_operator" => {
                // `Path / "arquivo"` → Path — o único operador que o motor modela (fechado);
                // a gramática expõe o operador como token anônimo, não como field
                let eh_div = {
                    let mut c = node.walk();
                    node.children(&mut c)
                        .any(|ch| !ch.is_named() && text(self.content, ch) == "/")
                };
                let left = node
                    .child_by_field_name("left")
                    .and_then(|l| self.tipo_de(l));
                match (eh_div, left) {
                    (true, Some(Tipo::Builtin("Path"))) => Some(Tipo::Builtin("Path")),
                    _ => None,
                }
            }
            "attribute" => self.tipo_do_atributo(node),
            _ => None,
        }
    }

    /// `x = Path(...) / "saida.txt"` and the like: the one operator we model (`Path / str` → Path).
    fn tipo_da_chamada(&self, node: Node) -> Option<Tipo> {
        let f = node.child_by_field_name("function")?;
        if f.kind() == "identifier" {
            let nome = text(self.content, f);
            if let Some(t) = call_result_type(nome) {
                return Some(Tipo::Builtin(t));
            }
            if let Some((cls, modulo)) = self.resolve_class_module(nome, self.proprio) {
                return Some(Tipo::Classe(cls, modulo));
            }
            if let Some((from, original)) = self.proprio.imports.get(nome).cloned() {
                let m = self.res.resolve_module(&from, self.path)?;
                if let Some((_, m_real, sig)) = self.resolve_funcao(&original, &m, 0)
                    && let Some(ret) = &sig.returns
                    && let Some((cls, rm)) = self.resolve_class_module(ret, &m_real)
                {
                    return Some(Tipo::Classe(cls, rm));
                }
            }
            // função LOCAL com anotação de retorno
            if let Some(sig) = self.proprio.functions.get(nome)
                && let Some(ret) = &sig.returns
                && let Some((cls, rm)) = self.resolve_class_module(ret, self.proprio)
            {
                return Some(Tipo::Classe(cls, rm));
            }
        }
        if f.kind() == "attribute"
            && let Some(obj) = f.child_by_field_name("object")
        {
            // construtores PONTUADOS fechados: `pathlib.Path(...)` — o módulo stdlib não resolve
            // em disco, mas o plain-import prova o nome (recall medido: a cadeia do st_m_size
            // morria aqui). Só o que é certeza; o resto segue as vias normais.
            if obj.kind() == "identifier"
                && matches!(
                    self.proprio
                        .plain_imports
                        .get(text(self.content, obj))
                        .map(String::as_str),
                    Some("pathlib")
                )
                && f.child_by_field_name("attribute")
                    .map(|a| text(self.content, a) == "Path")
                    .unwrap_or(false)
            {
                return Some(Tipo::Builtin("Path"));
            }
            if let Some(Tipo::Modulo(_, info)) = self.tipo_de(obj) {
                let attr = f.child_by_field_name("attribute")?;
                let nome = text(self.content, attr);
                if let Some(sig) = info.functions.get(nome)
                    && let Some(ret) = &sig.returns
                    && let Some((cls, rm)) = self.resolve_class_module(ret, &info)
                {
                    return Some(Tipo::Classe(cls, rm));
                }
                if let Some(t) = call_result_type(nome) {
                    return Some(Tipo::Builtin(t));
                }
            }
            // método com retorno conhecido fechado (ex.: Path.stat() → stat_result) —
            // vale para QUALQUER receptor cujo tipo esteja provado
            if self.tipo_de(obj).is_some() {
                let attr = f.child_by_field_name("attribute")?;
                if let Some(t) = call_result_type(text(self.content, attr)) {
                    return Some(Tipo::Builtin(t));
                }
            }
        }
        None
    }

    fn tipo_do_atributo(&self, node: Node) -> Option<Tipo> {
        let obj = node.child_by_field_name("object")?;
        let attr = node.child_by_field_name("attribute")?;
        let nome = text(self.content, attr);
        // `modulo.submodulo` quando o atributo é ele próprio um módulo
        if let Some(Tipo::Modulo(dotted, _)) = self.tipo_de(obj) {
            let sub = format!("{dotted}.{nome}");
            if let Some(m) = self.res.resolve_module(&sub, self.path) {
                return Some(Tipo::Modulo(sub, m));
            }
        }
        None
    }

    /// `x` é um módulo (via imports), uma instância de classe do projeto, ou um builtin provado.
    /// Guarda de ciclo: `line = line.strip()` (medida no corpus — stack overflow em 58 chamadas do
    /// baseline v2) fazia nome → atribuição → receptor → o MESMO nome sem visitados; ciclo vira
    /// unknown, nunca crash.
    fn tipo_do_nome(&self, nome: &str) -> Option<Tipo> {
        if !self.em_curso.borrow_mut().insert(nome.to_string()) {
            return None;
        }
        let r = self.tipo_do_nome_interno(nome);
        self.em_curso.borrow_mut().remove(nome);
        r
    }

    fn tipo_do_nome_interno(&self, nome: &str) -> Option<Tipo> {
        // PLAIN import (`import a.b` / `import a.b as A`): o nome É o módulo.
        if let Some(dotted) = self.proprio.plain_imports.get(nome) {
            if let Some(m) = self.res.resolve_module(dotted, self.path) {
                return Some(Tipo::Modulo(dotted.clone(), m));
            }
            if stdlib_module_members(dotted).is_some() {
                return Some(Tipo::Stdlib(dotted.clone()));
            }
            return None;
        }
        // FROM import (`from m import X`): se m.X resolve como módulo, X É o submódulo; senão X é
        // MEMBRO de m (classe → construtor; função → valor-função; demais → tipo desconhecido) —
        // NUNCA o módulo m (R1: 74 atributo + 4 import falsos no baseline).
        if let Some((from, original)) = self.proprio.imports.get(nome) {
            let sub = format!("{from}.{original}");
            if let Some(m) = self.res.resolve_module(&sub, self.path) {
                return Some(Tipo::Modulo(sub, m));
            }
            if let Some(m) = self.res.resolve_module(from, self.path) {
                if m.classes.contains_key(original) {
                    return None; // classe — chamadas constroem instâncias
                }
                if self.resolve_funcao(original, &m, 0).is_some() {
                    return Some(Tipo::Funcao);
                }
                return None; // membro de tipo desconhecido — unknown, nunca deny
            }
            return None;
        }
        // atribuições de `nome` no próprio módulo: tipo inferível de CADA uma; qualquer fonte
        // desconhecida na cadeia → unknown (disciplina de precisão)
        let mut tipo: Option<Tipo> = None;
        let mut ok = true;
        self.para_cada_atribuicao(nome, &mut |rhs| {
            let t = self.tipo_de(rhs);
            if t.is_none()
                || (tipo.is_some() && std::mem::discriminant(&tipo) != std::mem::discriminant(&t))
            {
                ok = false;
            } else {
                tipo = t;
            }
        });
        if ok { tipo } else { None }
    }

    /// Chama `f(rhs)` para cada `nome = rhs` do módulo, andando a árvore JÁ parseada — o
    /// baseline media um re-parse inteiro por nome inferido (p95 532 ms).
    fn para_cada_atribuicao(&self, nome: &str, f: &mut impl FnMut(Node)) {
        fn go(content: &str, nome: &str, node: Node, f: &mut impl FnMut(Node)) {
            if node.kind() == "assignment"
                && let (Some(l), Some(r)) = (
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                )
                && l.kind() == "identifier"
                && &content[l.byte_range()] == nome
            {
                f(r);
            }
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() {
                    go(content, nome, ch, f);
                }
            }
        }
        go(self.content, nome, self.root, f);
    }

    /// P2: /tmp e inline negam tudo; em arquivo do projeto, só alvo EXTERNO nega.
    fn sev(&self, externo: bool) -> &'static str {
        if self.deny_natural || externo {
            "deny"
        } else {
            "warn"
        }
    }

    fn res_root(&self) -> &std::path::Path {
        self.res.root_path()
    }
}

/// Aggregate a verdict from findings, falling back to `base` when there are none — the base
/// carries an earlier "unknown" (a merged layer must never turn a could-not-measure into allow).
pub fn verdict_dos(findings: &[Finding], base: &str) -> String {
    if findings.iter().any(|f| f.severidade == "deny") {
        "deny".to_string()
    } else if findings.is_empty() {
        base.to_string()
    } else {
        "warn".to_string()
    }
}

fn is_builtin_base(nome: &str) -> bool {
    matches!(
        nome,
        "object"
            | "ABC"
            | "ABCMeta"
            | "Exception"
            | "BaseException"
            | "NamedTuple"
            | "TypedDict"
            | "Protocol"
            | "Generic"
            | "Enum"
            | "IntEnum"
            | "StrEnum"
            | "Flag"
            | "IntFlag"
            | "dataclass"
            | "Record"
    )
}

fn reals_of_module(info: &ModuleInfo) -> Vec<String> {
    let mut v: Vec<String> = info
        .top_names
        .iter()
        .filter(|n| !n.starts_with('_'))
        .cloned()
        .collect();
    v.sort();
    v.truncate(8);
    v
}

fn out_find(
    out: &mut Vec<Finding>,
    classe: &str,
    nome: &str,
    linha: usize,
    motivo: &str,
    reais: Vec<String>,
    sev: &str,
) {
    if out.iter().any(|f| f.classe == classe && f.nome == nome) {
        return; // um achado por (classe, nome) basta — o juiz conta presença, não ocorrências
    }
    let sugestao = sugestao_proxima(nome, &reais)
        .map(|s| format!("{motivo}. Você quis dizer `{s}`?"))
        .unwrap_or_else(|| motivo.to_string());
    out.push(Finding::new(classe, nome, linha, sev, sugestao, reais));
}

/// O nome real mais próximo (distância de edição ≤ 2), se houver — a recusa ensina o caminho.
fn sugestao_proxima(nome: &str, reais: &[String]) -> Option<String> {
    reais
        .iter()
        .map(|r| (levenshtein(nome, r), r))
        .filter(|(d, _)| *d > 0 && *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, r)| r.clone())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push(
                (prev[j] + usize::from(ca != cb))
                    .min(cur[j] + 1)
                    .min(prev[j + 1] + 1),
            );
        }
        prev = cur;
    }
    prev[b.len()]
}

fn text<'a>(source: &'a str, node: Node) -> &'a str {
    &source[node.byte_range()]
}

#[cfg(test)]
mod tests;
