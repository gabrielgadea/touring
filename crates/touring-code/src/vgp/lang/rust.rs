//! VRS — the Rust adapter of the VGP layer-1 motor (VL0 §3 of `2026-10-08-vgp-motor-base`).
//!
//! Authorship: the resolver, the module model and the scope walk were written by the
//! `touring-a6` session (Kimi) as the free function `verificar_rust`, and ported here onto the
//! VL0 contract: the adapter MEASURES ([`Adaptador::verificar`]), `vgp::check` DECIDES (severity
//! by target × where the code runs, delta against the disk, verdict — `vgp::politica`).
//!
//! Pure layer 1: no daemon, no network, no process spawn. Judges the proposed `.rs` content
//! against the project's module tree read from disk (overlays first), following `pub use`
//! re-exports — plain, aliased and glob (the declaration beats the inference — the Touring
//! reexport lesson) — and `#[path = "…"]` module aliases.
//!
//! Classes (VL0 §4): `import` (E0432/E0433 — `use`/`mod` paths that provably miss a project
//! target) · `nome` (E0425 — bare names and path heads bound in no scope) · `kwarg` (E0560 —
//! struct-literal fields the project struct does not have; E0061 — arity of project free fns)
//! · `atributo` (E0599 — variants of project enums used through a path).
//!
//! Whom a finding is about ([`Alvo`]): [`Alvo::Projeto`] for every symbol of the project (the
//! consumer written before the provider is the RED of TDD — the policy makes it `deny` only
//! where the code runs now); [`Alvo::Incerto`] where the walk knows it may be wrong (an
//! item-level macro or an external glob can bring names in from outside; a project that itself
//! lives under `/tmp/`, where that prefix does not mean scratch). There is no [`Alvo::Externo`]:
//! layer 1 has no model of `std` or of an external crate, so it cannot PROVE an external
//! absence — layer 2 (rust-analyzer) can.
//!
//! Abstention is the fail-open direction (VL0 §5): item-level macros, unknown `cfg`, globs of
//! external crates, external-crate paths without a model, method receivers of unproven type,
//! `non_exhaustive` types, tuple structs and trait objects are NEVER a deny. What the adapter
//! TRIED to resolve and could not (an unreadable module, no `Cargo.toml` above the file, `super`
//! above the crate root, a re-export cycle) comes back as `nao_medido` with the reason — the
//! verdict is `unknown`, never a silent `allow`. What it never claimed (receivers, associated
//! items of a project struct, the items of an external crate) is not "unmeasured".
//!
//! References consulted (the Context7 MCP was not reachable from this session, so the
//! authoritative LOCAL equivalents were read and are cited):
//! - rustc error index, offline: `rustc --explain E0432 E0433 E0425 E0560 E0599 E0061` (rustc
//!   1.98.0, 88d9e12ae).
//! - Rust Reference shipped with the toolchain (`~/.rustup/toolchains/*/share/doc/rust/html/
//!   reference/`): `paths.html` (uniform paths, `crate`/`self`/`super`), `items/
//!   use-declarations.html` (use forms, `{self}`, aliases, glob imports shadowed by items),
//!   `items/modules.html` (`mod x;` → `x.rs` | `x/mod.rs`, `#[path]`, both forms at once is an
//!   error), `names/preludes.html` (the extern prelude: declared dependencies, `std`, `core`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tree_sitter::Node;

use super::{Adaptador, Verificacao, parse};
use crate::vgp::politica::Alvo;
use crate::vgp::{CheckInput, Finding};

/// The `rust` adapter registered in `super::REGISTRO`.
pub struct Rust;

impl Adaptador for Rust {
    fn lang(&self) -> &'static str {
        "rust"
    }

    /// Measure the proposed content. A syntax error is the one thing that cannot be measured
    /// whole — a partial AST would under-collect bindings and invent absent names — so it is
    /// `nao_medido`, never `deny`. The delta and the verdict are `vgp::check`'s.
    fn verificar(&self, input: &CheckInput) -> Verificacao {
        let tree = match parse("rust", &input.content) {
            Ok(t) => t,
            Err(e) => {
                return Verificacao::nao_medido(format!("parse da gramática rust falhou: {e}"));
            }
        };
        if tree.root_node().has_error() {
            return Verificacao::nao_medido(
                "o conteúdo proposto tem erro de sintaxe — um AST parcial inventaria nomes ausentes",
            );
        }
        let res = Resolver::new(&input.project_root, &input.overlays);
        let alvo = alvo_do_projeto(&input.path, &res);
        let (achados, abstencoes) =
            verificar_conteudo(&input.path, &input.content, tree.root_node(), &res, alvo);
        Verificacao {
            achados,
            nao_medido: motivo_nao_medido(&abstencoes),
        }
    }
}

/// The [`Alvo`] of a missing symbol of the project in the file at `path`. A file under `/tmp/`
/// is scratch the policy denies on — unless the project itself lives there (a `Cargo.toml`
/// above the file, inside the project root): then it is a project file and the missing symbol
/// may still be created by the next edit, so the adapter reports it as uncertain (`warn`).
fn alvo_do_projeto(path: &str, res: &Resolver) -> Alvo {
    let em_crate = !crate::vgp::is_inline(path) && res.crate_de(Path::new(path)).is_some();
    if path.starts_with("/tmp/") && em_crate {
        Alvo::Incerto
    } else {
        Alvo::Projeto
    }
}

/// The `nao_medido` reason for the resolutions the adapter tried and could not finish, or
/// `None` when every attempted resolution closed.
fn motivo_nao_medido(abstencoes: &[String]) -> Option<String> {
    if abstencoes.is_empty() {
        return None;
    }
    let amostra: Vec<&str> = abstencoes.iter().take(3).map(String::as_str).collect();
    let resto = abstencoes.len() - amostra.len();
    let mais = if resto > 0 {
        format!(" (+{resto})")
    } else {
        String::new()
    };
    Some(format!(
        "{} caminho(s) sem resolução estática: {}{mais}",
        abstencoes.len(),
        amostra.join("; ")
    ))
}

/// All the checks over one parsed content: the content's own model is registered as the
/// resolution target for `self::` paths (a new file may not exist on disk). Returns the
/// findings and the resolutions that were attempted and could not be closed.
fn verificar_conteudo(
    path: &str,
    content: &str,
    raiz: Node,
    res: &Resolver,
    alvo: Alvo,
) -> (Vec<Finding>, Vec<String>) {
    let modelo = Rc::new(extrair_modelo(content, raiz));
    res.registrar_proposto(Path::new(path), modelo.clone());
    let mut ctx = Ctx::new(path, content, res, modelo, alvo);
    ctx.checar_imports();
    ctx.caminhar(raiz);
    (ctx.out, ctx.abstencoes)
}

// ============================================================================ modelo de módulo

/// Everything computed from one parsed module (the proposed file or a resolved project module):
/// the addressable top-level names, the typed payloads the checks need and the `use` leaves
/// (with the inline-module trail they sit in, so `super::` resolves from the right place).
#[derive(Default)]
struct ModModel {
    /// Every addressable top-level name (fn/struct/enum/union/trait/type/const/static/mod/macro).
    itens: HashSet<String>,
    /// Free functions: name → (min, max) arity (`None` max = variadic).
    funcoes: HashMap<String, (usize, Option<usize>)>,
    /// Named-field structs; tuple/unit structs are NOT here (a literal check would be guesswork).
    structs: HashMap<String, Rc<StructModel>>,
    /// Enums: closed variant sets (macros/derives cannot add variants — the set is exact).
    enums: HashMap<String, Rc<EnumModel>>,
    /// `mod x;` (file-backed) declarations.
    mods_declarados: HashMap<String, ModDecl>,
    /// Inline `mod x { … }` modules by name (recursive, depth-capped at extraction).
    inline: HashMap<String, Rc<ModModel>>,
    /// `pub use` leaves (visible name = alias or last segment) — re-export chains to follow.
    reexportacoes: Vec<Reexport>,
    /// `pub use path::*;` glob re-exports (their targets' names become addressable here).
    glob_reexports: Vec<(Vec<String>, Vec<String>)>,
    /// Every `use` leaf anywhere in the file, with the inline-module trail of its position.
    usos: Vec<UseNo>,
    /// An item-level macro invocation may declare names — scope findings degrade to dynamic.
    macro_item: bool,
}

/// A `mod x;` declaration: its line and the optional `#[path = "…"]` override (the alias used
/// by `cli_handlers_decompose` in the touring tree — measured during the VRS scouting).
struct ModDecl {
    linha: usize,
    path_attr: Option<String>,
}

/// A named-field struct (the `kwarg`/E0560 target).
struct StructModel {
    campos: HashSet<String>,
    non_exhaustive: bool,
}

/// A project enum (the `atributo`/E0599 target): the closed variant set and, for struct
/// variants (`V { campo: … }`), the variant's own fields (also a `kwarg` target).
struct EnumModel {
    variantes: HashSet<String>,
    variantes_struct: HashMap<String, HashSet<String>>,
    /// Associated items of the `impl` blocks visible next to the enum (same module block).
    assoc: HashSet<String>,
    non_exhaustive: bool,
}

/// One `pub use` leaf: the inline trail it sits at, the path as written and the name it is
/// visible under (alias or last segment).
struct Reexport {
    trail: Vec<String>,
    segs: Vec<String>,
    visivel: String,
}

/// One `use` leaf at its position: `trail` = enclosing inline `mod` names (deepest last).
#[derive(Clone)]
struct UseNo {
    trail: Vec<String>,
    segs: Vec<String>,
    alias: Option<String>,
    glob: bool,
    linha: usize,
}

/// Extract a module model: top-level items, inline modules (depth cap 4), `use` leaves anywhere.
fn extrair_modelo(content: &str, raiz: Node) -> ModModel {
    let mut m = ModModel::default();
    extrair_itens(content, raiz, &mut m, 0);
    coleta_usos(content, raiz, &mut Vec::new(), &mut m);
    m
}

/// One level of items; inline `mod` bodies recurse into their own sub-model.
fn extrair_itens(content: &str, bloco: Node, m: &mut ModModel, prof: usize) {
    let assoc = assoc_por_tipo(content, bloco);
    let mut c = bloco.walk();
    for ch in bloco.children(&mut c) {
        if !ch.is_named() {
            continue;
        }
        match ch.kind() {
            "function_item" => {
                if let Some(n) = ch.child_by_field_name("name") {
                    let nome = nome_limpo(text(content, n));
                    m.itens.insert(nome.clone());
                    m.funcoes.insert(nome, aridade_de(ch));
                }
            }
            "struct_item" => {
                if let Some(n) = ch.child_by_field_name("name") {
                    let nome = nome_limpo(text(content, n));
                    m.itens.insert(nome.clone());
                    if let Some(corpo) = ch.child_by_field_name("body")
                        && corpo.kind() == "field_declaration_list"
                    {
                        m.structs.insert(
                            nome,
                            Rc::new(StructModel {
                                campos: campos_declarados(content, corpo),
                                non_exhaustive: tem_attr(content, ch, "non_exhaustive"),
                            }),
                        );
                    }
                }
            }
            "enum_item" => {
                if let Some(n) = ch.child_by_field_name("name") {
                    let nome = nome_limpo(text(content, n));
                    m.itens.insert(nome.clone());
                    let mut em = EnumModel {
                        variantes: HashSet::new(),
                        variantes_struct: HashMap::new(),
                        assoc: assoc.get(&nome).cloned().unwrap_or_default(),
                        non_exhaustive: tem_attr(content, ch, "non_exhaustive"),
                    };
                    if let Some(corpo) = ch.child_by_field_name("body") {
                        let mut cc = corpo.walk();
                        for v in corpo.children(&mut cc) {
                            if v.kind() != "enum_variant" {
                                continue;
                            }
                            let Some(vn) = v.child_by_field_name("name") else {
                                continue;
                            };
                            let vnome = nome_limpo(text(content, vn));
                            em.variantes.insert(vnome.clone());
                            if let Some(vb) = v.child_by_field_name("body")
                                && vb.kind() == "field_declaration_list"
                            {
                                em.variantes_struct
                                    .insert(vnome, campos_declarados(content, vb));
                            }
                        }
                    }
                    m.enums.insert(nome, Rc::new(em));
                }
            }
            "union_item" | "trait_item" | "type_item" => {
                if let Some(n) = ch.child_by_field_name("name") {
                    m.itens.insert(nome_limpo(text(content, n)));
                }
            }
            "const_item" | "static_item" => {
                if let Some(n) = ch.child_by_field_name("name") {
                    m.itens.insert(nome_limpo(text(content, n)));
                }
            }
            "foreign_mod_item" => {
                // `unsafe extern "C" { fn flock(..); static X: T; }`: o que o bloco declara é item
                // do módulo
                let corpo = ch.child_by_field_name("body").unwrap_or(ch);
                let mut cc = corpo.walk();
                for it in corpo.children(&mut cc) {
                    if matches!(
                        it.kind(),
                        "function_signature_item" | "static_item" | "foreign_item_type"
                    ) && let Some(n) = it.child_by_field_name("name")
                    {
                        m.itens.insert(nome_limpo(text(content, n)));
                    }
                }
            }
            "extern_crate_declaration" => {
                let nome = ch
                    .child_by_field_name("alias")
                    .or_else(|| ch.child_by_field_name("name"));
                if let Some(n) = nome {
                    m.itens.insert(nome_limpo(text(content, n)));
                }
            }
            "mod_item" => {
                if let Some(n) = ch.child_by_field_name("name") {
                    let nome = nome_limpo(text(content, n));
                    m.itens.insert(nome.clone());
                    if let Some(corpo) = ch.child_by_field_name("body") {
                        if prof < 4 {
                            let mut sub = ModModel::default();
                            extrair_itens(content, corpo, &mut sub, prof + 1);
                            m.inline.insert(nome, Rc::new(sub));
                        }
                    } else {
                        m.mods_declarados.insert(
                            nome,
                            ModDecl {
                                linha: ch.start_position().row + 1,
                                path_attr: path_attr_de(content, ch),
                            },
                        );
                    }
                }
            }
            "macro_definition" => {
                if let Some(n) = ch.child_by_field_name("name") {
                    m.itens.insert(nome_limpo(text(content, n)));
                }
            }
            "macro_invocation" => m.macro_item = true,
            // `gera_itens!();` — com `;` a invocação de item chega como instrução-expressão
            "expression_statement" if tem_macro_invocation(ch) => m.macro_item = true,
            _ => {}
        }
    }
}

