//! TypeScript diagnostic → finding, shared by the two servers that speak TypeScript's checker:
//! `tsserver` (its own protocol) and `deno lsp` (LSP, `source: "deno-ts"`). Both reduce to the same
//! shape here — `{code, message, range}` — so the table below is written once.
//!
//! Only the codes that say "a name you used does not exist" are findings; everything else a checker
//! knows (type mismatches, unused variables, strictness) is not what layer 2 is for:
//!
//! | code | meaning | class | scope |
//! |---|---|---|---|
//! | TS2304, TS2552 | `Cannot find name 'x'` | `nome` | `proprio` |
//! | TS2339, TS2551 | `Property 'x' does not exist on type 'T'` | `atributo` | `externo` for a lib/builtin `T`, else `projeto` |
//! | TS2305, TS2724 | `Module '"m"' has no exported member 'x'` | `import` | `externo` for an installed package/`node:`, else `projeto` |
//! | TS2307 | `Cannot find module 'm'` | `import` | `projeto` (it may be installed next) |
//! | TS2353, TS2561 | `Object literal may only specify known properties … 'x' does not exist in type 'T'` | `kwarg` | `externo` for a lib `T`, else `projeto` |
//!
//! The scope feeds the one severity policy ([`crate::achado::severidade`]): inline/`/tmp` code denies,
//! a project file denies only an `externo` target. Which types count as "lib/builtin" is a
//! calibration, not a measurement (the corpus has no TS edits yet): it errs towards `projeto`, which
//! only warns.

use std::path::Path;

use serde_json::Value;

use crate::achado::{Escopo, L2Achado, severidade};
use crate::lsp_nucleo::{Normalizador, Pedido};

/// Node's built-in modules importable without the `node:` prefix (sorted for `binary_search`).
const NODE_BUILTINS: [&str; 27] = [
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "crypto",
    "dgram",
    "dns",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "querystring",
    "readline",
    "stream",
    "string_decoder",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "zlib",
];

/// Global types of the ES and DOM libs (sorted for `binary_search`) — the receivers whose missing
/// members are a fact about the language/runtime, not about the project.
const TIPOS_DE_LIB: [&str; 63] = [
    "AbortController",
    "AbortSignal",
    "Array",
    "ArrayBuffer",
    "Blob",
    "Boolean",
    "CustomEvent",
    "DOMException",
    "DataView",
    "Date",
    "Document",
    "Element",
    "Error",
    "Event",
    "EventTarget",
    "File",
    "FormData",
    "Function",
    "Headers",
    "Intl",
    "JSON",
    "Map",
    "Math",
    "Node",
    "NodeList",
    "Number",
    "Object",
    "Partial",
    "Pick",
    "Promise",
    "ReadonlyArray",
    "Record",
    "Request",
    "RequestInit",
    "Required",
    "Response",
    "RegExp",
    "Set",
    "Storage",
    "String",
    "Symbol",
    "TextDecoder",
    "TextEncoder",
    "URL",
    "URLSearchParams",
    "WeakMap",
    "WeakSet",
    "WebSocket",
    "Window",
    "Worker",
    "XMLHttpRequest",
    "bigint",
    "boolean",
    "console",
    "document",
    "number",
    "string",
    "symbol",
    "undefined",
    "void",
    "window",
    "Uint8Array",
    "Float64Array",
];

/// Rules of a TypeScript code: which class it is and how its scope is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Regra {
    Nome,
    Membro,
    ImportDeModulo,
    ModuloAusente,
    CampoDeLiteral,
}

/// The codes that are findings, with their class.
const REGRAS: [(u64, Regra, &str); 9] = [
    (2304, Regra::Nome, "nome"),
    (2552, Regra::Nome, "nome"),
    (2339, Regra::Membro, "atributo"),
    (2551, Regra::Membro, "atributo"),
    (2305, Regra::ImportDeModulo, "import"),
    (2724, Regra::ImportDeModulo, "import"),
    (2307, Regra::ModuloAusente, "import"),
    (2353, Regra::CampoDeLiteral, "kwarg"),
    (2561, Regra::CampoDeLiteral, "kwarg"),
];

fn regra_do_codigo(code: u64) -> Option<(Regra, &'static str)> {
    REGRAS
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, regra, classe)| (*regra, *classe))
}

