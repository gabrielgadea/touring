//! Module resolution and per-module symbol tables, on demand — the V1 architecture decision:
//! the module's table is extracted from the file ON DEMAND and cached by (path, sha256), never
//! from an index that could be stale. Resolution order for `a.b.c`:
//!   1. `<root>/a/b/c.py` | `<root>/a/b/c/__init__.py`   (project)
//!   2. `<dir of importer>/a/b/c.py` | `…/__init__.py`   (sibling of a /tmp script)
//!   3. `<root>/.venv/lib/python3*/site-packages/…`      (installed env)
//!
//! Each candidate checks the overlay map FIRST (module content at the entry's instant), then the
//! filesystem. An unresolvable module yields `None` — the caller answers `unknown`, never `deny`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tree_sitter::Node;

use crate::ast::languages::Lang;
use crate::ast::parser::parse_thread_local;

/// Static signature of a project function/method (kwarg checks and return-type inference).
#[derive(Debug, Clone, Default)]
pub struct FuncSig {
    /// Positional + keyword-only parameter names, in order.
    pub params: Vec<String>,
    /// `**kwargs` present — unknown keywords are then absorbed (never denied).
    pub has_kwargs: bool,
    /// Return annotation's last segment (`-> pkg.Recibo` → `Recibo`), when annotated.
    pub returns: Option<String>,
}

/// Everything the motor knows about a class, from its definition module.
#[derive(Debug, Clone, Default)]
pub struct ClassInfo {
    /// Annotated fields and class-level assignments.
    pub fields: HashSet<String>,
    /// `self.x` assigned in ANY method (spec §4).
    pub self_attrs: HashSet<String>,
    /// Method names (properties included).
    pub methods: HashSet<String>,
    /// `__slots__` when statically defined (a slots class has no `__dict__`).
    pub slots: Option<HashSet<String>>,
    /// `__getattr__`/`__getattribute__` defined — the member set is then unknowable (never deny).
    pub has_getattr_instance: bool,
    /// Base classes as written (dotted allowed).
    pub bases: Vec<String>,
    /// `@dataclass` decorated — `__init__` params are then the fields.
    pub is_dataclass: bool,
    /// `__init__` signature (or the dataclass fields), for kwarg checks on construction.
    pub init: FuncSig,
}

impl ClassInfo {
    /// Every statically-known member name (without bases — the caller closes over them).
    pub fn own_members(&self) -> HashSet<String> {
        let mut m = self.fields.clone();
        m.extend(self.self_attrs.iter().cloned());
        m.extend(self.methods.iter().cloned());
        if let Some(slots) = &self.slots {
            m.extend(slots.iter().cloned());
        }
        m
    }
}

/// The on-demand symbol table of one module file.
#[derive(Debug, Clone, Default)]
pub struct ModuleInfo {
    /// Top-level bindings (assignments, defs, classes, import names).
    pub top_names: HashSet<String>,
    /// Static `__all__ = [...]` when fully static — it then narrows `from m import n`.
    pub dunder_all: Option<HashSet<String>>,
    /// Module-level `def __getattr__` — attributes are then unknowable (never deny).
    pub has_getattr: bool,
    /// `from x import *` present — the namespace is then dynamic (never deny).
    pub has_star: bool,
    /// FROM-import binding (`from m import X [as Y]`): the bound name (alias Y, or X) ↦
    /// (m, X) — the original name is required to type the receiver (submodule m.X, or the
    /// member X of m); the old `name ↦ m` lost it and aliased submodules became unresolvable.
    pub imports: HashMap<String, (String, String)>,
    /// PLAIN import binding (`import a.b` → a↦a · `import a.b as A` → A↦a.b): the name IS the
    /// module — indistinguishable from a from-import in the old single map (R1 regression).
    pub plain_imports: HashMap<String, String>,
    /// Classes defined in the module, by name.
    pub classes: HashMap<String, ClassInfo>,
    /// Functions defined in the module, by name.
    pub functions: HashMap<String, FuncSig>,
}

/// Does `name` resolve as a member of this module? `__all__` narrows when present;
/// `__getattr__` or `import *` makes the question unknowable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    /// Provably a member.
    Yes,
    /// Provably NOT a member (complete set).
    No,
    /// Unknowable (dynamic namespace) — never a deny.
    Unknown,
}

