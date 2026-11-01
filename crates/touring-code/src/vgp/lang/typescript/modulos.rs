//! Module summaries: what a TS/JS module EXPORTS and DECLARES, extracted on demand and cached for the
//! duration of one check (never from an index that could be stale — the V1 decision).
//!
//! A module's export table is trusted only when it is closed: `export *` from something that does not
//! resolve, `export =`, CommonJS assigned dynamically, a script with no `import`/`export` at all — each
//! makes the table `incompleto`, and a name absent from an incomplete table is `Incerto`, never `Nao`.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tree_sitter::Node;

use super::decls::{Decl, baixar_decl};
use super::jsdoc;
use super::projeto::{Projeto, Resolvido};
use super::util::{conteudo_string, tem_token, texto};
use crate::vgp::lang::parse;

/// How a local name was bound by an `import`.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Importado {
    /// `import X from 'spec'`
    Padrao(String),
    /// `import { orig as local } from 'spec'`
    Nomeado(String, String),
    /// `import * as X from 'spec'` (and `import X = require('spec')`)
    Namespace(String),
}

/// A three-valued answer: absence in an incomplete table is not absence.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Prova {
    Sim,
    Nao,
    Incerto(String),
}

/// Everything the checks need to know about one module.
#[derive(Debug, Default)]
pub(super) struct ModuloInfo {
    /// Has an `import`/`export` statement (an ES module) — a script's globals are not exports.
    pub eh_modulo: bool,
    /// Names exported directly (declarations, `export { a }`, re-exports by name).
    pub exports: HashSet<String>,
    pub tem_default: bool,
    /// `export * from 'spec'`.
    pub estrelas: Vec<String>,
    /// `export { orig as name } from 'spec'`: name → (spec, orig) (`orig` is `*` for `export * as name`).
    pub reexports: HashMap<String, (String, String)>,
    /// `export { local as name }`: name → local.
    pub exp_local: HashMap<String, String>,
    /// Why the export table is not closed.
    pub incompleto: Option<String>,
    /// CommonJS `module.exports` / `exports.x`.
    pub cjs: bool,
    /// Top-level declarations by local name (merged declarations are several entries).
    pub decls: HashMap<String, Vec<Decl>>,
    /// `import` bindings by local name.
    pub imports: HashMap<String, Importado>,
    /// The source had syntax errors: nothing it lacks is proof.
    pub com_erro: bool,
}

/// Summarise a parsed module. `dts`: a declaration file (every top-level declaration is visible).
pub(super) fn resumir(src: &str, raiz: Node, dts: bool, js: bool) -> ModuloInfo {
    let mut info = ModuloInfo {
        com_erro: raiz.has_error(),
        ..ModuloInfo::default()
    };
    let mut c = raiz.walk();
    for no in raiz.named_children(&mut c) {
        registrar_statement(&mut info, no, src, dts);
    }
    if js
        && info.incompleto.is_none()
        && !info.cjs
        && (src.contains("module.exports") || src.contains("exports."))
    {
        info.incompleto = Some("CommonJS atribuído fora do topo do módulo".into());
    }
    if js {
        for (nome, decl) in jsdoc::typedefs_do_arquivo(raiz, src) {
            info.decls.entry(nome).or_default().push(decl);
        }
    }
    info
}

fn registrar_statement(info: &mut ModuloInfo, no: Node, src: &str, dts: bool) {
    match no.kind() {
        "import_statement" => {
            info.eh_modulo = true;
            registrar_import(info, no, src);
        }
        "export_statement" => {
            info.eh_modulo = true;
            registrar_export(info, no, src);
        }
        "expression_statement" => registrar_expressao(info, no, src),
        "import_alias" => {
            if let Some(n) = no.named_child(0) {
                info.decls
                    .entry(texto(src, n).to_string())
                    .or_default()
                    .push(Decl::Outro);
            }
        }
        _ => registrar_decl(info, no, src, dts),
    }
}

