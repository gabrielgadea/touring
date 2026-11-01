//! End-to-end tests of the TS/TSX/JS adapters through `vgp::check` — the same entry the hooks and
//! the judge use. Every class has a RED test (the stub answered `unknown`) and controls that must
//! pass clean: the symbol exists, is re-exported, comes from an installed package, or hides behind
//! a dynamic construct (`nao_medido`, never a finding).

use std::collections::HashMap;
use std::path::Path;

use super::util::apoio;
use crate::vgp::{CheckInput, CheckOutput, Contexto, check_em};

/// A temp project (outside `/tmp`, see [`apoio`]): `(relative path, content)` files.
fn projeto(arquivos: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = apoio::dir_temporario();
    for (rel, conteudo) in arquivos {
        let p = tmp.path().join(rel);
        std::fs::create_dir_all(p.parent().expect("pai")).expect("mkdir");
        std::fs::write(p, conteudo).expect("write");
    }
    tmp
}

fn rodar(dir: &Path, rel: &str, lang: &str, conteudo: &str) -> CheckOutput {
    check_em(
        &CheckInput {
            path: dir.join(rel).to_string_lossy().to_string(),
            content: conteudo.to_string(),
            project_root: dir.to_path_buf(),
            disk: None,
            overlays: HashMap::new(),
            lang: lang.to_string(),
        },
        Contexto::Execucao,
    )
}

/// (classe, nome, severidade), sorted.
fn achados(out: &CheckOutput) -> Vec<(String, String, String)> {
    let mut v: Vec<_> = out
        .findings
        .iter()
        .map(|f| (f.classe.clone(), f.nome.clone(), f.severidade.clone()))
        .collect();
    v.sort();
    v
}

fn de(classe: &str, nome: &str, sev: &str) -> (String, String, String) {
    (classe.into(), nome.into(), sev.into())
}

fn limpo(out: &CheckOutput) {
    assert!(
        out.findings.is_empty(),
        "esperava limpo, veio {:?}",
        achados(out)
    );
}

// ======================================================================================== import

/// V-4 (31/10/2026): um specifier que sai das raízes permitidas não é
/// reportado como módulo faltando — e o caminho não é ecoado.
#[test]
fn um_import_fora_das_raizes_nao_e_reportado_como_faltando() {
    let p = projeto(&[("src/a.ts", "")]);
    let out = rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { x } from '../../../../etc/passwd';\n",
    );
    assert!(achados(&out).is_empty(), "{:?}", achados(&out));
}

#[test]
fn modulo_relativo_inexistente_e_achado_import_com_o_ultimo_segmento() {
    let p = projeto(&[("src/a.ts", "")]);
    let out = rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { x } from './__vgpModAusente1';\n",
    );
    assert_eq!(
        achados(&out),
        vec![de("import", "__vgpModAusente1", "warn")]
    );
    assert_eq!(out.verdict, "warn");
    assert_eq!(out.findings[0].linha, 1);
}

#[test]
fn modulo_inexistente_nega_onde_o_codigo_roda_agora() {
    let p = projeto(&[]);
    // O arquivo está em `/tmp`: ele só nega QUANDO ALGO O RODA (o FRAG,
    // 10/10/2026 — um fragmento em `/tmp` não é erro até algo rodar). Por
    // isso o teste PERGUNTA no contexto de execução.
    let out = crate::vgp::check_em(
        &CheckInput {
            path: "/tmp/vgp-vts-a.ts".into(),
            content: "import { x } from './nao-existe';\n".into(),
            project_root: p.path().to_path_buf(),
            disk: None,
            overlays: HashMap::new(),
            lang: "typescript".into(),
        },
        crate::vgp::Contexto::Execucao,
    );
    assert_eq!(achados(&out), vec![de("import", "nao-existe", "deny")]);
}

#[test]
fn resolucao_de_extensao_index_e_substituicao_js_para_ts() {
    let p = projeto(&[
        ("src/m.ts", "export const a = 1;"),
        ("src/c.tsx", "export const C = 1;"),
        ("src/pasta/index.ts", "export const i = 1;"),
        ("src/dados.json", "{}"),
        ("src/estilo.css", ""),
        ("src/logo.svg", ""),
    ]);
    let fonte = "import { a } from './m';\nimport { a as a2 } from './m.js';\nimport { C } from './c';\n\
                 import { i } from './pasta';\nimport dados from './dados.json';\nimport './estilo.css';\n\
                 import logo from './logo.svg?url';\nimport { i as i2 } from './pasta/index';\n";
    limpo(&rodar(p.path(), "src/a.ts", "typescript", fonte));
}

#[test]
fn alias_de_paths_do_tsconfig_resolve_e_inexistente_e_achado() {
    let p = projeto(&[
        (
            "tsconfig.json",
            "{ // comentário\n \"compilerOptions\": { \"paths\": { \"@/*\": [\"./src/*\"], }, }, }",
        ),
        ("src/util/fmt.ts", "export const fmt = 1;"),
    ]);
    limpo(&rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { fmt } from '@/util/fmt';\n",
    ));
    let out = rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { fmt } from '@/util/ausente';\n",
    );
    assert_eq!(achados(&out), vec![de("import", "ausente", "warn")]);
}

#[test]
fn extends_e_references_do_tsconfig_trazem_os_paths() {
    let p = projeto(&[
        (
            "tsconfig.json",
            r#"{ "files": [], "references": [{ "path": "./tsconfig.app.json" }] }"#,
        ),
        (
            "tsconfig.app.json",
            r#"{ "extends": "./tsconfig.base.json" }"#,
        ),
        (
            "tsconfig.base.json",
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "~/*": ["lib/*"] } } }"#,
        ),
        ("lib/x.ts", "export const x = 1;"),
    ]);
    limpo(&rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { x } from '~/x';\nimport { x as y } from 'lib/x';\n",
    ));
}

#[test]
fn nome_importado_que_o_modulo_nao_exporta_e_achado_com_sugestao() {
    let p = projeto(&[(
        "src/m.ts",
        "export const formatar = 1; export function tratar() {}",
    )]);
    let out = rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { formatr, tratar } from './m';\n",
    );
    assert_eq!(achados(&out), vec![de("import", "formatr", "warn")]);
    let f = &out.findings[0];
    assert!(
        f.sugestao.contains("Você quis dizer `formatar`"),
        "{}",
        f.sugestao
    );
    assert!(f.reais.contains(&"tratar".to_string()));
}