/// The `'…'` runs of a message, in order. TypeScript quotes names and types with single quotes and
/// string literals inside a type with double quotes, so a pair never straddles a type.
pub(crate) fn aspas_simples(msg: &str) -> Vec<&str> {
    msg.split('\'')
        .enumerate()
        .filter_map(|(i, parte)| (i % 2 == 1).then_some(parte))
        .collect()
}

/// The numeric code of a diagnostic (`2304` or `"2304"`, or `"TS2304"`).
fn codigo(d: &Value) -> Option<u64> {
    let code = d.get("code")?;
    code.as_u64().or_else(|| {
        code.as_str()
            .and_then(|s| s.trim_start_matches("TS").parse().ok())
    })
}

fn mensagem(d: &Value) -> &str {
    d.get("message").and_then(Value::as_str).unwrap_or("")
}

/// Is `tipo` a type of the language/runtime libraries (so a missing member is a fact about the
/// language)? Unions, intersections, object types and project names are not.
pub(crate) fn tipo_de_lib(tipo: &str) -> bool {
    let tipo = tipo.trim();
    if tipo.starts_with("typeof import(") || tipo.ends_with("[]") && !e_composto(tipo) {
        return true; // a module namespace, or an array of anything (an Array)
    }
    if e_composto(tipo) {
        return false;
    }
    let cabeca = cabeca_do_tipo(tipo);
    TIPOS_DE_LIB.contains(&cabeca)
        || (["HTML", "SVG"].iter().any(|p| cabeca.starts_with(p)) && cabeca.ends_with("Element"))
}

/// A union, an intersection or an object type: the receiver is not one nameable lib type.
fn e_composto(tipo: &str) -> bool {
    tipo.contains(" | ") || tipo.contains(" & ") || tipo.starts_with('{')
}

/// `Map<string, number>` → `Map`, `Intl.NumberFormat` → `Intl`.
fn cabeca_do_tipo(tipo: &str) -> &str {
    tipo.split(['<', '[', '.']).next().unwrap_or(tipo)
}

/// Is the module specifier an installed package, a Node built-in or a `node:` module — a target that
/// does not change with the edit? Relative paths, aliases (`@/`, `~/`, `#x`) and packages that are not
/// in any `node_modules` between the file and the root are the project's (or not there yet).
pub(crate) fn especificador_externo(especificador: &str, origem: &Path, raiz: &Path) -> bool {
    let spec = especificador.trim().trim_matches('"');
    if spec.starts_with("node:") {
        return true;
    }
    if spec.starts_with(['.', '/', '#', '~', '$']) || spec.starts_with("@/") {
        return false;
    }
    let pacote = nome_do_pacote(spec);
    if NODE_BUILTINS.binary_search(&pacote).is_ok() {
        return true;
    }
    let comeco = origem.parent().unwrap_or(raiz);
    comeco
        .ancestors()
        .take_while(|dir| dir.starts_with(raiz))
        .any(|dir| dir.join("node_modules").join(pacote).exists())
}

/// `react/jsx-runtime` → `react`, `@scope/pkg/sub` → `@scope/pkg`.
fn nome_do_pacote(spec: &str) -> &str {
    let mut partes = spec.splitn(3, '/');
    let primeiro = partes.next().unwrap_or(spec);
    if primeiro.starts_with('@') {
        let fim = primeiro.len() + partes.next().map_or(0, |s| s.len() + 1);
        &spec[..fim.min(spec.len())]
    } else {
        primeiro
    }
}

/// The finding of one TypeScript diagnostic, or `None` when it is not one of the name classes.
pub(crate) fn achado_de(d: &Value, pedido: &Pedido<'_>, raiz: &Path) -> Option<L2Achado> {
    let code = codigo(d)?;
    let (regra, classe) = regra_do_codigo(code)?;
    let msg = mensagem(d);
    let (nome, escopo) = identificar(regra, &aspas_simples(msg), pedido, raiz)?;
    let linha0 = d.pointer("/range/start/line").and_then(Value::as_u64)?;
    Some(L2Achado {
        classe: classe.to_string(),
        nome,
        linha: u32::try_from(linha0.saturating_add(1)).unwrap_or(u32::MAX),
        escopo,
        severidade: severidade(pedido.balde, escopo),
        regra: format!("TS{code}"),
        mensagem: msg.to_string(),
    })
}

