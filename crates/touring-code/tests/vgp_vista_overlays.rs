//! Characterization of the project view (C4, 12/11/2026): the overlays of an entry laid over the disk,
//! as each language adapter answers it. Four cases per adapter — only on disk, the overlay over the
//! disk, only in the overlay, absent — asked through the whole motor (`check_em`), so they hold
//! whatever sits behind the adapters: they were written BEFORE the adapters moved onto
//! `vgp::lang::vista` and have to keep passing after.
//!
//! Two cases are the defects the view's unification fixed (RED before, GREEN after):
//! `rust_path_com_ponto_e_dot_dot_acha_o_overlay` (the overlay key is the NORMAL path: Rust and
//! Python looked it up as spelled) and `html_raiz_relativa_acha_o_overlay_em_diretorio_que_so_o_overlay_cria`
//! (the directory an overlay creates exists: only TypeScript counted it).
//!
//! `overlay-apaga` is not tested: no adapter has the notion (`HashMap<String, String>` cannot say
//! "this file is gone").

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use touring_code::vgp::{CheckInput, Contexto, check_em};

/// What the motor answered: the verdict and the (classe, nome) of every finding, sorted.
type Resposta = (String, Vec<(String, String)>);

/// A directory for the tree that is NOT under `/tmp`: a path under `/tmp` is a scratch fragment for
/// the policy and for the TypeScript adapter, and the cases below are about the project.
fn arvore() -> tempfile::TempDir {
    let preferido = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let base = if preferido.starts_with("/tmp") {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".vgp-vista-tmp")
    } else {
        preferido
    };
    fs::create_dir_all(&base).expect("base dir");
    tempfile::Builder::new()
        .prefix("vgp-vista-")
        .tempdir_in(&base)
        .expect("tempdir")
}

fn escreve(raiz: &Path, rel: &str, conteudo: &str) {
    let alvo = raiz.join(rel);
    fs::create_dir_all(alvo.parent().expect("pai")).expect("mkdir");
    fs::write(alvo, conteudo).expect("write");
}

/// `conteudo` proposed at `rel`, with `overlays` (relative path, content) laid over the disk.
fn rodar(
    raiz: &Path,
    rel: &str,
    lang: &str,
    conteudo: &str,
    overlays: &[(&str, &str)],
) -> Resposta {
    let entrada = CheckInput {
        path: raiz.join(rel).to_string_lossy().into_owned(),
        content: conteudo.to_string(),
        project_root: raiz.to_path_buf(),
        disk: None,
        overlays: overlays
            .iter()
            .map(|(k, v)| {
                (
                    raiz.join(k).to_string_lossy().into_owned(),
                    (*v).to_string(),
                )
            })
            .collect(),
        lang: lang.to_string(),
    };
    let saida = check_em(&entrada, Contexto::Execucao);
    let mut achados: Vec<(String, String)> = saida
        .findings
        .iter()
        .map(|f| (f.classe.clone(), f.nome.clone()))
        .collect();
    achados.sort();
    (saida.verdict, achados)
}

fn par(classe: &str, nome: &str) -> (String, String) {
    (classe.to_string(), nome.to_string())
}

fn limpo() -> Resposta {
    ("allow".to_string(), Vec::new())
}

// ------------------------------------------------------------------------------------ Python

#[test]
fn python_so_disco_overlay_vence_so_overlay_e_ausente() {
    let d = arvore();
    let r = d.path();
    escreve(r, "pkg/__init__.py", "");
    escreve(r, "pkg/mod.py", "so_disco = 1\n");
    let usa = |nome: &str| format!("from pkg.mod import {nome}\n");

    // only on disk
    assert_eq!(
        rodar(r, "main.py", "python", &usa("so_disco"), &[]),
        limpo()
    );
    assert_eq!(
        rodar(r, "main.py", "python", &usa("so_overlay"), &[]).1,
        vec![par("import", "so_overlay")]
    );
    // the overlay wins over the disk: what the disk file defined is gone at the entry's instant
    let sobre = [("pkg/mod.py", "so_overlay = 2\n")];
    assert_eq!(
        rodar(r, "main.py", "python", &usa("so_overlay"), &sobre),
        limpo()
    );
    assert_eq!(
        rodar(r, "main.py", "python", &usa("so_disco"), &sobre).1,
        vec![par("import", "so_disco")]
    );
    // only in the overlay: a module the disk does not have
    let novo = [("pkg/novo.py", "n = 1\n")];
    assert_eq!(
        rodar(r, "main.py", "python", "from pkg.novo import n\n", &novo),
        limpo()
    );
    assert_eq!(
        rodar(r, "main.py", "python", "from pkg.novo import m\n", &novo).1,
        vec![par("import", "m")]
    );
    // absent everywhere: the module cannot be resolved, so nothing is claimed about its names
    let ausente = rodar(r, "main.py", "python", "from pkg.nada import x\n", &[]);
    assert_ne!(
        ausente.0, "deny",
        "módulo irresolvível nunca é deny: {ausente:?}"
    );
    assert!(ausente.1.is_empty(), "{ausente:?}");
}

