//! TDD fixtures for the VRS adapter — each test replicates a measured `corpus_rust` positive
//! (or a measured negative, in the precision guards). The fixture is a mini cargo workspace
//! (`alpha` + `beta` member crates) mirroring the shapes of the touring codebase: `mod`
//! declarations, `pub use` re-export chains, cross-crate workspace paths and inline test mods.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::vgp::lang::{Adaptador, Verificacao, adaptador_de};
use crate::vgp::politica::Alvo;
use crate::vgp::{CheckInput, Contexto, Finding, check_em};

use super::Rust;

/// The whole motor on one content (adapter + policy + delta + verdict): what the hook and the
/// judge see. Severity and verdict are `vgp::check`'s; the adapter only reports targets.
fn verificar_rust(input: &CheckInput) -> (String, Vec<Finding>) {
    let saida = check_em(input, Contexto::Execucao);
    (saida.verdict, saida.findings)
}

/// The adapter alone (no policy, no delta): findings with their [`Alvo`], or why not measured.
fn medir(input: &CheckInput) -> Verificacao {
    Rust.verificar(input)
}

/// V-4 (31/10/2026): o `#[path]` do CONTEUDO nao escolhe o arquivo lido — um
/// caminho ABSOLUTO e recusado (no `Path::join` ele SUBSTITUI a base), e um
/// relativo que fica dentro da raiz e lido.
#[test]
fn o_path_do_conteudo_nao_escapa_da_raiz() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let raiz = projeto(tmp.path());
    let fora = tmp.path().join("fora.rs");
    std::fs::write(&fora, "pub fn sinal_do_fora() {}\n").expect("write");
    // o caminho absoluto, montado em tempo de execucao (o gate F2.1 marca o
    // literal de traversal no proprio teste)
    let conteudo = format!(
        "#[path = {:?}]\nmod x;\nuse x::sinal_do_fora;\n",
        fora.to_string_lossy()
    );
    let entrada = CheckInput {
        path: raiz
            .join("crates/alpha/src/lib.rs")
            .to_string_lossy()
            .to_string(),
        content: conteudo,
        project_root: raiz.clone(),
        disk: None,
        overlays: HashMap::new(),
        lang: "rust".into(),
    };
    let (verdict, achados) = verificar_rust(&entrada);
    assert_ne!(
        verdict, "deny",
        "o `#[path]` fora da raiz nao e um modulo faltando: {achados:?}"
    );
    assert!(
        !achados.iter().any(|a| a.nome == "sinal_do_fora"),
        "o arquivo fora da raiz nao pode ter sido lido: {achados:?}"
    );
    // um `#[path]` RELATIVO que fica dentro da raiz e lido
    let dentro = tmp.path().join("dentro.rs");
    std::fs::write(&dentro, "pub fn sinal_de_dentro() {}\n").expect("write");
    let conteudo = "#[path = \"../../../dentro.rs\"]\nmod x;\nuse x::sinal_de_dentro;\n";
    let entrada = CheckInput {
        path: raiz
            .join("crates/alpha/src/lib.rs")
            .to_string_lossy()
            .to_string(),
        content: conteudo.to_string(),
        project_root: raiz.clone(),
        disk: None,
        overlays: HashMap::new(),
        lang: "rust".into(),
    };
    let (verdict, achados) = verificar_rust(&entrada);
    assert_eq!(
        verdict, "allow",
        "um `#[path]` relativo dentro da raiz e lido: {achados:?}"
    );
}

/// The mini workspace: root Cargo.toml ([workspace] members + [workspace.dependencies]),
/// `crates/alpha` (lib with mod tree, re-exports, a struct, an enum, free fns) and
/// `crates/beta` (depends on alpha by path — the cross-crate workspace shape).
fn projeto(tmp: &Path) -> PathBuf {
    let alpha = tmp.join("crates/alpha");
    let beta = tmp.join("crates/beta");
    std::fs::create_dir_all(alpha.join("src/motor")).unwrap();
    std::fs::create_dir_all(alpha.join("src/grupo")).unwrap();
    std::fs::create_dir_all(beta.join("src")).unwrap();
    std::fs::write(
        tmp.join("Cargo.toml"),
        "[workspace]\nmembers = ['crates/alpha', 'crates/beta']\nresolver = '2'\n\n\
         [workspace.dependencies]\nserde = '1'\n",
    )
    .unwrap();
    std::fs::write(
        alpha.join("Cargo.toml"),
        "[package]\nname = 'alpha'\nversion = '0.1.0'\nedition = '2021'\n\n\
         [dependencies]\nserde = { workspace = true }\n\n[dev-dependencies]\n",
    )
    .unwrap();
    std::fs::write(
        beta.join("Cargo.toml"),
        "[package]\nname = 'beta'\nversion = '0.1.0'\nedition = '2021'\n\n\
         [dependencies]\nalpha = { path = '../alpha' }\n",
    )
    .unwrap();
    std::fs::write(
        alpha.join("src/lib.rs"),
        "pub mod motor;\npub mod grupo;\n\
         pub use motor::Caixa;\npub use motor::Caixa as CaixaReexportada;\n\
         pub const RAIZ: u32 = 1;\nfn privada_da_raiz() {}\n",
    )
    .unwrap();
    std::fs::write(
        alpha.join("src/motor.rs"),
        "pub mod inner;\npub use inner::Coisa;\n\
         pub struct Caixa { pub campo: u32, pub nome: String }\n\
         pub struct Tupla(pub u32, pub u32);\n\
         pub enum Falha { Rede, Disco(u32), Arquivo { codigo: u32 } }\n\
         pub fn ligar(a: u8, b: u8) -> u8 { a + b }\n\
         pub fn tripla(a: u8, b: u8, c: u8) -> u8 { a + b + c }\n\
         pub const VELOCIDADE: u32 = 3;\n",
    )
    .unwrap();
    std::fs::write(
        alpha.join("src/motor/inner.rs"),
        "pub struct Coisa { pub x: u32 }\npub fn auxiliar() {}\n",
    )
    .unwrap();
    std::fs::write(
        alpha.join("src/grupo/mod.rs"),
        "pub mod folha;\npub fn item_do_grupo() {}\n",
    )
    .unwrap();
    std::fs::write(alpha.join("src/grupo/folha.rs"), "pub fn da_folha() {}\n").unwrap();
    std::fs::write(beta.join("src/lib.rs"), "pub fn usa() {}\n").unwrap();
    tmp.to_path_buf()
}

/// A CheckInput for the PROPOSED content of `rel` (a project file path under `root`).
fn input(root: &Path, rel: &str, content: &str) -> CheckInput {
    CheckInput {
        path: root.join(rel).to_string_lossy().into_owned(),
        content: content.to_string(),
        project_root: root.to_path_buf(),
        disk: None,
        overlays: HashMap::new(),
        lang: "rust".into(),
    }
}

fn input_tmp(path: &str, content: &str) -> CheckInput {
    CheckInput {
        path: path.to_string(),
        content: content.to_string(),
        project_root: PathBuf::from("/tmp"),
        disk: None,
        overlays: HashMap::new(),
        lang: "rust".into(),
    }
}

/// (classe, nome) of findings at the given severity, sorted — the judge-style assertion.
fn achados(fs: &[Finding], sev: &str) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = fs
        .iter()
        .filter(|f| f.severidade == sev)
        .map(|f| (f.classe.clone(), f.nome.clone()))
        .collect();
    v.sort();
    v
}

// --------------------------------------------------------------------------- nome (E0425)

#[test]
fn nome_chamada_direta_ausente_nega_em_tmp() {
    // corpus: cli_index_ingest (E0425, chamada direta sem definição em nenhum escopo)
    let (verdict, fs) = verificar_rust(&input_tmp("/tmp/vrs-a.rs", "fn f() { inexistente(1); }\n"));
    assert_eq!(verdict, "deny");
    assert_eq!(
        achados(&fs, "deny"),
        vec![("nome".to_string(), "inexistente".to_string())]
    );
}