#[test]
fn formas_de_export_que_contam() {
    let p = projeto(&[
        (
            "m.ts",
            "export const a = 1; export function f() {} export class C {} export interface I {} export type T = 1;\n\
             export enum E { X } export namespace N {} const loc = 1; export { loc as renomeado };\n\
             export { z } from './z'; export * from './estrela'; export default function () {}",
        ),
        ("z.ts", "export const z = 1;"),
        ("estrela.ts", "export const daEstrela = 1;"),
    ]);
    let fonte = "import d, { a, f, C, I, T, E, N, renomeado, z, daEstrela } from './m';\nexport { a as aa } from './m';\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn default_ausente_e_achado() {
    let p = projeto(&[("m.ts", "export const a = 1;")]);
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import Padrao from './m';\n",
    );
    assert_eq!(achados(&out), vec![de("import", "default", "warn")]);
}

#[test]
fn export_star_nao_resolvido_vira_nao_medido_e_nao_achado() {
    let p = projeto(&[(
        "m.ts",
        "export * from 'pacote-externo'; export const a = 1;",
    )]);
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import { talvez } from './m';\n",
    );
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
    assert!(
        out.motivo
            .as_deref()
            .is_some_and(|m| m.contains("pacote-externo")),
        "{:?}",
        out.motivo
    );
}

#[test]
fn commonjs_dinamico_e_nao_medido() {
    let p = projeto(&[("lib.js", "module.exports = construir();")]);
    let out = rodar(
        p.path(),
        "a.js",
        "javascript",
        "const { x } = require('./lib');\nimport { y } from './lib';\n",
    );
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
}

#[test]
fn pacote_instalado_ou_builtin_nunca_e_achado() {
    let p = projeto(&[("node_modules/react/index.js", "")]);
    limpo(&rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import React from 'react';\nimport fs from 'node:fs';\nimport path from 'path';\nimport x from 'nao-instalado-mas-nao-julgado';\n",
    ));
}

#[test]
fn require_e_import_dinamico_de_modulo_inexistente_sao_achados() {
    let p = projeto(&[]);
    let out = rodar(
        p.path(),
        "a.js",
        "javascript",
        "const a = require('./sumiu');\nasync function f() { return import('./tambem-sumiu'); }\n",
    );
    assert_eq!(
        achados(&out),
        vec![
            de("import", "sumiu", "warn"),
            de("import", "tambem-sumiu", "warn")
        ]
    );
}

#[test]
fn require_com_variavel_e_nao_medido() {
    let p = projeto(&[]);
    let out = rodar(
        p.path(),
        "a.js",
        "javascript",
        "const nome = process.argv[2];\nconst m = require(nome);\n",
    );
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
}

#[test]
fn sobreposicao_do_instante_vale_mais_que_o_disco() {
    let p = projeto(&[]);
    let alvo = p.path().join("m.ts").to_string_lossy().to_string();
    let out = check_em(
        &CheckInput {
            path: p.path().join("a.ts").to_string_lossy().to_string(),
            content: "import { so } from './m';\n".into(),
            project_root: p.path().to_path_buf(),
            disk: None,
            overlays: [(alvo, "export const so = 1;".to_string())].into(),
            lang: "typescript".into(),
        },
        Contexto::Execucao,
    );
    limpo(&out);
}

#[test]
fn codigo_inline_resolve_relativo_a_raiz_do_projeto() {
    let p = projeto(&[("ajuda.js", "exports.f = 1;")]);
    let inline = |c: &str| {
        check_em(
            &CheckInput {
                path: "<string>".into(),
                content: c.into(),
                project_root: p.path().to_path_buf(),
                disk: None,
                overlays: HashMap::new(),
                lang: "javascript".into(),
            },
            Contexto::Execucao,
        )
    };
    limpo(&inline("require('./ajuda');\n"));
    assert_eq!(
        achados(&inline("require('./nada');\n")),
        vec![de("import", "nada", "deny")]
    );
}

// ======================================================================================== nome

#[test]
fn nome_sem_declaracao_e_achado() {
    let p = projeto(&[("package.json", "{}")]);
    let out = rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "const x = 1;\nvgpAusente;\n",
    );
    assert_eq!(achados(&out), vec![de("nome", "vgpAusente", "warn")]);
    assert_eq!(out.findings[0].linha, 2);
}

#[test]
fn nome_nega_em_tmp() {
    let out = check_em(
        &CheckInput {
            path: "/tmp/vgp-vts-b.ts".into(),
            content: "faltando();\n".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "typescript".into(),
        },
        Contexto::Execucao,
    );
    assert_eq!(achados(&out), vec![de("nome", "faltando", "deny")]);
}

#[test]
fn nomes_declarados_globais_e_hoisted_passam() {
    let p = projeto(&[("package.json", "{}"), ("m.ts", "export const a = 1;")]);
    let fonte = "import { a } from './m';\nconst b = 1;\nfunction f(p = b) { return p + a + g(); }\nfunction g() { return console.log(window, document, process, setTimeout, Promise, JSON, Math); }\n\
                 class K { m() { return K; } }\nenum E { X }\nnamespace N { export const v = 1; }\nfor (const i of [1]) { i; }\ntry { f(); } catch (e) { e; }\nf(K, E, N, typeof naoDeclarada);\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn global_declarado_em_outro_arquivo_do_projeto_nao_e_achado() {
    let p = projeto(&[
        ("package.json", "{}"),
        ("src/globais.d.ts", "declare const __APP_VERSION__: string;"),
        ("src/script.js", "function ajudanteGlobal() {}"),
    ]);
    limpo(&rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "console.log(__APP_VERSION__);\n",
    ));
    limpo(&rodar(
        p.path(),
        "src/b.js",
        "javascript",
        "ajudanteGlobal();\n",
    ));
}

#[test]
fn global_de_pacote_types_nao_e_achado() {
    let p = projeto(&[
        ("package.json", "{}"),
        (
            "node_modules/@types/meu/index.d.ts",
            "declare var globalDoPacote: number;",
        ),
    ]);
    limpo(&rodar(p.path(), "a.ts", "typescript", "globalDoPacote;\n"));
}

#[test]
fn atribuicao_implicita_em_js_define_o_global() {
    let p = projeto(&[("package.json", "{}")]);
    limpo(&rodar(
        p.path(),
        "a.js",
        "javascript",
        "contador = 0;\nfunction inc() { contador += 1; }\n",
    ));
}

#[test]
fn desestruturacao_e_parametros_ligam_os_nomes() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "const { a, b: { c }, ...r } = obj();\nfunction obj(): any { return {}; }\nconst [d, e = 1] = [1];\n\
                 const f = ({ x, y = a }: { x: number; y?: number }, ...resto: number[]) => x + y + c + d + e + resto.length + Object.keys(r).length;\nf({ x: 1 });\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn erro_de_sintaxe_e_nao_medido_e_nunca_achado() {
    let p = projeto(&[]);
    let out = rodar(p.path(), "a.ts", "typescript", "const = ;\nvgpAusente;\n");
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
    assert!(
        out.motivo.as_deref().is_some_and(|m| m.contains("sintaxe")),
        "{:?}",
        out.motivo
    );
}

