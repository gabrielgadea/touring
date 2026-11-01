//! VGP layer 2 for TypeScript against the REAL `tsserver` of a project's `node_modules/typescript`
//! (VL0 §6). Nothing is installed: the fixture links the TypeScript that `analise/frontend` already has
//! (`VGP_L2_TS_PKG` overrides the path).
//!
//! `#[ignore]`: it needs that install and `node`. Run explicitly:
//! `cargo test -p touring-pyright --test tsserver_real -- --ignored`.

mod common;

use std::path::PathBuf;

use common::{Projeto, achados, encerrar_tudo_e_esperar_os_filhos, esperar_veredito, medir};
use touring_pyright::achado::{Escopo, Severidade, Verdict};
use touring_pyright::pool;
use touring_pyright::servidor::Servidor;

const M_TS: &str = "export interface Ponto {\n  x: number;\n}\nexport const origem: Ponto = { x: 0 };\nexport function soma(a: Ponto): number {\n  return a.x;\n}\n";
const LIMPO: &str =
    "import { origem, soma } from \"./m\";\nexport const n: number = soma(origem);\n";
const RUIM: &str = "import { origem, soma, naoExiste } from \"./m\";\nexport const n: number = soma(origem) + naoDefinido;\nexport const y = origem.z;\nconst arr: number[] = [];\narr.pusha(1);\n";

fn pacote_typescript() -> PathBuf {
    if let Some(custom) = std::env::var_os("VGP_L2_TS_PKG") {
        return PathBuf::from(custom);
    }
    let home = std::env::var_os("HOME").expect("HOME must be set");
    PathBuf::from(home).join("projects/analise/frontend/node_modules/typescript")
}

fn fixture(tag: &str) -> Projeto {
    let p = Projeto::novo(tag);
    p.escrever(
        "tsconfig.json",
        "{\"compilerOptions\":{\"target\":\"ES2022\",\"module\":\"ESNext\",\"moduleResolution\":\"bundler\",\"strict\":true,\"noEmit\":true,\"skipLibCheck\":true,\"lib\":[\"ES2022\",\"DOM\"]},\"include\":[\"src\"]}",
    );
    p.escrever("src/m.ts", M_TS);
    p.escrever("src/a.ts", LIMPO);
    let pacote = pacote_typescript();
    assert!(
        pacote.join("lib/tsserver.js").is_file(),
        "no TypeScript at {}",
        pacote.display()
    );
    std::fs::create_dir_all(p.root.join("node_modules")).expect("create node_modules");
    std::os::unix::fs::symlink(&pacote, p.root.join("node_modules/typescript"))
        .expect("link the project's TypeScript");
    p
}

#[test]
#[ignore = "needs the TypeScript install of analise/frontend and node"]
fn ts2304_ts2339_ts2305_are_found_and_the_clean_original_is_zero() {
    let p = fixture("ts");
    let (ruim, frio) = medir(Servidor::Tsserver, &p.root, "src/a.ts", RUIM, Some(LIMPO));
    println!(
        "cold start until the first measured result: {frio:?}; check took {} ms",
        ruim.elapsed_ms
    );
    assert_ne!(ruim.verdict, Verdict::Unknown, "{ruim:#?}");
    assert_eq!(
        achados(&ruim),
        [
            ("atributo".to_string(), "pusha".to_string()),
            ("atributo".to_string(), "z".to_string()),
            ("import".to_string(), "naoExiste".to_string()),
            ("nome".to_string(), "naoDefinido".to_string()),
        ],
        "{ruim:#?}"
    );
    let regras: Vec<&str> = ruim.achados.iter().map(|a| a.regra.as_str()).collect();
    for esperada in ["TS2304", "TS2339", "TS2305"] {
        assert!(regras.contains(&esperada), "{esperada} in {regras:?}");
    }
    let pusha = ruim
        .achados
        .iter()
        .find(|a| a.nome == "pusha")
        .expect("pusha");
    assert_eq!(
        (pusha.escopo, pusha.severidade),
        (Escopo::Externo, Severidade::Deny),
        "number[] is the lib's"
    );
    let z = ruim.achados.iter().find(|a| a.nome == "z").expect("z");
    assert_eq!(
        (z.escopo, z.severidade),
        (Escopo::Projeto, Severidade::Warn),
        "Ponto is the project's"
    );
    esperar_veredito(&ruim, Verdict::Deny);

    let limpo = pool::checar_em(
        Servidor::Tsserver,
        &p.root,
        "src/a.ts",
        LIMPO,
        Some(LIMPO),
        &[],
    );
    esperar_veredito(&limpo, Verdict::Allow);
    assert!(limpo.achados.is_empty(), "{limpo:#?}");
    let sem_delta = pool::checar_em(Servidor::Tsserver, &p.root, "src/a.ts", LIMPO, None, &[]);
    esperar_veredito(&sem_delta, Verdict::Allow);

    let quente = pool::checar_em(
        Servidor::Tsserver,
        &p.root,
        "src/a.ts",
        RUIM,
        Some(LIMPO),
        &[],
    );
    println!("warm check: {} ms", quente.elapsed_ms);
    assert!(quente.elapsed_ms < 1500, "{quente:#?}");
    assert!(
        encerrar_tudo_e_esperar_os_filhos(),
        "tsserver left a process behind"
    );
}