/// The invented name and whose it is, from the quoted runs of the message.
fn identificar(
    regra: Regra,
    tokens: &[&str],
    pedido: &Pedido<'_>,
    raiz: &Path,
) -> Option<(String, Escopo)> {
    let primeiro = tokens.first()?.to_string();
    Some(match regra {
        Regra::Nome => (primeiro, Escopo::Proprio),
        Regra::ModuloAusente => (primeiro, Escopo::Projeto),
        Regra::Membro | Regra::CampoDeLiteral => {
            let da_lib = tokens.get(1).is_some_and(|t| tipo_de_lib(t));
            (primeiro, escopo_por(da_lib))
        }
        Regra::ImportDeModulo => {
            let externo = especificador_externo(&primeiro, &pedido.origem, raiz);
            (tokens.get(1)?.to_string(), escopo_por(externo))
        }
    })
}

fn escopo_por(externo: bool) -> Escopo {
    if externo {
        Escopo::Externo
    } else {
        Escopo::Projeto
    }
}

/// The normalizer of TypeScript diagnostics. `fonte`, when set, is the only `source` accepted
/// (`deno lsp` mixes `deno-ts`, `deno-lint` and `deno`; tsserver diagnostics carry no source).
pub(crate) struct NormTs {
    pub(crate) fonte: Option<&'static str>,
}

impl Normalizador for NormTs {
    fn achados(
        &self,
        diags: &[Value],
        pedido: &Pedido<'_>,
        raiz: &Path,
    ) -> Result<Vec<L2Achado>, String> {
        Ok(diags
            .iter()
            .filter(|d| {
                self.fonte
                    .is_none_or(|fonte| d.get("source").and_then(Value::as_str) == Some(fonte))
            })
            .filter_map(|d| achado_de(d, pedido, raiz))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achado::{Balde, Severidade};
    use crate::testutil::Arvore;
    use serde_json::json;
    use std::time::Instant;

    fn diag(code: u64, message: &str, line0: u64) -> Value {
        json!({"code": code, "source": "deno-ts", "message": message, "severity": 1,
            "range": {"start": {"line": line0, "character": 0}, "end": {"line": line0, "character": 3}}})
    }

    fn pedido<'a>(arvore: &Arvore, balde: Balde, content: &'a str) -> Pedido<'a> {
        Pedido {
            uri: String::new(),
            origem: arvore.raiz.join("src/a.ts"),
            balde,
            content,
            disk: None,
            overlays: &[],
            deadline: Instant::now(),
        }
    }

    fn um(arvore: &Arvore, d: &Value, balde: Balde) -> L2Achado {
        let p = pedido(arvore, balde, "");
        achado_de(d, &p, &arvore.raiz).expect("a name-class diagnostic must be a finding")
    }

    // ── the three cases of the VL0 acceptance + the rest of the table ──────

    #[test]
    fn ts2304_unknown_name_is_nome_proprio() {
        let t = Arvore::nova("ts2304");
        let a = um(
            &t,
            &diag(2304, "Cannot find name 'naoDefinido'.", 4),
            Balde::Arquivo,
        );
        assert_eq!(
            (
                a.classe.as_str(),
                a.nome.as_str(),
                a.linha,
                a.escopo,
                a.regra.as_str()
            ),
            ("nome", "naoDefinido", 5, Escopo::Proprio, "TS2304")
        );
        assert_eq!(
            a.severidade,
            Severidade::Warn,
            "an own name in a project file only warns (P4)"
        );
        let inline = um(&t, &diag(2304, "Cannot find name 'x'.", 0), Balde::Inline);
        assert_eq!(
            inline.severidade,
            Severidade::Deny,
            "code that runs now denies"
        );
    }

    #[test]
    fn ts2339_missing_member_of_a_lib_type_is_externo_and_of_a_project_type_projeto() {
        let t = Arvore::nova("ts2339");
        let lib = um(
            &t,
            &diag(
                2339,
                "Property 'pusha' does not exist on type 'number[]'.",
                0,
            ),
            Balde::Arquivo,
        );
        assert_eq!(
            (lib.classe.as_str(), lib.nome.as_str(), lib.escopo),
            ("atributo", "pusha", Escopo::Externo)
        );
        assert_eq!(lib.severidade, Severidade::Deny);
        let proj = um(
            &t,
            &diag(2339, "Property 'z' does not exist on type 'Ponto'.", 0),
            Balde::Arquivo,
        );
        assert_eq!((proj.nome.as_str(), proj.escopo), ("z", Escopo::Projeto));
        assert_eq!(proj.severidade, Severidade::Warn);
    }