#[test]
fn e_comercial_solto_em_texto_jsx_nao_derruba_o_arquivo_e_nao_vira_nome() {
    // a gramática TSX recusa `&` solto em texto JSX e engole a palavra seguinte como identificador:
    // o texto é apagado e o resto do arquivo é verificado normalmente (`drop` não é nome livre)
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "export const A = () => <h3>Drag & drop your file</h3>;\nvgpNomeAusente;\n";
    let out = rodar(p.path(), "a.tsx", "tsx", fonte);
    assert_eq!(achados(&out), vec![de("nome", "vgpNomeAusente", "warn")]);
    assert_eq!(out.findings[0].linha, 2);
    // o texto apagado não esconde nem inventa nada: arquivo só com o `&` é limpo, não unknown
    let out = rodar(
        p.path(),
        "b.tsx",
        "tsx",
        "export const B = () => <p>Tom & Jerry</p>;\n",
    );
    limpo(&out);
    assert_eq!(out.verdict, "allow");
}

#[test]
fn erro_de_sintaxe_que_nao_e_texto_de_jsx_continua_sem_medir() {
    // `&` solto FORA de texto JSX, ou um erro que guarda código: a árvore não é confiável
    let p = projeto(&[("package.json", "{}")]);
    let fora = rodar(
        p.path(),
        "a.tsx",
        "tsx",
        "const a = 1 & ;\nvgpNomeAusente;\n",
    );
    limpo(&fora);
    assert_eq!(fora.verdict, "unknown");
    let chaves = rodar(
        p.path(),
        "b.tsx",
        "tsx",
        "export const B = () => <p>a & {vgpNomeAusente}</p>;\n",
    );
    assert!(
        chaves.findings.iter().all(|f| f.severidade != "deny"),
        "{:?}",
        achados(&chaves)
    );
}

// ======================================================================================== componente (tsx)

#[test]
fn componente_nao_importado_nem_definido_e_achado_no_tsx() {
    let p = projeto(&[
        ("package.json", "{}"),
        ("c.tsx", "export const Existe = () => null;"),
    ]);
    let fonte = "import { Existe } from './c';\nconst Local = () => null;\nexport const A = () => <div><Existe /><Local /><VgpCompAusente /></div>;\n";
    let out = rodar(p.path(), "a.tsx", "tsx", fonte);
    assert_eq!(
        achados(&out),
        vec![de("componente", "VgpCompAusente", "warn")]
    );
}

#[test]
fn tag_minuscula_e_membro_importado_nao_sao_componente_ausente() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte =
        "import * as Ui from './ui';\nexport const A = () => <div><Ui.Botao /><span /></div>;\n";
    // ui não existe (achado de import), mas NENHUM achado de componente
    let out = rodar(p.path(), "a.tsx", "tsx", fonte);
    assert!(
        out.findings.iter().all(|f| f.classe != "componente"),
        "{:?}",
        achados(&out)
    );
}

#[test]
fn jsx_em_javascript_tambem_acha_componente() {
    let p = projeto(&[("package.json", "{}")]);
    let out = rodar(
        p.path(),
        "a.jsx",
        "javascript",
        "export const A = () => <Faltando />;\n",
    );
    assert_eq!(achados(&out), vec![de("componente", "Faltando", "warn")]);
}

// ======================================================================================== atributo

#[test]
fn membro_inexistente_de_builtin_e_externo_e_nega() {
    let p = projeto(&[("package.json", "{}")]);
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "export const _p = new Date().vgpPropAusente;\n",
    );
    assert_eq!(
        achados(&out),
        vec![de("atributo", "vgpPropAusente", "deny")]
    );
    limpo(&rodar(
        p.path(),
        "b.ts",
        "typescript",
        "export const _p = new Date().getTime() + new Date().getFullYear();\n",
    ));
    let js = rodar(
        p.path(),
        "a.js",
        "javascript",
        "new Date().vgpPropAusente;\n",
    );
    assert_eq!(achados(&js), vec![de("atributo", "vgpPropAusente", "deny")]);
}

#[test]
fn vizinhanca_grande_demais_para_varrer_vira_nao_medido_e_nunca_allow() {
    // JS: o membro pode ter sido posto no protótipo por outra fonte; sem varrer TUDO, a ausência não está provada
    let p = projeto(&[("package.json", "{}")]);
    for i in 0..2600 {
        std::fs::write(p.path().join(format!("f{i}.js")), "x;\n").expect("write");
    }
    let out = rodar(
        p.path(),
        "a.js",
        "javascript",
        "new Date().vgpPropAusente;\n",
    );
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
    assert!(
        out.motivo
            .as_deref()
            .is_some_and(|m| m.contains("vizinhança") && m.contains("vgpPropAusente")),
        "{:?}",
        out.motivo
    );
}

#[test]
fn membros_de_string_array_e_map_conhecidos_passam() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "const s = 'abc';\nconst m = new Map<string, number>();\nexport const r = [s.trim(), s.length, [1].map(x => x), [1].at(0), m.get('a'), m.size, new Set([1]).has(1), new Error('x').stack, /a/.test(s)];\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn tipo_com_prototype_patch_no_projeto_nao_e_achado() {
    let p = projeto(&[
        ("package.json", "{}"),
        (
            "patch.js",
            "Date.prototype.formatar = function () { return ''; };",
        ),
    ]);
    limpo(&rodar(
        p.path(),
        "a.ts",
        "typescript",
        "new Date().formatar();\n",
    ));
    // o patch no próprio arquivo também vale
    limpo(&rodar(
        p.path(),
        "b.js",
        "javascript",
        "Date.prototype.outro = function () {};\nnew Date().outro();\n",
    ));
}

#[test]
fn membro_de_classe_do_projeto_e_this_sao_avisos() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "class Pessoa { nome = 'a'; saudar() { return this.nome + this.sobrenome; } }\nconst p = new Pessoa();\np.nome; p.saudar(); p.idade;\n";
    let out = rodar(p.path(), "a.ts", "typescript", fonte);
    assert_eq!(
        achados(&out),
        vec![
            de("atributo", "idade", "warn"),
            de("atributo", "sobrenome", "warn")
        ]
    );
}