/// The associated items (`fn`, `const`, `type`) declared by the `impl` blocks of ONE module
/// block, by the base name of the implemented type. An enum's `Enum::name` is decidable only
/// against the impls the adapter can SEE — the ones in the same module block as the enum.
fn assoc_por_tipo(content: &str, bloco: Node) -> HashMap<String, HashSet<String>> {
    let mut mapa: HashMap<String, HashSet<String>> = HashMap::new();
    let mut c = bloco.walk();
    for ch in bloco.children(&mut c) {
        if ch.kind() != "impl_item" {
            continue;
        }
        let (Some(tipo), Some(corpo)) = (
            ch.child_by_field_name("type"),
            ch.child_by_field_name("body"),
        ) else {
            continue;
        };
        let nomes = mapa.entry(nome_base_do_tipo(content, tipo)).or_default();
        let mut cc = corpo.walk();
        for item in corpo.children(&mut cc) {
            if matches!(item.kind(), "function_item" | "const_item" | "type_item")
                && let Some(n) = item.child_by_field_name("name")
            {
                nomes.insert(nome_limpo(text(content, n)));
            }
        }
    }
    mapa
}

/// The outer attributes of an item, as written (`#[path = "x.rs"]`, `#[non_exhaustive]`). In the
/// tree-sitter-rust grammar they are SIBLINGS that precede the item — not its children — with
/// only comments allowed in between (the Rust Reference, `attributes.html`).
fn atributos_do_item<'a>(content: &'a str, item: Node) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut atual = item.prev_sibling();
    while let Some(n) = atual {
        match n.kind() {
            "attribute_item" => out.push(text(content, n)),
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        atual = n.prev_sibling();
    }
    out
}

/// The `#[path = "…"]` string of a `mod` declaration, when present (the attribute must BE
/// `path = "…"` — a `#[cfg(feature = "pathfinder")]` is not a path override).
fn path_attr_de(content: &str, mod_node: Node) -> Option<String> {
    atributos_do_item(content, mod_node)
        .into_iter()
        .find_map(|a| {
            let corpo = a.strip_prefix("#[")?.strip_suffix(']')?.trim();
            let valor = corpo
                .strip_prefix("path")?
                .trim_start()
                .strip_prefix('=')?
                .trim();
            Some(valor.strip_prefix('"')?.strip_suffix('"')?.to_string())
        })
}

/// A private `use` is visible to the descendants of its module (`super::Nome`, `use super::*`),
/// so the module lookups (re-export and glob chains) treat it like a `pub use`.
const IMPORT_PRIVADO_VALE: bool = true;

/// Every `use` leaf in the whole tree (fn bodies included — corpus: `use super::super::burst_gate`
/// inside a test fn), carrying the inline-module trail of its position.
fn coleta_usos(content: &str, node: Node, trail: &mut Vec<String>, m: &mut ModModel) {
    if node.kind() == "use_declaration" {
        let publico = {
            let mut c = node.walk();
            node.children(&mut c)
                .any(|ch| ch.kind() == "visibility_modifier")
        };
        if let Some(arg) = node.child_by_field_name("argument") {
            let linha = node.start_position().row + 1;
            for (segs, alias, glob) in folhas_de_uso(content, arg) {
                if segs.is_empty() {
                    continue;
                }
                if publico || IMPORT_PRIVADO_VALE {
                    if glob {
                        m.glob_reexports.push((trail.clone(), segs.clone()));
                    } else {
                        let visivel = alias
                            .clone()
                            .unwrap_or_else(|| segs.last().cloned().unwrap_or_default());
                        m.reexportacoes.push(Reexport {
                            trail: trail.clone(),
                            segs: segs.clone(),
                            visivel,
                        });
                    }
                }
                m.usos.push(UseNo {
                    trail: trail.clone(),
                    segs,
                    alias,
                    glob,
                    linha,
                });
            }
        }
        return;
    }
    if node.kind() == "mod_item"
        && node.child_by_field_name("body").is_some()
        && let Some(n) = node.child_by_field_name("name")
    {
        trail.push(nome_limpo(text(content, n)));
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.is_named() {
                coleta_usos(content, ch, trail, m);
            }
        }
        trail.pop();
        return;
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.is_named() {
            coleta_usos(content, ch, trail, m);
        }
    }
}

/// Flatten a `use` argument into leaf paths: `a::b::{c, d as e, f::g}` →
/// `[a::b::c]`, `[a::b::d as e]`, `[a::b::f::g]`; `{self}` imports the parent path itself.
fn folhas_de_uso(content: &str, node: Node) -> Vec<(Vec<String>, Option<String>, bool)> {
    fn go(
        content: &str,
        node: Node,
        prefixo: &mut Vec<String>,
        out: &mut Vec<(Vec<String>, Option<String>, bool)>,
    ) {
        match node.kind() {
            "use_list" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() {
                        go(content, ch, prefixo, out);
                    }
                }
            }
            "scoped_use_list" => {
                let base = node
                    .child_by_field_name("path")
                    .map(|p| segs_do_caminho(content, p))
                    .unwrap_or_default();
                let n0 = prefixo.len();
                prefixo.extend(base);
                if let Some(lista) = node.child_by_field_name("list") {
                    go(content, lista, prefixo, out);
                }
                prefixo.truncate(n0);
            }
            "use_as_clause" => {
                let mut segs = node
                    .child_by_field_name("path")
                    .map(|p| segs_do_caminho(content, p))
                    .unwrap_or_default();
                let alias = node
                    .child_by_field_name("alias")
                    .map(|a| nome_limpo(text(content, a)));
                let n0 = prefixo.len();
                prefixo.append(&mut segs);
                out.push((prefixo.clone(), alias, false));
                prefixo.truncate(n0);
            }
            "use_wildcard" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() {
                        prefixo.extend(segs_do_caminho(content, ch));
                    }
                }
                out.push((prefixo.clone(), None, true));
            }
            // `use a::b::{self}` — o módulo ele mesmo, ligado pelo último segmento do prefixo
            "self" => out.push((prefixo.clone(), None, false)),
            "metavariable" => {} // contexto de macro — abstenção (nunca deny)
            _ => {
                let mut segs = segs_do_caminho(content, node);
                if !segs.is_empty() {
                    let n0 = prefixo.len();
                    prefixo.append(&mut segs);
                    out.push((prefixo.clone(), None, false));
                    prefixo.truncate(n0);
                }
            }
        }
    }
    let mut out = Vec::new();
    go(content, node, &mut Vec::new(), &mut out);
    out
}