#[test]
#[ignore = "needs the TypeScript install of analise/frontend and node"]
fn overlays_make_an_unsaved_module_visible_to_the_import() {
    let p = fixture("ts_overlay");
    let (primeiro, _) = medir(Servidor::Tsserver, &p.root, "src/a.ts", LIMPO, Some(LIMPO));
    esperar_veredito(&primeiro, Verdict::Allow);
    let m_novo = format!("{M_TS}export const naoExiste = 1;\n");
    pool::habilitar_neste_processo();
    let sem = pool::checar_em(
        Servidor::Tsserver,
        &p.root,
        "src/a.ts",
        RUIM,
        Some(LIMPO),
        &[],
    );
    assert!(
        sem.achados.iter().any(|a| a.nome == "naoExiste"),
        "{sem:#?}"
    );
    let com = pool::checar_em(
        Servidor::Tsserver,
        &p.root,
        "src/a.ts",
        RUIM,
        Some(LIMPO),
        &[("src/m.ts".to_string(), m_novo)],
    );
    assert!(
        !com.achados.iter().any(|a| a.nome == "naoExiste"),
        "the overlay defines it: {com:#?}"
    );
    assert!(encerrar_tudo_e_esperar_os_filhos());
}

#[test]
#[ignore = "needs the TypeScript install of analise/frontend and node"]
fn a_file_outside_every_tsconfig_and_a_syntax_error_are_unknown_not_allow() {
    let p = fixture("ts_unknown");
    let (primeiro, _) = medir(Servidor::Tsserver, &p.root, "src/a.ts", LIMPO, Some(LIMPO));
    esperar_veredito(&primeiro, Verdict::Allow);
    let fora = pool::checar_em(
        Servidor::Tsserver,
        &p.root,
        "fora/x.ts",
        "export const a: number = naoDefinido;\n",
        None,
        &[],
    );
    esperar_veredito(&fora, Verdict::Unknown);
    assert!(
        fora.motivo
            .as_deref()
            .is_some_and(|m| m.contains("tsconfig.json")),
        "{fora:#?}"
    );
    let sintaxe = pool::checar_em(
        Servidor::Tsserver,
        &p.root,
        "src/a.ts",
        "export const = ;\n",
        None,
        &[],
    );
    esperar_veredito(&sintaxe, Verdict::Unknown);
    assert!(
        sintaxe
            .motivo
            .as_deref()
            .is_some_and(|m| m.contains("sintaxe")),
        "{sintaxe:#?}"
    );
    // both unknowns were about the file: the warm program survives and still measures
    let depois = pool::checar_em(
        Servidor::Tsserver,
        &p.root,
        "src/a.ts",
        RUIM,
        Some(LIMPO),
        &[],
    );
    esperar_veredito(&depois, Verdict::Deny);
    assert!(encerrar_tudo_e_esperar_os_filhos());
}