#[test]
fn classe_js_define_membros_por_atribuicao_em_this() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "class A { constructor() { this.x = 1; } usar() { return this.x + this.y; } }\n";
    let out = rodar(p.path(), "a.js", "javascript", fonte);
    assert_eq!(achados(&out), vec![de("atributo", "y", "warn")]);
}

#[test]
fn classe_com_base_externa_ou_mixin_nunca_e_achado() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "import { Base, mixin } from 'pacote-externo';\nclass A extends Base { m() { return this.qualquer; } }\nclass B extends mixin(Base) { m() { return this.outro; } }\nclass C { constructor(o) { Object.assign(this, o); } m() { return this.dinamico; } }\n";
    limpo(&rodar(p.path(), "a.js", "javascript", fonte));
}

#[test]
fn estreitamento_por_instanceof_ou_guarda_abstem() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "class Animal { nome = ''; }\nclass Cao extends Animal { latir() {} }\n\
                 function f(a: Animal) { if (a instanceof Cao) { a.latir(); } }\n\
                 function g(a: Animal) { if (ehCao(a)) { a.latir(); } }\ndeclare function ehCao(a: Animal): a is Cao;\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn receptor_desconhecido_ou_reatribuido_nao_gera_achado() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "declare function externa(): any;\nconst a = externa();\na.qualquer;\nlet d = new Date();\nd = externa();\nd.qualquer;\nfunction f(x) { return x.y; }\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn escrita_em_propriedade_nova_e_expando_nao_e_achado() {
    let p = projeto(&[("package.json", "{}")]);
    limpo(&rodar(
        p.path(),
        "a.js",
        "javascript",
        "const d = new Date();\nd.rotulo = 'x';\n",
    ));
}

// ======================================================================================== kwarg

#[test]
fn chave_a_mais_em_literal_com_tipo_inline_e_achado() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "export const _p: { campo: number } = { campo: 1, vgpPropAusente: 2 };\n";
    let out = rodar(p.path(), "a.ts", "typescript", fonte);
    assert_eq!(achados(&out), vec![de("kwarg", "vgpPropAusente", "warn")]);
}

#[test]
fn interface_local_importada_e_alias_de_objeto() {
    let p = projeto(&[
        ("package.json", "{}"),
        (
            "tipos.ts",
            "export interface Opcoes { largura: number; altura?: number }\nexport type Cor = { r: number; g: number }",
        ),
    ]);
    let fonte = "import { Opcoes, Cor } from './tipos';\ninterface Local { a: number }\n\
                 const o1: Opcoes = { largura: 1, altura: 2 };\nconst o2: Opcoes = { largura: 1, profundidade: 3 };\n\
                 const o3: Local = { a: 1, b: 2 };\nconst o4: Cor = { r: 1, g: 2, azul: 3 };\n";
    let out = rodar(p.path(), "a.ts", "typescript", fonte);
    assert_eq!(
        achados(&out),
        vec![
            de("kwarg", "azul", "warn"),
            de("kwarg", "b", "warn"),
            de("kwarg", "profundidade", "warn")
        ]
    );
}

#[test]
fn literal_com_spread_indice_ou_tipo_aberto_nao_gera_achado() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "interface Aberto { [k: string]: number }\ninterface Base { a: number }\ninterface Filho extends Externo { b: number }\ndeclare const resto: any;\n\
                 const a: Aberto = { qualquer: 1 };\nconst b: Base = { ...resto, extra: 1 };\nconst c: Filho = { b: 1, dado_do_pai: 2 };\nconst d: Record<string, number> = { x: 1 };\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn satisfies_e_argumento_de_funcao_local_e_importada() {
    let p = projeto(&[
        ("package.json", "{}"),
        (
            "lib.ts",
            "export interface Cfg { a: number }\nexport function usar(c: Cfg) {}",
        ),
    ]);
    let fonte = "import { usar, Cfg } from './lib';\ninterface Op { x: number }\nfunction local(o: Op) {}\n\
                 local({ x: 1, y: 2 });\nusar({ a: 1, zz: 2 });\nusar({ a: 1 });\nconst s = ({ x: 1, w: 3 }) satisfies Op;\n";
    let out = rodar(p.path(), "a.ts", "typescript", fonte);
    assert_eq!(
        achados(&out),
        vec![
            de("kwarg", "w", "warn"),
            de("kwarg", "y", "warn"),
            de("kwarg", "zz", "warn")
        ]
    );
}

#[test]
fn literal_aninhado_checa_o_tipo_do_membro() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "interface Dentro { a: number }\ninterface Fora { dentro: Dentro }\nconst f: Fora = { dentro: { a: 1, sobra: 2 } };\n";
    let out = rodar(p.path(), "a.ts", "typescript", fonte);
    assert_eq!(achados(&out), vec![de("kwarg", "sobra", "warn")]);
}

// ======================================================================================== prop (tsx)

#[test]
fn prop_ausente_em_componente_do_projeto_e_achado() {
    let p = projeto(&[
        ("package.json", "{}"),
        (
            "btn.tsx",
            "export interface BtnProps { rotulo: string; onClick?: () => void }\nexport function Btn(p: BtnProps) { return null; }",
        ),
    ]);
    let fonte = "import { Btn } from './btn';\nexport const A = () => <Btn rotulo=\"x\" onClick={() => {}} vgpPropAusente={1} data-x=\"1\" key=\"k\" />;\n";
    let out = rodar(p.path(), "a.tsx", "tsx", fonte);
    assert_eq!(achados(&out), vec![de("prop", "vgpPropAusente", "warn")]);
}

#[test]
fn prop_de_componente_com_fc_e_props_abertas() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "type P = { a: number };\nconst C: React.FC<P> = (props) => null;\ninterface Aberto extends React.HTMLAttributes<HTMLDivElement> { a: number }\nconst D = (p: Aberto) => null;\n\
                 export const X = () => <><C a={1} b={2} /><D a={1} qualquer={2} className=\"x\" /></>;\n";
    let out = rodar(p.path(), "a.tsx", "tsx", fonte);
    assert_eq!(achados(&out), vec![de("prop", "b", "warn")]);
}

#[test]
fn spread_em_jsx_desliga_o_julgamento_de_props_do_componente() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "interface P { a: number }\nconst C = (p: P) => null;\ndeclare const resto: any;\nexport const X = () => <C a={1} extra={2} {...resto} />;\n";
    limpo(&rodar(p.path(), "a.tsx", "tsx", fonte));
}