/// The segments of a path node (`a::b::c` → `[a, b, c]`); `crate`/`self`/`super`/`Self` keep
/// their keyword text — the resolver dispatches on it. Generics peel to their base (`S::<T>` → `S`).
fn segs_do_caminho(content: &str, node: Node) -> Vec<String> {
    match node.kind() {
        "identifier" | "type_identifier" | "self" | "super" | "crate" | "Self" => {
            vec![nome_limpo(text(content, node))]
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let mut v = node
                .child_by_field_name("path")
                .map(|p| segs_do_caminho(content, p))
                .unwrap_or_default();
            if let Some(n) = node.child_by_field_name("name") {
                v.push(nome_limpo(text(content, n)));
            }
            v
        }
        "generic_type" | "generic_type_with_turbofish" | "generic_function" => {
            let mut c = node.walk();
            node.children(&mut c)
                .find(|ch| ch.is_named())
                .map(|ch| segs_do_caminho(content, ch))
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// `fn` arity: (required params, max) — `None` max for variadic (`fn f(a: u8, ...)`).
fn aridade_de(fn_node: Node) -> (usize, Option<usize>) {
    let Some(ps) = fn_node.child_by_field_name("parameters") else {
        return (0, Some(0));
    };
    let mut n = 0;
    let mut variadico = false;
    let mut c = ps.walk();
    for ch in ps.children(&mut c) {
        match ch.kind() {
            "parameter" => n += 1,
            "variadic_parameter" => variadico = true,
            _ => {}
        }
    }
    (n, if variadico { None } else { Some(n) })
}

fn campos_declarados(content: &str, lista: Node) -> HashSet<String> {
    let mut s = HashSet::new();
    let mut c = lista.walk();
    for ch in lista.children(&mut c) {
        if ch.kind() == "field_declaration"
            && let Some(n) = ch.child_by_field_name("name")
        {
            s.insert(nome_limpo(text(content, n)));
        }
    }
    s
}

/// `#[non_exhaustive]` (or any attribute containing the needle) directly on the item.
fn tem_attr(content: &str, item: Node, agulha: &str) -> bool {
    atributos_do_item(content, item)
        .iter()
        .any(|a| a.contains(agulha))
}

// ============================================================================ resolver

/// What a resolved path lands on. `EnumCaminho` = the path hit an ENUM before the last segment
/// (`a::Falha::V`) — the caller judges the variant (the variants set is closed, so a missing
/// one is provable; methods and associated fns are not, hence abstained).
#[derive(Clone)]
enum Encontrado {
    Funcao { min: usize, max: Option<usize> },
    Struct(Rc<StructModel>),
    Enum(Rc<EnumModel>),
    EnumCaminho(Rc<EnumModel>, Vec<String>),
    Modulo { file: PathBuf, trail: Vec<String> },
    Outro,
    Externo,
}

enum Resolucao {
    Ok(Encontrado),
    /// A provably missing segment (the project target exists and does not have it).
    Inexistente(String),
    /// Cannot measure — fail-open, never a deny.
    Abstencao(Abst),
}

/// Why a resolution gave up: outside what the adapter claims to check (silent), or something
/// it tried and could not close (reported as `nao_medido`, with the reason).
#[derive(Clone, Copy)]
enum Abst {
    /// Associated items of a project struct live in impls anywhere — never claimed.
    PorDesenho,
    /// The adapter tried to resolve and could not.
    Falha(&'static str),
}

/// A path that starts at `crate`/an extern name needs the crate the file belongs to.
const SEM_CRATE: Abst = Abst::Falha("sem Cargo.toml de crate acima do arquivo (dentro do projeto)");

/// Where a path is being resolved from: the file, its filesystem module path (when known) and
/// the inline-module trail inside that file.
struct Local<'a> {
    arquivo: &'a Path,
    fs_modpath: Option<Vec<String>>,
    inline: Vec<String>,
}

/// The crate a file belongs to: the root file its `crate::` paths start from, the import name of
/// its PACKAGE and the root of that package's library. They differ for an integration test, an
/// example, a bench or `build.rs`: those are crates rooted at themselves, and `use <package>::X`
/// inside them names the package's LIBRARY (`src/lib.rs`), not the file being checked.
struct CrateInfo {
    root_file: PathBuf,
    nome: String,
    /// The package's library root (`[lib] path`, else `src/lib.rs`); `None` for a bin-only
    /// package (its tests cannot `use <package>::…`).
    lib_root: Option<PathBuf>,
}

/// Project model: overlays-first file reads, parsed-module cache, crate/workspace discovery.
/// The PROPOSED file's model is registered (`registrar_proposto`) so `self::` resolutions use
/// the proposed content even when the file does not exist on disk yet.
struct Resolver<'a> {
    overlays: &'a HashMap<String, String>,
    raiz: &'a Path,
    proposto: RefCell<Option<(PathBuf, Rc<ModModel>)>>,
    cache_modelo: RefCell<HashMap<PathBuf, Option<Rc<ModModel>>>>,
    cache_crate: RefCell<HashMap<PathBuf, Option<Rc<CrateInfo>>>>,
    cache_modpath: RefCell<HashMap<PathBuf, Option<Vec<String>>>>,
    ws: RefCell<Option<Rc<HashMap<String, PathBuf>>>>,
    deps: RefCell<HashMap<PathBuf, Rc<HashSet<String>>>>,
}

impl<'a> Resolver<'a> {
    fn new(raiz: &'a Path, overlays: &'a HashMap<String, String>) -> Self {
        Self {
            overlays,
            raiz,
            proposto: RefCell::new(None),
            cache_modelo: RefCell::new(HashMap::new()),
            cache_crate: RefCell::new(HashMap::new()),
            cache_modpath: RefCell::new(HashMap::new()),
            ws: RefCell::new(None),
            deps: RefCell::new(HashMap::new()),
        }
    }

    /// The content currently being checked answers for its own path (proposed or disk model).
    fn registrar_proposto(&self, path: &Path, modelo: Rc<ModModel>) {
        *self.proposto.borrow_mut() = Some((path.to_path_buf(), modelo));
    }

    /// Overlay content first (the entry's instant), then disk.
    ///
    /// V-4 (31/10/2026): only what is INSIDE the allowed roots is read. The proposed
    /// content chooses the path (`#[path = "…"]`), and an absolute path or a `../..`
    /// put the read outside the project.
    fn ler(&self, p: &Path) -> Option<String> {
        if !crate::vgp::lang::caminho::dentro_das_raizes(&[self.raiz], p) {
            return None;
        }
        if let Some(c) = self.overlays.get(p.to_string_lossy().as_ref()) {
            return Some(c.clone());
        }
        std::fs::read_to_string(p).ok()
    }

    fn modelo(&self, p: &Path) -> Option<Rc<ModModel>> {
        if let Some((pp, pm)) = self.proposto.borrow().as_ref()
            && pp == p
        {
            return Some(pm.clone());
        }
        if let Some(hit) = self.cache_modelo.borrow().get(p) {
            return hit.clone();
        }
        let m = (|| {
            let conteudo = self.ler(p)?;
            let tree = parse("rust", &conteudo).ok()?;
            if tree.root_node().has_error() {
                return None; // módulo ilegível/incompleto → abstenção, nunca inventa ausência
            }
            Some(Rc::new(extrair_modelo(&conteudo, tree.root_node())))
        })();
        self.cache_modelo
            .borrow_mut()
            .insert(p.to_path_buf(), m.clone());
        m
    }

    /// The crate owning `path`: first ancestor dir with a Cargo.toml that has `[package]`,
    /// bounded at the project root. Files under `<crate>/src` use the lib/bin root; integration
    /// tests, examples and `build.rs` are standalone crates rooted at themselves (rustc's rule).
    fn crate_de(&self, path: &Path) -> Option<Rc<CrateInfo>> {
        let dir_chave = path.parent()?.to_path_buf();
        if let Some(hit) = self.cache_crate.borrow().get(&dir_chave) {
            return hit.clone();
        }
        let r = (|| {
            for dir in path.ancestors() {
                if !dir.starts_with(self.raiz) && dir != self.raiz {
                    break; // nunca reivindica um manifesto acima do projeto
                }
                let manifest = dir.join("Cargo.toml");
                if !manifest.is_file() {
                    continue;
                }
                let conteudo = self.ler(&manifest)?;
                let valor: toml::Value = toml::from_str(&conteudo).ok()?;
                let Some(nome) = valor
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                else {
                    continue; // manifesto virtual (só [workspace]) — não é um crate
                };
                let src = dir.join("src");
                let lib_path = valor
                    .get("lib")
                    .and_then(|l| l.get("path"))
                    .and_then(|p| p.as_str())
                    .map(|p| dir.join(p));
                let lib_root = lib_path
                    .filter(|p| p.is_file())
                    .or_else(|| src.join("lib.rs").is_file().then(|| src.join("lib.rs")));
                let root_file = if path.starts_with(&src) {
                    lib_root
                        .clone()
                        .or_else(|| src.join("main.rs").is_file().then(|| src.join("main.rs")))?
                } else {
                    path.to_path_buf() // teste de integração / exemplo / build.rs: crate dele mesmo
                };
                let nome = valor
                    .get("lib")
                    .and_then(|l| l.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or(nome)
                    .replace('-', "_");
                return Some(Rc::new(CrateInfo {
                    root_file,
                    nome,
                    lib_root,
                }));
            }
            None
        })();
        self.cache_crate.borrow_mut().insert(dir_chave, r.clone());
        r
    }

    /// The crate dir of a CrateInfo (first ancestor of the root file with a Cargo.toml).
    fn dir_da_crate(&self, k: &CrateInfo) -> PathBuf {
        for dir in k.root_file.ancestors() {
            if dir.join("Cargo.toml").is_file() {
                return dir.to_path_buf();
            }
        }
        k.root_file
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf()
    }

    /// The crate root file of a workspace member dir (`[lib] path`, else `src/lib.rs` | `main.rs`).
    fn raiz_de_membro(&self, crate_dir: &Path) -> Option<PathBuf> {
        let conteudo = self.ler(&crate_dir.join("Cargo.toml"))?;
        let valor: toml::Value = toml::from_str(&conteudo).ok()?;
        let lib_path = valor
            .get("lib")
            .and_then(|l| l.get("path"))
            .and_then(|p| p.as_str())
            .map(|p| crate_dir.join(p));
        lib_path
            .filter(|p| p.is_file())
            .or_else(|| {
                let l = crate_dir.join("src/lib.rs");
                l.is_file().then_some(l)
            })
            .or_else(|| {
                let m = crate_dir.join("src/main.rs");
                m.is_file().then_some(m)
            })
    }

    /// Workspace members: crate import name → crate dir (from the ROOT manifest; globs like
    /// `crates/*` expand one level; the root package itself, when one exists, included; every
    /// dir bounded to the project root).
    fn workspace(&self) -> Rc<HashMap<String, PathBuf>> {
        if let Some(ws) = self.ws.borrow().as_ref() {
            return ws.clone();
        }
        let mut mapa = HashMap::new();
        if let Some(conteudo) = self.ler(&self.raiz.join("Cargo.toml"))
            && let Ok(valor) = toml::from_str::<toml::Value>(&conteudo)
        {
            let mut dirs: Vec<PathBuf> = Vec::new();
            if let Some(membros) = valor
                .get("workspace")
                .and_then(|w| w.get("members"))
                .and_then(|m| m.as_array())
            {
                for m in membros.iter().filter_map(|m| m.as_str()) {
                    if let Some(base) = m.strip_suffix("/*") {
                        if let Ok(rd) = std::fs::read_dir(self.raiz.join(base)) {
                            dirs.extend(
                                rd.flatten()
                                    .map(|e| e.path())
                                    .filter(|p| p.join("Cargo.toml").is_file()),
                            );
                        }
                    } else {
                        dirs.push(self.raiz.join(m));
                    }
                }
            }
            dirs.push(self.raiz.to_path_buf()); // pacote de raiz, se houver
            for dir in dirs {
                if !dir.starts_with(self.raiz) {
                    continue;
                }
                if let Some((nome, _)) = self.nome_do_manifesto(&dir.join("Cargo.toml")) {
                    mapa.insert(nome, dir);
                }
            }
        }
        let ws = Rc::new(mapa);
        *self.ws.borrow_mut() = Some(ws.clone());
        ws
    }

    /// `[package].name` (normalized) + `[lib] name` of a manifest, when it has `[package]`.
    fn nome_do_manifesto(&self, manifest: &Path) -> Option<(String, Option<String>)> {
        let conteudo = self.ler(manifest)?;
        let valor: toml::Value = toml::from_str(&conteudo).ok()?;
        let nome = valor
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())?
            .replace('-', "_");
        let lib = valor
            .get("lib")
            .and_then(|l| l.get("name"))
            .and_then(|n| n.as_str())
            .map(|n| n.replace('-', "_"));
        Some((lib.unwrap_or_else(|| nome.clone()), Some(nome)))
    }

    /// Dependency names a crate may import (every deps table, target-specific included; the
    /// KEY is the `use` name, renames included — `package = "…"` never changes the key).
    fn deps_de(&self, crate_dir: &Path) -> Rc<HashSet<String>> {
        if let Some(d) = self.deps.borrow().get(crate_dir) {
            return d.clone();
        }
        let mut nomes = HashSet::new();
        if let Some(conteudo) = self.ler(&crate_dir.join("Cargo.toml"))
            && let Ok(valor) = toml::from_str::<toml::Value>(&conteudo)
        {
            let coleta = |t: &toml::value::Table, nomes: &mut HashSet<String>| {
                for secao in ["dependencies", "dev-dependencies", "build-dependencies"] {
                    if let Some(deps) = t.get(secao).and_then(|d| d.as_table()) {
                        nomes.extend(deps.keys().map(|k| k.replace('-', "_")));
                    }
                }
            };
            if let Some(t) = valor.as_table() {
                coleta(t, &mut nomes);
                if let Some(alvo) = t.get("target").and_then(|t| t.as_table()) {
                    for sub in alvo.values() {
                        if let Some(sub) = sub.as_table() {
                            coleta(sub, &mut nomes);
                        }
                    }
                }
            }
        }
        let d = Rc::new(nomes);
        self.deps
            .borrow_mut()
            .insert(crate_dir.to_path_buf(), d.clone());
        d
    }

    /// The module path of `path` inside its crate, from the FILE LAYOUT (the Rust Reference
    /// convention; a file displaced by `#[path]`/`include!` could mislead `super::` — accepted,
    /// measured zero in corpus_rust; the alternative — verifying `mod` declarations — loses
    /// files that no longer exist on disk, like corpus snaps do).
    ///
    /// The layout is confirmed against the declarations: a file the layout does not reach but a
    /// neighbour declares with `#[path = "…"]` (the `foo_tests.rs` of `foo.rs`) takes the module
    /// path of its declaration, so `super::` inside it climbs to `foo`, not to the directory.
    /// A file nobody declares (new, or gone from the disk) keeps the layout path.
    fn modpath_de(&self, path: &Path) -> Option<Vec<String>> {
        self.modpath_com_prof(path, 0)
    }

    fn modpath_com_prof(&self, path: &Path, prof: usize) -> Option<Vec<String>> {
        if let Some(hit) = self.cache_modpath.borrow().get(path) {
            return hit.clone();
        }
        let r = self.modpath_do_layout(path).map(|layout| {
            self.modpath_declarado(path, &layout, prof)
                .unwrap_or(layout)
        });
        self.cache_modpath
            .borrow_mut()
            .insert(path.to_path_buf(), r.clone());
        r
    }

    /// The module path from a neighbour's `#[path]` declaration — only when the layout path does
    /// NOT lead back to `path` (a confirmed layout, or the crate root, needs no search).
    fn modpath_declarado(
        &self,
        path: &Path,
        layout: &[String],
        prof: usize,
    ) -> Option<Vec<String>> {
        if layout.is_empty() || prof > 3 {
            return None;
        }
        let krate = self.crate_de(path)?;
        if let Some((f, t)) = self.localizar_modulo(&krate, layout)
            && t.is_empty()
            && f == path
        {
            return None;
        }
        let nome = path.file_name()?.to_str()?;
        for cand in vizinhos_rs(path) {
            // só o texto que cita o arquivo vale um parse
            if !self.ler(&cand).is_some_and(|c| c.contains(nome)) {
                continue;
            }
            let Some(seg) = self.nome_do_mod_que_aponta(&cand, path) else {
                continue;
            };
            let mut base = self.modpath_com_prof(&cand, prof + 1)?;
            base.push(seg);
            return Some(base);
        }
        None
    }

    /// The name of the `#[path]`-declared `mod` in `declarante` whose file is `alvo`.
    fn nome_do_mod_que_aponta(&self, declarante: &Path, alvo: &Path) -> Option<String> {
        let modelo = self.modelo(declarante)?;
        let mut decls: Vec<(&String, &ModDecl)> = modelo
            .mods_declarados
            .iter()
            .filter(|(_, d)| d.path_attr.is_some())
            .collect();
        decls.sort_by(|a, b| a.0.cmp(b.0));
        decls
            .into_iter()
            .find(|(seg, d)| {
                self.filho_de(declarante, &[], seg, d.path_attr.as_deref())
                    .as_deref()
                    == Some(alvo)
            })
            .map(|(seg, _)| seg.clone())
    }

    /// The module path of `path` from the file layout alone.
    fn modpath_do_layout(&self, path: &Path) -> Option<Vec<String>> {
        let krate = self.crate_de(path)?;
        let src_root = krate.root_file.parent()?.to_path_buf();
        if path == krate.root_file {
            return Some(vec![]);
        }
        let rel = path.strip_prefix(&src_root).ok()?.to_path_buf();
        let mut segs: Vec<String> = rel
            .with_extension("")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if matches!(
            segs.last().map(String::as_str),
            Some("mod" | "lib" | "main")
        ) {
            segs.pop();
        }
        Some(segs)
    }

    /// The base dir of module children of (file, trail): `mod.rs`/`lib.rs`/`main.rs` →
    /// `<dir>/<trail>`; any other `foo.rs` → `<dir>/foo/<trail>` (the Reference's rule).
    fn dir_de_modulos(&self, file: &Path, trail: &[String]) -> Option<PathBuf> {
        let dir = file.parent()?;
        let nome = file.file_name().map(|n| n.to_string_lossy());
        let mut base = if matches!(nome.as_deref(), Some("mod.rs" | "lib.rs" | "main.rs")) {
            dir.to_path_buf()
        } else {
            dir.join(file.file_stem().unwrap_or_default())
        };
        for t in trail {
            base = base.join(t);
        }
        Some(base)
    }

    /// `mod seg` child file candidates: the `#[path]` override first (it is the only candidate
    /// then), else `<seg>.rs` | `<seg>/mod.rs` under the module-children dir. A `#[path]` outside
    /// an inline block is relative to the directory of the SOURCE FILE itself — also for a
    /// non-`mod-rs` `foo.rs`, whose ordinary children live under `foo/` (the Reference's
    /// "Module Source Filenames").
    fn candidatos_de(
        &self,
        file: &Path,
        trail: &[String],
        seg: &str,
        path_attr: Option<&str>,
    ) -> Vec<PathBuf> {
        let Some(base) = self.dir_de_modulos(file, trail) else {
            return Vec::new();
        };
        if let Some(p) = path_attr {
            // V-4 (31/10/2026): an ABSOLUTE `#[path]` does not concatenate — in the
            // `Path::join` it REPLACES the base, so the CONTENT would choose the file
            // to read (`#[path = "/etc/passwd"]`). Only a relative piece is accepted,
            // and the file it names must stay inside the project root.
            if !crate::vgp::lang::caminho::derivado_aceitavel(p) {
                return Vec::new();
            }
            let relativo_a = if trail.is_empty() {
                file.parent().map(Path::to_path_buf)
            } else {
                Some(base)
            };
            return relativo_a
                .map(|d| vec![d.join(p)])
                .unwrap_or_default()
                .into_iter()
                .filter(|c| crate::vgp::lang::caminho::dentro_das_raizes(&[self.raiz], c))
                .collect();
        }
        vec![
            base.join(format!("{seg}.rs")),
            base.join(seg).join("mod.rs"),
        ]
    }

    /// The first existing candidate (overlay counts as existing).
    fn filho_de(
        &self,
        file: &Path,
        trail: &[String],
        seg: &str,
        path_attr: Option<&str>,
    ) -> Option<PathBuf> {
        self.candidatos_de(file, trail, seg, path_attr)
            .into_iter()
            .find(|c| self.ler(c).is_some() || self.e_o_proposto(c))
    }

    /// The file being checked answers for its own path even when it is not on disk yet.
    fn e_o_proposto(&self, p: &Path) -> bool {
        self.proposto
            .borrow()
            .as_ref()
            .is_some_and(|(pp, _)| pp == p)
    }

    /// The model of the module at (file, inline trail).
    fn model_at(&self, file: &Path, trail: &[String]) -> Option<Rc<ModModel>> {
        let mut m = self.modelo(file)?;
        for t in trail {
            m = m.inline.get(t)?.clone();
        }
        Some(m)
    }

    /// (file, inline trail) of a full module path inside `krate`, descending declarations.
    fn localizar_modulo(
        &self,
        krate: &CrateInfo,
        modpath: &[String],
    ) -> Option<(PathBuf, Vec<String>)> {
        let mut file = krate.root_file.clone();
        let mut trail: Vec<String> = Vec::new();
        for seg in modpath {
            let sub = self.model_at(&file, &trail)?;
            if let Some(decl) = sub.mods_declarados.get(seg) {
                file = self.filho_de(&file, &trail, seg, decl.path_attr.as_deref())?;
                trail = Vec::new();
            } else if sub.inline.contains_key(seg) {
                trail.push(seg.clone());
            } else {
                return None;
            }
        }
        Some((file, trail))
    }

    /// Resolve a path from `local`. `prof` bounds re-export chains; `visitados` kills cycles.
    fn resolver_caminho(
        &self,
        local: &Local,
        segs: &[String],
        prof: usize,
        visitados: &mut HashSet<(PathBuf, String)>,
    ) -> Resolucao {
        if segs.is_empty() || prof > 8 {
            return Resolucao::Abstencao(Abst::Falha("cadeia de reexportação funda demais"));
        }
        let krate = self.crate_de(local.arquivo);
        let head = segs[0].as_str();
        // uniform paths (edição 2018+): uma cabeça nua que é item do módulo corrente resolve
        // como `self::` — `pub use lifecycle::*` em decompose.rs é o caso medido no touring
        let head_local = !matches!(head, "crate" | "self" | "super" | "std" | "core" | "alloc")
            && (self
                .model_at(local.arquivo, &local.inline)
                .is_some_and(|m| {
                    m.mods_declarados.contains_key(head)
                        || m.inline.contains_key(head)
                        || m.enums.contains_key(head)
                        || m.structs.contains_key(head)
                        || m.itens.contains(head)
                })
                // reexport na posição de `local` (um `mod x { pub use … }` inline incluído); um
                // `use` cujo caminho COMEÇA pelo nome que ele introduz (`pub use dep;`,
                // `use rstest::rstest;`) liga o crate externo, não um item local — descer por ele
                // giraria sobre si mesmo
                || self.modelo(local.arquivo).is_some_and(|raiz| {
                    raiz.reexportacoes.iter().any(|r| {
                        r.trail == local.inline
                            && r.visivel == head
                            && r.segs.first().is_some_and(|s| s != head)
                    })
                }));
        let (file, trail, i): (PathBuf, Vec<String>, usize) = if head_local {
            (local.arquivo.to_path_buf(), local.inline.clone(), 0)
        } else {
            match head {
                "crate" => match &krate {
                    Some(k) => (k.root_file.clone(), Vec::new(), 1),
                    None => return Resolucao::Abstencao(SEM_CRATE),
                },
                "self" => (local.arquivo.to_path_buf(), local.inline.clone(), 1),
                "super" => {
                    let k = segs.iter().take_while(|s| s.as_str() == "super").count();
                    let Some(fs) = &local.fs_modpath else {
                        return Resolucao::Abstencao(Abst::Falha(
                            "posição do arquivo na árvore de módulos desconhecida",
                        ));
                    };
                    let mut efetivo = fs.clone();
                    efetivo.extend(local.inline.iter().cloned());
                    if k > efetivo.len() {
                        // super acima da raiz — o rustc julga
                        return Resolucao::Abstencao(Abst::Falha("`super` acima da raiz do crate"));
                    }
                    efetivo.truncate(efetivo.len() - k);
                    if efetivo.len() >= fs.len() {
                        (local.arquivo.to_path_buf(), efetivo[fs.len()..].to_vec(), k)
                    } else {
                        let Some(k) = &krate else {
                            return Resolucao::Abstencao(SEM_CRATE);
                        };
                        match self.localizar_modulo(k, &efetivo) {
                            Some((f, t)) => (
                                f,
                                t,
                                segs.iter().take_while(|s| s.as_str() == "super").count(),
                            ),
                            None => {
                                return Resolucao::Abstencao(Abst::Falha(
                                    "módulo pai ilegível ou fora da árvore declarada",
                                ));
                            }
                        }
                    }
                }
                "std" | "core" | "alloc" => return Resolucao::Ok(Encontrado::Externo),
                cabeca => {
                    let Some(k) = &krate else {
                        return Resolucao::Abstencao(SEM_CRATE);
                    };
                    if cabeca == k.nome {
                        match &k.lib_root {
                            Some(lib) => (lib.clone(), Vec::new(), 1),
                            None => {
                                return Resolucao::Abstencao(Abst::Falha(
                                    "pacote sem biblioteca (`src/lib.rs`) para `use <pacote>::…`",
                                ));
                            }
                        }
                    } else if let Some(crate_dir) = self.workspace().get(cabeca).cloned() {
                        match self.raiz_de_membro(&crate_dir) {
                            Some(root) => (root, Vec::new(), 1),
                            None => {
                                return Resolucao::Abstencao(Abst::Falha(
                                    "raiz do crate membro do workspace ilegível",
                                ));
                            }
                        }
                    } else if self.deps_de(&self.dir_da_crate(k)).contains(cabeca) {
                        return Resolucao::Ok(Encontrado::Externo);
                    } else {
                        return Resolucao::Inexistente(cabeca.to_string());
                    }
                }
            }
        };
        // ---------- descida pelos segmentos
        self.descer(file, trail, &segs[i..], prof, visitados)
    }

    /// Descend `resto` from the module (file, trail): declared and inline modules, the module's
    /// items, its explicit re-exports and — through [`Self::descer`] itself — the globs it
    /// re-exports, so a chain `pub use a::*; … pub use b::nome;` of any depth is followed.
    fn descer(
        &self,
        mut file: PathBuf,
        mut trail: Vec<String>,
        resto: &[String],
        prof: usize,
        visitados: &mut HashSet<(PathBuf, String)>,
    ) -> Resolucao {
        if prof > 8 {
            return Resolucao::Abstencao(Abst::Falha("cadeia de reexportação funda demais"));
        }
        if resto.is_empty() {
            // `self` / `super` / `crate` / `<crate>` sem item depois: o próprio módulo (o alvo de
            // `use super::*;` num `mod tests`)
            return Resolucao::Ok(Encontrado::Modulo { file, trail });
        }
        for (idx, seg) in resto.iter().enumerate() {
            let ultimo = idx + 1 == resto.len();
            let Some(model) = self.model_at(&file, &trail) else {
                return Resolucao::Abstencao(Abst::Falha(
                    "módulo do projeto ilegível ou com erro de sintaxe",
                ));
            };
            if let Some(decl) = model.mods_declarados.get(seg) {
                match self.filho_de(&file, &trail, seg, decl.path_attr.as_deref()) {
                    Some(f) => {
                        if ultimo {
                            return Resolucao::Ok(Encontrado::Modulo {
                                file: f,
                                trail: Vec::new(),
                            });
                        }
                        file = f;
                        trail = Vec::new();
                        continue;
                    }
                    None => return Resolucao::Inexistente(seg.clone()),
                }
            }
            if model.inline.contains_key(seg) {
                if ultimo {
                    return Resolucao::Ok(Encontrado::Modulo {
                        file: file.clone(),
                        trail: empurra_seg(&trail, seg),
                    });
                }
                trail.push(seg.clone());
                continue;
            }
            if ultimo {
                if let Some((min, max)) = model.funcoes.get(seg) {
                    return Resolucao::Ok(Encontrado::Funcao {
                        min: *min,
                        max: *max,
                    });
                }
                if let Some(s) = model.structs.get(seg) {
                    return Resolucao::Ok(Encontrado::Struct(s.clone()));
                }
                if let Some(e) = model.enums.get(seg) {
                    return Resolucao::Ok(Encontrado::Enum(e.clone()));
                }
                if model.itens.contains(seg) {
                    return Resolucao::Ok(Encontrado::Outro);
                }
            } else {
                if let Some(e) = model.enums.get(seg) {
                    return Resolucao::Ok(Encontrado::EnumCaminho(
                        e.clone(),
                        resto[idx + 1..].to_vec(),
                    ));
                }
                if model.structs.contains_key(seg) || model.itens.contains(seg) {
                    // `a::Struct::associada` (também unit/tupla, trait, alias, união) — os
                    // impls vivem em qualquer arquivo: abstenção
                    return Resolucao::Abstencao(Abst::PorDesenho);
                }
            }
            // reexportações explícitas (`pub use …::seg[ as seg]`) — a declaração vence. Elas vêm
            // do modelo do ARQUIVO, filtradas pela posição: o submodelo de um `mod x { … }`
            // inline guarda só itens
            let raiz = self.modelo(&file);
            let mut reexport_aberto: Option<Abst> = None;
            let reexports = raiz
                .iter()
                .flat_map(|m| m.reexportacoes.iter())
                .filter(|r| r.trail == trail);
            for r in reexports {
                if &r.visivel != seg {
                    continue;
                }
                if !visitados.insert((file.clone(), seg.clone())) {
                    return Resolucao::Abstencao(Abst::Falha("ciclo de reexportação (`pub use`)"));
                }
                let Some(base) = self.modpath_de(&file) else {
                    return Resolucao::Abstencao(Abst::Falha(
                        "posição do arquivo na árvore de módulos desconhecida",
                    ));
                };
                let rlocal = Local {
                    arquivo: &file,
                    fs_modpath: Some(base),
                    inline: r.trail.clone(),
                };
                match self.resolver_caminho(&rlocal, &r.segs, prof + 1, visitados) {
                    // não fechou: ainda restam os globs — e, se nenhum trouxer o nome, o reexport
                    // que o declara continua sendo a razão de a ausência não estar provada
                    Resolucao::Abstencao(a) => {
                        reexport_aberto.get_or_insert(a);
                    }
                    // o segmento é um MÓDULO alcançado por reexport (`pub use a::b as c;`): o
                    // resto do caminho segue DENTRO dele
                    Resolucao::Ok(Encontrado::Modulo { file: f, trail: t }) if !ultimo => {
                        return self.descer(f, t, &resto[idx + 1..], prof + 1, visitados);
                    }
                    Resolucao::Ok(Encontrado::Enum(e)) if !ultimo => {
                        return Resolucao::Ok(Encontrado::EnumCaminho(
                            e,
                            resto[idx + 1..].to_vec(),
                        ));
                    }
                    Resolucao::Ok(Encontrado::Externo) => {
                        return Resolucao::Ok(Encontrado::Externo);
                    }
                    // `Struct::assoc` / `fn::x` por um reexport: os impls não são visíveis
                    Resolucao::Ok(_) if !ultimo => {
                        return Resolucao::Abstencao(Abst::PorDesenho);
                    }
                    achado => return achado,
                }
            }
            // globs reexportados (`pub use alvo::*`): o nome pode vir de qualquer um dos alvos —
            // `resto[idx..]` é procurado DENTRO de cada alvo, que segue os próprios reexports e
            // globs (cadeia de qualquer profundidade; o ciclo é cortado em `visitados`)
            let mut algum_aberto = false;
            let globs: Vec<(Vec<String>, Vec<String>)> = raiz
                .iter()
                .flat_map(|m| m.glob_reexports.iter())
                .filter(|(gt, _)| *gt == trail)
                .cloned()
                .collect();
            for (gtrail, gsegs) in globs {
                let Some(base) = self.modpath_de(&file) else {
                    algum_aberto = true;
                    continue;
                };
                let glocal = Local {
                    arquivo: &file,
                    fs_modpath: Some(base),
                    inline: gtrail,
                };
                match self.resolver_caminho(&glocal, &gsegs, prof + 1, visitados) {
                    Resolucao::Ok(Encontrado::Modulo {
                        file: gf,
                        trail: gt,
                    }) => {
                        let chave = (gf.clone(), format!("*{}", resto[idx..].join("::")));
                        if !visitados.insert(chave) {
                            continue; // este alvo já foi consultado para este caminho (ciclo)
                        }
                        match self.descer(gf, gt, &resto[idx..], prof + 1, visitados) {
                            Resolucao::Inexistente(faltou) if faltou == *seg => {} // o alvo não o traz
                            Resolucao::Abstencao(_) => algum_aberto = true,
                            achado => return achado,
                        }
                    }
                    // glob de crate externo, módulo ilegível…: o nome pode vir dele
                    _ => algum_aberto = true,
                }
            }
            if algum_aberto {
                return Resolucao::Abstencao(Abst::Falha(
                    "`pub use alvo::*` com alvo irresolúvel pode trazer o nome",
                ));
            }
            // a ausência só se afirma com o módulo inteiro enumerável: uma macro em nível de item
            // pode declarar o nome
            if model.macro_item {
                return Resolucao::Abstencao(Abst::Falha(
                    "módulo com macro em nível de item pode declarar o nome",
                ));
            }
            if let Some(a) = reexport_aberto {
                return Resolucao::Abstencao(a);
            }
            return Resolucao::Inexistente(seg.clone());
        }
        Resolucao::Ok(Encontrado::Outro)
    }
}

// ============================================================================ caminhante

/// Bindings collected for the scope check: bare names and the typed targets path heads resolve
/// to (local items + resolved imports). Over-binding is the fail-open direction; under-binding
/// is what invents findings, so collection is generous and position-blind.
struct Vinculos {
    nomes: HashSet<String>,
    alvos: HashMap<String, Encontrado>,
    glob_externo: bool,
}

/// The walk state: resolution context, scope stack and the findings being assembled.
struct Ctx<'a> {
    path: &'a str,
    content: &'a str,
    res: &'a Resolver<'a>,
    modelo: Rc<ModModel>,
    arquivo: PathBuf,
    /// Whom a missing project symbol is about in this file ([`alvo_do_projeto`]).
    alvo: Alvo,
    modpath: Option<Vec<String>>,
    vinculos: Vinculos,
    bindings: Vec<HashSet<String>>,
    mod_stack: Vec<String>,
    /// Names bound inside each inline `mod x { … }` (by its path from the file root): besides the
    /// module's own items, only these are visible there — the file root's names are not.
    nomes_do_modulo: HashMap<Vec<String>, HashSet<String>>,
    /// The module position of the `use` leaf being bound (empty = the file root).
    destino: Vec<String>,
    impl_stack: Vec<Option<ImplAlvo>>,
    out: Vec<Finding>,
    /// Resolutions attempted in this content and not closed (→ `nao_medido`), deduplicated.
    abstencoes: Vec<String>,
}

#[derive(Clone)]
enum ImplAlvo {
    Enum(Rc<EnumModel>),
    Struct(Rc<StructModel>),
}

impl<'a> Ctx<'a> {
    fn new(
        path: &'a str,
        content: &'a str,
        res: &'a Resolver<'a>,
        modelo: Rc<ModModel>,
        alvo: Alvo,
    ) -> Self {
        let arquivo = PathBuf::from(path);
        let modpath = if crate::vgp::is_inline(path) {
            None
        } else {
            res.modpath_de(&arquivo)
        };
        let mut vinculos = Vinculos {
            nomes: modelo.itens.clone(),
            alvos: HashMap::new(),
            glob_externo: false,
        };
        // alvos locais: itens tipados do próprio arquivo + módulos declarados e inline
        for (nome, (min, max)) in &modelo.funcoes {
            vinculos.alvos.insert(
                nome.clone(),
                Encontrado::Funcao {
                    min: *min,
                    max: *max,
                },
            );
        }
        for (nome, s) in &modelo.structs {
            vinculos
                .alvos
                .insert(nome.clone(), Encontrado::Struct(s.clone()));
        }
        for (nome, e) in &modelo.enums {
            vinculos
                .alvos
                .insert(nome.clone(), Encontrado::Enum(e.clone()));
        }
        if !crate::vgp::is_inline(path) {
            for (nome, decl) in &modelo.mods_declarados {
                if let Some(f) = res.filho_de(&arquivo, &[], nome, decl.path_attr.as_deref()) {
                    vinculos.alvos.insert(
                        nome.clone(),
                        Encontrado::Modulo {
                            file: f,
                            trail: Vec::new(),
                        },
                    );
                }
            }
            for nome in modelo.inline.keys() {
                vinculos.alvos.insert(
                    nome.clone(),
                    Encontrado::Modulo {
                        file: arquivo.clone(),
                        trail: vec![nome.clone()],
                    },
                );
            }
        }
        Self {
            path,
            content,
            res,
            modelo,
            arquivo,
            alvo,
            modpath,
            vinculos,
            bindings: vec![preludio()],
            mod_stack: Vec::new(),
            nomes_do_modulo: HashMap::new(),
            destino: Vec::new(),
            impl_stack: Vec::new(),
            out: Vec::new(),
            abstencoes: Vec::new(),
        }
    }

    fn dinamismo(&self) -> bool {
        self.modelo.macro_item || self.vinculos.glob_externo || self.macro_no_modulo_atual()
    }

    /// The model of the inline module the walk is inside (`mod_stack` is its path from the file
    /// root, deepest last); `None` when the module is nested too deep to have been modelled.
    fn modelo_atual(&self) -> Option<Rc<ModModel>> {
        let mut atual = Some(self.modelo.clone());
        for nome in &self.mod_stack {
            atual = atual.and_then(|m| m.inline.get(nome).cloned());
        }
        atual
    }

    /// An item-level macro in the inline module the walk is inside may declare the name — and a
    /// module that was not modelled cannot say what it declares, so it never proves absence.
    fn macro_no_modulo_atual(&self) -> bool {
        !self.mod_stack.is_empty() && self.modelo_atual().is_none_or(|m| m.macro_item)
    }

    /// Bind `nome` where the `use` leaf being processed sits: the file root or an inline module.
    fn liga(&mut self, nome: String) {
        if self.destino.is_empty() {
            self.vinculos.nomes.insert(nome);
        } else {
            self.nomes_do_modulo
                .entry(self.destino.clone())
                .or_default()
                .insert(nome);
        }
    }

    /// Is `nome` bound by a `use` (or glob) at the module `trail`? The file root's names do not
    /// reach an inline module, and an inline module's names do not reach the root.
    fn ligado_em(&self, trail: &[String], nome: &str) -> bool {
        if trail.is_empty() {
            self.vinculos.nomes.contains(nome)
        } else {
            self.nomes_do_modulo
                .get(trail)
                .is_some_and(|s| s.contains(nome))
        }
    }

    /// A resolution that gave up: a deliberate limit stays silent, a failed attempt is
    /// remembered (once per path) for the `nao_medido` reason.
    fn abstem(&mut self, abst: Abst, caminho: &[String]) {
        let Abst::Falha(porque) = abst else {
            return;
        };
        let item = format!("`{}` ({porque})", caminho.join("::"));
        if !self.abstencoes.contains(&item) {
            self.abstencoes.push(item);
        }
    }

    fn local(&self, trail: &[String]) -> Local<'_> {
        Local {
            arquivo: &self.arquivo,
            fs_modpath: self.modpath.clone(),
            inline: trail.to_vec(),
        }
    }

    fn achou_nome(&mut self, nome: &str, linha: usize) {
        if self.dinamismo() {
            // o nome pode vir de uma macro de item ou de um `use *` externo: não se afirma que
            // falta — o adaptador tentou e não decide (`nao_medido`), nunca um achado
            self.abstem(
                Abst::Falha(
                    "nome possivelmente dinâmico (macro em nível de item / `use *` externo)",
                ),
                &[nome.to_string()],
            );
        } else {
            empurrar(
                &mut self.out,
                "nome",
                nome,
                linha,
                self.alvo,
                "nome usado sem definição em nenhum escopo",
                Vec::new(),
            );
        }
    }

    // ------------------------------------------------------------------ imports

    /// Check every `use` leaf and `mod x;` declaration; fill the bindings they bring.
    fn checar_imports(&mut self) {
        // os globs primeiro: uma folha `use Cor::{Rubra}` pode ter a cabeça trazida por eles
        let mut usos = self.modelo.usos.clone();
        usos.sort_by_key(|u| !u.glob);
        for uso in &usos {
            self.destino.clone_from(&uso.trail);
            let local = self.local(&uso.trail);
            let mut visitados = HashSet::new();
            let r = self
                .res
                .resolver_caminho(&local, &uso.segs, 0, &mut visitados);
            if uso.glob {
                match r {
                    Resolucao::Ok(Encontrado::Modulo { file, trail }) => {
                        self.mesclar_glob(&file, &trail);
                    }
                    Resolucao::Ok(Encontrado::Enum(e)) => {
                        // `use Falha::*` — variantes entram no escopo (uniform paths)
                        for v in &e.variantes {
                            self.liga(v.clone());
                        }
                    }
                    Resolucao::Ok(Encontrado::Externo) => self.vinculos.glob_externo = true,
                    Resolucao::Inexistente(seg) => {
                        if self.cabeca_do_uso_ligada(&seg, uso) {
                            self.vinculos.glob_externo = true; // `use Cor::*` de um nome já em escopo
                        } else {
                            self.falha_import(&seg, uso.linha, &uso.segs);
                        }
                    }
                    Resolucao::Abstencao(abst) => {
                        self.vinculos.glob_externo = true;
                        self.abstem(abst, &uso.segs);
                    }
                    Resolucao::Ok(_) => self.vinculos.glob_externo = true,
                }
                continue;
            }
            match r {
                Resolucao::Ok(Encontrado::EnumCaminho(e, resto)) => {
                    // `use a::Falha::Rede` — import de variante
                    if resto.len() == 1 && e.variantes.contains(&resto[0]) {
                        self.liga(resto[0].clone());
                    } else if resto.len() == 1 {
                        self.falha_import(&resto[0], uso.linha, &uso.segs);
                    }
                }
                Resolucao::Ok(enc) => {
                    let nome = uso
                        .alias
                        .clone()
                        .unwrap_or_else(|| uso.segs.last().cloned().unwrap_or_default());
                    self.liga(nome.clone());
                    self.vinculos.alvos.insert(nome, enc);
                }
                Resolucao::Inexistente(seg) => {
                    if self.cabeca_do_uso_ligada(&seg, uso) {
                        let nome = uso
                            .alias
                            .clone()
                            .unwrap_or_else(|| uso.segs.last().cloned().unwrap_or_default());
                        self.liga(nome);
                    } else {
                        self.falha_import(&seg, uso.linha, &uso.segs);
                    }
                }
                Resolucao::Abstencao(abst) => {
                    // irresolúvel: o nome textual continua ligado (fail-open), sem alvo tipado
                    let nome = uso
                        .alias
                        .clone()
                        .unwrap_or_else(|| uso.segs.last().cloned().unwrap_or_default());
                    self.liga(nome);
                    self.abstem(abst, &uso.segs);
                }
            }
        }
        self.destino.clear();
        // `mod x;` sem arquivo (nem overlay, nem o próprio proposto) — E0432
        if !crate::vgp::is_inline(self.path) {
            let mut pendentes: Vec<(String, usize, Vec<String>, Option<String>)> = Vec::new();
            let mut pilha: Vec<(&ModModel, Vec<String>)> = vec![(self.modelo.as_ref(), Vec::new())];
            while let Some((m, trail)) = pilha.pop() {
                for (seg, decl) in &m.mods_declarados {
                    pendentes.push((
                        seg.clone(),
                        decl.linha,
                        trail.clone(),
                        decl.path_attr.clone(),
                    ));
                }
                for (nome, sub) in &m.inline {
                    let mut t = trail.clone();
                    t.push(nome.clone());
                    pilha.push((sub.as_ref(), t));
                }
            }
            for (seg, linha, trail, path_attr) in pendentes {
                let candidatos =
                    self.res
                        .candidatos_de(&self.arquivo, &trail, &seg, path_attr.as_deref());
                let existe = candidatos
                    .iter()
                    .any(|c| c == &self.arquivo || self.res.ler(c).is_some());
                if !existe {
                    empurrar(
                        &mut self.out,
                        "import",
                        &seg,
                        linha,
                        self.alvo,
                        &format!(
                            "módulo `{seg}` declarado sem arquivo (`{seg}.rs` | `{seg}/mod.rs`)"
                        ),
                        Vec::new(),
                    );
                }
            }
        }
    }

    /// `use Cor::{Rubra, Verde};` in a fn body or a `mod tests` where `Cor` reached the scope
    /// through a glob or another `use`: with uniform paths the head is looked up among the names
    /// in scope, not only among the crate's own and the extern prelude. When it is bound, the
    /// leaf is bound too and nothing is claimed about it (the head's members are not enumerated
    /// here). `seg` = the first segment the resolver could not place.
    fn cabeca_do_uso_ligada(&self, seg: &str, uso: &UseNo) -> bool {
        uso.segs.first().is_some_and(|c| c == seg) && self.ligado_em(&uso.trail, seg)
    }

    /// `use path::*` of a project module: every addressable name joins the scope — the module's
    /// own items, what its `use` leaves bind (the private ones too: a glob taken from a
    /// descendant, the `use super::*;` of a `mod tests`, sees them) and, transitively, what its
    /// own globs bring. A glob that cannot be enumerated (external crate, unreadable module,
    /// item-level macro) opens the scope instead: every unbound name becomes possibly dynamic.
    fn mesclar_glob(&mut self, file: &Path, trail: &[String]) {
        let mut vistos = HashSet::new();
        self.mesclar_glob_rec(file, trail, 0, &mut vistos);
    }

    fn mesclar_glob_rec(
        &mut self,
        file: &Path,
        trail: &[String],
        prof: usize,
        vistos: &mut HashSet<(PathBuf, Vec<String>)>,
    ) {
        if !vistos.insert((file.to_path_buf(), trail.to_vec())) {
            return; // ciclo (`a` re-exporta `b`, `b` re-exporta `a`): os nomes já entraram
        }
        let Some(alvo) = self.res.model_at(file, trail) else {
            self.vinculos.glob_externo = true;
            return;
        };
        if prof > 8 {
            self.vinculos.glob_externo = true; // cadeia funda demais para enumerar
            return;
        }
        if alvo.macro_item {
            self.vinculos.glob_externo = true; // uma macro de item pode declarar nomes
        }
        for nome in &alvo.itens {
            self.liga(nome.clone());
        }
        for (nome, (min, max)) in &alvo.funcoes {
            self.vinculos.alvos.insert(
                nome.clone(),
                Encontrado::Funcao {
                    min: *min,
                    max: *max,
                },
            );
        }
        for (nome, s) in &alvo.structs {
            self.vinculos
                .alvos
                .insert(nome.clone(), Encontrado::Struct(s.clone()));
        }
        for (nome, e) in &alvo.enums {
            self.vinculos
                .alvos
                .insert(nome.clone(), Encontrado::Enum(e.clone()));
        }
        for r in &alvo.reexportacoes {
            self.liga(r.visivel.clone());
        }
        for (nome, decl) in &alvo.mods_declarados {
            self.liga(nome.clone());
            if let Some(f) = self
                .res
                .filho_de(file, trail, nome, decl.path_attr.as_deref())
            {
                self.vinculos.alvos.insert(
                    nome.clone(),
                    Encontrado::Modulo {
                        file: f,
                        trail: Vec::new(),
                    },
                );
            }
        }
        for nome in alvo.inline.keys() {
            self.liga(nome.clone());
            self.vinculos.alvos.insert(
                nome.clone(),
                Encontrado::Modulo {
                    file: file.to_path_buf(),
                    trail: empurra_seg(trail, nome),
                },
            );
        }
        self.mesclar_usos_do_alvo(file, trail, prof, vistos);
    }

    /// The `use` leaves that sit in the glob target's own module block: the names they bind and,
    /// for a glob among them, the names *that* module brings (followed with the cycle guard).
    fn mesclar_usos_do_alvo(
        &mut self,
        file: &Path,
        trail: &[String],
        prof: usize,
        vistos: &mut HashSet<(PathBuf, Vec<String>)>,
    ) {
        let (Some(raiz), Some(base)) = (self.res.modelo(file), self.res.modpath_de(file)) else {
            self.vinculos.glob_externo = true;
            return;
        };
        for uso in raiz.usos.iter().filter(|u| u.trail.as_slice() == trail) {
            if !uso.glob {
                let nome = uso
                    .alias
                    .clone()
                    .unwrap_or_else(|| uso.segs.last().cloned().unwrap_or_default());
                self.liga(nome);
                continue;
            }
            let local = Local {
                arquivo: file,
                fs_modpath: Some(base.clone()),
                inline: uso.trail.clone(),
            };
            let mut visitados = HashSet::new();
            match self
                .res
                .resolver_caminho(&local, &uso.segs, 0, &mut visitados)
            {
                Resolucao::Ok(Encontrado::Modulo { file: f, trail: t }) => {
                    self.mesclar_glob_rec(&f, &t, prof + 1, vistos);
                }
                Resolucao::Ok(Encontrado::Enum(e)) => {
                    for v in &e.variantes {
                        self.liga(v.clone());
                    }
                }
                Resolucao::Abstencao(abst) => {
                    self.vinculos.glob_externo = true;
                    self.abstem(abst, &uso.segs);
                }
                _ => self.vinculos.glob_externo = true,
            }
        }
    }

    fn falha_import(&mut self, seg: &str, linha: usize, segs: &[String]) {
        let caminho = segs.join("::");
        let motivo = if segs.first().map(String::as_str) == Some(seg) && segs.len() > 1 {
            format!("crate `{seg}` não está nas dependências deste crate nem é membro do workspace")
        } else {
            format!("`{seg}` não encontrado ao resolver `{caminho}`")
        };
        empurrar(
            &mut self.out,
            "import",
            seg,
            linha,
            self.alvo,
            &motivo,
            Vec::new(),
        );
    }

    // ------------------------------------------------------------------ a caminhada

    fn caminhar(&mut self, node: Node) {
        match node.kind() {
            "attribute_item" | "inner_attribute_item" | "use_declaration" | "macro_invocation"
            | "macro_definition" | "lifetime" | "label" | "where_clause" | "type_item"
            // declarações puras: nomes e tipos em posição de declaração nunca são usos
            | "struct_item" | "enum_item" | "union_item" | "extern_crate_declaration"
            | "function_signature_item" | "associated_type" => {}
            "mod_item" => {
                let Some(corpo) = node.child_by_field_name("body") else {
                    return;
                };
                let nome = node
                    .child_by_field_name("name")
                    .map(|n| nome_limpo(text(self.content, n)))
                    .unwrap_or_default();
                self.mod_stack.push(nome.clone());
                let frame = self.itens_do_modulo_inline();
                self.bindings.push(frame);
                self.caminhar(corpo);
                self.bindings.pop();
                self.mod_stack.pop();
            }
            "function_item" | "closure_expression" => {
                let frame = self.coleta_frame(node);
                self.bindings.push(frame);
                if let Some(corpo) = node.child_by_field_name("body") {
                    self.caminhar(corpo);
                }
                self.bindings.pop();
            }
            "impl_item" | "trait_item" => {
                let alvo = self.alvo_do_impl(node);
                self.impl_stack.push(alvo);
                if let Some(corpo) = node.child_by_field_name("body") {
                    self.caminhar(corpo);
                }
                self.impl_stack.pop();
            }
            "let_declaration" => {
                if let Some(v) = node.child_by_field_name("value") {
                    self.caminhar(v);
                }
                if let Some(a) = node.child_by_field_name("alternative") {
                    self.caminhar(a);
                }
            }
            "let_condition" => {
                if let Some(v) = node.child_by_field_name("value") {
                    self.caminhar(v);
                }
            }
            "match_arm" => {
                let padrao = node.child_by_field_name("pattern").map(|p| p.id());
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() && Some(ch.id()) != padrao {
                        self.caminhar(ch);
                    }
                }
            }
            "for_expression" => {
                if let Some(v) = node.child_by_field_name("value") {
                    self.caminhar(v);
                }
                if let Some(b) = node.child_by_field_name("body") {
                    self.caminhar(b);
                }
            }
            "type_cast_expression" => {
                // `x as T` — a posição de tipo nunca é uso de valor
                if let Some(v) = node.child_by_field_name("value") {
                    self.caminhar(v);
                }
            }
            "const_item" | "static_item" => {
                // `pub const ALL: [Self; 3] = …` — o nome é declaração e o tipo não é uso de
                // valor (`static Q: OnceLock<tree_sitter::Query>`); só o inicializador conta
                if let Some(v) = node.child_by_field_name("value") {
                    self.caminhar(v);
                }
            }
            "struct_expression" => self.checar_struct_literal(node),
            "field_expression" => {
                // obj.attr / obj.metodo(..) — o campo nunca é uso; o receptor é verificado
                if let Some(v) = node.child_by_field_name("value") {
                    self.caminhar(v);
                }
            }
            "call_expression" => self.checar_chamada(node),
            "scoped_identifier" => {
                self.checar_caminho(node, false);
            }
            "identifier" => self.checar_ident(node),
            "self" | "unit_expression" => {}
            _ => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() {
                        self.caminhar(ch);
                    }
                }
            }
        }
    }

    /// `nome` is declared again below the file level: in a binding frame of an inline module or
    /// of the function the walk is in (fn/const/param/pattern) or by a `use` that sits in one of
    /// the inline modules. A call to it is not a call to the file-level function of that name.
    fn nome_sombreado(&self, nome: &str) -> bool {
        if self
            .bindings
            .iter()
            .skip(1)
            .any(|frame| frame.contains(nome))
        {
            return true;
        }
        (1..=self.mod_stack.len()).any(|n| {
            let trail = &self.mod_stack[..n];
            self.modelo.usos.iter().any(|u| {
                !u.glob
                    && u.trail.as_slice() == trail
                    && u.alias.as_deref().or(u.segs.last().map(String::as_str)) == Some(nome)
            })
        })
    }

    /// The item names of the inline module the walk is inside (`mod_stack` is its path from the
    /// file root, deepest last — nested modules included).
    fn itens_do_modulo_inline(&self) -> HashSet<String> {
        self.modelo_atual()
            .map(|m| m.itens.clone())
            .unwrap_or_default()
    }

    /// Function/closure scope frame: params + every pattern binding of the body subtree
    /// (position-blind and branch-blind — over-binding is the safe direction).
    fn coleta_frame(&self, fn_node: Node) -> HashSet<String> {
        let mut s = HashSet::new();
        if let Some(ps) = fn_node.child_by_field_name("parameters") {
            coleta_padroes(self.content, ps, &mut s);
        }
        if let Some(tp) = fn_node.child_by_field_name("type_parameters") {
            coleta_identificadores(self.content, tp, &mut s);
        }
        if let Some(corpo) = fn_node.child_by_field_name("body") {
            coleta_padroes_de_corpo(self.content, corpo, &mut s);
        }
        s
    }

    fn alvo_do_impl(&self, impl_node: Node) -> Option<ImplAlvo> {
        let tipo = impl_node.child_by_field_name("type")?;
        let nome = nome_base_do_tipo(self.content, tipo);
        if let Some(e) = self.modelo.enums.get(&nome) {
            return Some(ImplAlvo::Enum(e.clone()));
        }
        if let Some(s) = self.modelo.structs.get(&nome) {
            return Some(ImplAlvo::Struct(s.clone()));
        }
        if let Some(Encontrado::Enum(e)) = self.vinculos.alvos.get(&nome) {
            return Some(ImplAlvo::Enum(e.clone()));
        }
        if let Some(Encontrado::Struct(s)) = self.vinculos.alvos.get(&nome) {
            return Some(ImplAlvo::Struct(s.clone()));
        }
        None
    }

    fn ligado(&self, nome: &str) -> bool {
        self.ligado_em(&self.mod_stack, nome)
            || self.bindings.iter().rev().any(|f| f.contains(nome))
    }

    fn checar_ident(&mut self, node: Node) {
        let nome = nome_limpo(text(self.content, node));
        if self.ligado(&nome) {
            return;
        }
        let linha = node.start_position().row + 1;
        self.achou_nome(&nome, linha);
    }

    /// A path in expression position (`a::b::c`): head dispatch — keywords resolve against the
    /// project; a bound head resolves through its target (module/enum/fn/struct); an UNBOUND
    /// head falls through the extern prelude (workspace member / dependency) and only then is
    /// the E0433/E0425 case (`Capability`, `Tri`).
    fn checar_caminho(&mut self, node: Node, em_chamada: bool) -> Option<Encontrado> {
        let segs = segs_do_caminho(self.content, node);
        if segs.is_empty() {
            return None;
        }
        let linha = node.start_position().row + 1;
        match segs[0].as_str() {
            "Self" => {
                let resto = &segs[1..];
                match self.impl_stack.last() {
                    None => self.achou_nome("Self", linha),
                    Some(None) => {}
                    Some(Some(ImplAlvo::Enum(e))) => {
                        if resto.len() == 1 {
                            let e = e.clone();
                            self.checar_variante(&e, &resto[0], linha);
                        }
                    }
                    Some(Some(ImplAlvo::Struct(_))) => {}
                }
                None
            }
            "crate" | "self" | "super" => {
                let local = self.local(&self.mod_stack.clone());
                let mut visitados = HashSet::new();
                let r = self.res.resolver_caminho(&local, &segs, 0, &mut visitados);
                self.trata_resolucao(r, &segs, linha, em_chamada)
            }
            cabeca => {
                let resto = &segs[1..];
                if let Some(alvo) = self
                    .vinculos
                    .alvos
                    .get(cabeca)
                    .filter(|_| self.ligado(cabeca))
                    .cloned()
                {
                    match alvo {
                        Encontrado::Modulo { file, trail } => {
                            let mut visitados = HashSet::new();
                            let local = Local {
                                arquivo: &file,
                                fs_modpath: self.res.modpath_de(&file),
                                inline: trail,
                            };
                            let r = self.res.resolver_caminho(&local, resto, 0, &mut visitados);
                            self.trata_resolucao(r, &segs, linha, em_chamada)
                        }
                        Encontrado::Enum(e) => {
                            if resto.len() == 1 {
                                self.checar_variante(&e, &resto[0], linha);
                            }
                            None
                        }
                        Encontrado::EnumCaminho(e, r2) => {
                            if r2.len() == 1 {
                                self.checar_variante(&e, &r2[0], linha);
                            }
                            None
                        }
                        _ => None,
                    }
                } else if self.ligado(cabeca) {
                    None // ligado como valor/tipo não modelado — nada a provar
                } else if self.res.crate_de(&self.arquivo).is_none() {
                    // sem contexto de crate (/tmp avulso, inline): não há prelude externa a
                    // consultar — a cabeça não ligada é o nome ausente (E0433/E0425)
                    self.achou_nome(cabeca, linha);
                    None
                } else {
                    // extern prelude: `touring_intelligence::rl::…` sem `use` — resolve ou,
                    // se nem a cabeça existe, é o nome ausente (E0433/E0425)
                    let local = self.local(&self.mod_stack.clone());
                    let mut visitados = HashSet::new();
                    let r = self.res.resolver_caminho(&local, &segs, 0, &mut visitados);
                    match r {
                        Resolucao::Inexistente(seg) if seg == cabeca => {
                            self.achou_nome(cabeca, linha);
                            None
                        }
                        r => self.trata_resolucao(r, &segs, linha, em_chamada),
                    }
                }
            }
        }
    }

    fn trata_resolucao(
        &mut self,
        r: Resolucao,
        segs: &[String],
        linha: usize,
        em_chamada: bool,
    ) -> Option<Encontrado> {
        match r {
            Resolucao::Inexistente(seg) => {
                self.falha_import(&seg, linha, segs);
                None
            }
            Resolucao::Ok(Encontrado::EnumCaminho(e, resto)) => {
                if resto.len() == 1 {
                    self.checar_variante(&e, &resto[0], linha);
                }
                None
            }
            Resolucao::Ok(enc @ Encontrado::Funcao { .. }) => {
                if em_chamada {
                    Some(enc)
                } else {
                    None
                }
            }
            Resolucao::Ok(enc @ Encontrado::Struct(_)) => Some(enc),
            Resolucao::Abstencao(abst) => {
                self.abstem(abst, segs);
                None
            }
            Resolucao::Ok(_) => None,
        }
    }

    fn checar_variante(&mut self, e: &Rc<EnumModel>, variante: &str, linha: usize) {
        if e.variantes.contains(variante) || e.assoc.contains(variante) {
            return;
        }
        if !parece_variante(variante) {
            // item associado de um `impl` que o adaptador não vê: tentou e não decide
            self.abstem(
                Abst::Falha("item associado de enum do projeto (o `impl` não está visível)"),
                &[variante.to_string()],
            );
            return;
        }
        let mut reais: Vec<String> = e.variantes.iter().take(8).cloned().collect();
        reais.sort();
        empurrar(
            &mut self.out,
            "atributo",
            variante,
            linha,
            self.alvo,
            &format!("o enum não tem variante `{variante}`"),
            reais,
        );
    }

    /// A call: bare callee (scope + arity), path callee (resolution + arity), method (abstain —
    /// the receiver's type is not provable at layer 1) or an arbitrary callee expression.
    fn checar_chamada(&mut self, node: Node) {
        if let Some(f) = node.child_by_field_name("function") {
            match f.kind() {
                "identifier" => {
                    let nome = nome_limpo(text(self.content, f));
                    // um `fn`/`use`/binding de mesmo nome no módulo inline ou na fn em que a
                    // chamada está sombreia o do arquivo: a aridade do outro não vale aqui
                    let sig = if self.nome_sombreado(&nome) {
                        None
                    } else {
                        self.modelo.funcoes.get(&nome).copied().or_else(|| {
                            match self.vinculos.alvos.get(&nome) {
                                Some(Encontrado::Funcao { min, max }) => Some((*min, *max)),
                                _ => None,
                            }
                        })
                    };
                    // a assinatura do arquivo só vale onde o nome está em escopo: num `mod x { … }`
                    // inline ele precisa vir de um `use super::…` (E0425 caso contrário)
                    let ligado = self.ligado(&nome);
                    if let Some((min, max)) = sig.filter(|_| ligado) {
                        self.checar_aridade(&nome, min, max, node);
                    } else if !ligado {
                        self.achou_nome(&nome, f.start_position().row + 1);
                    }
                }
                "generic_function" => {
                    if let Some(g) = f.child_by_field_name("function") {
                        match g.kind() {
                            "identifier" => {
                                let nome = nome_limpo(text(self.content, g));
                                if !self.ligado(&nome) {
                                    self.achou_nome(&nome, g.start_position().row + 1);
                                }
                            }
                            "scoped_identifier" => {
                                self.checar_caminho(g, true);
                            }
                            _ => self.caminhar(g),
                        }
                    }
                }
                "scoped_identifier" => {
                    if let Some(Encontrado::Funcao { min, max }) = self.checar_caminho(f, true) {
                        let nome = segs_do_caminho(self.content, f)
                            .last()
                            .cloned()
                            .unwrap_or_default();
                        self.checar_aridade(&nome, min, max, node);
                    }
                }
                "field_expression" => {
                    if let Some(v) = f.child_by_field_name("value") {
                        self.caminhar(v);
                    }
                }
                _ => self.caminhar(f),
            }
        }
        if let Some(args) = node.child_by_field_name("arguments") {
            let mut c = args.walk();
            for ch in args.children(&mut c) {
                if ch.is_named() {
                    self.caminhar(ch);
                }
            }
        }
    }

    /// E0061: a project free fn with a proven signature called with the wrong argument count.
    fn checar_aridade(&mut self, nome: &str, min: usize, max: Option<usize>, call: Node) {
        let Some(args) = call.child_by_field_name("arguments") else {
            return;
        };
        // comentários dentro da lista de argumentos (`f(a, // porque\n b)`) não são argumentos
        let n = args
            .named_children(&mut args.walk())
            .filter(|ch| {
                !matches!(
                    ch.kind(),
                    "attribute_item" | "line_comment" | "block_comment"
                )
            })
            .count();
        let estourou = n < min || max.is_some_and(|m| n > m);
        if !estourou {
            return;
        }
        let esperado = match max {
            Some(m) if m == min => format!("{min}"),
            Some(m) => format!("{min}..={m}"),
            None => format!("pelo menos {min}"),
        };
        empurrar(
            &mut self.out,
            "kwarg",
            nome,
            call.start_position().row + 1,
            self.alvo,
            &format!("`{nome}` aceita {esperado} argumento(s), mas {n} foram fornecidos"),
            Vec::new(),
        );
    }

    /// E0560: fields of a struct literal whose struct is proven (local, imported or pathed;
    /// enum struct-variants included). `non_exhaustive`, tuple/unit and unproven types abstain.
    fn checar_struct_literal(&mut self, node: Node) {
        let mut campos: Option<Rc<StructModel>> = None;
        let mut campos_variante: Option<HashSet<String>> = None;
        let mut enum_ne = false; // enum `#[non_exhaustive]` — a variante também abstém
        if let Some(nn) = node.child_by_field_name("name") {
            if text(self.content, nn) == "Self" {
                if let Some(Some(ImplAlvo::Struct(s))) = self.impl_stack.last() {
                    campos = Some(s.clone());
                }
            } else {
                let segs = segs_do_caminho(self.content, nn);
                if segs.len() == 1 {
                    if let Some(Encontrado::Struct(s)) = self.vinculos.alvos.get(&segs[0]) {
                        campos = Some(s.clone());
                    }
                } else if segs.len() > 1 {
                    match self.checar_caminho(nn, false) {
                        Some(Encontrado::Struct(s)) => campos = Some(s),
                        _ => {
                            if segs.len() == 2
                                && let Some(Encontrado::Enum(e)) =
                                    self.vinculos.alvos.get(&segs[0]).cloned()
                            {
                                enum_ne = e.non_exhaustive;
                                campos_variante = e.variantes_struct.get(&segs[1]).cloned();
                            }
                        }
                    }
                }
            }
        }
        let Some(corpo) = node.child_by_field_name("body") else {
            return;
        };
        let pular = enum_ne || campos.as_ref().is_some_and(|s| s.non_exhaustive);
        let mut c = corpo.walk();
        for ch in corpo.children(&mut c) {
            if !ch.is_named() {
                continue;
            }
            match ch.kind() {
                "field_initializer" => {
                    if let Some(f) = ch.child_by_field_name("field") {
                        let campo = nome_limpo(text(self.content, f));
                        self.checar_campo(
                            &campo,
                            f.start_position().row + 1,
                            &campos,
                            &campos_variante,
                            pular,
                        );
                    }
                    if let Some(v) = ch.child_by_field_name("value") {
                        self.caminhar(v);
                    }
                }
                "shorthand_field_initializer" => {
                    // `{ campo }` — nome de campo E uso de valor ao mesmo tempo
                    let mut cc = ch.walk();
                    for sub in ch.children(&mut cc) {
                        if sub.kind() == "identifier" {
                            let campo = nome_limpo(text(self.content, sub));
                            self.checar_campo(
                                &campo,
                                sub.start_position().row + 1,
                                &campos,
                                &campos_variante,
                                pular,
                            );
                            self.checar_ident(sub);
                        }
                    }
                }
                _ => self.caminhar(ch),
            }
        }
    }

    fn checar_campo(
        &mut self,
        campo: &str,
        linha: usize,
        campos: &Option<Rc<StructModel>>,
        campos_variante: &Option<HashSet<String>>,
        pular: bool,
    ) {
        if pular {
            return;
        }
        let (pertence, mut reais): (bool, Vec<String>) = match (campos, campos_variante) {
            (Some(s), _) => (
                s.campos.contains(campo),
                s.campos.iter().take(8).cloned().collect(),
            ),
            (None, Some(v)) => (v.contains(campo), v.iter().take(8).cloned().collect()),
            (None, None) => return, // tipo não provado — abstenção
        };
        if !pertence {
            reais.sort();
            empurrar(
                &mut self.out,
                "kwarg",
                campo,
                linha,
                self.alvo,
                &format!("o literal não tem campo `{campo}` no tipo alvo"),
                reais,
            );
        }
    }
}

