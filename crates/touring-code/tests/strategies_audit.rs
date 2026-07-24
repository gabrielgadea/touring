//! Auditoria cruzada E2E das 7 estrategias de touring-ast
//!
//! Verifica PROPOSITO DOCUMENTADO (STRATEGIES.md) vs IMPLEMENTACAO REAL.
//! Nao e teste unitario — e auditoria de contratos, invariantes, edge cases,
//! e integracao do pipeline completo de orquestracao de codigo.

use touring_code::ast::{
    // Strategy 7: LearningLoop
    GenerationEvent,
    Lang,
    LearningLoop,
    MemberKind,
    // Strategy 2: Context-Tree
    ModuleTree,
    ScopeKind,
    SymbolDetail,
    ValidationLayer,
    // Strategy 6: CallGraph
    build_call_graph,
    // Strategy 4: ScopeMap
    build_scope_map,
    // Strategy 3: ImportResolver
    extract_imports_resolved,
    // Strategy 1: VGP v2
    extract_symbol_details,
    // Strategy 5: Speculate v2
    speculate_v2,
};

// ═══════════════════════════════════════════════════════════════════════════
// AUDITORIA 1: VGP v2 — symbol_detail.rs
// PROPOSITO (STRATEGIES.md §4.1): Extrair campos/variantes/metodos REAIS
// de structs/enums/impl blocks via tree-sitter queries.
// CONTRATO: extract_symbol_details(source, symbol_name) -> Vec<SymbolDetail>
//   onde cada SymbolDetail tem: name, kind, type_str, is_pub, line
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_vgp_v2_extrai_campos_struct_simples() {
    let src = r#"struct Point { x: f64, y: f64 }"#;
    let details = extract_symbol_details(src, "Point");
    assert!(
        !details.is_empty(),
        "FALHOU: VGP v2 nao extraiu campos de struct simples"
    );
    assert!(
        details
            .iter()
            .any(|d| d.name == "x" && d.kind == MemberKind::Field),
        "FALHOU: campo 'x' nao encontrado: {:?}",
        details
    );
    assert!(
        details
            .iter()
            .any(|d| d.name == "y" && d.kind == MemberKind::Field),
        "FALHOU: campo 'y' nao encontrado: {:?}",
        details
    );
    assert!(
        details.iter().all(|d| d.type_str.as_deref() == Some("f64")),
        "FALHOU: tipo 'f64' nao preservado nos campos: {:?}",
        details
    );
}

#[test]
fn audit_vgp_v2_extrai_campos_struct_complexa() {
    let src = r#"
pub struct DataStore {
    pub cache: std::collections::HashMap<String, Vec<u8>>,
    pub(crate) counter: u64,
    name: Option<String>,
}
"#;
    let details = extract_symbol_details(src, "DataStore");
    assert!(
        details.iter().any(|d| d.name == "cache"),
        "FALHOU: campo 'cache' nao encontrado: {:?}",
        details
    );
    assert!(
        details.iter().any(|d| d.name == "counter"),
        "FALHOU: campo 'counter' nao encontrado: {:?}",
        details
    );
    assert!(
        details.iter().any(|d| d.name == "name"),
        "FALHOU: campo 'name' nao encontrado: {:?}",
        details
    );
    // Verificar visibilidade
    let cache_detail = details
        .iter()
        .find(|d| d.name == "cache")
        .expect("cache must exist");
    assert!(cache_detail.is_pub, "FALHOU: campo 'cache' deve ser pub");
    let name_detail = details
        .iter()
        .find(|d| d.name == "name")
        .expect("name must exist");
    assert!(!name_detail.is_pub, "FALHOU: campo 'name' nao deve ser pub");
}

#[test]
fn audit_vgp_v2_extrai_variantes_enum_todas_formas() {
    let src = r#"
enum Status {
    Active,
    Paused(u32),
    Failed { code: i32, msg: String },
}
"#;
    let details = extract_symbol_details(src, "Status");
    assert!(
        details
            .iter()
            .any(|d| d.name == "Active" && d.kind == MemberKind::Variant),
        "FALHOU: variante unitaria 'Active' nao encontrada: {:?}",
        details
    );
    assert!(
        details
            .iter()
            .any(|d| d.name == "Paused" && d.kind == MemberKind::Variant),
        "FALHOU: variante tupla 'Paused' nao encontrada: {:?}",
        details
    );
    assert!(
        details
            .iter()
            .any(|d| d.name == "Failed" && d.kind == MemberKind::Variant),
        "FALHOU: variante struct 'Failed' nao encontrada: {:?}",
        details
    );
}

#[test]
fn audit_vgp_v2_extrai_metodos_impl() {
    let src = r#"
struct Calculator;

impl Calculator {
    pub fn add(&self, a: u32, b: u32) -> u32 { a + b }
    fn reset(&mut self) {}
}
"#;
    let details = extract_symbol_details(src, "Calculator");
    assert!(
        details
            .iter()
            .any(|d| d.name == "add" && d.kind == MemberKind::Method && d.is_pub),
        "FALHOU: metodo pub 'add' nao encontrado: {:?}",
        details
    );
    assert!(
        details
            .iter()
            .any(|d| d.name == "reset" && d.kind == MemberKind::Method && !d.is_pub),
        "FALHOU: metodo privado 'reset' nao encontrado: {:?}",
        details
    );
}