#[test]
fn atributo_desconhecido_em_elemento_intrinseco_do_react_e_aviso() {
    let p = projeto(&[
        ("package.json", "{}"),
        (
            "tsconfig.json",
            r#"{ "compilerOptions": { "jsx": "react-jsx" } }"#,
        ),
        ("node_modules/react/index.js", ""),
    ]);
    let fonte = "export const A = () => <div className=\"x\" id=\"y\" onClick={() => {}} aria-label=\"l\" data-t=\"1\" style={{}} vgpPropAusente={1} />;\n";
    let out = rodar(p.path(), "a.tsx", "tsx", fonte);
    assert_eq!(achados(&out), vec![de("prop", "vgpPropAusente", "warn")]);
}

#[test]
fn sem_react_instalado_ou_com_outro_jsx_nao_julga_atributo_intrinseco() {
    let p = projeto(&[("package.json", "{}")]);
    limpo(&rodar(
        p.path(),
        "a.tsx",
        "tsx",
        "export const A = () => <div class=\"x\" qualquer={1} />;\n",
    ));
    let q = projeto(&[
        ("package.json", "{}"),
        (
            "tsconfig.json",
            r#"{ "compilerOptions": { "jsx": "preserve", "jsxImportSource": "solid-js" } }"#,
        ),
        ("node_modules/react/index.js", ""),
    ]);
    limpo(&rodar(
        q.path(),
        "a.tsx",
        "tsx",
        "export const A = () => <div class=\"x\" qualquer={1} />;\n",
    ));
}

// ======================================================================================== despacho

#[test]
fn as_tres_linguagens_respondem_pelo_pipeline_e_nao_pelo_stub() {
    let p = projeto(&[("package.json", "{}")]);
    for (lang, rel) in [
        ("typescript", "a.ts"),
        ("tsx", "a.tsx"),
        ("javascript", "a.js"),
    ] {
        let out = rodar(p.path(), rel, lang, "import { x } from './nao-existe';\n");
        assert_eq!(out.lang, lang);
        assert_eq!(
            achados(&out),
            vec![de("import", "nao-existe", "warn")],
            "{lang}"
        );
        assert!(
            out.motivo
                .as_deref()
                .is_none_or(|m| !m.contains("ainda não implementado")),
            "{lang}: {:?}",
            out.motivo
        );
    }
}

#[test]
fn ts_e_tsx_sao_gramaticas_distintas() {
    let p = projeto(&[("package.json", "{}")]);
    // `<T>x` é uma asserção em .ts; em .tsx é um elemento JSX sem fechamento (erro de sintaxe)
    let ts = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "declare const x: unknown;\nexport const y = <string>x;\n",
    );
    assert_eq!(ts.verdict, "allow", "{:?}", ts.motivo);
    let tsx = rodar(
        p.path(),
        "a.tsx",
        "tsx",
        "declare const x: unknown;\nexport const y = <string>x;\n",
    );
    assert_eq!(tsx.verdict, "unknown");
}

#[test]
fn arquivo_limpo_e_allow_e_nao_unknown() {
    let p = projeto(&[("package.json", "{}")]);
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "export const a = 1;\nexport function f(x: number) { return x + a; }\n",
    );
    assert_eq!(out.verdict, "allow");
    assert!(out.motivo.is_none());
}

// ======================================================================================== resolução e abstenção

/// The names of the `import` findings, in discovery order.
fn nomes_de_import(out: &CheckOutput) -> Vec<String> {
    out.findings
        .iter()
        .filter(|f| f.classe == "import")
        .map(|f| f.nome.clone())
        .collect::<Vec<_>>()
}

#[test]
fn mjs_e_cjs_resolvem_para_mts_e_cts() {
    let p = projeto(&[
        ("src/m.mts", "export const a = 1;"),
        ("src/c.cts", "export const b = 1;"),
    ]);
    let fonte = "import { a, zz } from './m.mjs';\nimport { b, yy } from './c.cjs';\n";
    let out = rodar(p.path(), "src/a.ts", "typescript", fonte);
    // os módulos foram encontrados (nenhum achado de módulo ausente): só os nomes inexistentes sobram
    assert_eq!(
        achados(&out),
        vec![de("import", "yy", "warn"), de("import", "zz", "warn")]
    );
}

#[test]
fn extends_do_tsconfig_traz_os_paths_do_arquivo_base() {
    let p = projeto(&[
        ("tsconfig.json", r#"{ "extends": "./cfg/base.json" }"#),
        // sem baseUrl, o alvo é relativo ao arquivo que DEFINE o `paths`
        (
            "cfg/base.json",
            r#"{ "compilerOptions": { "paths": { "~/*": ["../lib/*"] } } }"#,
        ),
        ("lib/x.ts", "export const x = 1;"),
    ]);
    let out = rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { x, faltaAlias } from '~/x';\n",
    );
    assert_eq!(achados(&out), vec![de("import", "faltaAlias", "warn")]);
}

#[test]
fn references_do_tsconfig_trazem_os_paths_do_projeto_referenciado() {
    let p = projeto(&[
        (
            "tsconfig.json",
            r#"{ "files": [], "references": [{ "path": "./cfg/web.json" }] }"#,
        ),
        (
            "cfg/web.json",
            r#"{ "compilerOptions": { "paths": { "~/*": ["../lib/*"] } } }"#,
        ),
        ("lib/x.ts", "export const x = 1;"),
    ]);
    let out = rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { x, faltaAlias } from '~/x';\n",
    );
    assert_eq!(achados(&out), vec![de("import", "faltaAlias", "warn")]);
}

#[test]
fn base_url_resolve_especificador_nu_e_ancora_os_paths() {
    let p = projeto(&[
        (
            "tsconfig.json",
            r#"{ "compilerOptions": { "baseUrl": "./src", "paths": { "@/*": ["util/*"] } } }"#,
        ),
        ("src/util/fmt.ts", "export const fmt = 1;"),
    ]);
    // baseUrl: `util/fmt` resolve a partir de src/
    let nu = rodar(
        p.path(),
        "app/a.ts",
        "typescript",
        "import { fmt, zz } from 'util/fmt';\n",
    );
    assert_eq!(achados(&nu), vec![de("import", "zz", "warn")]);
    // paths: o alvo `util/*` é relativo ao baseUrl (src/), não ao tsconfig
    let alias = rodar(
        p.path(),
        "app/b.ts",
        "typescript",
        "import { fmt, yy } from '@/fmt';\n",
    );
    assert_eq!(achados(&alias), vec![de("import", "yy", "warn")]);
}