fn registrar_decl(info: &mut ModuloInfo, no: Node, src: &str, exportado: bool) {
    for (nome, decl) in baixar_decl(no, src) {
        if exportado {
            info.exports.insert(nome.clone());
        }
        info.decls.entry(nome).or_default().push(decl);
    }
}

fn registrar_import(info: &mut ModuloInfo, no: Node, src: &str) {
    let Some(spec) = no
        .child_by_field_name("source")
        .and_then(|s| conteudo_string(src, s))
    else {
        // `import x = require('y')` keeps the specifier inside the clause
        registrar_import_require(info, no, src);
        return;
    };
    let mut c = no.walk();
    for clausula in no
        .named_children(&mut c)
        .filter(|f| f.kind() == "import_clause")
    {
        let mut cc = clausula.walk();
        for f in clausula.named_children(&mut cc) {
            match f.kind() {
                "identifier" => {
                    info.imports
                        .insert(texto(src, f).to_string(), Importado::Padrao(spec.clone()));
                }
                "namespace_import" => {
                    if let Some(n) = f.named_child(0) {
                        info.imports.insert(
                            texto(src, n).to_string(),
                            Importado::Namespace(spec.clone()),
                        );
                    }
                }
                "named_imports" => registrar_importados_nomeados(info, f, src, &spec),
                _ => {}
            }
        }
    }
}

fn registrar_importados_nomeados(info: &mut ModuloInfo, named: Node, src: &str, spec: &str) {
    let mut c = named.walk();
    for s in named
        .named_children(&mut c)
        .filter(|f| f.kind() == "import_specifier")
    {
        let Some(nome) = s.child_by_field_name("name") else {
            continue;
        };
        let local = s.child_by_field_name("alias").unwrap_or(nome);
        info.imports.insert(
            texto(src, local).to_string(),
            Importado::Nomeado(spec.to_string(), texto(src, nome).to_string()),
        );
    }
}

fn registrar_import_require(info: &mut ModuloInfo, no: Node, src: &str) {
    let mut c = no.walk();
    for clausula in no
        .named_children(&mut c)
        .filter(|f| f.kind() == "import_require_clause")
    {
        let id = clausula.named_child(0);
        let spec = clausula
            .child_by_field_name("source")
            .and_then(|s| conteudo_string(src, s));
        if let (Some(id), Some(spec)) = (id, spec) {
            info.imports
                .insert(texto(src, id).to_string(), Importado::Namespace(spec));
        }
    }
}

fn registrar_export(info: &mut ModuloInfo, no: Node, src: &str) {
    if tem_token(no, "=") {
        info.incompleto = Some("`export =` (a forma do módulo não é um conjunto de nomes)".into());
        return;
    }
    if tem_token(no, "as") && tem_token(no, "namespace") {
        return; // `export as namespace X` (UMD global)
    }
    let eh_default = tem_token(no, "default");
    if let Some(decl) = no.child_by_field_name("declaration") {
        info.tem_default |= eh_default;
        registrar_decl(info, decl, src, !eh_default);
        return;
    }
    if no.child_by_field_name("value").is_some() {
        info.tem_default = true; // export default <expression>
        return;
    }
    let fonte = no
        .child_by_field_name("source")
        .and_then(|s| conteudo_string(src, s));
    if !registrar_clausulas(info, no, src, fonte.as_deref())
        && let Some(s) = fonte
    {
        info.estrelas.push(s); // `export * from 'x'`
    }
}