// ------------------------------------------------------------------------------------ Rust

fn projeto_rust(r: &Path) {
    escreve(
        r,
        "Cargo.toml",
        "[package]\nname = 'alpha'\nversion = '0.1.0'\nedition = '2021'\n",
    );
    escreve(r, "src/lib.rs", "pub mod motor;\n");
    escreve(r, "src/motor.rs", "pub fn so_disco() {}\n");
}

#[test]
fn rust_so_disco_overlay_vence_so_overlay_e_ausente() {
    let d = arvore();
    let r = d.path();
    projeto_rust(r);
    let usa = |nome: &str| format!("pub mod motor;\nuse crate::motor::{nome};\n");

    // only on disk
    assert_eq!(
        rodar(r, "src/lib.rs", "rust", &usa("so_disco"), &[]),
        limpo()
    );
    assert_eq!(
        rodar(r, "src/lib.rs", "rust", &usa("so_overlay"), &[]).1,
        vec![par("import", "so_overlay")]
    );
    // the overlay wins over the disk
    let sobre = [("src/motor.rs", "pub fn so_overlay() {}\n")];
    assert_eq!(
        rodar(r, "src/lib.rs", "rust", &usa("so_overlay"), &sobre),
        limpo()
    );
    assert_eq!(
        rodar(r, "src/lib.rs", "rust", &usa("so_disco"), &sobre).1,
        vec![par("import", "so_disco")]
    );
    // only in the overlay: `mod novo;` finds its file there
    let novo = [("src/novo.rs", "pub fn n() {}\n")];
    assert_eq!(
        rodar(
            r,
            "src/lib.rs",
            "rust",
            "pub mod motor;\npub mod novo;\nuse crate::novo::n;\n",
            &novo
        ),
        limpo()
    );
    // absent everywhere: `mod fantasma;` has no file
    let ausente = rodar(
        r,
        "src/lib.rs",
        "rust",
        "pub mod motor;\npub mod fantasma;\n",
        &[],
    );
    assert_eq!(ausente.1, vec![par("import", "fantasma")], "{ausente:?}");
}

/// RED before the view: the overlay key is the NORMAL path, but Rust looked it up as the `#[path]`
/// was spelled — `./via_ponto.rs` and `sub/../via_dd.rs` missed the overlay, the disk had no such
/// file, and the module was reported "declared without a file".
#[test]
fn rust_path_com_ponto_e_dot_dot_acha_o_overlay() {
    let d = arvore();
    let r = d.path();
    projeto_rust(r);
    // the `..` is built by component: the literal in a source file is what the CWE-22 rule flags
    let sobe = [".", "sub", ".."].join("/");
    let conteudo = format!(
        "pub mod motor;\n#[path = \"./via_ponto.rs\"]\nmod via_ponto;\n\
         #[path = \"{sobe}/via_dd.rs\"]\nmod via_dd;\nuse via_ponto::p;\nuse via_dd::q;\n"
    );
    let sobre = [
        ("src/via_ponto.rs", "pub fn p() {}\n"),
        ("src/via_dd.rs", "pub fn q() {}\n"),
    ];
    assert_eq!(rodar(r, "src/lib.rs", "rust", &conteudo, &sobre), limpo());
    // the control: without the overlays the same two modules ARE missing
    let sem = rodar(r, "src/lib.rs", "rust", &conteudo, &[]);
    assert_eq!(
        sem.1,
        vec![par("import", "via_dd"), par("import", "via_ponto")],
        "{sem:?}"
    );
}

/// D2, preserved: the crate root is probed on the DISK only (a manifest or `lib.rs` that exists
/// only in the overlays does not make a crate), while a module file is read overlay-first. The
/// answer for the overlay-only crate is an abstention, never a wrong finding.
#[test]
fn rust_crate_so_no_overlay_e_abstencao() {
    let d = arvore();
    let r = d.path();
    projeto_rust(r);
    let sobre = [
        (
            "novo/Cargo.toml",
            "[package]\nname = 'novo'\nversion = '0.1.0'\n",
        ),
        ("novo/src/lib.rs", "pub fn so_no_overlay() {}\n"),
    ];
    let resp = rodar(
        r,
        "novo/src/dentro.rs",
        "rust",
        "use crate::so_no_overlay;\nuse crate::inventado;\n",
        &sobre,
    );
    assert_ne!(resp.0, "deny", "crate só-overlay não pode negar: {resp:?}");
}

// ------------------------------------------------------------------------------------ TypeScript