#[test]
fn alias_coringa_puro_e_pacote_instalado_nunca_provam_ausencia() {
    let p = projeto(&[
        (
            "tsconfig.json",
            r#"{ "compilerOptions": { "paths": { "*": ["./types/*"], "@app/*": ["./src/*"] } } }"#,
        ),
        ("node_modules/@app/ui/index.js", "module.exports = {};"),
    ]);
    // `*` cai para o node_modules (o TS também): `lodash` não é prova de nada
    limpo(&rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import _ from 'lodash';\n",
    ));
    // `@app/ui` existe como pacote instalado: ganha do alias cujo alvo não existe
    limpo(&rodar(
        p.path(),
        "b.ts",
        "typescript",
        "import { ui } from '@app/ui';\n",
    ));
    // sem o pacote instalado, o alias com alvo ausente É prova
    let q = projeto(&[(
        "tsconfig.json",
        r#"{ "compilerOptions": { "paths": { "@app/*": ["./src/*"] } } }"#,
    )]);
    assert_eq!(
        nomes_de_import(&rodar(
            q.path(),
            "c.ts",
            "typescript",
            "import { ui } from '@app/ui';\n"
        )),
        vec!["ui".to_string()]
    );
}

#[test]
fn diretorio_com_package_json_resolve_pela_entrada_declarada() {
    let p = projeto(&[
        (
            "lib/package.json",
            r#"{ "name": "lib", "main": "./dist/main.js" }"#,
        ),
        ("lib/dist/main.js", "export const a = 1;"),
    ]);
    let out = rodar(
        p.path(),
        "src/a.ts",
        "typescript",
        "import { a, zz } from '../lib';\n",
    );
    assert_eq!(achados(&out), vec![de("import", "zz", "warn")]);
}

#[test]
fn especificador_fora_do_alcance_e_abstencao_com_o_motivo() {
    let p = projeto(&[("package.json", "{}")]);
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import a from '/raiz/do/servidor';\nimport b from 'virtual:pwa-register';\n",
    );
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
    let m = out.motivo.as_deref().unwrap_or_default();
    assert!(
        m.contains("caminho absoluto") && m.contains("módulo virtual"),
        "{m}"
    );
}

#[test]
fn export_estrela_nao_reexporta_o_default() {
    let p = projeto(&[
        ("barril.ts", "export * from './x';"),
        ("x.ts", "export default 1; export const n = 1;"),
    ]);
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import d, { n } from './barril';\n",
    );
    assert_eq!(achados(&out), vec![de("import", "default", "warn")]);
}

#[test]
fn modulo_que_nao_da_para_ler_como_tabela_de_exports_e_abstencao() {
    let p = projeto(&[
        ("quebrado.ts", "export const a = ;\n"),
        ("script.js", "function util() {}\n"),
        ("aninhado.js", "exports.a.b = 1;\n"),
        ("junta.js", "Object.assign(module.exports, { a: 1 });\n"),
    ]);
    for (modulo, esperado) in [
        ("quebrado", "erro de sintaxe"),
        ("script.js", "script"),
        ("aninhado.js", "aninhado"),
        ("junta.js", "Object.assign"),
    ] {
        let fonte = format!("import {{ zz }} from './{modulo}';\n");
        let out = rodar(p.path(), "a.ts", "typescript", &fonte);
        limpo(&out);
        assert_eq!(out.verdict, "unknown", "{modulo}");
        assert!(
            out.motivo.as_deref().unwrap_or_default().contains(esperado),
            "{modulo}: {:?}",
            out.motivo
        );
    }
}

#[test]
fn export_default_nomeado_nao_exporta_o_nome() {
    let p = projeto(&[("m.ts", "export default function f() {}\n")]);
    let out = rodar(p.path(), "a.ts", "typescript", "import { f } from './m';\n");
    assert_eq!(achados(&out), vec![de("import", "f", "warn")]);
}

#[test]
fn enum_como_tipo_do_receptor_nao_prova_ausencia_de_membro() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "enum Cor { Vermelho }\nexport function f(c: Cor) { return c.toFixed(); }\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn declaracoes_conflitantes_do_mesmo_nome_abrem_a_forma() {
    // `interface A` + `type A` é erro do TS; o motor não escolhe um lado: a forma fica aberta
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "interface A { x: number }\ntype A = { y: number };\nexport function f(a: A) { return a.zzz; }\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn variavel_local_que_esconde_a_classe_de_topo_nao_e_a_classe() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "declare function make(): any;\nclass K { a = 1 }\nexport function f() { const K = make(); return new K().zzz; }\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", fonte));
}

#[test]
fn sinal_de_maior_ou_chave_solta_em_texto_jsx_continua_sem_medir() {
    // diferente do `&`, o erro destes engole um caractere que abre/fecha sintaxe: o arquivo abstém
    let p = projeto(&[("package.json", "{}")]);
    for texto in ["a > b", "a } b"] {
        let fonte = format!("export const A = () => <p>{texto}</p>;\nvgpNomeAusente;\n");
        let out = rodar(p.path(), "a.tsx", "tsx", &fonte);
        limpo(&out);
        assert_eq!(out.verdict, "unknown", "{texto}");
    }
}

#[test]
fn chave_presente_em_um_membro_da_uniao_nao_e_excedente() {
    let p = projeto(&[("package.json", "{}")]);
    let uniao = "type U = { a: number } | { b: number };\n";
    let bom = format!("{uniao}export const u: U = {{ a: 1 }};\nexport const v: U = {{ b: 1 }};\n");
    limpo(&rodar(p.path(), "a.ts", "typescript", &bom));
    // a chave que NENHUM membro tem continua sendo excedente
    let ruim = format!("{uniao}export const u: U = {{ zz: 1 }};\n");
    let out = rodar(p.path(), "b.ts", "typescript", &ruim);
    assert_eq!(achados(&out), vec![de("kwarg", "zz", "warn")]);
}

#[test]
fn instanceof_in_e_invariant_fora_de_if_tambem_abstem_do_atributo() {
    let p = projeto(&[("package.json", "{}")]);
    // sem nenhum desses construtos o membro ausente É achado (o controle que dá sentido aos três abaixo)
    let sem = "class Animal { nome = ''; }\nexport function f(a: Animal) { return a.latir; }\n";
    assert_eq!(
        achados(&rodar(p.path(), "a.ts", "typescript", sem)),
        vec![de("atributo", "latir", "warn")]
    );
    for fonte in [
        "class Animal { nome = ''; }\nclass Cao extends Animal { latir() {} }\nexport function f(a: Animal) { const e = a instanceof Cao; return [e, a.latir]; }\n",
        "class Animal { nome = ''; }\nexport function f(a: Animal) { const e = 'latir' in a; return [e, a.latir]; }\n",
        "class Animal { nome = ''; }\ndeclare function invariant(v: unknown): void;\nexport function f(a: Animal) { invariant(a); return a.latir; }\n",
    ] {
        limpo(&rodar(p.path(), "b.ts", "typescript", fonte));
    }
}