#[test]
fn nome_referencia_como_valor_e_path_head() {
    // corpus: default_commitments_path em `.unwrap_or_else(default_commitments_path)` (referência,
    // não chamada) e `Capability::Net` (path head não ligado — E0433 rotulado import na corpus)
    let (_, fs) = verificar_rust(&input_tmp(
        "/tmp/vrs-b.rs",
        "fn f(o: Option<u8>) { let _ = o.unwrap_or_else(caminho_padrao); }\n",
    ));
    assert_eq!(
        achados(&fs, "deny"),
        vec![("nome".to_string(), "caminho_padrao".to_string())]
    );
    let (_, fs) = verificar_rust(&input_tmp(
        "/tmp/vrs-c.rs",
        "fn f() { let _ = Capability::Net(1); }\n",
    ));
    assert_eq!(
        achados(&fs, "deny"),
        vec![("nome".to_string(), "Capability".to_string())]
    );
}

#[test]
fn nome_controles_de_escopo_passam_limpos() {
    // controles medidos nos negativos: fn local, closure ligada por let, parâmetro chamável,
    // padrões (match/for/if-let/struct/shorthand), métodos e macros — nada pode disparar
    let src = "fn local() {}\n\
               fn f(p: fn()) {\n\
               \x20   let clo = |q: u8| q + 1;\n\
               \x20   local(); clo(1); p();\n\
               \x20   let opt = Some(1u8);\n\
               \x20   match opt { Some(x) => x, None => 0 };\n\
               \x20   for i in 0..3 { let _ = i; }\n\
               \x20   if let Some(y) = opt { let _ = y; }\n\
               \x20   struct P { a: u8, b: u8 }\n\
               \x20   let P { a, b: bb } = P { a: 1, b: 2 };\n\
               \x20   let _ = a + bb;\n\
               \x20   let s = String::new();\n\
               \x20   let _ = s.len();\n\
               \x20   let _ = format!(\"{a}\");\n\
               \x20   let v = vec![1, 2];\n\
               \x20   let _ = v.len();\n\
               }\n";
    let (verdict, fs) = verificar_rust(&input_tmp("/tmp/vrs-d.rs", src));
    assert_eq!(verdict, "allow", "falso achado: {:?}", fs);
}

#[test]
fn nome_self_fora_de_impl_e_dentro() {
    // corpus: Self::source_declares fora de impl (knowledge_wiring.rs L3199)
    let (_, fs) = verificar_rust(&input_tmp("/tmp/vrs-e.rs", "fn f() { Self::novo(); }\n"));
    assert_eq!(
        achados(&fs, "deny"),
        vec![("nome".to_string(), "Self".to_string())]
    );
    let (verdict, fs) = verificar_rust(&input_tmp(
        "/tmp/vrs-f.rs",
        "enum E { A }\nimpl E { fn novo() -> E { Self::A } }\n",
    ));
    assert_eq!(verdict, "allow", "Self dentro de impl: {:?}", fs);
}

#[test]
fn nome_glob_externo_e_macro_nunca_negam() {
    // abstenção (VL0 §5): glob de crate externo e macro em nível de item — warn dinâmico, nunca deny
    let glob = input_tmp("/tmp/vrs-g.rs", "use serde::*;\nfn f() { algo(); }\n");
    let (_, fs) = verificar_rust(&glob);
    assert!(
        fs.iter().all(|f| f.severidade != "deny"),
        "glob externo: {:?}",
        fs
    );
    // o adaptador diz POR QUÊ: o nome é do tipo "pode vir de fora" (Incerto), nunca do projeto
    let v = medir(&glob);
    assert!(
        v.achados.is_empty(),
        "o glob externo não decide: {:?}",
        v.achados
    );
    assert!(
        v.nao_medido.as_deref().is_some_and(|m| m.contains("algo")),
        "{:?}",
        v.nao_medido
    );

    let macro_item = input_tmp(
        "/tmp/vrs-h.rs",
        "meu_macro! { static ref X: u32 = 1; }\nfn f() { let _ = Y.clone(); }\n",
    );
    let (_, fs) = verificar_rust(&macro_item);
    assert!(
        fs.iter().all(|f| f.severidade != "deny"),
        "macro de item: {:?}",
        fs
    );
    let v = medir(&macro_item);
    assert!(v.achados.is_empty(), "macro de item: {:?}", v.achados);
    assert!(
        v.nao_medido.as_deref().is_some_and(|m| m.contains("`Y`")),
        "{:?}",
        v.nao_medido
    );
    // o controle: o mesmo `Y` SEM macro de item é nome ausente do projeto (nega onde roda)
    let (veredito, fs) = verificar_rust(&input_tmp(
        "/tmp/vrs-h2.rs",
        "fn f() { let _ = Y.clone(); }\n",
    ));
    assert_eq!(veredito, "deny", "sem macro o nome ausente nega: {:?}", fs);
}

// --------------------------------------------------------------------------- import (E0432/E0433)

#[test]
fn import_nome_inventado_em_modulo_do_projeto() {
    let tmp = std::env::temp_dir().join("vrs-t-import");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mau = "use crate::motor::CaixaInexistente;\n";
    let (_, fs) = verificar_rust(&input(&root, "crates/alpha/src/motor/inner.rs", mau));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "CaixaInexistente".to_string())]
    );
    let bom = "use crate::motor::Caixa;\n";
    let (verdict, fs) = verificar_rust(&input(&root, "crates/alpha/src/motor/inner.rs", bom));
    assert_eq!(verdict, "allow", "nome real: {:?}", fs);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn import_segmento_intermediario_e_lista_mista() {
    let tmp = std::env::temp_dir().join("vrs-t-segmento");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    // o conteúdo proposto precisa trazer as declarações `mod` (um snap real traz o arquivo
    // inteiro) — sem elas o primeiro segmento irresolúvel é `motor`, não `fantasma`
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nuse crate::motor::fantasma::Coisa;\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "fantasma".to_string())]
    );
    // lista mista: só o inventado é acusado (o (b) do aceite do from-import do Python)
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nuse crate::motor::{Caixa, Falha, Fantasma};\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "Fantasma".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn import_mod_decl_sem_arquivo_e_com_arquivo() {
    let tmp = std::env::temp_dir().join("vrs-t-moddecl");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "mod fantasma;\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "fantasma".to_string())]
    );
    let (verdict, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "mod folha;\n",
    ));
    assert_eq!(verdict, "allow", "mod existente: {:?}", fs);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn import_pub_use_reexport_passa_e_e_seguido() {
    // a declaração vence a inferência: `use crate::Caixa` (reexport em lib.rs) e
    // `use crate::motor::Coisa` (reexport de submódulo em motor.rs) NÃO são achados
    let tmp = std::env::temp_dir().join("vrs-t-reexport");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let (verdict, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use crate::Caixa;\nuse crate::CaixaReexportada;\nuse crate::motor::Coisa;\n",
    ));
    assert_eq!(verdict, "allow", "pub use ignorado?: {:?}", fs);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn import_super_e_self_resolvem_do_lugar_certo() {
    // corpus: `use super::super::burst_gate` dentro de fn de teste (cli_suggester_tests.rs)
    let tmp = std::env::temp_dir().join("vrs-t-super");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let (verdict, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "pub fn da_folha() {}\n\
         use super::item_do_grupo;\n\
         fn t() {\n\
         \x20   use super::super::RAIZ;\n\
         \x20   self::da_folha();\n\
         \x20   item_do_grupo();\n\
         \x20   let _ = RAIZ;\n\
         }\n",
    ));
    assert_eq!(verdict, "allow", "super/self: {:?}", fs);
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use super::fantasma;\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "fantasma".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn import_cross_crate_do_workspace() {
    // corpus: touring_intelligence::rl::memory::retirement (membro do workspace)
    let tmp = std::env::temp_dir().join("vrs-t-crosscrate");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let (verdict, fs) = verificar_rust(&input(
        &root,
        "crates/beta/src/lib.rs",
        "use alpha::motor::Caixa;\n",
    ));
    assert_eq!(verdict, "allow", "membro do workspace: {:?}", fs);
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/beta/src/lib.rs",
        "use alpha::motor::Fantasma;\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "Fantasma".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn import_crate_nao_declarada_avisa_externo_declarado_abstem() {
    // corpus: `use serial_test::serial` (E0432 — crate ausente do Cargo.toml no instante)
    let tmp = std::env::temp_dir().join("vrs-t-deps");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/lib.rs",
        "use nao_declarada::Coisa;\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "nao_declarada".to_string())]
    );
    let (verdict, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/lib.rs",
        "use serde::Serialize;\n",
    ));
    assert_eq!(verdict, "allow", "crate externo declarado: {:?}", fs);
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- kwarg (E0560 · E0061)