// ============================================================================ coletores de escopo

/// Every identifier a pattern subtree binds (shorthand fields bind; field names and variant
/// paths do not). Type positions are skipped — they bind nothing.
fn coleta_padroes(content: &str, node: Node, s: &mut HashSet<String>) {
    match node.kind() {
        "scoped_identifier" | "scoped_type_identifier" => return,
        "identifier" | "shorthand_field_identifier" => {
            s.insert(nome_limpo(text(content, node)));
            return;
        }
        "field_pattern" => {
            // `nome: padrao` liga o padrão; `nome` sozinho (shorthand) liga o nome
            if let Some(p) = node.child_by_field_name("pattern") {
                coleta_padroes(content, p, s);
            } else if let Some(n) = node.child_by_field_name("name") {
                s.insert(nome_limpo(text(content, n)));
            }
            return;
        }
        "self_parameter" => {
            s.insert("self".into());
            return;
        }
        _ => {}
    }
    let tipo = node.child_by_field_name("type").map(|t| t.id());
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.is_named() && Some(ch.id()) != tipo {
            coleta_padroes(content, ch, s);
        }
    }
}

/// Pattern positions of a function body: `let`/`for`/`match` arms/conditions and nested closure
/// params — all merged into the function frame (position-blind, branch-blind).
fn coleta_padroes_de_corpo(content: &str, node: Node, s: &mut HashSet<String>) {
    match node.kind() {
        "let_declaration" | "for_expression" | "match_arm" | "let_condition" | "parameter" => {
            if let Some(p) = node.child_by_field_name("pattern") {
                coleta_padroes(content, p, s);
            }
        }
        // items declared INSIDE the body (`const N: usize`, a nested `fn`, a local `struct`…) are
        // in scope for the whole block, wherever they sit
        "function_item" | "const_item" | "static_item" | "struct_item" | "enum_item"
        | "union_item" | "type_item" | "trait_item" | "mod_item" | "macro_definition" => {
            if let Some(n) = node.child_by_field_name("name") {
                s.insert(nome_limpo(text(content, n)));
            }
        }
        _ => {}
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.is_named() {
            coleta_padroes_de_corpo(content, ch, s);
        }
    }
}