/// Cache de processo do resolver: (caminho, sha256 do conteúdo) → tabela do módulo.
type ModuleCache = std::cell::RefCell<HashMap<(PathBuf, String), Option<Rc<ModuleInfo>>>>;

/// On-demand module resolver: project → sibling → venv, overlay first, (path, sha) cached.
pub struct Resolver<'a> {
    root: &'a Path,
    overlays: &'a HashMap<String, String>,
    cache: ModuleCache,
    fs_cache: std::cell::RefCell<HashMap<PathBuf, Option<String>>>,
}

impl<'a> Resolver<'a> {
    /// A resolver over `root`, with `overlays` (path → content) laid over the disk.
    pub fn new(root: &'a Path, overlays: &'a HashMap<String, String>) -> Self {
        Self {
            root,
            overlays,
            cache: ModuleCache::new(HashMap::new()),
            fs_cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// The project root modules resolve against.
    pub fn root_path(&self) -> &'a Path {
        self.root
    }

    /// The overlay map (path → content at the entry's instant).
    pub fn overlays_map(&self) -> &'a HashMap<String, String> {
        self.overlays
    }

    /// Resolve `a.b.c` imported from `importer` (a virtual path; `<string>` for inline).
    /// Resolve `a.b.c` imported from `importer` (a virtual path; `<string>` for inline).
    pub fn resolve_module(&self, dotted: &str, importer: &str) -> Option<Rc<ModuleInfo>> {
        let rel = dotted.replace('.', "/");
        let mut candidates: Vec<PathBuf> = vec![
            self.root.join(format!("{rel}.py")),
            self.root.join(rel.clone()).join("__init__.py"),
        ];
        if !crate::vgp::is_inline(importer)
            && let Some(dir) = Path::new(importer).parent()
        {
            candidates.push(dir.join(format!("{rel}.py")));
            candidates.push(dir.join(&rel).join("__init__.py"));
        }
        candidates.extend(self.venv_candidates(&rel));
        for cand in candidates {
            if let Some(info) = self.module_at(&cand) {
                return Some(info);
            }
        }
        None
    }

    /// Does `dotted` resolve as a module at all (without loading it)? Used for submodule checks.
    /// Does `dotted` resolve as a module at all (without loading it)? Used for submodule checks.
    pub fn module_exists(&self, dotted: &str, importer: &str) -> bool {
        self.resolve_module(dotted, importer).is_some()
    }

    /// The module bound to `path` if it is a file (overlay first, then disk).
    /// The module bound to `path` if it is a file (overlay first, then disk).
    pub fn module_at(&self, path: &Path) -> Option<Rc<ModuleInfo>> {
        let content = self.content_of(path)?;
        let sha = sha256(&content);
        let key = (path.to_path_buf(), sha);
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let info = parse_thread_local(&content, Lang::Python)
            .ok()
            .map(|tree| extract_module_info(&content, tree.root_node()));
        self.cache
            .borrow_mut()
            .insert(key, info.clone().map(Rc::new));
        info.map(Rc::new)
    }

    /// Overlay first; then the filesystem (empty dir is not a file).
    fn content_of(&self, path: &Path) -> Option<String> {
        let key = path.to_string_lossy().to_string();
        if let Some(c) = self.overlays.get(&key) {
            return Some(c.clone());
        }
        if let Some(hit) = self.fs_cache.borrow().get(path) {
            return hit.clone();
        }
        let c = std::fs::read_to_string(path)
            .ok()
            .filter(|s| !s.is_empty() || path.exists());
        self.fs_cache
            .borrow_mut()
            .insert(path.to_path_buf(), c.clone());
        c
    }

    fn venv_candidates(&self, rel: &str) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let venv = self.root.join(".venv/lib");
        if let Ok(rd) = std::fs::read_dir(&venv) {
            for e in rd.flatten() {
                let p = e.path();
                if p.file_name()
                    .map(|n| n.to_string_lossy().starts_with("python"))
                    .unwrap_or(false)
                {
                    let sp = p.join("site-packages");
                    out.push(sp.join(format!("{rel}.py")));
                    out.push(sp.join(rel).join("__init__.py"));
                }
            }
        }
        out
    }
}

