//! VGP layer 2 for Rust against the REAL `rust-analyzer` of the toolchain (VL0 §6).
//!
//! `#[ignore]`: it needs `rustup` with the `rust-analyzer` component. Run explicitly:
//! `cargo test -p touring-pyright --test rust_real -- --ignored`.
//! The fixture is a 2-file crate; the proposed content invents a name (E0425), a struct-literal field
//! (E0560) and a method (E0599); the clean original must measure zero.

mod common;

use common::{Projeto, achados, encerrar_tudo_e_esperar_os_filhos, esperar_veredito, medir};
use touring_pyright::achado::{Escopo, Severidade, Verdict};
use touring_pyright::pool;
use touring_pyright::servidor::Servidor;

const CARGO_TOML: &str =
    "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n";
const UTIL_RS: &str = "pub fn real() -> i32 {\n    1\n}\n";
const LIMPO: &str = "mod util;\n\npub struct Ponto {\n    pub x: i32,\n}\n\npub fn soma(p: &Ponto) -> i32 {\n    p.x + util::real()\n}\n";
const RUIM: &str = "mod util;\n\npub struct Ponto {\n    pub x: i32,\n}\n\npub fn soma(p: &Ponto) -> i32 {\n    p.x + util::real()\n}\n\npub fn ruim() -> i32 {\n    let q = Ponto { x: 1, y: 2 };\n    let v: Vec<i32> = Vec::new();\n    v.pusha(3);\n    q.x + nome_solto\n}\n";

fn fixture() -> Projeto {
    let p = Projeto::novo("ra");
    p.escrever("Cargo.toml", CARGO_TOML);
    p.escrever("src/lib.rs", LIMPO);
    p.escrever("src/util.rs", UTIL_RS);
    p
}

#[test]
#[ignore = "needs rustup with the rust-analyzer component"]
fn invented_name_field_and_method_are_found_and_the_clean_original_is_zero() {
    let p = fixture();

    let (ruim, frio) = medir(
        Servidor::RustAnalyzer,
        &p.root,
        "src/lib.rs",
        RUIM,
        Some(LIMPO),
    );
    println!(
        "cold start until the first measured result: {frio:?}; check took {} ms",
        ruim.elapsed_ms
    );
    assert_ne!(ruim.verdict, Verdict::Unknown, "{ruim:#?}");
    assert_eq!(
        achados(&ruim),
        [
            ("atributo".to_string(), "pusha".to_string()),
            ("kwarg".to_string(), "y".to_string()),
            ("nome".to_string(), "nome_solto".to_string()),
        ],
        "{ruim:#?}"
    );
    let pusha = ruim
        .achados
        .iter()
        .find(|a| a.nome == "pusha")
        .expect("pusha");
    assert_eq!(
        (pusha.escopo, pusha.severidade),
        (Escopo::Externo, Severidade::Deny),
        "Vec is std's"
    );
    esperar_veredito(&ruim, Verdict::Deny);
    for a in &ruim.achados {
        assert_eq!(
            a.linha,
            if a.nome == "y" {
                12
            } else if a.nome == "pusha" {
                14
            } else {
                15
            },
            "{a:?}"
        );
    }

    // the clean original: with the delta and without it
    let limpo = pool::checar_em(
        Servidor::RustAnalyzer,
        &p.root,
        "src/lib.rs",
        LIMPO,
        Some(LIMPO),
        &[],
    );
    esperar_veredito(&limpo, Verdict::Allow);
    assert!(limpo.achados.is_empty(), "{limpo:#?}");
    let sem_delta = pool::checar_em(
        Servidor::RustAnalyzer,
        &p.root,
        "src/lib.rs",
        LIMPO,
        None,
        &[],
    );
    esperar_veredito(&sem_delta, Verdict::Allow);

    // warm checks are fast (the cold start is paid once)
    let quente = pool::checar_em(
        Servidor::RustAnalyzer,
        &p.root,
        "src/lib.rs",
        RUIM,
        Some(LIMPO),
        &[],
    );
    println!("warm check: {} ms", quente.elapsed_ms);
    assert!(quente.elapsed_ms < 2000, "{quente:#?}");

    assert!(
        encerrar_tudo_e_esperar_os_filhos(),
        "rust-analyzer left a process behind"
    );
}

#[test]
#[ignore = "needs rustup with the rust-analyzer component"]
fn a_file_no_module_declares_is_unknown_not_allow() {
    let p = fixture();
    // prime the workspace with a normal check first
    let (primeiro, _) = medir(
        Servidor::RustAnalyzer,
        &p.root,
        "src/lib.rs",
        LIMPO,
        Some(LIMPO),
    );
    esperar_veredito(&primeiro, Verdict::Allow);
    let orfao = pool::checar_em(
        Servidor::RustAnalyzer,
        &p.root,
        "src/orfao.rs",
        "pub fn f() -> i32 { nome_solto }\n",
        None,
        &[],
    );
    esperar_veredito(&orfao, Verdict::Unknown);
    let motivo = orfao.motivo.expect("reason");
    assert!(motivo.contains("unlinked-file"), "{motivo}");
    // and the warm workspace survived the unknown (it is about the file)
    let depois = pool::checar_em(
        Servidor::RustAnalyzer,
        &p.root,
        "src/lib.rs",
        RUIM,
        Some(LIMPO),
        &[],
    );
    esperar_veredito(&depois, Verdict::Deny);
    assert!(encerrar_tudo_e_esperar_os_filhos());
}

#[test]
#[ignore = "needs rustup with the rust-analyzer component"]
fn a_workspace_whose_dependencies_do_not_load_is_degraded_and_never_denies() {
    let p = Projeto::novo("ra_degraded");
    // an unresolvable dependency, offline: `cargo metadata` with dependencies fails and rust-analyzer
    // falls back to a workspace without them, reporting `health: warning` — a use of the declared crate
    // is then unresolved for a reason that is not the edit's
    p.escrever(
        "Cargo.toml",
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncrate-que-nao-existe-vgp-l2 = \"9.9.9\"\n",
    );
    let fonte = "use crate_que_nao_existe_vgp_l2::Coisa;\npub fn f() -> i32 {\n    nome_solto\n}\n";
    p.escrever("src/lib.rs", fonte);
    let (r, _) = medir(Servidor::RustAnalyzer, &p.root, "src/lib.rs", fonte, None);
    println!("result: {r:#?}");
    assert_ne!(r.verdict, Verdict::Unknown, "{r:#?}");
    assert!(
        !r.achados.is_empty(),
        "the genuine finding is still reported: {r:#?}"
    );
    assert!(
        r.achados.iter().all(|a| a.severidade != Severidade::Deny),
        "a degraded workspace must never block: {r:#?}"
    );
    assert!(
        r.achados.iter().any(|a| a.mensagem.contains("degradado")),
        "the reason travels with the finding: {r:#?}"
    );
    assert!(encerrar_tudo_e_esperar_os_filhos());
}