fn coleta_identificadores(content: &str, node: Node, s: &mut HashSet<String>) {
    if matches!(node.kind(), "identifier" | "type_identifier") {
        s.insert(nome_limpo(text(content, node)));
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.is_named() {
            coleta_identificadores(content, ch, s);
        }
    }
}

// ============================================================================ utilitários

/// The std prelude + primitives + crate-root names — STRICT (only what rustc really brings in;
/// over-binding here would hide real E0425s).
fn preludio() -> HashSet<String> {
    [
        // `std::prelude::v1` + `rust_2021` + `rust_2024` of the installed toolchain's std docs:
        // the functions (`drop`, `size_of`…) and the 2024 additions the list below grew from
        "drop",
        "size_of",
        "size_of_val",
        "align_of",
        "align_of_val",
        "AsyncFn",
        "AsyncFnMut",
        "AsyncFnOnce",
        "Future",
        "IntoFuture",
        "Box",
        "String",
        "Vec",
        "Option",
        "Result",
        "Some",
        "None",
        "Ok",
        "Err",
        "Cow",
        "ToOwned",
        "Clone",
        "Copy",
        "Drop",
        "Default",
        "Send",
        "Sync",
        "Sized",
        "Unpin",
        "From",
        "Into",
        "TryFrom",
        "TryInto",
        "FromIterator",
        "Iterator",
        "DoubleEndedIterator",
        "ExactSizeIterator",
        "Extend",
        "IntoIterator",
        "AsRef",
        "AsMut",
        "Deref",
        "DerefMut",
        "Fn",
        "FnMut",
        "FnOnce",
        "Eq",
        "PartialEq",
        "Ord",
        "PartialOrd",
        "Hash",
        "Debug",
        "Display",
        "ToString",
        "FromStr",
        "std",
        "core",
        "alloc",
        "self",
        "Self",
        "super",
        "crate",
        "u8",
        "u16",
        "u32",
        "u64",
        "u128",
        "usize",
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "isize",
        "f32",
        "f64",
        "bool",
        "char",
        "str",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The base name of a type node (`Foo<Bar>` → `Foo`, `a::Foo` → `Foo`).
fn nome_base_do_tipo(content: &str, node: Node) -> String {
    match node.kind() {
        "type_identifier" | "identifier" => nome_limpo(text(content, node)),
        "scoped_type_identifier" | "scoped_identifier" => node
            .child_by_field_name("name")
            .map(|n| nome_limpo(text(content, n)))
            .unwrap_or_default(),
        "generic_type" | "generic_type_with_turbofish" => {
            let mut c = node.walk();
            node.children(&mut c)
                .find(|ch| ch.is_named())
                .map(|ch| nome_base_do_tipo(content, ch))
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// Can `name` be an enum VARIANT? The variant set is closed, so a missing one is provable — but
/// only a CamelCase name is a variant by the language's convention (`non_camel_case_types`): a
/// snake_case name (`Enum::from_path`) or a SCREAMING_CASE one (`Enum::ALL`) is an associated
/// fn/const, which lives in an `impl` of ANY file and is never provable at layer 1.
/// An expression statement that is just a macro call (`nome!(…);`).
fn tem_macro_invocation(stmt: Node) -> bool {
    stmt.named_child(0)
        .is_some_and(|c| c.kind() == "macro_invocation")
}

/// The `.rs` files that may declare `path` with a `#[path]`: its directory's siblings and, one
/// level up, the `mod.rs` / `<dir>.rs` that own that directory. Sorted — the search is stable.
fn vizinhos_rs(path: &Path) -> Vec<PathBuf> {
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs") && p != path)
        .collect();
    if let (Some(pai), Some(dn)) = (dir.parent(), dir.file_name()) {
        v.push(pai.join("mod.rs"));
        v.push(pai.join(format!("{}.rs", dn.to_string_lossy())));
    }
    v.sort();
    v
}

fn parece_variante(name: &str) -> bool {
    name.starts_with(|c: char| c.is_uppercase()) && name.chars().any(|c| c.is_lowercase())
}

fn nome_limpo(s: &str) -> String {
    s.strip_prefix("r#").unwrap_or(s).to_string()
}

fn text<'a>(source: &'a str, node: Node) -> &'a str {
    &source[node.byte_range()]
}

/// `trail` + one more segment (the `[&t[..], &[seg]].concat()` idiom, without the clone-slice).
fn empurra_seg(trail: &[String], seg: &str) -> Vec<String> {
    let mut v = trail.to_vec();
    v.push(seg.to_string());
    v
}

/// One finding per (classe, nome) — the judge counts presence, not occurrences. The severity is
/// not chosen here: [`Finding::candidato`] carries the [`Alvo`] and `vgp::check` decides.
fn empurrar(
    out: &mut Vec<Finding>,
    classe: &str,
    nome: &str,
    linha: usize,
    alvo: Alvo,
    motivo: &str,
    reais: Vec<String>,
) {
    if out.iter().any(|f| f.classe == classe && f.nome == nome) {
        return;
    }
    let sugestao = sugestao_proxima(nome, &reais)
        .map(|s| format!("{motivo}. Você quis dizer `{s}`?"))
        .unwrap_or_else(|| motivo.to_string());
    out.push(Finding::candidato(
        classe, nome, linha, alvo, sugestao, reais,
    ));
}

/// The closest real name (edit distance ≤ 2) — the refusal teaches the way.
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

#[cfg(test)]
mod tests;