/// `from . import x` / `from ..pkg import y`: the absolute-ish dotted form relative to the
/// importer's package, or None for inline (no package to anchor to).
pub fn resolve_relative(importer: &str, level: usize, rest: &str, root: &Path) -> Option<String> {
    if crate::vgp::is_inline(importer) {
        return None;
    }
    let mut dir = Path::new(importer).parent()?.to_path_buf();
    for _ in 1..level {
        dir = dir.parent()?.to_path_buf();
    }
    let base = dir.strip_prefix(root).ok()?;
    let base_dotted = base.to_string_lossy().replace('/', ".");
    let base_trimmed = base_dotted.trim_matches('.');
    Some(if rest.is_empty() || base_trimmed.is_empty() {
        format!("{}{}", base_trimmed, rest)
            .trim_matches('.')
            .to_string()
    } else {
        format!("{base_trimmed}.{rest}")
    })
}

// --------------------------------------------------------------------------- extraction

/// Extract the on-demand symbol table of a module source (one tree-sitter pass, top level only).
pub fn extract_module_info(source: &str, root: Node) -> ModuleInfo {
    let mut info = ModuleInfo::default();
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        match node.kind() {
            "expression_statement" | "assignment" | "augmented_assignment" => {
                collect_top_assignment(&mut info, source, node);
            }
            "function_definition" | "decorated_definition" => {
                collect_def(&mut info, source, node);
            }
            "class_definition" => {
                collect_class(&mut info, source, node, &[]);
            }
            "import_statement" => collect_import(&mut info, source, node),
            "import_from_statement" => collect_import_from(&mut info, source, node),
            _ => {}
        }
    }
    info
}

fn collect_top_assignment(info: &mut ModuleInfo, source: &str, node: Node) {
    let target = if node.kind() == "expression_statement" {
        match node.child(0) {
            Some(c) if c.kind() == "assignment" => c,
            _ => return,
        }
    } else {
        node
    };
    let Some(left) = target.child_by_field_name("left") else {
        return;
    };
    for name in pattern_identifiers(source, left) {
        info.top_names.insert(name);
    }
    if let (Some(l), Some(r)) = (
        target.child_by_field_name("left"),
        target.child_by_field_name("right"),
    ) && text(source, l) == "__all__"
    {
        info.dunder_all = static_string_list(source, r);
    }
}

fn collect_def(info: &mut ModuleInfo, source: &str, node: Node) {
    let def = unwrap_decorated(node).unwrap_or(node);
    let decorators = if node.kind() == "decorated_definition" {
        let mut c = node.walk();
        node.children(&mut c)
            .filter(|ch| ch.kind() == "decorator")
            .map(|d| text(source, d).trim_start_matches('@').to_string())
            .collect()
    } else {
        Vec::new()
    };
    match def.kind() {
        "function_definition" => {
            let Some(name) = def.child_by_field_name("name") else {
                return;
            };
            let nome = text(source, name).to_string();
            info.top_names.insert(nome.clone());
            let sig = func_sig(source, def);
            if nome == "__getattr__" {
                info.has_getattr = true;
            }
            info.functions.insert(nome, sig);
        }
        "class_definition" => collect_class(info, source, def, &decorators),
        _ => {}
    }
}