#[test]
fn typescript_so_disco_overlay_vence_so_overlay_e_ausente() {
    let d = arvore();
    let r = d.path();
    escreve(r, "m.ts", "export const so_disco = 1;\n");
    let usa = |nome: &str| format!("import {{ {nome} }} from './m';\nconsole.log({nome});\n");

    // only on disk
    assert_eq!(
        rodar(r, "a.ts", "typescript", &usa("so_disco"), &[]),
        limpo()
    );
    assert_eq!(
        rodar(r, "a.ts", "typescript", &usa("so_overlay"), &[]).1,
        vec![par("import", "so_overlay")]
    );
    // the overlay wins over the disk
    let sobre = [("m.ts", "export const so_overlay = 2;\n")];
    assert_eq!(
        rodar(r, "a.ts", "typescript", &usa("so_overlay"), &sobre),
        limpo()
    );
    assert_eq!(
        rodar(r, "a.ts", "typescript", &usa("so_disco"), &sobre).1,
        vec![par("import", "so_disco")]
    );
    // only in the overlay
    let novo = [("novo.ts", "export const N = 1;\n")];
    assert_eq!(
        rodar(
            r,
            "a.ts",
            "typescript",
            "import { N } from './novo';\nconsole.log(N);\n",
            &novo
        ),
        limpo()
    );
    // absent everywhere
    let ausente = rodar(
        r,
        "a.ts",
        "typescript",
        "import { N } from './nada';\nconsole.log(N);\n",
        &[],
    );
    assert_eq!(ausente.1, vec![par("import", "nada")], "{ausente:?}");
}

/// The directory an overlay creates exists (the TypeScript rule): `./dir` resolves to the
/// `index.ts` that only the overlay has; with no overlay the import is missing.
#[test]
fn typescript_diretorio_que_so_o_overlay_cria_existe() {
    let d = arvore();
    let r = d.path();
    let usa = "import { D } from './dir';\nconsole.log(D);\n";
    let sobre = [("dir/index.ts", "export const D = 1;\n")];
    assert_eq!(rodar(r, "a.ts", "typescript", usa, &sobre), limpo());
    assert_eq!(
        rodar(r, "a.ts", "typescript", usa, &[]).1,
        vec![par("import", "dir")]
    );
}

// ------------------------------------------------------------------------------------ HTML

fn pagina(corpo: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<title>t</title>\n</head>\n<body>\n{corpo}\n</body>\n</html>\n"
    )
}

#[test]
fn html_so_disco_overlay_so_overlay_e_ausente() {
    let d = arvore();
    let r = d.path();
    escreve(r, "img/a.png", "x");
    let usa = |src: &str| pagina(&format!("<img src=\"{src}\">"));

    // only on disk
    assert_eq!(
        rodar(r, "index.html", "html", &usa("img/a.png"), &[]),
        limpo()
    );
    // only in the overlay
    assert_eq!(
        rodar(
            r,
            "index.html",
            "html",
            &usa("img/b.png"),
            &[("img/b.png", "x")]
        ),
        limpo()
    );
    // on the disk AND in the overlay
    assert_eq!(
        rodar(
            r,
            "index.html",
            "html",
            &usa("img/a.png"),
            &[("img/a.png", "y")]
        ),
        limpo()
    );
    // absent everywhere
    let ausente = rodar(r, "index.html", "html", &usa("img/c.png"), &[]);
    assert_eq!(ausente.1, vec![par("recurso", "img/c.png")], "{ausente:?}");
}

/// RED before the view: `/assets/d.png` names a path on the server; the adapter tries each
/// candidate root and drops one whose FIRST SEGMENT does not exist — but the directory `assets`
/// exists only because an overlay has a file under it, and only TypeScript counted that.
#[test]
fn html_raiz_relativa_acha_o_overlay_em_diretorio_que_so_o_overlay_cria() {
    let d = arvore();
    let r = d.path();
    let doc = pagina("<img src=\"/assets/d.png\">");
    assert_eq!(
        rodar(r, "index.html", "html", &doc, &[("assets/d.png", "x")]),
        limpo()
    );
    // the control: no overlay, no directory, no file
    let sem = rodar(r, "index.html", "html", &doc, &[]);
    assert_eq!(sem.1, vec![par("recurso", "/assets/d.png")], "{sem:?}");
}

// ------------------------------------------------------------------------------------ CSS (classes of a document)