#[test]
fn kwarg_campo_inventado_em_literal_de_struct() {
    let tmp = std::env::temp_dir().join("vrs-t-kwarg");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "use crate::motor::Caixa;\n\
               fn f() { let _ = Caixa { campo: 1, nome: String::new(), inventado: 2 }; }\n";
    let (_, fs) = verificar_rust(&input(&root, "crates/alpha/src/grupo/folha.rs", src));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("kwarg".to_string(), "inventado".to_string())]
    );
    // controles: campos exatos, shorthand, struct-tupla, literal de variante de enum
    let ok = "use crate::motor::{Caixa, Tupla, Falha};\n\
              fn f() {\n\
              \x20   let campo = 1;\n\
              \x20   let _ = Caixa { campo, nome: String::new() };\n\
              \x20   let _ = Tupla(1, 2);\n\
              \x20   let _ = Falha::Arquivo { codigo: 1 };\n\
              }\n";
    let (verdict, fs) = verificar_rust(&input(&root, "crates/alpha/src/grupo/folha.rs", ok));
    assert_eq!(verdict, "allow", "controles kwarg: {:?}", fs);
    // campo inventado em literal de VARIANTE de enum também é kwarg
    let bad = "use crate::motor::Falha;\nfn f() { let _ = Falha::Arquivo { fantasma: 1 }; }\n";
    let (_, fs) = verificar_rust(&input(&root, "crates/alpha/src/grupo/folha.rs", bad));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("kwarg".to_string(), "fantasma".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn kwarg_aridade_de_funcao_do_projeto() {
    // corpus: evaluate (E0061 — função do projeto com 3 parâmetros chamada com 2)
    let tmp = std::env::temp_dir().join("vrs-t-aridade");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use crate::motor::tripla;\nfn f() { tripla(1, 2); }\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("kwarg".to_string(), "tripla".to_string())]
    );
    let (verdict, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use crate::motor::{ligar, tripla};\nfn f() { ligar(1, 2); tripla(1, 2, 3); }\n",
    ));
    assert_eq!(verdict, "allow", "aridade certa: {:?}", fs);
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use crate::motor::ligar;\nfn f() { ligar(1, 2, 3); }\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("kwarg".to_string(), "ligar".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- atributo (E0599)

#[test]
fn atributo_variante_inventada_de_enum_do_projeto() {
    // corpus: GraphError::UnresolvedDependencies (E0599, variante inexistente)
    let tmp = std::env::temp_dir().join("vrs-t-variante");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let (_, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use crate::motor::Falha;\nfn f() { let _ = Falha::Inexistente; }\n",
    ));
    assert_eq!(
        achados(&fs, "warn"),
        vec![("atributo".to_string(), "Inexistente".to_string())]
    );
    let (verdict, fs) = verificar_rust(&input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use crate::motor::Falha;\nfn f() { let _ = Falha::Rede; let _ = Falha::Disco(1); }\n",
    ));
    assert_eq!(verdict, "allow", "variantes reais: {:?}", fs);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn atributo_item_associado_de_enum_nao_e_variante_ausente() {
    // `Falha::nova()`, `Falha::LIMITE` e `Self::nova()` apontam para itens ASSOCIADOS: só o
    // conjunto de variantes é fechado — uma variante é CamelCase. O item associado é decidido
    // pelo `impl` que o adaptador VÊ (o do módulo do enum); fora dele, na dúvida: não medido.
    let tmp = Path::new("/tmp").join("vrs-t-assoc");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "use crate::motor::Falha;\n\
               fn f() { let _ = Falha::nova(); let _ = Falha::LIMITE; }\n\
               impl Falha { fn g() { let _ = Self::nova(); let _ = Self::LIMITE; } }\n";
    let inp = input(&root, "crates/alpha/src/grupo/folha.rs", src);
    let v = medir(&inp);
    assert!(
        v.achados.is_empty(),
        "item associado tomado por variante: {:?}",
        v.achados
    );
    let motivo = v
        .nao_medido
        .expect("impl invisível → não medido, nunca allow");
    assert!(
        motivo.contains("nova") && motivo.contains("LIMITE"),
        "{motivo}"
    );
    assert_eq!(check_em(&inp, Contexto::Execucao).verdict, "unknown");
    // o `impl` VISÍVEL (no módulo do enum, por overlay) decide: nada a medir, nada a acusar
    let mut visivel = input(&root, "crates/alpha/src/grupo/folha.rs", src);
    visivel.overlays.insert(
        root.join("crates/alpha/src/motor.rs")
            .to_string_lossy()
            .into_owned(),
        "pub enum Falha { Rede }\n\
         impl Falha { pub fn nova() -> Self { Falha::Rede } pub const LIMITE: u32 = 1; }\n"
            .into(),
    );
    let (verdict, fs) = verificar_rust(&visivel);
    assert_eq!(verdict, "allow", "impl visível decide: {fs:?}");
    assert!(medir(&visivel).nao_medido.is_none());
    // o controle: a variante inventada (CamelCase) continua acusada, no caminho e no `Self::`
    let mau = "use crate::motor::Falha;\n\
               fn f() { let _ = Falha::Inventada; }\n\
               impl Falha { fn g() { let _ = Self::OutraInventada; } }\n";
    let (_, fs) = verificar_rust(&input(&root, "crates/alpha/src/grupo/folha.rs", mau));
    assert_eq!(
        achados(&fs, "warn"),
        vec![
            ("atributo".to_string(), "Inventada".to_string()),
            ("atributo".to_string(), "OutraInventada".to_string())
        ]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- crate do teste · prelúdio · itens locais

#[test]
fn import_pacote_em_tests_benches_examples_resolve_a_biblioteca_do_pacote() {
    // um teste de integração, um exemplo e um bench são crates enraizados neles mesmos, mas
    // `use alpha::X` ali nomeia a BIBLIOTECA do pacote (`src/lib.rs`), não o arquivo checado
    let tmp = Path::new("/tmp").join("vrs-t-pacote");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    for rel in [
        "crates/alpha/tests/integ.rs",
        "crates/alpha/examples/ex.rs",
        "crates/alpha/benches/b.rs",
    ] {
        let bom = "use alpha::motor::Caixa;\n\
                   fn t() { let _ = Caixa { campo: 1, nome: String::new() }; }\n";
        let (verdict, fs) = verificar_rust(&input(&root, rel, bom));
        assert_eq!(verdict, "allow", "{rel}: {fs:?}");
        // o controle: um nome inventado nesse pacote continua acusado
        let (_, fs) = verificar_rust(&input(&root, rel, "use alpha::motor::Fantasma;\n"));
        assert_eq!(
            achados(&fs, "warn"),
            vec![("import".to_string(), "Fantasma".to_string())],
            "{rel}"
        );
    }
    // pacote só de binário: não há biblioteca para `use gamma::…` — tentou e não decide
    std::fs::create_dir_all(root.join("crates/gamma/src")).unwrap();
    std::fs::write(
        root.join("crates/gamma/Cargo.toml"),
        "[package]\nname = 'gamma'\nversion = '0.1.0'\nedition = '2021'\n",
    )
    .unwrap();
    std::fs::write(root.join("crates/gamma/src/main.rs"), "fn main() {}\n").unwrap();
    let inp = input(
        &root,
        "crates/gamma/tests/t.rs",
        "use gamma::algo::Coisa;\n",
    );
    let v = medir(&inp);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    assert!(
        v.nao_medido
            .as_deref()
            .is_some_and(|m| m.contains("sem biblioteca")),
        "{:?}",
        v.nao_medido
    );
    assert_eq!(check_em(&inp, Contexto::Execucao).verdict, "unknown");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn nome_preludio_e_itens_locais_de_funcao_estao_no_escopo() {
    // `drop`/`size_of` são do prelúdio (std::prelude::v1 · 2024); `const`/`fn`/`struct` declarados
    // DENTRO do corpo valem no bloco inteiro
    let src = "fn f() -> u8 {\n\
               \x20   const N: u8 = 3;\n\
               \x20   static NOME: &str = \"x\";\n\
               \x20   struct P { a: u8 }\n\
               \x20   fn dobro(x: u8) -> u8 { x * 2 }\n\
               \x20   let p = P { a: N };\n\
               \x20   let v = vec![1u8];\n\
               \x20   drop(v);\n\
               \x20   let _ = (size_of::<u8>(), NOME);\n\
               \x20   dobro(p.a)\n\
               }\n";
    let (verdict, fs) = verificar_rust(&input_tmp("/tmp/vrs-prel.rs", src));
    assert_eq!(verdict, "allow", "{fs:?}");
    // o controle: o que NÃO está em escopo algum continua acusado (nem `N` fora da função)
    let mau = "fn f() { const N: u8 = 3; let _ = N; }\nfn g() { inexistente(N); }\n";
    let (_, fs) = verificar_rust(&input_tmp("/tmp/vrs-prel2.rs", mau));
    assert_eq!(
        achados(&fs, "deny"),
        vec![
            ("nome".to_string(), "N".to_string()),
            ("nome".to_string(), "inexistente".to_string())
        ],
        "o `N` local de `f` não vaza para `g`"
    );
}

// --------------------------------------------------------------------------- política · delta · unknown

#[test]
fn politica_arquivo_do_projeto_so_avisa_e_onde_roda_agora_nega() {
    // VL0 §5: símbolo do projeto inexistente num arquivo do projeto = warn (o RED do TDD); o
    // mesmo código onde roda agora (/tmp avulso, inline, .cache da raiz) = deny. A decisão é do
    // `check`: o adaptador só diz que o alvo é do PROJETO.
    let src = "fn f() { perdido(1); }\n";
    let em = |path: &str| {
        let mut i = input_tmp(path, src);
        i.project_root = PathBuf::from("/home/u/proj");
        i
    };
    for (path, esperado) in [
        ("/home/u/proj/src/a.rs", "warn"),
        ("/tmp/vrs-pol.rs", "deny"),
        ("<string>", "deny"),
        ("/home/u/proj/.cache/scratch/a.rs", "deny"),
    ] {
        let (verdict, fs) = verificar_rust(&em(path));
        assert_eq!(verdict, esperado, "{path}: {fs:?}");
    }
    let v = medir(&em("/home/u/proj/src/a.rs"));
    assert_eq!(v.achados.len(), 1);
    assert_eq!(v.achados[0].alvo, Alvo::Projeto);
    assert!(
        v.nao_medido.is_none(),
        "tudo o que havia a resolver fechou: {:?}",
        v.nao_medido
    );
}

#[test]
fn projeto_sob_tmp_nao_e_scratch_o_ausente_vira_incerto_e_so_avisa() {
    // o `/tmp/` da política é "scratch que roda agora"; um projeto que MORA em /tmp (Cargo.toml
    // acima do arquivo, dentro da raiz) é projeto: o ausente pode nascer na próxima edição
    let tmp = Path::new("/tmp").join("vrs-t-sob-tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let inp = input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use crate::motor::Fantasma;\nfn f() { perdido(1); }\n",
    );
    let v = medir(&inp);
    assert_eq!(v.achados.len(), 2, "{:?}", v.achados);
    assert!(v.achados.iter().all(|f| f.alvo == Alvo::Incerto));
    let (verdict, fs) = verificar_rust(&inp);
    assert_eq!(verdict, "warn");
    assert!(fs.iter().all(|f| f.severidade == "warn"), "{fs:?}");
    // o controle: o mesmo código num /tmp AVULSO (sem Cargo.toml) é scratch e nega
    let (verdict, _) = verificar_rust(&input_tmp("/tmp/vrs-avulso.rs", "fn f() { perdido(1); }\n"));
    assert_eq!(verdict, "deny");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn delta_pre_existente_vira_warn_nunca_deny() {
    let mut inp = input_tmp("/tmp/vrs-delta.rs", "fn f() { inexistente(1); }\n");
    inp.disk = Some("fn f() { inexistente(1); }\n".into());
    let (verdict, fs) = verificar_rust(&inp);
    assert_eq!(verdict, "warn");
    assert!(fs.iter().all(|f| f.severidade == "warn"), "delta: {:?}", fs);
    inp.disk = Some("fn f() {}\n".into());
    let (verdict, _) = verificar_rust(&inp);
    assert_eq!(verdict, "deny", "o que a edição introduz nega");
}

#[test]
fn parse_quebrado_e_unknown_com_motivo_nunca_deny_nem_allow() {
    // abstenção total: o AST parcial inventaria nomes ausentes — não medido, com o porquê
    let inp = input_tmp("/tmp/vrs-x.rs", "fn (\n");
    let v = medir(&inp);
    assert!(v.achados.is_empty());
    assert!(
        v.nao_medido
            .as_deref()
            .is_some_and(|m| m.contains("sintaxe")),
        "{:?}",
        v.nao_medido
    );
    let saida = check_em(&inp, Contexto::Execucao);
    assert_eq!(saida.verdict, "unknown");
    assert!(saida.findings.is_empty());
    assert!(saida.motivo.is_some());
}

// --------------------------------------------------------------------------- nao_medido · despacho

#[test]
fn abstencao_tentada_e_nao_fechada_e_nao_medido_com_motivo_nunca_allow() {
    // `use crate::…` num arquivo sem Cargo.toml acima: o adaptador TENTOU resolver e não pôde
    let inp = input_tmp(
        "/tmp/vrs-sem-crate.rs",
        "use crate::motor::Caixa;\nfn f() { let _ = Caixa; }\n",
    );
    let v = medir(&inp);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    let motivo = v.nao_medido.expect("o que não fechou carrega o motivo");
    assert!(
        motivo.contains("crate::motor::Caixa") && motivo.contains("Cargo.toml"),
        "{motivo}"
    );
    let saida = check_em(&inp, Contexto::Execucao);
    assert_eq!(saida.verdict, "unknown", "abstenção nunca vira allow");
    assert!(saida.motivo.is_some());

    // controles: o que o adaptador NUNCA reivindicou não é "não medido" — `std`, crate externo
    // declarado e o item associado de uma struct do projeto fecham limpos (allow, sem motivo)
    let tmp = Path::new("/tmp").join("vrs-t-abstem");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let limpos = [
        (
            "crates/alpha/src/lib.rs",
            "pub mod motor;\npub mod grupo;\nuse std::collections::HashMap;\nfn f() { let _: HashMap<u8, u8> = HashMap::new(); }\n",
        ),
        (
            "crates/alpha/src/lib.rs",
            "pub mod motor;\npub mod grupo;\nuse serde::Serialize;\n",
        ),
        (
            "crates/alpha/src/grupo/folha.rs",
            "fn f() { let _ = crate::motor::Caixa::nova(); }\n",
        ),
        ("crates/alpha/src/grupo/folha.rs", "fn f() -> u8 { 1 }\n"),
    ];
    for (rel, src) in limpos {
        let saida = check_em(&input(&root, rel, src), Contexto::Execucao);
        assert_eq!(
            saida.verdict, "allow",
            "{src}: {:?} {:?}",
            saida.findings, saida.motivo
        );
        assert!(saida.motivo.is_none(), "{src}: {:?}", saida.motivo);
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn despacho_do_nucleo_rust_responde_pelo_adaptador_real_nao_pelo_stub() {
    assert_eq!(adaptador_de("rust").map(|a| a.lang()), Some("rust"));
    let saida = check_em(
        &input_tmp("/tmp/vrs-disp.rs", "fn f() { inexistente(1); }\n"),
        Contexto::Execucao,
    );
    assert_eq!(saida.lang, "rust");
    assert_eq!(saida.verdict, "deny");
    assert!(
        saida
            .motivo
            .as_deref()
            .is_none_or(|m| !m.contains("ainda não implementado")),
        "{:?}",
        saida.motivo
    );
}

// --------------------------------------------------------------------------- regras de resolução (glob · #[path] · non_exhaustive)

#[test]
fn import_pub_use_glob_e_seguido() {
    // `pub use folha::*;` em grupo/mod.rs: o nome vem do alvo do glob (a declaração vence a
    // inferência). O módulo é dado por OVERLAY — o estado do projeto no instante da edição.
    let tmp = Path::new("/tmp").join("vrs-t-glob");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut bom = input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nuse crate::grupo::da_folha;\n",
    );
    bom.overlays.insert(
        root.join("crates/alpha/src/grupo/mod.rs")
            .to_string_lossy()
            .into_owned(),
        "pub mod folha;\npub use folha::*;\npub fn item_do_grupo() {}\n".into(),
    );
    let (verdict, fs) = verificar_rust(&bom);
    assert_eq!(verdict, "allow", "glob reexportado ignorado?: {fs:?}");
    // o controle: um nome que o glob NÃO traz continua ausente
    let mut mau = input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nuse crate::grupo::nao_existe_no_glob;\n",
    );
    mau.overlays = bom.overlays.clone();
    let (_, fs) = verificar_rust(&mau);
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "nao_existe_no_glob".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn import_mod_com_path_attr_resolve_o_arquivo_apontado() {
    // `#[path = "outro.rs"] mod apelido;` — o arquivo é o apontado, não `apelido.rs`
    let tmp = Path::new("/tmp").join("vrs-t-path-attr");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut inp = input(
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\n#[path = \"renomeado.rs\"]\nmod apelido;\nuse self::apelido::coisa;\nuse self::apelido::fantasma;\n",
    );
    inp.overlays.insert(
        root.join("crates/alpha/src/grupo/renomeado.rs")
            .to_string_lossy()
            .into_owned(),
        "pub fn coisa() {}\n".into(),
    );
    let (_, fs) = verificar_rust(&inp);
    assert_eq!(
        achados(&fs, "warn"),
        vec![("import".to_string(), "fantasma".to_string())],
        "só o inventado: `apelido` existe via #[path]"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn kwarg_non_exhaustive_e_struct_tupla_abstem() {
    // `#[non_exhaustive]` e struct-tupla/unit: o campo não é provável (derive/impl gerado) —
    // abstenção, nunca achado
    let src = "#[non_exhaustive]\npub struct Aberta { pub a: u8 }\npub struct Tupla(pub u8);\n\
               fn f() { let _ = Aberta { a: 1, extra: 2 }; let _ = Tupla { 0: 1, 1: 2 }; }\n";
    let (verdict, fs) = verificar_rust(&input_tmp("/tmp/vrs-ne.rs", src));
    assert_eq!(verdict, "allow", "{fs:?}");
}

#[test]
fn nome_glob_de_crate_declarado_e_incerto_nunca_projeto() {
    // `use serde::*;` num crate que declara `serde`: o glob resolve a um crate EXTERNO sem
    // modelo — qualquer nome solto pode vir dele. Warn dinâmico (Incerto), nunca deny.
    let tmp = Path::new("/tmp").join("vrs-t-glob-externo");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let inp = input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nuse serde::*;\nfn f() { algo(); }\n",
    );
    let v = medir(&inp);
    assert!(
        v.achados.is_empty(),
        "o glob externo não decide: {:?}",
        v.achados
    );
    // …e pelo MOTIVO certo: o escopo é dinâmico, declarado em `nao_medido`
    let motivo = v.nao_medido.expect("o nome solto não decide: nao_medido");
    assert!(
        motivo.contains("algo") && motivo.contains("dinâmico"),
        "{motivo}"
    );
    // o controle: sem o glob, o mesmo nome solto é nome ausente do PROJETO
    let sem = input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nfn f() { algo(); }\n",
    );
    let v = medir(&sem);
    assert_eq!(v.achados.len(), 1);
    assert_eq!(v.achados[0].alvo, Alvo::Incerto, "projeto sob /tmp");
    assert!(
        !v.achados[0].sugestao.contains("dinâmico"),
        "sem glob o escopo é fechado: {:?}",
        v.achados[0].sugestao
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- glob de módulo do projeto · controle de código que compila

/// Names of the findings, sorted — the adapter alone.
fn nomes(v: &Verificacao) -> Vec<String> {
    let mut n: Vec<String> = v.achados.iter().map(|f| f.nome.clone()).collect();
    n.sort();
    n
}

/// `true` when every finding says the scope was OPEN (possibly dynamic): the glob/macro could
/// not be enumerated. A closed scope is the claim "this name is absent".
fn todos_dinamicos(v: &Verificacao) -> bool {
    v.achados.iter().all(|f| f.sugestao.contains("dinâmico"))
}

fn overlay(inp: &mut CheckInput, root: &Path, rel: &str, conteudo: &str) {
    inp.overlays.insert(
        root.join(rel).to_string_lossy().into_owned(),
        conteudo.into(),
    );
}

#[test]
fn glob_super_de_mod_tests_inline_traz_itens_e_usos_do_pai_e_fecha_o_escopo() {
    // o `mod tests { use super::*; … }` de todo arquivo: o pai tem uma fn, um `use` PRIVADO e o
    // filho chama as duas coisas. Só o nome que o pai não tem é ausente — e o escopo é FECHADO
    // (antes o glob virava "dinâmico" e nada era afirmado).
    let tmp = Path::new("/tmp").join("vrs-t-glob-super-inline");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "use std::collections::HashMap;\npub fn do_pai() {}\n\
               mod tests {\n    use super::*;\n    fn t() {\n        do_pai();\n        \
               let _: HashMap<u8, u8> = HashMap::new();\n        fantasma();\n    }\n}\n";
    let v = medir(&input(&root, "crates/alpha/src/grupo/folha.rs", src));
    assert_eq!(nomes(&v), vec!["fantasma".to_string()], "{:?}", v.achados);
    assert!(
        !todos_dinamicos(&v),
        "o glob do módulo do projeto fecha o escopo: {:?}",
        v.achados
    );
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn glob_super_de_arquivo_filho_declarado_resolve_o_pai_em_disco() {
    // `folha.rs` (declarado por `grupo/mod.rs`) com `use super::*;`: o pai é o `mod.rs` — a fn
    // dele e o `use` PRIVADO dele (`HashMap`) chegam ao filho; só o ausente é acusado
    let tmp = Path::new("/tmp").join("vrs-t-glob-super-arquivo");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "use super::*;\nfn t() {\n    item_do_grupo();\n    \
               let _: HashMap<u8, u8> = HashMap::new();\n    ausente_no_pai();\n}\n";
    let mut inp = input(&root, "crates/alpha/src/grupo/folha.rs", src);
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\nuse std::collections::HashMap;\npub fn item_do_grupo() {}\n",
    );
    let v = medir(&inp);
    assert_eq!(
        nomes(&v),
        vec!["ausente_no_pai".to_string()],
        "{:?}",
        v.achados
    );
    assert!(!todos_dinamicos(&v), "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn glob_em_cadeia_com_ciclo_segue_o_reexporte_e_termina() {
    // grupo/mod.rs → `pub use extra::*;` e extra.rs → `pub use super::*;` (ciclo): o nome do
    // fim da cadeia entra, o ciclo não trava, o ausente continua ausente
    let tmp = Path::new("/tmp").join("vrs-t-glob-ciclo");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut inp = input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use super::*;\nfn t() { via_cadeia(); ausente(); }\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\npub mod extra;\npub use extra::*;\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/extra.rs",
        "pub fn via_cadeia() {}\npub use super::*;\n",
    );
    let v = medir(&inp);
    assert_eq!(nomes(&v), vec!["ausente".to_string()], "{:?}", v.achados);
    assert!(!todos_dinamicos(&v), "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn glob_super_cujo_pai_tem_glob_externo_ou_macro_de_item_e_dinamico() {
    // o que o pai traz de um crate externo (`use serde::*;`) ou de uma macro de item não é
    // enumerável: o nome solto é "possivelmente dinâmico" — nunca afirmado ausente
    let tmp = Path::new("/tmp").join("vrs-t-glob-super-dinamico");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    for pai in [
        "pub mod folha;\nuse serde::*;\n",
        "pub mod folha;\ngera_itens!();\n",
    ] {
        let mut inp = input(
            &root,
            "crates/alpha/src/grupo/folha.rs",
            "use super::*;\nfn t() { qualquer_um(); }\n",
        );
        overlay(&mut inp, &root, "crates/alpha/src/grupo/mod.rs", pai);
        let v = medir(&inp);
        assert!(v.achados.is_empty(), "{pai}: {:?}", v.achados);
        let motivo = v.nao_medido.expect("o nome solto não decide: nao_medido");
        assert!(
            motivo.contains("qualquer_um") && motivo.contains("dinâmico"),
            "{pai}: {motivo}"
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn path_attr_fora_de_inline_e_relativo_ao_diretorio_do_arquivo_nao_a_foo() {
    // `motor.rs` (não-mod-rs: os filhos comuns vivem em `motor/`) declara
    // `#[path = "motor_tests.rs"] mod tests;` — o arquivo é `src/motor_tests.rs`, ao lado dele
    let tmp = Path::new("/tmp").join("vrs-t-path-irmao");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "pub mod inner;\n#[cfg(test)]\n#[path = \"motor_tests.rs\"]\nmod tests;\n";
    let mut inp = input(&root, "crates/alpha/src/motor.rs", src);
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/motor_tests.rs",
        "fn x() {}\n",
    );
    let v = medir(&inp);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    // o controle: o arquivo apontado que não existe continua sendo o módulo sem arquivo
    let sem = input(&root, "crates/alpha/src/motor.rs", src);
    let v = medir(&sem);
    assert_eq!(nomes(&v), vec!["tests".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn super_de_arquivo_declarado_por_path_attr_sobe_ao_declarante_nao_ao_diretorio() {
    // `motor_tests.rs` é o `mod tests` de `motor.rs` por `#[path]`: o `super` dele é `motor`
    // (tem `ligar`), não o `mod.rs`/`lib.rs` do diretório (que não tem)
    let tmp = Path::new("/tmp").join("vrs-t-super-path");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let motor = "pub mod inner;\npub fn ligar(a: u8, b: u8) -> u8 { a + b }\n\
                 #[cfg(test)]\n#[path = \"motor_tests.rs\"]\nmod tests;\n";
    let mut inp = input(
        &root,
        "crates/alpha/src/motor_tests.rs",
        "use super::*;\nfn t() { ligar(1, 2); ausente_no_motor(); }\n",
    );
    overlay(&mut inp, &root, "crates/alpha/src/motor.rs", motor);
    let v = medir(&inp);
    assert_eq!(
        nomes(&v),
        vec!["ausente_no_motor".to_string()],
        "{:?}",
        v.achados
    );
    assert!(!todos_dinamicos(&v), "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn super_super_de_modulo_inline_em_arquivo_por_path_attr_olha_o_declarante() {
    // `motor_tests.rs` é o `mod tests` de `motor.rs` por `#[path]`; o `mod interno { use
    // super::super::Y; }` dele sobe para `motor` (o declarante), não para a raiz do crate — que é
    // onde o LAYOUT do arquivo o poria: posicionar pelo layout dá falso achado quando `motor` tem
    // `Y` e a raiz do crate não
    let tmp = Path::new("/tmp").join("vrs-t-path-super-super");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let verificar = |com_y: bool| {
        let motor = format!(
            "pub mod inner;\n{}#[cfg(test)]\n#[path = \"motor_tests.rs\"]\nmod tests;\n",
            if com_y { "pub struct Y;\n" } else { "" }
        );
        let mut inp = input(
            &root,
            "crates/alpha/src/motor_tests.rs",
            "mod interno {\n    use super::super::Y;\n}\n",
        );
        overlay(&mut inp, &root, "crates/alpha/src/motor.rs", &motor);
        medir(&inp)
    };
    // (ii) o declarante tem `Y` e a raiz do crate não: nada a acusar
    let v = verificar(true);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    // (i) o declarante não tem `Y`: o import é inexistente
    let v = verificar(false);
    assert_eq!(nomes(&v), vec!["Y".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn struct_unit_ou_tupla_com_associada_nao_e_import_inexistente() {
    // `crate::motor::Tupla::nova(..)`: `Tupla` existe (struct-tupla) e `nova` mora num impl que
    // o adaptador não vê — abstenção; o que NÃO existe no módulo continua acusado
    let tmp = Path::new("/tmp").join("vrs-t-tupla-assoc");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let bom =
        "pub mod motor;\npub mod grupo;\nfn f() { let _ = crate::motor::Tupla::nova(1, 2); }\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", bom));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    let mau =
        "pub mod motor;\npub mod grupo;\nfn f() { let _ = crate::motor::Fantasma::nova(1); }\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", mau));
    assert_eq!(nomes(&v), vec!["Fantasma".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn enum_alcancado_por_glob_reexportado_julga_a_variante() {
    // `pub use crate::motor::*;` em grupo/mod.rs: `crate::grupo::Falha::Rede` passa pelo glob
    // até o enum — a variante existente fecha, a inventada é `atributo`
    let tmp = Path::new("/tmp").join("vrs-t-enum-glob");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut bom = input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nfn f() { let _ = crate::grupo::Falha::Rede; }\n",
    );
    overlay(
        &mut bom,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\npub use crate::motor::*;\n",
    );
    let v = medir(&bom);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    let mut mau = input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nfn f() { let _ = crate::grupo::Falha::Fantasma; }\n",
    );
    mau.overlays = bom.overlays.clone();
    let v = medir(&mau);
    assert_eq!(nomes(&v), vec!["Fantasma".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn glob_reexportado_segue_reexport_explicito_e_glob_aninhado_do_alvo() {
    // `touring_hooks::runtime::HookRuntime`: o crate faz `pub use dispatch::*;` e o `dispatch`
    // tem `pub use outro::runtime;` — o nome está DOIS reexports adiante do glob.
    // grupo/mod.rs → `pub use extra::*;` ; extra.rs → reexport explícito com alias + glob de motor
    let tmp = Path::new("/tmp").join("vrs-t-glob-cadeia");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let lib = "pub mod motor;\npub mod grupo;\nuse crate::grupo::ligar_renomeado;\n\
               use crate::grupo::Caixa;\nuse crate::grupo::nao_existe;\n";
    let mut inp = input(&root, "crates/alpha/src/lib.rs", lib);
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\npub mod extra;\npub use extra::*;\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/extra.rs",
        "pub use crate::motor::ligar as ligar_renomeado;\npub use crate::motor::*;\n",
    );
    let v = medir(&inp);
    assert_eq!(nomes(&v), vec!["nao_existe".to_string()], "{:?}", v.achados);
    // o controle: com um glob de crate EXTERNO na cadeia o nome pode vir dele — não se afirma nada
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/extra.rs",
        "pub use crate::motor::ligar as ligar_renomeado;\npub use crate::motor::*;\npub use serde::*;\n",
    );
    let v = medir(&inp);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    assert!(
        v.nao_medido.is_some(),
        "o glob externo é abstenção declarada"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn modulo_alcancado_por_reexport_segue_dentro_dele_nao_vira_o_nome_do_item() {
    // `pub use crate::motor as maquina;` (o `pub use touring_storage::knowledge;` do touring):
    // `use crate::grupo::maquina::Caixa;` liga `Caixa` à STRUCT do motor — não ao módulo
    // `maquina` inteiro, que faria de `Caixa::nova()` um «`nova` não encontrado»
    let tmp = Path::new("/tmp").join("vrs-t-reexport-modulo");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut inp = input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nuse crate::grupo::maquina::Caixa;\n\
         use crate::grupo::maquina::fantasma;\nfn f() { let _ = Caixa::nova(); }\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\npub use crate::motor as maquina;\n",
    );
    let v = medir(&inp);
    assert_eq!(nomes(&v), vec!["fantasma".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn reexport_dentro_de_modulo_inline_e_seguido_pela_posicao() {
    // `pub mod fachada { pub use crate::motor::{Caixa, ligar}; }` — o submodelo do módulo inline
    // guarda só itens: os reexports vêm do modelo do arquivo, filtrados pela posição
    let tmp = Path::new("/tmp").join("vrs-t-reexport-inline");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "pub mod motor;\npub mod grupo;\n\
               pub mod fachada { pub use crate::motor::{Caixa, ligar}; }\n\
               use crate::fachada::Caixa;\nuse crate::fachada::fantasma;\n\
               fn f() { let _ = crate::fachada::ligar(1, 2); let _: Option<Caixa> = None; }\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", src));
    assert_eq!(nomes(&v), vec!["fantasma".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn glob_reexportado_em_ciclo_termina_e_o_ausente_continua_ausente() {
    // grupo/mod.rs → `pub use extra::*;` e extra.rs → `pub use super::*;` (ciclo): a busca de
    // `nao_existe` pelos alvos dos globs corta o ciclo e conclui «ausente» — sem a guarda ela só
    // pararia na profundidade máxima, e aí abstém (nada seria acusado)
    let tmp = Path::new("/tmp").join("vrs-t-glob-reexport-ciclo");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut inp = input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\nuse crate::grupo::nao_existe;\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\npub mod extra;\npub use extra::*;\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/extra.rs",
        "pub fn presente() {}\npub use super::*;\n",
    );
    let v = medir(&inp);
    assert_eq!(nomes(&v), vec!["nao_existe".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn glob_reexportado_dentro_de_modulo_inline_e_seguido_pela_posicao() {
    // `pub mod fachada { pub use crate::motor::*; }`: o glob mora num módulo INLINE — o submodelo
    // dele guarda só itens, então os globs vêm do modelo do arquivo, filtrados pela posição
    let tmp = Path::new("/tmp").join("vrs-t-glob-inline");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "pub mod motor;\npub mod grupo;\n\
               pub mod fachada { pub use crate::motor::*; }\n\
               use crate::fachada::Caixa;\nuse crate::fachada::fantasma;\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", src));
    assert_eq!(nomes(&v), vec!["fantasma".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn use_de_enum_antes_do_glob_que_o_traz_tambem_fecha() {
    // a folha `use Cor::Rubra;` vem ANTES, no texto, do `use super::*;` que traz `Cor`: os globs
    // são processados primeiro, não na ordem do arquivo
    let tmp = Path::new("/tmp").join("vrs-t-use-enum-ordem");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut inp = input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "fn t() { use Cor::Rubra; let _ = Rubra; }\nuse super::*;\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\npub enum Cor { Rubra }\n",
    );
    let v = medir(&inp);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn use_cujo_caminho_comeca_pelo_proprio_nome_liga_o_crate_externo_e_nao_um_item_local() {
    // `use serde::serde;` (o shape de `use rstest::rstest;` em tests/): o `use` privado introduz
    // `serde`, mas a cabeça do caminho é o CRATE — tratá-la como módulo local fazia a busca girar
    // sobre si mesma e concluir «crate `serde` não está nas dependências» (controle de 130
    // arquivos que compilam). O controle: um crate que de fato não existe continua acusado.
    let tmp = Path::new("/tmp").join("vrs-t-use-autorreferente");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let v = medir(&input(
        &root,
        "crates/alpha/src/motor.rs",
        "use serde::serde;\nuse serde::Serialize;\npub fn f() {}\n",
    ));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    let v = medir(&input(
        &root,
        "crates/alpha/src/motor.rs",
        "use fantasma::fantasma;\n",
    ));
    assert_eq!(nomes(&v), vec!["fantasma".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn modulo_com_macro_de_item_nao_prova_a_ausencia_do_nome() {
    // `grupo/mod.rs` declara itens por macro (`gera_itens!(Foo);`): `use crate::grupo::Foo;` pode
    // ser válido — a ausência não está provada, então nada é acusado e o motivo vai a `nao_medido`.
    // O controle: sem a macro, o mesmo caminho continua acusado.
    let tmp = Path::new("/tmp").join("vrs-t-modulo-com-macro");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "pub mod motor;\npub mod grupo;\nuse crate::grupo::Foo;\n";
    let mut inp = input(&root, "crates/alpha/src/lib.rs", src);
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\ngera_itens!(Foo);\n",
    );
    let v = medir(&inp);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    assert!(
        v.nao_medido
            .as_deref()
            .is_some_and(|m| m.contains("`crate::grupo::Foo`") && m.contains("macro")),
        "{:?}",
        v.nao_medido
    );
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", src));
    assert_eq!(nomes(&v), vec!["Foo".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn reexport_do_nome_que_nao_fechou_nao_vira_ausencia() {
    // `pub use grupo::Fantasma;` no lib.rs declara `Fantasma`; o `grupo/mod.rs` está ilegível
    // (sintaxe quebrada), então o reexport não fecha. O nome existe — só não se sabe onde: o
    // consumidor `use crate::Fantasma;` não é acusado e o motivo vai a `nao_medido`.
    let tmp = Path::new("/tmp").join("vrs-t-reexport-aberto");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut inp = input(
        &root,
        "crates/alpha/src/motor.rs",
        "use crate::Fantasma;\npub fn f() {}\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\npub use grupo::Fantasma;\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\nfn {{{ quebrado\n",
    );
    let v = medir(&inp);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    assert!(
        v.nao_medido
            .as_deref()
            .is_some_and(|m| m.contains("`crate::Fantasma`")),
        "{:?}",
        v.nao_medido
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn modulo_inline_so_ve_do_arquivo_o_que_o_use_super_traz() {
    // `index.rs:3167` reduzido: `ingestao` é item da raiz do arquivo e o `mod por_que` importa só
    // `reconstroi` com `use super::…`; chamar `ingestao(1)` lá dentro é E0425 (antes o adaptador
    // ligava o arquivo inteiro a todo `mod` inline e a aridade do item da raiz encobria a falta)
    let tmp = Path::new("/tmp").join("vrs-t-inline-escopo");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let com = |uso: &str| {
        format!(
            "pub fn ingestao(a: u8) {{}}\npub fn reconstroi() {{}}\n\
             mod por_que {{\n    {uso}\n    fn t() {{ reconstroi(); ingestao(1); }}\n}}\n"
        )
    };
    let v = medir(&input(
        &root,
        "crates/alpha/src/motor.rs",
        &com("use super::reconstroi;"),
    ));
    assert_eq!(nomes(&v), vec!["ingestao".to_string()], "{:?}", v.achados);
    // os controles: o que o `use super::…` traz (lista, glob) está em escopo
    for uso in ["use super::{reconstroi, ingestao};", "use super::*;"] {
        let v = medir(&input(&root, "crates/alpha/src/motor.rs", &com(uso)));
        assert!(v.achados.is_empty(), "{uso}: {:?}", v.achados);
    }
    // o próprio arquivo e o item do próprio módulo continuam em escopo
    let src = "pub fn ingestao(a: u8) {}\nfn raiz() { ingestao(1); }\n\
               mod m {\n    fn propria() {}\n    fn t() { propria(); }\n}\n";
    let v = medir(&input(&root, "crates/alpha/src/motor.rs", src));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn cabeca_de_caminho_no_modulo_inline_exige_o_use() {
    // `Cor::Rubra` num `mod` inline sem `use super::Cor`: a cabeça (enum da raiz do arquivo) não
    // está em escopo ali — o caminho tipado do arquivo não a empresta ao módulo
    let tmp = Path::new("/tmp").join("vrs-t-inline-cabeca");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let com = |uso: &str| {
        format!(
            "pub enum Cor {{ Rubra }}\nmod m {{\n    {uso}\n    fn t() {{ let _ = Cor::Rubra; }}\n}}\n"
        )
    };
    let v = medir(&input(&root, "crates/alpha/src/motor.rs", &com("")));
    assert_eq!(nomes(&v), vec!["Cor".to_string()], "{:?}", v.achados);
    let v = medir(&input(
        &root,
        "crates/alpha/src/motor.rs",
        &com("use super::Cor;"),
    ));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn macro_de_item_dentro_do_modulo_inline_abre_so_aquele_escopo() {
    // `mod m { gera!(Foo); … Foo … }`: o nome pode vir da macro DAQUELE módulo — nada é acusado e
    // o motivo vai a `nao_medido`. O controle: sem a macro, o mesmo nome ausente é acusado.
    let v = medir(&input_tmp(
        "/tmp/vrs-mac-inline.rs",
        "mod m {\n    gera!(Foo);\n    fn t() { let _ = Foo; }\n}\n",
    ));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    assert!(
        v.nao_medido
            .as_deref()
            .is_some_and(|m| m.contains("`Foo`") && m.contains("dinâmico")),
        "{:?}",
        v.nao_medido
    );
    let v = medir(&input_tmp(
        "/tmp/vrs-mac-inline2.rs",
        "mod m {\n    fn t() { let _ = Foo; }\n}\n",
    ));
    assert_eq!(nomes(&v), vec!["Foo".to_string()], "{:?}", v.achados);
}

#[test]
fn modulo_fundo_demais_para_modelar_nao_prova_ausencia() {
    // o submodelo vai até 4 níveis: no 5º o adaptador não sabe o que o módulo declara (nem os
    // itens dele) — nenhum nome ali é acusado, e o motivo vai a `nao_medido`
    let src = "mod a { mod b { mod c { mod d { mod e {\n    fn propria() {}\n    \
               fn t() { propria(); fantasma(); }\n} } } } }\n";
    let v = medir(&input_tmp("/tmp/vrs-fundo.rs", src));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    assert!(
        v.nao_medido
            .as_deref()
            .is_some_and(|m| m.contains("`fantasma`")),
        "{:?}",
        v.nao_medido
    );
}

#[test]
fn use_de_enum_no_modulo_inline_com_a_cabeca_trazida_por_glob_do_mesmo_modulo() {
    // `mod m { use super::super::*; fn t() { use Cor::Rubra; … } }`: `Cor` entra em escopo pelo glob
    // DO PRÓPRIO `m` (não pela raiz do arquivo, que não o tem) — a folha `use Cor::Rubra` fecha
    let tmp = Path::new("/tmp").join("vrs-t-inline-glob-cabeca");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut inp = input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "mod m {\n    use super::super::*;\n    fn t() { use Cor::Rubra; let _ = Rubra; }\n}\n",
    );
    overlay(
        &mut inp,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\npub enum Cor { Rubra }\n",
    );
    let v = medir(&inp);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn funcao_declarada_em_bloco_extern_e_item_do_modulo() {
    // `unsafe extern "C" { fn flock(..); }` (daemon.rs, 2 negativos naturais): a chamada `flock(..)`
    // não é nome ausente — o bloco declara o item. O controle: o que o bloco NÃO declara continua
    // acusado.
    for abre in ["unsafe extern \"C\" {", "extern \"C\" {"] {
        let bom = format!(
            "{abre}\n    fn flock(fd: i32, op: i32) -> i32;\n}}\n\
             fn t() {{ let _ = unsafe {{ flock(1, 2) }}; }}\n"
        );
        let v = medir(&input_tmp("/tmp/vrs-extern.rs", &bom));
        assert!(v.achados.is_empty(), "{abre}: {:?}", v.achados);
    }
    let mau = "unsafe extern \"C\" {\n    fn flock(fd: i32, op: i32) -> i32;\n}\n\
               fn t() { let _ = unsafe { funlock(1, 2) }; }\n";
    let v = medir(&input_tmp("/tmp/vrs-extern2.rs", mau));
    assert_eq!(nomes(&v), vec!["funlock".to_string()], "{:?}", v.achados);
}

#[test]
fn comentarios_na_lista_de_argumentos_nao_contam_como_argumento() {
    // `dois(1, // porque\n 2, /* nada */)` tem 2 argumentos: os comentários são extras da gramática
    let (_, fs) = verificar_rust(&input_tmp(
        "/tmp/vrs-com.rs",
        "fn dois(a: u8, b: u8) {}\nfn t() {\n    dois(\n        1, // porque\n        2, /* nada */\n    );\n}\n",
    ));
    assert!(fs.is_empty(), "{fs:?}");
    // o controle: UM argumento mais um comentário continua sendo um argumento (aridade errada)
    let (_, fs) = verificar_rust(&input_tmp(
        "/tmp/vrs-com2.rs",
        "fn dois(a: u8, b: u8) {}\nfn t() {\n    dois(\n        1, // só um\n    );\n}\n",
    ));
    assert_eq!(
        achados(&fs, "deny"),
        vec![("kwarg".to_string(), "dois".to_string())]
    );
}

#[test]
fn fn_de_mesmo_nome_em_mod_inline_ou_na_fn_sombreia_a_do_arquivo() {
    // a fn do arquivo tem 4 parâmetros; o `mod tests` tem a SUA de 3 e a fn `h` uma local de 1:
    // chamar essas não é chamar a do arquivo (E0061 falso)
    let src = "fn alvo(a: u8, b: u8, c: u8, d: u8) {}\n\
               mod tests {\n    fn alvo(a: u8, b: u8, c: u8) {}\n    fn t() { alvo(1, 2, 3); }\n}\n\
               fn h() { fn alvo(a: u8) {} alvo(1); }\n";
    let (_, fs) = verificar_rust(&input_tmp("/tmp/vrs-som.rs", src));
    assert!(fs.is_empty(), "{fs:?}");
    // o controle: a chamada errada à fn do ARQUIVO continua acusada
    let (_, fs) = verificar_rust(&input_tmp(
        "/tmp/vrs-som2.rs",
        &format!("{src}fn g() {{ alvo(1, 2); }}\n"),
    ));
    assert_eq!(
        achados(&fs, "deny"),
        vec![("kwarg".to_string(), "alvo".to_string())]
    );
}

#[test]
fn import_privado_do_pai_e_visivel_ao_filho_por_super() {
    // `use crate::motor::Caixa;` PRIVADO em grupo/mod.rs: o filho `folha.rs` o alcança por
    // `use super::Caixa;` (um import privado é visível aos descendentes do módulo)
    let tmp = Path::new("/tmp").join("vrs-t-import-privado");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mut bom = input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use super::Caixa;\nfn t(_c: Option<Caixa>) {}\n",
    );
    overlay(
        &mut bom,
        &root,
        "crates/alpha/src/grupo/mod.rs",
        "pub mod folha;\nuse crate::motor::Caixa;\n",
    );
    let v = medir(&bom);
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    // o controle: um nome que o pai NÃO importa continua ausente
    let mut mau = input(
        &root,
        "crates/alpha/src/grupo/folha.rs",
        "use super::Fantasma;\n",
    );
    mau.overlays = bom.overlays.clone();
    let v = medir(&mau);
    assert_eq!(nomes(&v), vec!["Fantasma".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn pub_use_de_dependencia_liga_o_crate_externo_nao_um_item_local() {
    // `pub use serde;` (o `pub use tree_sitter;` do touring): re-exporta a dependência
    let tmp = Path::new("/tmp").join("vrs-t-pub-use-dep");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let v = medir(&input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\npub use serde;\n",
    ));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    // o controle: um crate que o manifesto não declara continua sendo import inexistente
    let v = medir(&input(
        &root,
        "crates/alpha/src/lib.rs",
        "pub mod motor;\npub mod grupo;\npub use nao_declarado;\n",
    ));
    assert_eq!(
        nomes(&v),
        vec!["nao_declarado".to_string()],
        "{:?}",
        v.achados
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn use_de_enum_ligado_por_glob_dentro_de_fn_nao_e_import_inexistente() {
    // `use Cor::{Rubra, Verde};` no corpo de uma fn de `mod tests`, com `Cor` trazido por
    // `use super::*;` (caminhos uniformes: a cabeça é procurada entre os nomes em escopo)
    let tmp = Path::new("/tmp").join("vrs-t-use-enum-fn");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let bom = "pub mod motor;\npub mod grupo;\nenum Cor { Rubra, Verde }\nmod tests {\n    \
               use super::*;\n    fn t() { use Cor::{Rubra, Verde}; let _ = (Rubra, Verde); }\n}\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", bom));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    // o controle: a cabeça que NINGUÉM trouxe continua sendo import inexistente
    let mau = "pub mod motor;\npub mod grupo;\nfn t() { use Fantasma::{Rubra}; }\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", mau));
    assert_eq!(nomes(&v), vec!["Fantasma".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn const_e_static_declaram_o_nome_e_o_tipo_nao_e_uso_de_valor() {
    // `pub const ALL: [Self; 1]` num impl declara `ALL`; `static Q: Option<serde::Value>` num
    // corpo de fn tem um caminho de TIPO. Só o inicializador é lido como uso.
    let tmp = Path::new("/tmp").join("vrs-t-const-decl");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let bom = "pub mod motor;\npub mod grupo;\npub enum K { A }\n\
               impl K { pub const ALL: [Self; 1] = [Self::A]; }\n\
               fn f() { static Q: Option<serde::Value> = None; let _ = (K::ALL, &Q); }\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", bom));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    // o controle: o inicializador continua sendo verificado
    let mau = "pub mod motor;\npub mod grupo;\npub struct S;\nimpl S { pub const N: u8 = inexistente(); }\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", mau));
    assert_eq!(
        nomes(&v),
        vec!["inexistente".to_string()],
        "{:?}",
        v.achados
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn modulos_inline_aninhados_enxergam_os_itens_do_proprio_bloco() {
    // `mod a { mod b { const X…; fn f() { X } } }`: o item do módulo mais interno é dele
    let tmp = Path::new("/tmp").join("vrs-t-inline-aninhado");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let bom =
        "pub mod motor;\npub mod grupo;\nmod a { mod b { const X: u8 = 1; fn f() -> u8 { X } } }\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", bom));
    assert!(v.achados.is_empty(), "{:?}", v.achados);
    // o controle: o nome que nenhum bloco tem continua ausente
    let mau = "pub mod motor;\npub mod grupo;\nmod a { mod b { fn f() -> u8 { Y } } }\n";
    let v = medir(&input(&root, "crates/alpha/src/lib.rs", mau));
    assert_eq!(nomes(&v), vec!["Y".to_string()], "{:?}", v.achados);
    let _ = std::fs::remove_dir_all(&tmp);
}