    #[test]
    fn ts2305_unknown_export_reads_name_and_module_and_scopes_by_the_module() {
        let t = Arvore::nova("ts2305");
        t.arquivo("node_modules/react/package.json", "{}");
        let local = um(
            &t,
            &diag(
                2305,
                "Module '\"./m\"' has no exported member 'naoExiste'.",
                0,
            ),
            Balde::Arquivo,
        );
        assert_eq!(
            (local.classe.as_str(), local.nome.as_str(), local.escopo),
            ("import", "naoExiste", Escopo::Projeto)
        );
        let pacote = um(
            &t,
            &diag(
                2305,
                "Module '\"react\"' has no exported member 'useInventado'.",
                0,
            ),
            Balde::Arquivo,
        );
        assert_eq!(
            (pacote.nome.as_str(), pacote.escopo),
            ("useInventado", Escopo::Externo)
        );
        assert_eq!(
            pacote.severidade,
            Severidade::Deny,
            "an installed package lacking the export"
        );
        let sugestao = um(
            &t,
            &diag(
                2724,
                "'\"react\"' has no exported member named 'useStat'. Did you mean 'useState'?",
                0,
            ),
            Balde::Arquivo,
        );
        assert_eq!(
            sugestao.nome, "useStat",
            "the name is the invented one, not the suggestion"
        );
        assert_eq!(sugestao.regra, "TS2724");
    }

    #[test]
    fn unknown_object_literal_property_is_a_kwarg() {
        let t = Arvore::nova("ts2353");
        let a = um(
            &t,
            &diag(
                2353,
                "Object literal may only specify known properties, and 'w' does not exist in type '{ x: number; }'.",
                3,
            ),
            Balde::Arquivo,
        );
        assert_eq!(
            (a.classe.as_str(), a.nome.as_str(), a.escopo),
            ("kwarg", "w", Escopo::Projeto)
        );
        let sugerido = um(
            &t,
            &diag(
                2561,
                "Object literal may only specify known properties, but 'colour' does not exist in type 'RequestInit'. Did you mean to write 'color'?",
                0,
            ),
            Balde::Arquivo,
        );
        assert_eq!(
            (sugerido.nome.as_str(), sugerido.escopo),
            ("colour", Escopo::Externo)
        );
    }

    #[test]
    fn a_missing_module_only_warns_because_it_may_be_installed_next() {
        let t = Arvore::nova("ts2307");
        let a = um(
            &t,
            &diag(
                2307,
                "Cannot find module 'pacote-fantasma' or its corresponding type declarations.",
                0,
            ),
            Balde::Arquivo,
        );
        assert_eq!(
            (a.classe.as_str(), a.nome.as_str(), a.escopo),
            ("import", "pacote-fantasma", Escopo::Projeto)
        );
        assert_eq!(a.severidade, Severidade::Warn);
        let inline = um(
            &t,
            &diag(
                2307,
                "Cannot find module 'pacote-fantasma' or its corresponding type declarations.",
                0,
            ),
            Balde::Inline,
        );
        assert_eq!(
            inline.severidade,
            Severidade::Deny,
            "inline code runs now: the import will fail"
        );
    }

    #[test]
    fn diagnostics_outside_the_name_classes_are_not_findings() {
        let t = Arvore::nova("outros");
        for (code, msg) in [
            (2322, "Type 'string' is not assignable to type 'number'."),
            (6133, "'x' is declared but its value is never read."),
            (
                2345,
                "Argument of type 'string' is not assignable to parameter of type 'number'.",
            ),
            (7006, "Parameter 'x' implicitly has an 'any' type."),
            (
                2580,
                "Cannot find name 'process'. Do you need to install type definitions for node?",
            ),
        ] {
            let p = pedido(&t, Balde::Arquivo, "");
            assert!(
                achado_de(&diag(code, msg, 0), &p, &t.raiz).is_none(),
                "TS{code}"
            );
        }
    }