/// `export { … }` and `export * as ns` clauses of one statement; `true` if there was any.
fn registrar_clausulas(info: &mut ModuloInfo, no: Node, src: &str, fonte: Option<&str>) -> bool {
    let mut tem_clausula = false;
    let mut c = no.walk();
    for f in no.named_children(&mut c) {
        match f.kind() {
            "export_clause" => {
                tem_clausula = true;
                registrar_clausula(info, f, src, fonte);
            }
            "namespace_export" => {
                tem_clausula = true;
                let Some(id) = f.named_child(0) else { continue };
                let nome = texto(src, id).to_string();
                info.exports.insert(nome.clone());
                if let Some(s) = fonte {
                    info.reexports.insert(nome, (s.to_string(), "*".into()));
                }
            }
            _ => {}
        }
    }
    tem_clausula
}

fn registrar_clausula(info: &mut ModuloInfo, clausula: Node, src: &str, fonte: Option<&str>) {
    let mut c = clausula.walk();
    for s in clausula
        .named_children(&mut c)
        .filter(|f| f.kind() == "export_specifier")
    {
        let Some(nome) = s.child_by_field_name("name") else {
            continue;
        };
        let original = texto(src, nome).to_string();
        let exportado = s
            .child_by_field_name("alias")
            .map_or_else(|| original.clone(), |a| texto(src, a).to_string());
        info.exports.insert(exportado.clone());
        info.tem_default |= exportado == "default";
        match fonte {
            Some(spec) => {
                info.reexports
                    .insert(exportado, (spec.to_string(), original));
            }
            None => {
                info.exp_local.insert(exportado, original);
            }
        }
    }
}

/// Top-level expression statements: `namespace X {}` and the CommonJS assignment forms.
fn registrar_expressao(info: &mut ModuloInfo, no: Node, src: &str) {
    let Some(e) = no.named_child(0) else { return };
    match e.kind() {
        "internal_module" | "module" => registrar_decl(info, e, src, false),
        "assignment_expression" => registrar_cjs(info, e, src),
        "call_expression" => registrar_cjs_chamada(info, e, src),
        _ => {}
    }
}

fn registrar_cjs(info: &mut ModuloInfo, atrib: Node, src: &str) {
    let Some(esq) = atrib.child_by_field_name("left") else {
        return;
    };
    let escrito = texto(src, esq);
    if escrito == "module.exports" {
        info.cjs = true;
        match atrib.child_by_field_name("right") {
            Some(d) if d.kind() == "object" => registrar_objeto_cjs(info, d, src),
            _ => info.incompleto = Some("`module.exports = <expressão>`".into()),
        }
    } else if let Some(nome) = escrito
        .strip_prefix("module.exports.")
        .or_else(|| escrito.strip_prefix("exports."))
    {
        info.cjs = true;
        registrar_nome_cjs(info, nome);
    } else if escrito.starts_with("module.exports[") || escrito.starts_with("exports[") {
        info.cjs = true;
        info.incompleto = Some("`exports[chave]` dinâmico".into());
    }
}

/// `exports.NAME = …`: a plain name is exported; `exports.a.b = …` is nested, so the table is open.
fn registrar_nome_cjs(info: &mut ModuloInfo, nome: &str) {
    if nome
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    {
        info.exports.insert(nome.to_string());
    } else {
        info.incompleto = Some("`exports.a.b = …` aninhado".into());
    }
}

fn registrar_objeto_cjs(info: &mut ModuloInfo, objeto: Node, src: &str) {
    let mut c = objeto.walk();
    for p in objeto.named_children(&mut c) {
        match p.kind() {
            "pair" | "method_definition" => match p
                .child_by_field_name("key")
                .or_else(|| p.child_by_field_name("name"))
            {
                Some(k) if matches!(k.kind(), "property_identifier" | "identifier") => {
                    info.exports.insert(texto(src, k).to_string());
                }
                Some(k) if k.kind() == "string" => {
                    if let Some(s) = conteudo_string(src, k) {
                        info.exports.insert(s);
                    }
                }
                _ => info.incompleto = Some("`module.exports = { [chave]: … }` computado".into()),
            },
            "shorthand_property_identifier" => {
                info.exports.insert(texto(src, p).to_string());
            }
            "spread_element" => info.incompleto = Some("`module.exports = { ...x }`".into()),
            _ => {}
        }
    }
}

