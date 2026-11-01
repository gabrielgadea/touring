//! VGP layer 2 for a Deno project against the REAL `deno lsp` (VL0 §6).
//!
//! `#[ignore]`: it needs `deno` on `PATH` (the test says so and passes vacuously when it is not
//! installed — Deno is optional). Run explicitly:
//! `cargo test -p touring-pyright --test deno_real -- --ignored --nocapture`.

mod common;

use std::process::{Command, Stdio};

use common::{Projeto, achados, encerrar_tudo_e_esperar_os_filhos, esperar_veredito, medir};
use touring_pyright::achado::Verdict;
use touring_pyright::pool;
use touring_pyright::servidor::Servidor;

const M_TS: &str = "export interface Ponto {\n  x: number;\n}\nexport const origem: Ponto = { x: 0 };\nexport function soma(a: Ponto): number {\n  return a.x;\n}\n";
const LIMPO: &str =
    "import { origem, soma } from \"./m.ts\";\nexport const n: number = soma(origem);\n";
const RUIM: &str = "import { origem, soma, naoExiste } from \"./m.ts\";\nexport const n: number = soma(origem) + naoDefinido;\nexport const y = origem.z;\nconst arr: number[] = [];\narr.pusha(1);\n";

fn deno_instalado() -> bool {
    Command::new("deno")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn fixture() -> Projeto {
    let p = Projeto::novo("deno");
    p.escrever("deno.json", "{\"compilerOptions\":{\"strict\":true}}");
    p.escrever("m.ts", M_TS);
    p.escrever("a.ts", LIMPO);
    p
}

#[test]
#[ignore = "needs deno on PATH"]
fn ts2304_ts2339_ts2305_are_found_by_deno_lsp_and_the_clean_original_is_zero() {
    if !deno_instalado() {
        eprintln!("deno não está instalado: teste não executado (não é uma falha)");
        return;
    }
    let p = fixture();
    assert_eq!(
        Servidor::para_lang("typescript", &p.root, "a.ts"),
        Some(Servidor::DenoLsp)
    );
    let (ruim, frio) = medir(Servidor::DenoLsp, &p.root, "a.ts", RUIM, Some(LIMPO));
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
    esperar_veredito(&ruim, Verdict::Deny);

    let limpo = pool::checar_em(Servidor::DenoLsp, &p.root, "a.ts", LIMPO, Some(LIMPO), &[]);
    esperar_veredito(&limpo, Verdict::Allow);
    assert!(
        limpo.achados.is_empty(),
        "lint and info diagnostics must not leak in: {limpo:#?}"
    );
    assert!(
        encerrar_tudo_e_esperar_os_filhos(),
        "deno lsp left a process behind"
    );
}