#[test]
fn arquivo_de_declaracao_so_tem_os_imports_verificados() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "import { x } from './ausente';\ndeclare const a: Date;\nvgpAusente;\n";
    let out = rodar(p.path(), "tipos.d.ts", "typescript", fonte);
    assert_eq!(achados(&out), vec![de("import", "ausente", "warn")]);
    assert!(
        out.motivo.as_deref().unwrap_or_default().contains(".d.ts"),
        "{:?}",
        out.motivo
    );
}

#[test]
fn reexport_nomeado_pede_o_nome_ao_modulo() {
    let p = projeto(&[("m.ts", "export const a = 1;")]);
    let out = rodar(
        p.path(),
        "b.ts",
        "typescript",
        "export { a, zz } from './m';\n",
    );
    assert_eq!(achados(&out), vec![de("import", "zz", "warn")]);
}

#[test]
fn elemento_customizado_e_atributo_com_hifen_nao_sao_julgados() {
    let p = projeto(&[("package.json", "{}"), ("node_modules/react/index.js", "")]);
    let fonte = "export const A = () => <div><my-element qualquer=\"x\" /><span data-x=\"1\" aria-label=\"y\" /></div>;\n";
    limpo(&rodar(p.path(), "a.tsx", "tsx", fonte));
}

// ======================================================================================== precisão em código real

#[test]
fn nome_que_o_proprio_arquivo_detecta_nao_e_achado() {
    let p = projeto(&[("package.json", "{}")]);
    // `typeof X` e `window.X`: o arquivo trata o nome como global OPCIONAL do host
    let fonte = "export function f() {\n  if (typeof MozWebSocket !== 'undefined') { return MozWebSocket; }\n  if (window.mermaid) { mermaid.initialize({}); }\n  return vgpAusente;\n}\n";
    let out = rodar(p.path(), "a.js", "javascript", fonte);
    assert_eq!(achados(&out), vec![de("nome", "vgpAusente", "warn")]);
}

#[test]
fn arquivo_minificado_ou_empacotado_nao_e_medido() {
    let p = projeto(&[("package.json", "{}")]);
    let corpo = format!("var a=1;{}vgpAusente();", "b=a;".repeat(600));
    let out = rodar(p.path(), "vendor.min.js", "javascript", &corpo);
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
    assert!(
        out.motivo
            .as_deref()
            .unwrap_or_default()
            .contains("minificado"),
        "{:?}",
        out.motivo
    );
}

#[test]
fn script_classico_avisa_que_a_pagina_pode_fornecer_o_nome_e_modulo_nao() {
    let p = projeto(&[("package.json", "{}")]);
    let script = rodar(
        p.path(),
        "app.js",
        "javascript",
        "gsap.to('.x', { x: 1 });\n",
    );
    assert_eq!(achados(&script), vec![de("nome", "gsap", "warn")]);
    assert!(
        script.findings[0].sugestao.contains("script clássico"),
        "{}",
        script.findings[0].sugestao
    );
    let modulo = rodar(
        p.path(),
        "mod.js",
        "javascript",
        "import fs from 'node:fs';\ngsap.to(fs, {});\n",
    );
    assert_eq!(achados(&modulo), vec![de("nome", "gsap", "warn")]);
    assert!(
        !modulo.findings[0].sugestao.contains("script clássico"),
        "{}",
        modulo.findings[0].sugestao
    );
}

#[test]
fn script_que_roda_agora_continua_negando_o_nome_ausente() {
    // em /tmp o host é o Node: nada mais fornece o nome (ao contrário de uma página com <script>)
    let out = check_em(
        &CheckInput {
            path: "/tmp/vgp-vts-c.js".into(),
            content: "gsap.to('.x', {});\n".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "javascript".into(),
        },
        Contexto::Execucao,
    );
    assert_eq!(achados(&out), vec![de("nome", "gsap", "deny")]);
}

#[test]
fn globais_de_carregador_amd_e_worker_sao_conhecidos() {
    let p = projeto(&[("package.json", "{}")]);
    limpo(&rodar(
        p.path(),
        "a.js",
        "javascript",
        "define(['a'], function () {});\nrequirejs.config({});\nimportScripts('x.js');\n",
    ));
}

#[test]
fn script_com_return_no_topo_roda_num_executor_que_injeta_nomes() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "phase('x');\nconst q = args;\nif (!q) { return { erro: 1 }; }\nawait agent(q);\n";
    let out = rodar(p.path(), "wf.js", "javascript", fonte);
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
    assert!(
        out.motivo
            .as_deref()
            .unwrap_or_default()
            .contains("`return` no topo"),
        "{:?}",
        out.motivo
    );
    // um `return` DENTRO de função não é sinal de executor: o nome ausente continua achado
    let comum = rodar(
        p.path(),
        "b.js",
        "javascript",
        "function f() { return 1; }\nvgpAusente(f);\n",
    );
    assert_eq!(achados(&comum), vec![de("nome", "vgpAusente", "warn")]);
    // CommonJS devolve no topo por causa do invólucro do Node: ali o host é o Node e o nome continua achado
    let modulo = rodar(
        p.path(),
        "c.js",
        "javascript",
        "const fs = require('node:fs');\nif (fs) { return 1; }\nvgpAusente();\n",
    );
    assert_eq!(achados(&modulo), vec![de("nome", "vgpAusente", "warn")]);
}

#[test]
fn grammar_js_do_tree_sitter_recebe_o_dsl_do_executor() {
    let p = projeto(&[("package.json", "{}")]);
    let dsl = "module.exports = grammar({ name: 'x', rules: { a: $ => seq(optional('a'), prec(1, choice('b', repeat1('c')))) } });\n";
    limpo(&rodar(p.path(), "grammar.js", "javascript", dsl));
    // o DSL é só do `grammar.js`; em outro arquivo os mesmos nomes continuam livres
    let outro = rodar(p.path(), "regras.js", "javascript", dsl);
    assert!(
        outro.findings.iter().any(|f| f.nome == "grammar"),
        "{:?}",
        achados(&outro)
    );
    // e dentro do `grammar.js` um nome que não é do DSL continua achado
    let ruim = "module.exports = grammar({ name: 'x', rules: { a: $ => seq(vgpAusente) } });\n";
    assert_eq!(
        achados(&rodar(p.path(), "grammar.js", "javascript", ruim)),
        vec![de("nome", "vgpAusente", "warn")]
    );
}