fn collect_class(info: &mut ModuleInfo, source: &str, node: Node, decorators: &[String]) {
    let Some(name) = node.child_by_field_name("name") else {
        return;
    };
    let nome = text(source, name).to_string();
    info.top_names.insert(nome.clone());
    let mut ci = ClassInfo::default();
    ci.is_dataclass = decorators.iter().any(|d| {
        let base = d.trim_end_matches(')');
        base == "dataclass"
            || base.ends_with(".dataclass")
            || base.starts_with("dataclass(")
            || base.contains(".dataclass(")
    });
    if let Some(sup) = node.child_by_field_name("superclasses") {
        let mut c = sup.walk();
        for ch in sup.children(&mut c) {
            match ch.kind() {
                "identifier" | "dotted_name" | "attribute" => {
                    ci.bases.push(text(source, ch).to_string())
                }
                "keyword_argument" => {
                    if let Some(v) = ch.child_by_field_name("value") {
                        ci.bases.push(text(source, v).to_string());
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        let mut c = body.walk();
        for stmt in body.children(&mut c) {
            collect_class_stmt(&mut ci, source, stmt);
        }
    }
    if ci.is_dataclass {
        ci.init.params = ci.fields.iter().cloned().collect();
        // @dataclass(slots=True) GERA __slots__ com os campos — instância não tem __dict__
        if decorators
            .iter()
            .any(|d| d.contains("slots=True") || d.contains("slots = True"))
            && ci.slots.is_none()
        {
            ci.slots = Some(ci.fields.clone());
        }
    }
    info.classes.insert(nome, ci);
}

fn collect_class_stmt(ci: &mut ClassInfo, source: &str, stmt: Node) {
    match stmt.kind() {
        "assignment" => {
            if let Some(left) = stmt.child_by_field_name("left") {
                if let (Some(l), Some(r)) = (
                    stmt.child_by_field_name("left"),
                    stmt.child_by_field_name("right"),
                ) && text(source, l) == "__slots__"
                {
                    ci.slots = static_string_list(source, r);
                }
                for name in pattern_identifiers(source, left) {
                    ci.fields.insert(name);
                }
            }
        }
        "expression_statement" => {
            if let Some(a) = stmt.child(0).filter(|c| c.kind() == "assignment") {
                collect_class_stmt(ci, source, a);
            }
        }
        "function_definition" | "decorated_definition" => {
            let def = unwrap_decorated(stmt);
            let Some(def) = def.filter(|d| d.kind() == "function_definition").or(def) else {
                return;
            };
            let Some(name) = def.child_by_field_name("name") else {
                return;
            };
            let nome = text(source, name).to_string();
            if matches!(nome.as_str(), "__getattr__" | "__getattribute__") {
                ci.has_getattr_instance = true;
            }
            ci.methods.insert(nome.clone());
            if nome == "__init__" {
                ci.init = func_sig(source, def);
            }
            if let Some(body) = def.child_by_field_name("body") {
                collect_self_attrs(&mut ci.self_attrs, source, body);
            }
        }
        _ => {}
    }
}

/// `self.x = …` / `self.x: T = …` in ANY method — the V1 member set (spec §4).
fn collect_self_attrs(out: &mut HashSet<String>, source: &str, body: Node) {
    fn walk(out: &mut HashSet<String>, source: &str, node: Node) {
        if node.kind() == "assignment"
            && let Some(left) = node.child_by_field_name("left")
        {
            collect_self_target(out, source, left);
        }
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            // do not descend into nested def/class/lambda — `self` there is another object
            if !matches!(
                ch.kind(),
                "function_definition" | "class_definition" | "lambda" | "decorated_definition"
            ) {
                walk(out, source, ch);
            }
        }
    }
    walk(out, source, body);
}

fn collect_self_target(out: &mut HashSet<String>, source: &str, node: Node) {
    match node.kind() {
        "attribute" => {
            if let Some(obj) = node.child_by_field_name("object")
                && obj.kind() == "identifier"
                && text(source, obj) == "self"
                && let Some(attr) = node.child_by_field_name("attribute")
            {
                out.insert(text(source, attr).to_string());
            }
        }
        "pattern_list" | "tuple_pattern" | "list_pattern" | "list" | "tuple" => {
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                collect_self_target(out, source, ch);
            }
        }
        _ => {}
    }
}

fn collect_import(info: &mut ModuleInfo, source: &str, node: Node) {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        match ch.kind() {
            "dotted_name" => {
                let dotted = text(source, ch).to_string();
                if let Some(first) = dotted.split('.').next() {
                    info.top_names.insert(first.to_string());
                    info.plain_imports
                        .insert(first.to_string(), first.to_string());
                }
            }
            "aliased_import" => {
                let nome_mod = ch
                    .child_by_field_name("name")
                    .map(|n| text(source, n).to_string());
                if let (Some(dotted), Some(alias)) = (nome_mod, last_identifier(ch)) {
                    let a = text(source, alias).to_string();
                    info.top_names.insert(a.clone());
                    info.plain_imports.insert(a, dotted);
                }
            }
            _ => {}
        }
    }
}

fn collect_import_from(info: &mut ModuleInfo, source: &str, node: Node) {
    // gramática 0.25: o PRIMEIRO dotted_name é o módulo; os seguintes são os nomes importados
    let mut module = String::new();
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        match ch.kind() {
            "wildcard_import" => info.has_star = true,
            "aliased_import" => {
                if let Some(alias) = last_identifier(ch) {
                    let a = text(source, alias).to_string();
                    let original = ch
                        .child_by_field_name("name")
                        .map(|n| text(source, n).to_string())
                        .unwrap_or_default();
                    info.top_names.insert(a.clone());
                    info.imports.insert(a, (module.clone(), original));
                }
            }
            "dotted_name" => {
                if module.is_empty() {
                    module = text(source, ch).to_string();
                } else {
                    let n = text(source, ch)
                        .split('.')
                        .next_back()
                        .unwrap_or("")
                        .to_string();
                    info.top_names.insert(n.clone());
                    info.imports.insert(n.clone(), (module.clone(), n));
                }
            }
            _ => {}
        }
    }
}