    #[test]
    fn the_code_may_arrive_as_a_string_and_a_missing_range_drops_the_finding() {
        let t = Arvore::nova("codes");
        let p = pedido(&t, Balde::Arquivo, "");
        let d = json!({"code": "TS2304", "message": "Cannot find name 'x'.",
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}});
        assert!(achado_de(&d, &p, &t.raiz).is_some());
        let sem_range = json!({"code": 2304, "message": "Cannot find name 'x'."});
        assert!(
            achado_de(&sem_range, &p, &t.raiz).is_none(),
            "no line, no finding"
        );
    }

    // ── the normalizer filters by source (deno mixes lint and type errors) ──

    #[test]
    fn the_deno_normalizer_accepts_only_the_type_checker_source() {
        let t = Arvore::nova("fonte");
        let mut lint = diag(2304, "Cannot find name 'x'.", 0);
        lint["source"] = json!("deno-lint");
        let ts = diag(2304, "Cannot find name 'y'.", 1);
        let norm = NormTs {
            fonte: Some("deno-ts"),
        };
        let p = pedido(&t, Balde::Arquivo, "");
        let achados = norm
            .achados(&[lint.clone(), ts.clone()], &p, &t.raiz)
            .unwrap();
        assert_eq!(achados.len(), 1);
        assert_eq!(achados[0].nome, "y");
        let sem_filtro = NormTs { fonte: None };
        assert_eq!(
            sem_filtro.achados(&[lint, ts], &p, &t.raiz).unwrap().len(),
            2
        );
    }

    // ── pure helpers ───────────────────────────────────────────────────────

    #[test]
    fn quoted_runs_pair_up_single_quotes_in_order() {
        assert_eq!(
            aspas_simples("Module '\"./m\"' has no exported member 'x'."),
            ["\"./m\"", "x"]
        );
        assert_eq!(aspas_simples("sem aspas"), Vec::<&str>::new());
    }

    #[test]
    fn lib_types_are_recognised_by_head_and_object_or_union_types_are_not() {
        for lib in [
            "string",
            "number[]",
            "Array<string>",
            "Promise<void>",
            "Map<string, number>",
            "Date",
            "HTMLInputElement",
            "SVGPathElement",
            "typeof import(\"fs\")",
            "RequestInit",
        ] {
            assert!(tipo_de_lib(lib), "{lib}");
        }
        for nao in [
            "Ponto",
            "{ x: number; }",
            "string | undefined",
            "A & B",
            "Props",
            "Foo<string>",
            "",
        ] {
            assert!(!tipo_de_lib(nao), "{nao}");
        }
    }

    #[test]
    fn lib_type_tables_are_sorted_for_binary_search() {
        let mut sorted = NODE_BUILTINS;
        sorted.sort_unstable();
        assert_eq!(sorted, NODE_BUILTINS);
        // TIPOS_DE_LIB is searched with `contains`, but a duplicate would hide a typo
        let mut vistos = std::collections::HashSet::new();
        assert!(
            TIPOS_DE_LIB.iter().all(|t| vistos.insert(*t)),
            "duplicate entry"
        );
    }

    #[test]
    fn package_names_keep_the_scope_and_drop_the_subpath() {
        assert_eq!(nome_do_pacote("react"), "react");
        assert_eq!(nome_do_pacote("react/jsx-runtime"), "react");
        assert_eq!(nome_do_pacote("@scope/pkg"), "@scope/pkg");
        assert_eq!(nome_do_pacote("@scope/pkg/sub/dir"), "@scope/pkg");
    }

    #[test]
    fn a_specifier_is_external_only_when_it_is_installed_or_a_node_module() {
        let t = Arvore::nova("spec");
        t.arquivo("node_modules/@acme/ui/package.json", "{}");
        t.arquivo("web/node_modules/local-pkg/package.json", "{}");
        let origem = t.raiz.join("web/src/a.ts");
        assert!(especificador_externo("node:fs", &origem, &t.raiz));
        assert!(
            especificador_externo("fs", &origem, &t.raiz),
            "built-in without the prefix"
        );
        assert!(
            especificador_externo("@acme/ui/button", &origem, &t.raiz),
            "scoped, found at the root"
        );
        assert!(
            especificador_externo("local-pkg", &origem, &t.raiz),
            "found in web/node_modules"
        );
        assert!(!especificador_externo(
            "pacote-nao-instalado",
            &origem,
            &t.raiz
        ));
        assert!(!especificador_externo("./m", &origem, &t.raiz));
        assert!(!especificador_externo("../m", &origem, &t.raiz));
        assert!(
            !especificador_externo("@/lib/x", &origem, &t.raiz),
            "tsconfig alias, not a package"
        );
        assert!(!especificador_externo("#internal", &origem, &t.raiz));
        // a node_modules ABOVE the project root is not the project's
        let fora = origem.parent().unwrap().join("../../..");
        assert!(!especificador_externo(
            "pacote-nao-instalado",
            &fora.join("x.ts"),
            &t.raiz
        ));
    }
}