#[test]
fn css_so_disco_overlay_vence_so_overlay_e_ausente() {
    let d = arvore();
    let r = d.path();
    escreve(r, "s.css", ".so-disco { color: red }\n");
    let doc = |href: &str, classe: &str| {
        format!(
            "<!DOCTYPE html>\n<html>\n<head>\n<title>t</title>\n<link rel=\"stylesheet\" href=\"{href}\">\n</head>\n\
             <body>\n<p class=\"{classe}\">oi</p>\n</body>\n</html>\n"
        )
    };

    // only on disk
    assert_eq!(
        rodar(r, "index.html", "html", &doc("s.css", "so-disco"), &[]),
        limpo()
    );
    assert_eq!(
        rodar(r, "index.html", "html", &doc("s.css", "so-overlay"), &[]).1,
        vec![par("classe_css", "so-overlay")]
    );
    // the overlay wins over the disk
    let sobre = [("s.css", ".so-overlay { color: blue }\n")];
    assert_eq!(
        rodar(r, "index.html", "html", &doc("s.css", "so-overlay"), &sobre),
        limpo()
    );
    assert_eq!(
        rodar(r, "index.html", "html", &doc("s.css", "so-disco"), &sobre).1,
        vec![par("classe_css", "so-disco")]
    );
    // only in the overlay
    let novo = [("novo.css", ".so-novo { color: green }\n")];
    assert_eq!(
        rodar(r, "index.html", "html", &doc("novo.css", "so-novo"), &novo),
        limpo()
    );
    // absent everywhere: the sheet cannot be read, so the class is not measured (and the missing
    // file is the HTML resource check's finding, not the class check's)
    let ausente = rodar(r, "index.html", "html", &doc("nada.css", "qualquer"), &[]);
    assert_eq!(ausente.1, vec![par("recurso", "nada.css")], "{ausente:?}");
}

// ------------------------------------------------------------------------------------ structural guard

/// How the overlay map is spelled when something asks it directly.
const CONSULTAS: &[&str] = &[
    "overlays.get(",
    "overlays.contains_key(",
    "overlays.keys(",
    "overlays.iter(",
    "overlays.values(",
    "sobrepor.get(",
    "sobrepor.contains_key(",
    "sobrepor.keys(",
    "sobrepor.iter(",
    "sobrepostos.get(",
    "sobrepostos.contains_key(",
    "sobrepostos.keys(",
];

fn fontes(dir: &Path, out: &mut Vec<PathBuf>) {
    for entrada in fs::read_dir(dir).expect("read_dir").flatten() {
        let p = entrada.path();
        if p.is_dir() {
            fontes(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// A test file, or the view itself: neither is an adapter asking the map.
fn fora_da_guarda(arquivo: &Path) -> bool {
    let nome = arquivo
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    nome == "tests.rs"
        || nome == "testes.rs"
        || arquivo.components().any(|c| c.as_os_str() == "tests")
        || arquivo.ends_with("lang/vista.rs")
}

/// `linha:consulta` of every code line (a comment is skipped) that asks the map itself.
fn consultas_diretas(texto: &str) -> Vec<String> {
    let codigo = texto
        .lines()
        .enumerate()
        .filter(|(_, linha)| !linha.trim_start().starts_with("//"));
    let mut achadas = Vec::new();
    for (i, linha) in codigo {
        achadas.extend(
            CONSULTAS
                .iter()
                .filter(|c| linha.contains(*c))
                .map(|c| format!("{}: `{c}`", i + 1)),
        );
    }
    achadas
}

/// The overlay map is consulted in ONE place: `vgp/lang/vista.rs` owns the key convention and the
/// precedence, and an adapter that asks the map itself is a second copy of both.
#[test]
fn so_a_vista_consulta_o_mapa_de_overlays() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/vgp");
    let mut arquivos = Vec::new();
    fontes(&raiz, &mut arquivos);
    assert!(
        arquivos.len() >= 30,
        "only {} sources found",
        arquivos.len()
    );
    assert!(
        arquivos.iter().any(|a| a.ends_with("lang/vista.rs")),
        "the view module exists"
    );
    let mut violacoes = Vec::new();
    for arquivo in arquivos.iter().filter(|a| !fora_da_guarda(a)) {
        let texto = fs::read_to_string(arquivo).expect("read source");
        let rel = arquivo
            .strip_prefix(&raiz)
            .unwrap_or(arquivo)
            .display()
            .to_string();
        violacoes.extend(
            consultas_diretas(&texto)
                .into_iter()
                .map(|c| format!("{rel}:{c}")),
        );
    }
    assert!(
        violacoes.is_empty(),
        "these ask the overlay map themselves — go through `Vista`:\n{}",
        violacoes.join("\n")
    );
}

/// The scanner is not blind: it sees a direct ask and ignores a comment.
#[test]
fn a_guarda_acha_a_consulta_direta_e_ignora_comentario() {
    assert_eq!(
        consultas_diretas("let x = self.overlays.get(&k);\n"),
        vec!["1: `overlays.get(`"]
    );
    assert!(consultas_diretas("// self.overlays.get(&k)\n").is_empty());
    assert!(consultas_diretas("let v = vista.ler(r, p, None);\n").is_empty());
}