fn func_sig(source: &str, def: Node) -> FuncSig {
    let mut sig = FuncSig::default();
    if let Some(ret) = def.child_by_field_name("return_type") {
        let t = text(source, ret).to_string();
        sig.returns = Some(
            t.rsplit('.')
                .next()
                .unwrap_or(&t)
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string(),
        );
    }
    if let Some(params) = def.child_by_field_name("parameters") {
        let mut c = params.walk();
        for ch in params.children(&mut c) {
            match ch.kind() {
                "identifier" => sig.params.push(text(source, ch).to_string()),
                "typed_parameter" | "typed_default_parameter" | "default_parameter" => {
                    // os splats TIPADOS vivem aqui dentro (`**kw: Unpack[X]` · `*a: str`) —
                    // sem a descida, has_kwargs nunca ligava e os kwargs reais eram negados
                    let mut cc = ch.walk();
                    for sub in ch.children(&mut cc) {
                        match sub.kind() {
                            "dictionary_splat_pattern" => sig.has_kwargs = true,
                            "list_splat_pattern" => {
                                let mut ci = sub.walk();
                                for inner in sub.children(&mut ci) {
                                    if inner.kind() == "identifier" {
                                        sig.params.push(text(source, inner).to_string());
                                        break;
                                    }
                                }
                            }
                            "identifier" => {
                                sig.params.push(text(source, sub).to_string());
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                "dictionary_splat_pattern" => sig.has_kwargs = true,
                "list_splat_pattern" => {}
                _ => {}
            }
        }
    }
    sig
}

// --------------------------------------------------------------------------- helpers

/// The last bare `identifier` child — the alias in `import x as y` / `from m import x as y`
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

fn unwrap_decorated(node: Node) -> Option<Node> {
    if node.kind() != "decorated_definition" {
        return Some(node);
    }
    let mut c = node.walk();
    node.children(&mut c)
        .find(|ch| matches!(ch.kind(), "function_definition" | "class_definition"))
}

/// Identifiers a binding target introduces (`x`, `(a, b)`, `[p, q]` — not attribute/subscript).
/// Identifiers a binding target introduces (`x`, `(a, b)`, `[p, q]` — not attribute/subscript).
pub fn pattern_identifiers(source: &str, node: Node) -> Vec<String> {
    fn go(out: &mut Vec<String>, source: &str, node: Node) {
        match node.kind() {
            "identifier" => out.push(text(source, node).to_string()),
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "list"
            | "tuple"
            | "parenthesized_expression" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    go(out, source, ch);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    go(&mut out, source, node);
    out
}

/// A static `["a", "b"]` / `("a", "b")` of strings, or None when not fully static.
fn static_string_list(source: &str, node: Node) -> Option<HashSet<String>> {
    if !matches!(node.kind(), "list" | "tuple" | "set") {
        return None;
    }
    let mut out = HashSet::new();
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "string" {
            let t = text(source, ch);
            out.insert(t.trim_matches(|c| c == '"' || c == '\'').to_string());
        } else if ch.is_named() {
            return None;
        }
    }
    Some(out)
}

fn text<'a>(source: &'a str, node: Node) -> &'a str {
    &source[node.byte_range()]
}

fn sha256(content: &str) -> String {
    // FNV-1a 64 — chave de cache de processo, não fingerprint criptográfico
    let mut h: u64 = 0xcbf29ce484222325;
    for b in content.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info_of(src: &str) -> ModuleInfo {
        let tree = parse_thread_local(src, Lang::Python).expect("parse");
        extract_module_info(src, tree.root_node())
    }

    #[test]
    fn top_names_cobrem_atribuicao_def_classe_e_imports() {
        let info = info_of(
            "import os\nimport a.b as ab\nfrom m import C as D, e\nx = 1\ny: int = 2\ndef f():\n    pass\nclass K:\n    pass\n",
        );
        for n in ["os", "ab", "D", "e", "x", "y", "f", "K"] {
            assert!(info.top_names.contains(n), "faltou {n}");
        }
        assert_eq!(
            info.plain_imports.get("ab").map(String::as_str),
            Some("a.b")
        );
        assert_eq!(
            info.imports.get("D").map(|(m, x)| (m.as_str(), x.as_str())),
            Some(("m", "C"))
        );
    }

    #[test]
    fn dunder_all_e_getattr_e_star() {
        let info =
            info_of("__all__ = ['a', 'b']\ndef __getattr__(n):\n    return 1\nfrom x import *\n");
        assert!(info.dunder_all.as_ref().unwrap().contains("a"));
        assert!(info.has_getattr);
        assert!(info.has_star);
    }

    #[test]
    fn classe_campos_metodos_self_attrs_e_slots() {
        let src = "class K:\n    f1: int\n    f2 = 2\n    __slots__ = ('f1', 'f2')\n    def m(self):\n        self.a = 1\n    @property\n    def p(self):\n        return self.a\n    def outro(self):\n        self.b: int = 3\n";
        let ci = info_of(src).classes.remove("K").unwrap();
        assert!(ci.fields.contains("f1") && ci.fields.contains("f2"));
        assert!(ci.self_attrs.contains("a") && ci.self_attrs.contains("b"));
        assert!(ci.methods.contains("m") && ci.methods.contains("p"));
        assert!(ci.slots.as_ref().unwrap().contains("f1"));
    }

    #[test]
    fn self_attrs_somem_de_qualquer_metodo_mas_nao_de_nested() {
        let src = "class K:\n    def a(self):\n        self.um = 1\n    def b(self):\n        def nested():\n            self.nao_conta = 2\n";
        let ci = info_of(src).classes.remove("K").unwrap();
        assert!(ci.self_attrs.contains("um"));
        assert!(!ci.self_attrs.contains("nao_conta"));
    }

    #[test]
    fn dataclass_init_sao_os_campos() {
        let src = "import dataclasses\n@dataclasses.dataclass\nclass R:\n    chave: str\n    gravado: bool\n";
        let ci = info_of(src).classes.remove("R").unwrap();
        assert!(ci.is_dataclass);
        assert!(ci.init.params.contains(&"chave".to_string()));
        assert!(ci.init.params.contains(&"gravado".to_string()));
    }

    #[test]
    fn getattr_de_instancia_marca_incompleto() {
        let src = "class D:\n    def __getattr__(self, n):\n        return 0\n";
        assert!(
            info_of(src)
                .classes
                .remove("D")
                .unwrap()
                .has_getattr_instance
        );
    }

    #[test]
    fn func_sig_params_kwargs_e_retorno() {
        let src = "def f(a, b: int = 1, *args, c: str, **kwargs) -> Recibo:\n    pass\n";
        let sig = info_of(src).functions.remove("f").unwrap();
        assert!(sig.has_kwargs);
        assert_eq!(sig.returns.as_deref(), Some("Recibo"));
        assert!(sig.params.contains(&"a".to_string()) && sig.params.contains(&"c".to_string()));
    }
}

#[cfg(test)]
mod debug_tree {
    use crate::ast::languages::Lang;
    use crate::ast::parser::parse_thread_local;
    use tree_sitter::Node;

    fn dump(node: Node, src: &str, depth: usize) {
        println!(
            "{}{} [{}]",
            "  ".repeat(depth),
            node.kind(),
            src[node.byte_range()].lines().next().unwrap_or("")
        );
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.is_named() {
                dump(ch, src, depth + 1);
            }
        }
    }

    #[test]
    fn debug_shapes() {
        for src in [
            "import subprocess\nfrom m import C as D, e\n",
            "ys = [x for x in xs]\n",
            "def f(a, b: int = 1, *args, c: str, **kwargs) -> Recibo:\n    pass\n",
            "@dataclasses.dataclass\nclass R:\n    chave: str\n",
            "import re\nm = re.search('a','b')\nprint(m.group(0))\n",
        ] {
            println!("=== {src:?}");
            let tree = parse_thread_local(src, Lang::Python).unwrap();
            dump(tree.root_node(), src, 0);
        }
    }
}