fn registrar_cjs_chamada(info: &mut ModuloInfo, chamada: Node, src: &str) {
    let t = texto(src, chamada);
    if t.starts_with("Object.defineProperty(exports")
        || t.starts_with("Object.defineProperty(module.exports")
    {
        info.cjs = true;
        let nome = chamada
            .child_by_field_name("arguments")
            .and_then(|a| a.named_child(1))
            .and_then(|n| conteudo_string(src, n));
        match nome {
            Some(n) => {
                info.exports.insert(n);
            }
            None => info.incompleto = Some("`Object.defineProperty(exports, <dinâmico>)`".into()),
        }
    } else if t.starts_with("Object.assign(module.exports")
        || t.starts_with("Object.assign(exports")
    {
        info.cjs = true;
        info.incompleto = Some("`Object.assign(module.exports, …)`".into());
    }
}

// ---------------------------------------------------------------------------------- loader

/// The grammar for a module file by extension; `None` for non-code (json, css, images…).
pub(super) fn lang_do_arquivo(p: &Path) -> Option<&'static str> {
    let nome = p.file_name()?.to_str()?;
    let ext = nome.rsplit('.').next()?;
    match ext {
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        _ => None,
    }
}

/// Loads, parses and summarises modules by path — each at most once per check.
pub(super) struct Carregador<'a> {
    projeto: &'a Projeto<'a>,
    cache: RefCell<HashMap<PathBuf, Option<Rc<ModuloInfo>>>>,
    carregados: Cell<usize>,
}

/// Modules parsed per check — a bound on the work an import fan-out can cost.
const MAX_MODULOS: usize = 80;
/// Depth of re-export / type-resolution chains.
pub(super) const MAX_PROFUNDIDADE: u8 = 8;

impl<'a> Carregador<'a> {
    pub fn novo(projeto: &'a Projeto<'a>) -> Self {
        Self {
            projeto,
            cache: RefCell::new(HashMap::new()),
            carregados: Cell::new(0),
        }
    }

    /// The summary of the module at `caminho` (`None`: not code, unreadable, or over budget).
    pub fn carregar(&self, caminho: &Path) -> Option<Rc<ModuloInfo>> {
        if let Some(c) = self.cache.borrow().get(caminho) {
            return c.clone();
        }
        let resultado = self.ler_e_resumir(caminho);
        self.cache
            .borrow_mut()
            .insert(caminho.to_path_buf(), resultado.clone());
        resultado
    }

    /// Seed the cache with the summary of the file under check (so self-imports and cycles see it).
    pub fn semear(&self, caminho: &Path, info: Rc<ModuloInfo>) {
        self.cache
            .borrow_mut()
            .insert(caminho.to_path_buf(), Some(info));
    }

    fn ler_e_resumir(&self, caminho: &Path) -> Option<Rc<ModuloInfo>> {
        let lang = lang_do_arquivo(caminho)?;
        if self.carregados.get() >= MAX_MODULOS {
            return None;
        }
        self.carregados.set(self.carregados.get() + 1);
        let src = self.projeto.ler(caminho)?;
        let arvore = parse(lang, &src).ok()?;
        let dts = caminho.to_string_lossy().ends_with(".d.ts");
        Some(Rc::new(resumir(
            &src,
            arvore.root_node(),
            dts,
            lang == "javascript",
        )))
    }

    /// Resolve `spec` from `de` and load it: (path, summary).
    pub fn resolver_e_carregar(&self, de: &Path, spec: &str) -> Option<(PathBuf, Rc<ModuloInfo>)> {
        match self.projeto.resolver(de, spec) {
            Resolvido::Arquivo(p) => {
                let info = self.carregar(&p)?;
                Some((p, info))
            }
            _ => None,
        }
    }