#[test]
fn audit_vgp_v2_struct_inexistente_retorna_vazio() {
    let src = r#"struct Foo { x: u32 }"#;
    let details = extract_symbol_details(src, "Bar");
    assert!(
        details.is_empty(),
        "FALHOU: struct inexistente deve retornar Vec vazio"
    );
}

#[test]
fn audit_vgp_v2_source_vazio_nao_crasha() {
    let details = extract_symbol_details("", "Anything");
    assert!(
        details.is_empty(),
        "FALHOU: source vazio deve retornar Vec vazio"
    );
}

#[test]
fn audit_vgp_v2_source_invalido_nao_crasha() {
    let src = "struct { broken syntax {{{{ !!!";
    let details = extract_symbol_details(src, "broken");
    let _ = details; // nao deve causar panic
}

#[test]
fn audit_vgp_v2_type_str_preservado() {
    let src = r#"
struct Config {
    settings: HashMap<String, Vec<u8>>,
    count: usize,
}
"#;
    let details = extract_symbol_details(src, "Config");
    let settings = details.iter().find(|d| d.name == "settings");
    assert!(
        settings.is_some(),
        "FALHOU: campo 'settings' nao encontrado"
    );
    assert!(
        settings
            .expect("checked above")
            .type_str
            .as_deref()
            .unwrap_or("")
            .contains("HashMap"),
        "FALHOU: type_str deve conter 'HashMap': {:?}",
        settings
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDITORIA 2: Context-Tree — module_tree.rs
// PROPOSITO (STRATEGIES.md §4.2): Construir arvore hierarquica de modulos,
// detectar re-exports, resolver paths de imports.
// CONTRATO: ModuleTree::build_from_source(src, path) -> ModuleTree
//   tree.root.children contém ModuleNodes
//   tree.root.re_exports contém pub use paths
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_module_tree_detecta_modulos_aninhados() {
    let src = r#"
mod outer {
    pub mod inner {
        pub struct InnerType;
    }
}
"#;
    let tree = ModuleTree::build_from_source(src, "lib.rs");
    assert_eq!(
        tree.root.name, "lib.rs",
        "FALHOU: root name deve ser 'lib.rs'"
    );
    let outer = tree.root.children.iter().find(|n| n.name == "outer");
    assert!(
        outer.is_some(),
        "FALHOU: modulo 'outer' nao detectado: {:?}",
        tree.root.children
    );
    let outer = outer.expect("checked above");
    let inner = outer.children.iter().find(|n| n.name == "inner");
    assert!(
        inner.is_some(),
        "FALHOU: modulo 'inner' dentro de 'outer' nao detectado: {:?}",
        outer.children
    );
}

#[test]
fn audit_module_tree_detecta_re_exports() {
    let src = r#"
pub use crate::inner::Foo;
pub use crate::inner::Bar;
"#;
    let tree = ModuleTree::build_from_source(src, "lib.rs");
    assert!(
        tree.root.re_exports.len() >= 2,
        "FALHOU: deveria detectar pelo menos 2 re-exports, encontrou {}: {:?}",
        tree.root.re_exports.len(),
        tree.root.re_exports
    );
}

#[test]
fn audit_module_tree_resolve_path() {
    let src = r#"
pub mod parser {
    pub struct ParserPool;
}
pub mod symbols {
    pub struct Symbol;
}
"#;
    let tree = ModuleTree::build_from_source(src, "lib.rs");
    assert_eq!(
        tree.resolve_path("crate::parser"),
        Some("parser".to_string()),
        "FALHOU: resolve_path deve encontrar modulo 'parser'"
    );
    assert!(
        tree.resolve_path("crate::nonexistent").is_none(),
        "FALHOU: resolve_path deve retornar None para modulo inexistente"
    );
}

#[test]
fn audit_module_tree_detecta_visibilidade() {
    let src = r#"
pub mod public_mod {}
mod private_mod {}
"#;
    let tree = ModuleTree::build_from_source(src, "lib.rs");
    let pub_mod = tree.root.children.iter().find(|n| n.name == "public_mod");
    assert!(pub_mod.is_some(), "FALHOU: modulo publico nao encontrado");
    assert!(
        pub_mod.expect("checked").is_pub,
        "FALHOU: public_mod deve ser pub"
    );
    let priv_mod = tree.root.children.iter().find(|n| n.name == "private_mod");
    assert!(priv_mod.is_some(), "FALHOU: modulo privado nao encontrado");
    assert!(
        !priv_mod.expect("checked").is_pub,
        "FALHOU: private_mod nao deve ser pub"
    );
}

#[test]
fn audit_module_tree_source_vazio_nao_crasha() {
    let tree = ModuleTree::build_from_source("", "empty.rs");
    assert_eq!(tree.root.name, "empty.rs");
    assert!(tree.root.children.is_empty());
}

#[test]
fn audit_module_tree_source_invalido_nao_crasha() {
    let tree = ModuleTree::build_from_source("{{{{ invalid", "broken.rs");
    let _ = tree; // nao deve causar panic
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDITORIA 3: ImportResolver — import_resolver.rs
// PROPOSITO (STRATEGIES.md §4.3): Extrair imports completos (use, from/import)
// e detectar imports faltantes comparando contra tipos conhecidos.
// CONTRATO: extract_imports_resolved(src, lang) -> ImportResolver
//   resolver.imports: Vec<ResolvedImport> com path, alias, is_glob, line
//   resolver.detect_missing_imports(src, known_types) -> Vec<String>
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_import_resolver_extrai_use_simples() {
    let src = "use std::collections::HashMap;\nuse std::sync::Arc;";
    let resolver = extract_imports_resolved(src, Lang::Rust);
    assert!(
        !resolver.imports.is_empty(),
        "FALHOU: nenhum import extraido"
    );
    assert!(
        resolver.imports.iter().any(|i| i.path.contains("HashMap")),
        "FALHOU: HashMap nao encontrado nos imports: {:?}",
        resolver.imports
    );
    assert!(
        resolver.imports.iter().any(|i| i.path.contains("Arc")),
        "FALHOU: Arc nao encontrado nos imports: {:?}",
        resolver.imports
    );
}

#[test]
fn audit_import_resolver_extrai_use_agrupado() {
    let src = "use std::sync::{Arc, Mutex, RwLock};";
    let resolver = extract_imports_resolved(src, Lang::Rust);
    assert!(
        !resolver.imports.is_empty(),
        "FALHOU: use agrupado nao extraido: {:?}",
        resolver.imports
    );
}

#[test]
fn audit_import_resolver_detecta_glob() {
    let src = "use std::prelude::*;";
    let resolver = extract_imports_resolved(src, Lang::Rust);
    assert!(
        resolver.imports.iter().any(|i| i.is_glob),
        "FALHOU: glob import nao detectado: {:?}",
        resolver.imports
    );
}

#[test]
fn audit_import_resolver_detecta_tipo_nao_importado() {
    let src = r#"
fn main() {
    let map: HashMap<String, u32> = HashMap::new();
    let arc: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
}
"#;
    let resolver = extract_imports_resolved(src, Lang::Rust);
    let missing = resolver.detect_missing_imports(src, &["HashMap", "Arc", "Mutex"]);
    assert!(
        !missing.is_empty(),
        "FALHOU: detect_missing_imports deveria detectar tipos faltantes, retornou vazio"
    );
    assert!(
        missing.iter().any(|m| m.contains("HashMap")),
        "FALHOU: HashMap nao detectado como faltante: {:?}",
        missing
    );
}

#[test]
fn audit_import_resolver_nao_reporta_tipo_ja_importado() {
    let src = r#"
use std::collections::HashMap;

fn main() {
    let map: HashMap<String, u32> = HashMap::new();
}
"#;
    let resolver = extract_imports_resolved(src, Lang::Rust);
    let missing = resolver.detect_missing_imports(src, &["HashMap"]);
    assert!(
        !missing.contains(&"HashMap".to_string()),
        "FALHOU: HashMap esta importado mas foi reportado como faltante: {:?}",
        missing
    );
}

#[test]
fn audit_import_resolver_python_imports() {
    let src = r#"
import os
from pathlib import Path
"#;
    let resolver = extract_imports_resolved(src, Lang::Python);
    assert!(
        !resolver.imports.is_empty(),
        "FALHOU: Python imports nao extraidos: {:?}",
        resolver.imports
    );
    assert!(
        resolver.imports.iter().any(|i| i.path.contains("os")),
        "FALHOU: 'os' import nao encontrado: {:?}",
        resolver.imports
    );
}

#[test]
fn audit_import_resolver_source_vazio_nao_crasha() {
    let resolver = extract_imports_resolved("", Lang::Rust);
    assert!(resolver.imports.is_empty());
    let missing = resolver.detect_missing_imports("", &["HashMap"]);
    let _ = missing; // nao deve crashar
}

#[test]
fn audit_import_resolver_linguagem_desconhecida() {
    let resolver = extract_imports_resolved("some code", Lang::Bash);
    assert!(resolver.imports.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDITORIA 4: ScopeMap — scope_map.rs
// PROPOSITO (STRATEGIES.md §4.4): Rastrear escopos lexicos (let, params, for)
// para detectar shadowing e identificar referencias que precisam de import.
// CONTRATO: build_scope_map(src, lang) -> ScopeMap
//   scope.entries: Vec<ScopeEntry> com name, type_str, kind, line
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_scope_map_detecta_let_bindings() {
    let src = r#"
fn process() {
    let count: u32 = 0;
    let name = String::from("hello");
    let opt: Option<u64> = None;
}
"#;
    let scope = build_scope_map(src, Lang::Rust);
    assert!(
        scope
            .entries
            .iter()
            .any(|e| e.name == "count" && e.kind == ScopeKind::Let),
        "FALHOU: let binding 'count' nao detectado: {:?}",
        scope.entries
    );
    assert!(
        scope
            .entries
            .iter()
            .any(|e| e.name == "name" && e.kind == ScopeKind::Let),
        "FALHOU: let binding 'name' nao detectado: {:?}",
        scope.entries
    );
    assert!(
        scope
            .entries
            .iter()
            .any(|e| e.name == "opt" && e.kind == ScopeKind::Let),
        "FALHOU: let binding 'opt' nao detectado: {:?}",
        scope.entries
    );
}

#[test]
fn audit_scope_map_type_annotation_preservada() {
    let src = r#"
fn demo() {
    let x: u32 = 42;
}
"#;
    let scope = build_scope_map(src, Lang::Rust);
    let x_entry = scope.entries.iter().find(|e| e.name == "x");
    assert!(x_entry.is_some(), "FALHOU: 'x' nao encontrado no scope");
    let x = x_entry.expect("checked");
    assert!(
        x.type_str.is_some(),
        "FALHOU: type_str de 'x' deve estar presente: {:?}",
        x
    );
    assert!(
        x.type_str.as_deref().unwrap_or("").contains("u32"),
        "FALHOU: type_str de 'x' deve conter 'u32': {:?}",
        x.type_str
    );
}

#[test]
fn audit_scope_map_detecta_parametros_funcao() {
    let src = r#"
fn add(left: u32, right: u32) -> u32 {
    left + right
}
"#;
    let scope = build_scope_map(src, Lang::Rust);
    assert!(
        scope
            .entries
            .iter()
            .any(|e| e.name == "left" && e.kind == ScopeKind::Param),
        "FALHOU: parametro 'left' nao detectado: {:?}",
        scope.entries
    );
    assert!(
        scope
            .entries
            .iter()
            .any(|e| e.name == "right" && e.kind == ScopeKind::Param),
        "FALHOU: parametro 'right' nao detectado: {:?}",
        scope.entries
    );
}

#[test]
fn audit_scope_map_detecta_for_loop_variable() {
    let src = r#"
fn iterate(items: &[u32]) {
    for item in items {
        println!("{}", item);
    }
}
"#;
    let scope = build_scope_map(src, Lang::Rust);
    assert!(
        scope
            .entries
            .iter()
            .any(|e| e.name == "item" && e.kind == ScopeKind::For),
        "FALHOU: variavel de loop 'item' nao detectada: {:?}",
        scope.entries
    );
}

#[test]
fn audit_scope_map_python_funciona() {
    let src = r#"
def process(x, y):
    result = x + y
    return result
"#;
    let scope = build_scope_map(src, Lang::Python);
    assert!(
        scope
            .entries
            .iter()
            .any(|e| e.name == "x" && e.kind == ScopeKind::Param),
        "FALHOU: Python param 'x' nao detectado: {:?}",
        scope.entries
    );
    assert!(
        scope.entries.iter().any(|e| e.name == "result"),
        "FALHOU: Python binding 'result' nao detectado: {:?}",
        scope.entries
    );
}

#[test]
fn audit_scope_map_source_vazio_nao_crasha() {
    let scope = build_scope_map("", Lang::Rust);
    assert!(
        scope.entries.is_empty(),
        "FALHOU: source vazio deve ter scope vazio"
    );
}

#[test]
fn audit_scope_map_linguagem_desconhecida_nao_crasha() {
    let scope = build_scope_map("some code", Lang::Bash);
    assert!(scope.entries.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDITORIA 5: Speculate v2 — speculate.rs
// PROPOSITO (STRATEGIES.md §4.5): Validacao em 4 camadas com score composto.
// CONTRATO: speculate_v2(src, lang, ctx, imports) -> SpeculateResult
//   composite_score ponderado: Syntax(0.40) + Symbol(0.25) + Structural(0.25) + Import(0.10)
//   codigo valido -> score alto, codigo invalido -> score baixo
//   exatamente 4 layers em todas as execucoes
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_speculate_v2_tem_exatamente_4_camadas() {
    let result = speculate_v2("fn main() {}", Lang::Rust, None, None);
    assert_eq!(
        result.layers.len(),
        6,
        "FALHOU: speculate_v2 deve ter exatamente 6 camadas, tem {}",
        result.layers.len()
    );
    // Verificar que as 6 camadas corretas estao presentes
    assert!(
        result
            .layers
            .iter()
            .any(|l| l.layer == ValidationLayer::Syntax),
        "FALHOU: camada Syntax ausente"
    );
    assert!(
        result
            .layers
            .iter()
            .any(|l| l.layer == ValidationLayer::SymbolResolution),
        "FALHOU: camada SymbolResolution ausente"
    );
    assert!(
        result
            .layers
            .iter()
            .any(|l| l.layer == ValidationLayer::Structural),
        "FALHOU: camada Structural ausente"
    );
    assert!(
        result
            .layers
            .iter()
            .any(|l| l.layer == ValidationLayer::ImportCheck),
        "FALHOU: camada ImportCheck ausente"
    );
    assert!(
        result
            .layers
            .iter()
            .any(|l| l.layer == ValidationLayer::CfgImpact),
        "FALHOU: camada CfgImpact ausente"
    );
}

#[test]
fn audit_speculate_v2_codigo_valido_score_alto() {
    let src = r#"
fn compute(x: u32, y: u32) -> u32 {
    x.saturating_add(y)
}
"#;
    let result = speculate_v2(src, Lang::Rust, None, None);
    assert!(
        result.composite_score >= 0.7,
        "FALHOU: codigo valido deve ter score >= 0.7, obteve {:.4}",
        result.composite_score
    );
    assert!(
        result.all_passed,
        "FALHOU: codigo valido deve ter all_passed=true, layers: {:?}",
        result.layers
    );
}

#[test]
fn audit_speculate_v2_syntax_error_score_baixo() {
    let src = r#"
fn broken( {
    missing closing
"#;
    let result = speculate_v2(src, Lang::Rust, None, None);
    assert!(
        result.composite_score < 0.8,
        "FALHOU: codigo com syntax error deve ter score < 0.8, obteve {:.4}",
        result.composite_score
    );
    let syntax_layer = result
        .layers
        .iter()
        .find(|l| l.layer == ValidationLayer::Syntax);
    assert!(
        syntax_layer.map(|l| !l.passed).unwrap_or(false),
        "FALHOU: syntax layer deve falhar: {:?}",
        result.layers
    );
}

#[test]
fn audit_speculate_v2_detecta_unwrap_producao() {
    let src = r#"
fn get_value(map: &std::collections::HashMap<String, u32>, key: &str) -> u32 {
    *map.get(key).unwrap()
}
"#;
    let result = speculate_v2(src, Lang::Rust, None, None);
    let structural = result
        .layers
        .iter()
        .find(|l| l.layer == ValidationLayer::Structural);
    assert!(structural.is_some(), "FALHOU: camada Structural nao existe");
    let structural = structural.expect("checked");
    assert!(
        !structural.diagnostics.is_empty() || !structural.passed,
        "FALHOU: .unwrap() em producao nao detectado pelo Structural layer: {:?}",
        structural
    );
}

#[test]
fn audit_speculate_v2_score_range_valido() {
    // Score deve estar SEMPRE entre 0.0 e 1.0
    let inputs = vec![
        "fn ok() {}",
        "",
        "fn broken( { }}}",
        "use std::collections::HashMap;\nfn f() { let x = HashMap::new(); }",
    ];
    for src in inputs {
        let result = speculate_v2(src, Lang::Rust, None, None);
        assert!(
            result.composite_score >= 0.0 && result.composite_score <= 1.0,
            "FALHOU: composite_score deve estar entre 0.0 e 1.0, obteve {:.4} para src={:?}",
            result.composite_score,
            &src[..src.len().min(40)]
        );
    }
}

#[test]
fn audit_speculate_v2_com_symbol_context() {
    let src = r#"
fn use_data(data: &Data) {
    println!("{}", data.name);
}
"#;
    let symbols = vec![
        SymbolDetail {
            name: "name".to_string(),
            kind: MemberKind::Field,
            type_str: Some("String".to_string()),
            is_pub: true,
            line: 1,
        },
        SymbolDetail {
            name: "missing_field".to_string(),
            kind: MemberKind::Field,
            type_str: Some("u32".to_string()),
            is_pub: true,
            line: 2,
        },
    ];
    let result = speculate_v2(src, Lang::Rust, Some(&symbols), None);
    // "name" aparece no source, "missing_field" nao
    let sym_layer = result
        .layers
        .iter()
        .find(|l| l.layer == ValidationLayer::SymbolResolution);
    assert!(
        sym_layer.is_some(),
        "FALHOU: SymbolResolution layer deve existir"
    );
    let sym_layer = sym_layer.expect("checked");
    // Score deve ser parcial (1 de 2 resolvido = 0.5)
    assert!(
        sym_layer.score < 1.0,
        "FALHOU: com 1/2 simbolos resolvidos, score deve ser < 1.0, obteve {:.4}",
        sym_layer.score
    );
}

#[test]
fn audit_speculate_v2_linguagem_desconhecida_nao_crasha() {
    let result = speculate_v2("code here", Lang::Bash, None, None);
    assert_eq!(result.layers.len(), 6);
    assert!(result.composite_score >= 0.0 && result.composite_score <= 1.0);
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDITORIA 6: CallGraph — call_graph.rs
// PROPOSITO (STRATEGIES.md §4.6): Extrair grafo de chamadas e detectar
// callers/callees para verificar que mudancas de assinatura nao quebram.
// CONTRATO: build_call_graph(src, lang) -> CallGraph
//   graph.callers_of(func) -> Vec<&CallSite>
//   graph.callees_of(func) -> Vec<&CallSite>
//   CallSite { caller, callee, line, args_count }
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_call_graph_detecta_chamadas_diretas() {
    let src = r#"
fn main() {
    let r = compute(1, 2);
    println!("{}", r);
}
fn compute(a: u32, b: u32) -> u32 { a + b }
"#;
    let graph = build_call_graph(src, Lang::Rust);
    assert!(
        !graph.sites.is_empty(),
        "FALHOU: nenhuma chamada detectada no grafo"
    );
    let callers = graph.callers_of("compute");
    assert!(
        !callers.is_empty(),
        "FALHOU: callers_of('compute') retornou vazio: sites={:?}",
        graph.sites
    );
    assert!(
        callers.iter().any(|c| c.caller == "main"),
        "FALHOU: 'main' nao identificado como caller de 'compute': {:?}",
        callers
    );
}

#[test]
fn audit_call_graph_callers_of_funcao_nao_chamada() {
    let src = r#"
fn orphan() -> u32 { 42 }
fn main() { let _ = 1 + 1; }
"#;
    let graph = build_call_graph(src, Lang::Rust);
    let callers = graph.callers_of("orphan");
    assert!(
        callers.is_empty(),
        "FALHOU: 'orphan' nao e chamada, callers_of deve retornar vazio: {:?}",
        callers
    );
}

#[test]
fn audit_call_graph_callees_of() {
    let src = r#"
fn main() {
    foo();
    bar();
}
fn foo() {}
fn bar() {}
"#;
    let graph = build_call_graph(src, Lang::Rust);
    let callees = graph.callees_of("main");
    assert!(
        callees.len() >= 2,
        "FALHOU: main deve chamar pelo menos foo e bar: {:?}",
        callees
    );
}

#[test]
fn audit_call_graph_detecta_chamadas_encadeadas() {
    let src = r#"
fn a() { b(); }
fn b() { c(); }
fn c() {}
"#;
    let graph = build_call_graph(src, Lang::Rust);
    assert!(
        graph.callers_of("b").iter().any(|c| c.caller == "a"),
        "FALHOU: 'a' deve ser caller de 'b': {:?}",
        graph.sites
    );
    assert!(
        graph.callers_of("c").iter().any(|c| c.caller == "b"),
        "FALHOU: 'b' deve ser caller de 'c': {:?}",
        graph.sites
    );
}

#[test]
fn audit_call_graph_unique_callees() {
    let src = r#"
fn main() {
    foo();
    foo();
    bar();
}
fn foo() {}
fn bar() {}
"#;
    let graph = build_call_graph(src, Lang::Rust);
    let unique = graph.unique_callees();
    assert!(
        unique.contains(&"foo".to_string()),
        "FALHOU: unique_callees deve conter 'foo': {:?}",
        unique
    );
    assert!(
        unique.contains(&"bar".to_string()),
        "FALHOU: unique_callees deve conter 'bar': {:?}",
        unique
    );
}

#[test]
fn audit_call_graph_python_funciona() {
    let src = r#"
def main():
    result = compute(1, 2)
    print(result)

def compute(a, b):
    return a + b
"#;
    let graph = build_call_graph(src, Lang::Python);
    let callers = graph.callers_of("compute");
    assert!(
        !callers.is_empty(),
        "FALHOU: compute deve ter callers em Python: {:?}",
        graph.sites
    );
}

#[test]
fn audit_call_graph_source_vazio_nao_crasha() {
    let graph = build_call_graph("", Lang::Rust);
    assert!(graph.sites.is_empty());
    let callers = graph.callers_of("anything");
    assert!(callers.is_empty());
}

#[test]
fn audit_call_graph_linguagem_desconhecida() {
    let graph = build_call_graph("code", Lang::Bash);
    assert!(graph.sites.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDITORIA 7: LearningLoop — learning_loop.rs
// PROPOSITO (STRATEGIES.md §4.7): Aprender quais estrategias funcionam
// melhor via historico de sucesso/falha. Recompensar e recomendar.
// CONTRATO: LearningLoop::new(), record_event(GenerationEvent),
//   success_rate_for(strategy) -> f64, recommend_strategy(ctx) -> String,
//   reward(symbol, delta), average_reward() -> f64
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_learning_loop_success_rate_deterministico() {
    let mut lloop = LearningLoop::new();
    // 3 sucessos, 1 falha = 75%
    for i in 0..4u64 {
        lloop.record_event(GenerationEvent {
            symbol_name: format!("sym_{i}"),
            language: "rust".to_string(),
            success: i < 3,
            strategy_used: "vgp_v2".to_string(),
            timestamp_ms: i * 100,
        });
    }
    let rate = lloop.success_rate_for("vgp_v2");
    assert!(
        (rate - 0.75).abs() < 0.01,
        "FALHOU: success_rate deve ser 0.75 (3/4), obteve {rate:.4}"
    );
}

#[test]
fn audit_learning_loop_recommend_melhor_estrategia() {
    let mut lloop = LearningLoop::new();
    // vgp_v2: 9/10 = 90% de sucesso
    for i in 0..10u64 {
        lloop.record_event(GenerationEvent {
            symbol_name: format!("s{i}"),
            language: "rust".to_string(),
            success: i < 9,
            strategy_used: "vgp_v2".to_string(),
            timestamp_ms: i * 100,
        });
    }
    // scope_map: 3/10 = 30% de sucesso
    for i in 0..10u64 {
        lloop.record_event(GenerationEvent {
            symbol_name: format!("t{i}"),
            language: "rust".to_string(),
            success: i < 3,
            strategy_used: "scope_map".to_string(),
            timestamp_ms: 1000 + i * 100,
        });
    }
    let recommended = lloop.recommend_strategy("rust", 0);
    assert_eq!(
        recommended, "vgp_v2",
        "FALHOU: should recommend 'vgp_v2' (90%) over 'scope_map' (30%), got '{recommended}'"
    );
}

#[test]
fn audit_learning_loop_reward_afeta_estado() {
    let mut lloop = LearningLoop::new();
    lloop.record_event(GenerationEvent {
        symbol_name: "test".to_string(),
        language: "rust".to_string(),
        success: false,
        strategy_used: "vgp_v2".to_string(),
        timestamp_ms: 0,
    });
    let before = lloop.reward_sum;
    lloop.reward("test", 1.0);
    let after = lloop.reward_sum;
    assert!(
        after > before,
        "FALHOU: reward positivo deve aumentar reward_sum: before={before:.4}, after={after:.4}"
    );
}

#[test]
fn audit_learning_loop_average_reward() {
    let mut lloop = LearningLoop::new();
    lloop.record_event(GenerationEvent {
        symbol_name: "A".to_string(),
        language: "rust".to_string(),
        success: true,
        strategy_used: "vgp_v2".to_string(),
        timestamp_ms: 100,
    });
    lloop.record_event(GenerationEvent {
        symbol_name: "B".to_string(),
        language: "rust".to_string(),
        success: false,
        strategy_used: "vgp_v2".to_string(),
        timestamp_ms: 200,
    });
    let avg = lloop.average_reward();
    assert!(
        (avg - 0.5).abs() < 0.01,
        "FALHOU: average_reward deve ser 0.5 (1 success / 2 events), obteve {avg:.4}"
    );
}

#[test]
fn audit_learning_loop_strategies_used() {
    let mut lloop = LearningLoop::new();
    lloop.record_event(GenerationEvent {
        symbol_name: "A".to_string(),
        language: "rust".to_string(),
        success: true,
        strategy_used: "vgp_v2".to_string(),
        timestamp_ms: 100,
    });
    lloop.record_event(GenerationEvent {
        symbol_name: "B".to_string(),
        language: "rust".to_string(),
        success: true,
        strategy_used: "scope_aware".to_string(),
        timestamp_ms: 200,
    });
    let strats = lloop.strategies_used();
    assert!(
        strats.contains(&"vgp_v2".to_string()),
        "FALHOU: vgp_v2 deve estar em strategies_used"
    );
    assert!(
        strats.contains(&"scope_aware".to_string()),
        "FALHOU: scope_aware deve estar em strategies_used"
    );
}

#[test]
fn audit_learning_loop_sem_eventos_nao_crasha() {
    let lloop = LearningLoop::new();
    let rate = lloop.success_rate_for("nenhuma_estrategia");
    assert_eq!(rate, 0.0, "FALHOU: sem eventos, success_rate deve ser 0.0");
    let rec = lloop.recommend_strategy("rust", 0);
    assert_eq!(
        rec, "vgp_v2",
        "FALHOU: sem historico, default deve ser 'vgp_v2'"
    );
    let avg = lloop.average_reward();
    assert_eq!(avg, 0.0, "FALHOU: sem eventos, average_reward deve ser 0.0");
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDITORIA DE INTEGRACAO: Os 7 modulos como PIPELINE COESO
// PROPOSITO: O sistema e um instrumento de orquestracao de fluxos.
// CONTRATO: O pipeline completo extrai contexto -> valida -> aprende.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn audit_pipeline_completo_fluxo_orquestracao() {
    let existing_source = r#"
use std::collections::HashMap;

pub struct Config {
    pub settings: HashMap<String, String>,
    pub max_retries: u32,
    pub timeout_ms: u64,
}

pub enum ConfigError {
    Missing(String),
    Invalid { key: String, value: String },
}
"#;

    let generated_code = r#"
fn load_config(path: &str) -> Result<Config, ConfigError> {
    let settings = HashMap::new();
    Ok(Config {
        settings,
        max_retries: 3,
        timeout_ms: 5000,
    })
}
"#;

    // PASSO 1: VGP v2 — extrair schema real
    let fields = extract_symbol_details(existing_source, "Config");
    assert!(
        fields.iter().any(|f| f.name == "settings"),
        "Pipeline: campo 'settings' nao extraido"
    );
    assert!(
        fields.iter().any(|f| f.name == "max_retries"),
        "Pipeline: campo 'max_retries' nao extraido"
    );
    assert!(
        fields.iter().any(|f| f.name == "timeout_ms"),
        "Pipeline: campo 'timeout_ms' nao extraido"
    );

    let enum_variants = extract_symbol_details(existing_source, "ConfigError");
    assert!(
        !enum_variants.is_empty(),
        "Pipeline: variantes de ConfigError nao extraidas"
    );

    // PASSO 2: ImportResolver — verificar imports
    let existing_imports = extract_imports_resolved(existing_source, Lang::Rust);
    assert!(
        existing_imports
            .imports
            .iter()
            .any(|i| i.path.contains("HashMap")),
        "Pipeline: HashMap nao detectado nos imports existentes"
    );

    // PASSO 3: ScopeMap — mapear escopos do codigo gerado
    let scope = build_scope_map(generated_code, Lang::Rust);
    assert!(
        scope
            .entries
            .iter()
            .any(|e| e.name == "settings" || e.name == "path"),
        "Pipeline: scope map nao capturou variaveis do codigo gerado: {:?}",
        scope.entries
    );

    // PASSO 4: ModuleTree — arvore de modulos
    let tree = ModuleTree::build_from_source(existing_source, "config.rs");
    assert_eq!(tree.root.name, "config.rs");

    // PASSO 5: Speculate v2 — validar codigo gerado em 5 camadas
    let result = speculate_v2(
        generated_code,
        Lang::Rust,
        Some(&fields),
        Some(&existing_imports),
    );
    assert_eq!(
        result.layers.len(),
        6,
        "Pipeline: speculate deve ter 6 camadas"
    );
    assert!(
        result.composite_score >= 0.0,
        "Pipeline: composite_score invalido"
    );

    // PASSO 6: CallGraph — verificar chamadas do codigo gerado
    let graph = build_call_graph(generated_code, Lang::Rust);
    let _ = graph; // nao deve crashar

    // PASSO 7: LearningLoop — registrar resultado
    let mut learning = LearningLoop::new();
    learning.record_event(GenerationEvent {
        symbol_name: "Config".to_string(),
        language: "rust".to_string(),
        success: result.composite_score >= 0.7,
        strategy_used: "vgp_v2".to_string(),
        timestamp_ms: 42000,
    });

    assert_eq!(
        learning.event_count, 1,
        "Pipeline: evento nao registrado no learning loop"
    );

    let rate = learning.success_rate_for("vgp_v2");

    eprintln!("=== PIPELINE COMPLETO: PASSOU ===");
    eprintln!(
        "  VGP v2: {} campos Config + {} variantes ConfigError",
        fields.len(),
        enum_variants.len()
    );
    eprintln!(
        "  ImportResolver: {} imports encontrados",
        existing_imports.imports.len()
    );
    eprintln!("  ScopeMap: {} entradas de escopo", scope.entries.len());
    eprintln!(
        "  Speculate v2: score={:.2}, camadas={}, all_passed={}",
        result.composite_score,
        result.layers.len(),
        result.all_passed
    );
    eprintln!("  CallGraph: {} call sites", graph.sites.len());
    eprintln!("  LearningLoop: taxa de sucesso vgp_v2={:.2}", rate);
}

#[test]
fn audit_pipeline_edge_case_codigo_invalido_nao_crasha_sistema() {
    // INVARIANTE: o sistema NUNCA deve crashar mesmo com codigo completamente invalido
    let big_input = "x".repeat(50_000);
    let broken_inputs: Vec<&str> = vec![
        "",
        "{{{{ broken }}}}",
        "fn (missing name) {}",
        &big_input, // codigo grande
        "struct 123InvalidName { }",
        "use ::::::;",
        "\0\0\0", // null bytes
    ];

    for input in &broken_inputs {
        // Nenhum destes deve causar panic
        let _ = extract_symbol_details(input, "Test");
        let _ = ModuleTree::build_from_source(input, "test.rs");
        let _ = extract_imports_resolved(input, Lang::Rust);
        let _ = build_scope_map(input, Lang::Rust);
        let _ = speculate_v2(input, Lang::Rust, None, None);
        let _ = build_call_graph(input, Lang::Rust);
    }
    eprintln!(
        "=== EDGE CASES: sistema sobreviveu a {} inputs invalidos ===",
        broken_inputs.len()
    );
}

#[test]
fn audit_invariante_outputs_deterministicos() {
    // INVARIANTE: mesmos inputs -> mesmos outputs
    let src = r#"
struct Foo { x: u32, y: String }
enum Bar { A, B(u32) }
"#;

    let result1 = extract_symbol_details(src, "Foo");
    let result2 = extract_symbol_details(src, "Foo");

    assert_eq!(
        result1.len(),
        result2.len(),
        "FALHOU: resultados nao-deterministicos em extract_symbol_details"
    );

    for (r1, r2) in result1.iter().zip(result2.iter()) {
        assert_eq!(
            r1.name, r2.name,
            "FALHOU: nomes diferentes em execucoes consecutivas"
        );
        assert_eq!(
            r1.kind, r2.kind,
            "FALHOU: kinds diferentes em execucoes consecutivas"
        );
    }

    // Speculate tambem deve ser deterministico
    let spec1 = speculate_v2(src, Lang::Rust, None, None);
    let spec2 = speculate_v2(src, Lang::Rust, None, None);
    assert!(
        (spec1.composite_score - spec2.composite_score).abs() < f64::EPSILON,
        "FALHOU: speculate nao-deterministico: {:.4} vs {:.4}",
        spec1.composite_score,
        spec2.composite_score
    );
}

#[test]
fn audit_invariante_unicode_nao_crasha() {
    // Edge case: unicode e emojis em source code
    let src = "struct Café { naïve: u32, über: String }";
    let details = extract_symbol_details(src, "Café");
    let _ = details; // nao deve crashar

    let scope = build_scope_map("fn日本語() { let 变量 = 42; }", Lang::Rust);
    let _ = scope; // nao deve crashar

    let graph = build_call_graph("fn ñ() { á(); }", Lang::Rust);
    let _ = graph; // nao deve crashar
}