#[test]
fn require_e_import_dinamico_marcam_o_arquivo_como_modulo() {
    let p = projeto(&[("package.json", "{}"), ("lib.js", "exports.v = 1;\n")]);
    for fonte in [
        "const lib = require('./lib');\ngsap.to(lib, {});\n",
        "import('./lib').then((l) => gsap.to(l, {}));\n",
    ] {
        let out = rodar(p.path(), "mod.js", "javascript", fonte);
        assert_eq!(achados(&out), vec![de("nome", "gsap", "warn")], "{fonte}");
        assert!(
            !out.findings[0].sugestao.contains("script clássico"),
            "{fonte}: {}",
            out.findings[0].sugestao
        );
    }
}

#[test]
fn funcao_local_que_esconde_a_de_topo_nao_checa_o_literal_contra_a_de_topo() {
    let p = projeto(&[("package.json", "{}")]);
    let topo = "interface Op { x: number }\nfunction usar(o: Op) {}\n";
    // sem o local, o literal TEM chave excedente contra a função de topo
    let sem = format!("{topo}export const r = usar({{ y: 1 }});\n");
    assert_eq!(
        achados(&rodar(p.path(), "a.ts", "typescript", &sem)),
        vec![de("kwarg", "y", "warn")]
    );
    // com `const usar` local no escopo da chamada, a chamada é de OUTRA função
    let com = format!(
        "{topo}export function g() {{\n  const usar = (q: {{ y: number }}) => q.y;\n  return usar({{ y: 1 }});\n}}\n"
    );
    limpo(&rodar(p.path(), "a.ts", "typescript", &com));
}

#[test]
fn classe_local_que_esconde_a_de_topo_nao_e_a_de_topo() {
    let p = projeto(&[("package.json", "{}")]);
    let sem = "class K { a = 1 }\nexport const v = new K().zzz;\n";
    assert!(
        !rodar(p.path(), "a.ts", "typescript", sem)
            .findings
            .is_empty(),
        "a classe de topo K não tem `zzz`"
    );
    let com =
        "class K { a = 1 }\nexport function f() {\n  class K { b = 2 }\n  return new K().zzz;\n}\n";
    limpo(&rodar(p.path(), "a.ts", "typescript", com));
}

#[test]
fn componente_local_que_esconde_o_de_topo_nao_checa_as_props_do_de_topo() {
    let p = projeto(&[("package.json", "{}")]);
    let topo = "interface P { a: number }\nconst Card = (p: P) => <div>{p.a}</div>;\n";
    let sem = format!("{topo}export const x = <Card b={{1}} />;\n");
    assert_eq!(
        achados(&rodar(p.path(), "a.tsx", "tsx", &sem)),
        vec![de("prop", "b", "warn")]
    );
    let com = format!(
        "{topo}export function g() {{\n  const Card = (q: {{ b: number }}) => <span>{{q.b}}</span>;\n  return <Card b={{1}} />;\n}}\n"
    );
    limpo(&rodar(p.path(), "a.tsx", "tsx", &com));
}

// ======================================================================================== pacotes externos

/// O motivo `nao_medido` do arquivo, ou texto vazio.
fn motivo(out: &CheckOutput) -> String {
    out.motivo.clone().unwrap_or_default()
}

#[test]
fn import_de_pacote_externo_vira_uma_linha_de_nao_medido_e_nunca_um_achado() {
    let p = projeto(&[("package.json", "{}")]);
    let fonte = "import React, { useState } from 'react';\nimport { debounce } from 'lodash/fp';\n\
                 import type { Foo } from '@scope/pkg/sub';\nimport { jsx } from 'react/jsx-runtime';\n\
                 export const x = [React, useState, debounce, jsx];\nexport type T = Foo;\n";
    let out = rodar(p.path(), "a.ts", "typescript", fonte);
    limpo(&out);
    assert_eq!(out.verdict, "unknown");
    let m = motivo(&out);
    // UMA linha, com cada PACOTE uma vez só (subcaminho e import repetido não multiplicam)
    assert_eq!(m.matches("pacote(s) externo(s)").count(), 1, "{m}");
    assert!(m.contains("3 pacote(s) externo(s)"), "{m}");
    assert!(m.contains("react, lodash, @scope/pkg"), "{m}");
    assert!(
        !m.contains("lodash/fp") && !m.contains("@scope/pkg/sub"),
        "{m}"
    );
    assert!(m.contains("camada 1") && m.contains("camada 2"), "{m}");
}

#[test]
fn builtin_do_node_e_require_e_import_dinamico_de_pacote_tambem_entram_na_linha() {
    let p = projeto(&[("package.json", "{}")]);
    let ts = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import fs from 'node:fs/promises';\nexport const f = fs;\n",
    );
    assert!(motivo(&ts).contains("node:fs"), "{:?}", ts.motivo);
    let js = rodar(
        p.path(),
        "b.js",
        "javascript",
        "const _ = require('lodash');\nasync function f() { return import('zod'); }\nmodule.exports = [_, f];\n",
    );
    limpo(&js);
    let m = motivo(&js);
    assert!(
        m.contains("2 pacote(s) externo(s)") && m.contains("lodash, zod"),
        "{m}"
    );
}

#[test]
fn arquivo_so_com_imports_relativos_nao_ganha_a_linha_de_pacotes() {
    let p = projeto(&[("package.json", "{}"), ("m.ts", "export const a = 1;\n")]);
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import { a } from './m';\nexport const b = a;\n",
    );
    limpo(&out);
    assert_eq!(out.verdict, "allow");
    assert!(out.motivo.is_none(), "{:?}", out.motivo);
}

#[test]
fn a_linha_de_pacotes_lista_poucos_nomes_e_conta_o_resto() {
    let p = projeto(&[("package.json", "{}")]);
    let imports: String = (1..=9)
        .map(|i| format!("import p{i} from 'pkg{i}';\n"))
        .collect();
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        &format!("{imports}export {{ p1 }};\n"),
    );
    let m = motivo(&out);
    assert!(m.contains("9 pacote(s) externo(s)"), "{m}");
    assert!(
        m.contains("pkg1, pkg2, pkg3, pkg4, pkg5, pkg6 e mais 3"),
        "{m}"
    );
    assert!(!m.contains("pkg7"), "{m}");
}

#[test]
fn achado_e_linha_de_pacotes_convivem_e_o_achado_decide_o_veredito() {
    let p = projeto(&[("package.json", "{}")]);
    let out = rodar(
        p.path(),
        "a.ts",
        "typescript",
        "import React from 'react';\nexport const y = [React, vgpNomeAusente];\n",
    );
    assert_eq!(achados(&out), vec![de("nome", "vgpNomeAusente", "warn")]);
    assert_eq!(out.verdict, "warn");
    assert!(
        motivo(&out).contains("1 pacote(s) externo(s)"),
        "{:?}",
        out.motivo
    );
}