    /// Does module `info` (at `caminho`) export `nome`?
    pub fn tem_export(&self, caminho: &Path, info: &ModuloInfo, nome: &str, prof: u8) -> Prova {
        if info.exports.contains(nome) || (nome == "default" && info.tem_default) {
            return Prova::Sim;
        }
        if prof >= MAX_PROFUNDIDADE {
            return Prova::Incerto("cadeia de re-exports funda demais".into());
        }
        let duvida = match self.nas_estrelas(caminho, info, nome, prof) {
            Prova::Sim => return Prova::Sim,
            Prova::Incerto(m) => Some(m),
            Prova::Nao => None,
        };
        if let Some(motivo) = info.incompleto.clone().or(duvida) {
            return Prova::Incerto(motivo);
        }
        if info.com_erro {
            return Prova::Incerto("módulo com erro de sintaxe".into());
        }
        if !info.eh_modulo && !info.cjs {
            return Prova::Incerto("arquivo sem import/export (script, não módulo)".into());
        }
        Prova::Nao
    }

    /// Look for `nome` through the `export * from` chains of `info` (`default` is never starred).
    fn nas_estrelas(&self, caminho: &Path, info: &ModuloInfo, nome: &str, prof: u8) -> Prova {
        if nome == "default" {
            return Prova::Nao;
        }
        let mut duvida: Option<String> = None;
        for spec in &info.estrelas {
            let prova = match self.resolver_e_carregar(caminho, spec) {
                Some((p, outro)) => self.tem_export(&p, &outro, nome, prof + 1),
                None => Prova::Incerto(format!("`export * from '{spec}'` não resolvido")),
            };
            match prova {
                Prova::Sim => return Prova::Sim,
                Prova::Incerto(m) => duvida = duvida.or(Some(m)),
                Prova::Nao => {}
            }
        }
        duvida.map_or(Prova::Nao, Prova::Incerto)
    }

    /// Every name `info` exports (own + `export *` chains), capped — the «real names» of a refusal.
    pub fn nomes_exportados(&self, caminho: &Path, info: &ModuloInfo, prof: u8) -> Vec<String> {
        let mut nomes: Vec<String> = info.exports.iter().cloned().collect();
        if info.tem_default {
            nomes.push("default".into());
        }
        if prof < MAX_PROFUNDIDADE {
            for spec in &info.estrelas {
                if let Some((p, outro)) = self.resolver_e_carregar(caminho, spec) {
                    nomes.extend(self.nomes_exportados(&p, &outro, prof + 1));
                }
            }
        }
        nomes.sort();
        nomes.dedup();
        nomes
    }

