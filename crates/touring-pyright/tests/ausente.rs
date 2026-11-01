//! A verifier that is not in the project is an `unknown` with the reason — never an empty `allow`, never
//! a process, never an install (VL0 §6). Hermetic: no server is needed; every project below lacks the
//! thing that would make its verifier exist. Runs in the normal suite.

mod common;

use common::{Projeto, filhos_do_processo};
use touring_pyright::achado::Verdict;
use touring_pyright::pool;
use touring_pyright::servidor::Servidor;

fn esperar_ausencia(servidor: Servidor, root: &std::path::Path, path: &str, trecho: &str) {
    let r = pool::checar_em(servidor, root, path, "x\n", None, &[]);
    assert_eq!(r.verdict, Verdict::Unknown, "{servidor:?}: {r:#?}");
    assert!(r.achados.is_empty(), "{servidor:?}: {r:#?}");
    let motivo = r.motivo.as_deref().unwrap_or("");
    assert!(
        motivo.contains(trecho),
        "{servidor:?}: `{motivo}` should mention `{trecho}`"
    );
    let de_novo = pool::checar_em(servidor, root, path, "x\n", None, &[]);
    assert!(
        de_novo
            .motivo
            .as_deref()
            .is_some_and(|m| m.contains("backoff") && m.contains(trecho)),
        "{servidor:?}: the absence must be remembered, not rediscovered: {de_novo:#?}"
    );
}

#[test]
fn every_absent_verifier_is_unknown_with_its_reason_and_starts_no_process() {
    pool::habilitar_neste_processo();
    let sem_cargo = Projeto::novo("ausente_rs");
    sem_cargo.escrever("src/lib.rs", "pub fn f() {}\n");
    esperar_ausencia(
        Servidor::RustAnalyzer,
        &sem_cargo.root,
        "src/lib.rs",
        "Cargo.toml",
    );

    let sem_typescript = Projeto::novo("ausente_ts");
    sem_typescript.escrever("package.json", "{}");
    sem_typescript.escrever("src/a.ts", "export {};\n");
    esperar_ausencia(
        Servidor::Tsserver,
        &sem_typescript.root,
        "src/a.ts",
        "node_modules/typescript",
    );

    let nao_deno = Projeto::novo("ausente_deno");
    nao_deno.escrever("package.json", "{}");
    esperar_ausencia(
        Servidor::DenoLsp,
        &nao_deno.root,
        "a.ts",
        "não está num projeto Deno",
    );

    let sem_venv = Projeto::novo("ausente_py");
    sem_venv.escrever("a.py", "x = 1\n");
    esperar_ausencia(
        Servidor::Pyright,
        &sem_venv.root,
        "a.py",
        "pyright-langserver",
    );

    assert!(
        filhos_do_processo().is_empty(),
        "an absent verifier must not leave a process: {:?}",
        filhos_do_processo()
    );
}

#[test]
fn the_language_entry_point_routes_to_the_project_verifier_or_says_there_is_none() {
    pool::habilitar_neste_processo();
    let sem_typescript = Projeto::novo("lang_ts");
    sem_typescript.escrever("src/a.ts", "export {};\n");
    let ts = pool::checar_lang(
        "typescript",
        &sem_typescript.root,
        "src/a.ts",
        "x\n",
        None,
        &[],
    );
    assert_eq!(ts.verdict, Verdict::Unknown);
    assert!(
        ts.motivo
            .as_deref()
            .is_some_and(|m| m.contains("node_modules/typescript")),
        "{ts:#?}"
    );
    let tsx = pool::checar_lang("tsx", &sem_typescript.root, "src/a.tsx", "x\n", None, &[]);
    assert!(
        tsx.motivo
            .as_deref()
            .is_some_and(|m| m.contains("node_modules/typescript") || m.contains("backoff")),
        "{tsx:#?}"
    );
    for lang in ["html", "css", "markdown"] {
        let r = pool::checar_lang(lang, &sem_typescript.root, "x.html", "x\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Unknown, "{lang}");
        assert!(
            r.motivo
                .as_deref()
                .is_some_and(|m| m.contains("não há verificador")),
            "{lang}: {r:#?}"
        );
    }
}