    /// Follow the export `nome` of the module at `caminho` to the module that DECLARES it:
    /// (declaring module path, its summary, the local name there).
    pub fn seguir_export(
        &self,
        caminho: &Path,
        info: &Rc<ModuloInfo>,
        nome: &str,
        prof: u8,
    ) -> Option<(PathBuf, Rc<ModuloInfo>, String)> {
        if prof >= MAX_PROFUNDIDADE {
            return None;
        }
        if let Some((spec, original)) = info.reexports.get(nome) {
            let (p, outro) = self.resolver_e_carregar(caminho, spec)?;
            return if original == "*" {
                None
            } else {
                self.seguir_export(&p, &outro, original, prof + 1)
            };
        }
        let local = info.exp_local.get(nome).map_or(nome, String::as_str);
        if info.decls.contains_key(local) && (info.exports.contains(nome) || nome == "default") {
            return Some((caminho.to_path_buf(), info.clone(), local.to_string()));
        }
        if let Some(Importado::Nomeado(spec, orig)) = info.imports.get(local) {
            let (p, outro) = self.resolver_e_carregar(caminho, spec)?;
            return self.seguir_export(&p, &outro, orig, prof + 1);
        }
        if nome != "default" {
            for spec in &info.estrelas {
                if let Some((p, outro)) = self.resolver_e_carregar(caminho, spec)
                    && let Some(r) = self.seguir_export(&p, &outro, nome, prof + 1)
                {
                    return Some(r);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resumo(lang: &str, src: &str) -> ModuloInfo {
        let arvore = parse(lang, src).expect("parse");
        resumir(src, arvore.root_node(), false, lang == "javascript")
    }

    #[test]
    fn exports_de_declaracoes_clausulas_e_default() {
        let m = resumo(
            "typescript",
            "export const a = 1; export function f() {} export class C {} export interface I {} export type T = 1; \
             export enum E { X } export namespace N {} const loc = 1; export { loc as renomeado }; \
             export { z } from './z'; export default function () {}",
        );
        for n in ["a", "f", "C", "I", "T", "E", "N", "renomeado", "z"] {
            assert!(m.exports.contains(n), "{n} em {:?}", m.exports);
        }
        assert!(m.tem_default);
        assert_eq!(
            m.exp_local.get("renomeado").map(String::as_str),
            Some("loc")
        );
        assert_eq!(
            m.reexports.get("z"),
            Some(&("./z".to_string(), "z".to_string()))
        );
        assert!(m.incompleto.is_none());
    }

    #[test]
    fn export_star_e_namespace_e_igual() {
        let m = resumo(
            "typescript",
            "export * from './a'; export * as ns from './b';",
        );
        assert_eq!(m.estrelas, vec!["./a".to_string()]);
        assert!(m.exports.contains("ns"));
        let m = resumo("typescript", "export = Foo;");
        assert!(m.incompleto.is_some());
        let m = resumo(
            "typescript",
            "export as namespace MyLib; export const x = 1;",
        );
        assert!(m.incompleto.is_none() && m.exports.contains("x"));
    }

    #[test]
    fn imports_por_forma() {
        let m = resumo(
            "typescript",
            "import d, { a as b, c } from './m'; import * as ns from './n'; import type { T } from './t'; import eq = require('./e');",
        );
        assert_eq!(m.imports.get("d"), Some(&Importado::Padrao("./m".into())));
        assert_eq!(
            m.imports.get("b"),
            Some(&Importado::Nomeado("./m".into(), "a".into()))
        );
        assert_eq!(
            m.imports.get("c"),
            Some(&Importado::Nomeado("./m".into(), "c".into()))
        );
        assert_eq!(
            m.imports.get("ns"),
            Some(&Importado::Namespace("./n".into()))
        );
        assert_eq!(
            m.imports.get("T"),
            Some(&Importado::Nomeado("./t".into(), "T".into()))
        );
        assert_eq!(
            m.imports.get("eq"),
            Some(&Importado::Namespace("./e".into()))
        );
    }

    #[test]
    fn commonjs_estatico_fecha_a_tabela_e_dinamico_abre() {
        let m = resumo(
            "javascript",
            "exports.a = 1; module.exports.b = 2; module.exports = { c, d: 1, e() {} };",
        );
        for n in ["a", "b", "c", "d", "e"] {
            assert!(m.exports.contains(n), "{n}");
        }
        assert!(m.cjs && m.incompleto.is_none());
        let m = resumo("javascript", "module.exports = factory();");
        assert!(m.incompleto.is_some());
        let m = resumo("javascript", "module.exports = { ...base, x: 1 };");
        assert!(m.incompleto.is_some());
        let m = resumo("javascript", "(function () { module.exports = 1; })();");
        assert!(m.incompleto.is_some(), "CJS fora do topo");
    }

    #[test]
    fn declaracoes_de_topo_ficam_em_decls_mesmo_sem_export() {
        let m = resumo(
            "typescript",
            "interface P { a: number } class K {} function f() {} const v = 1;",
        );
        for n in ["P", "K", "f", "v"] {
            assert!(m.decls.contains_key(n), "{n}");
        }
        assert!(m.exports.is_empty());
    }
}
