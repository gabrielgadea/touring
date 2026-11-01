//! TDD fixtures of the html adapter — one module per class, every rule of resolution with a
//! negative control (the thing exists, is dynamic, is out of scope → no finding, and the reason
//! when it could not be measured). Files live in a real temp directory so `recurso` is judged
//! against a real disk.

mod ancora;
mod recurso;
mod script;

use std::collections::HashMap;
use std::path::Path;

use tempfile::TempDir;

use super::{Adaptador, Html};
use crate::vgp::lang::Verificacao;
use crate::vgp::politica::Alvo;
use crate::vgp::{CheckInput, CheckOutput, check};

const INICIO: &str = "<!DOCTYPE html>\n<html>\n<head>\n<title>t</title>\n</head>\n<body>\n";
const FIM: &str = "\n</body>\n</html>\n";
const ABRE_SCRIPT: &str = "\n<script>\n";
const FECHA_SCRIPT: &str = "\n</script>";

/// A complete page whose body starts at line 7.
fn pagina(corpo: &str) -> String {
    [INICIO, corpo, FIM].concat()
}

/// A page with `corpo` on line 7 and an inline script whose first line is line 9.
fn com_script(js: &str, corpo: &str) -> String {
    pagina(&[corpo, ABRE_SCRIPT, js, FECHA_SCRIPT].concat())
}

/// A scratch project on a real disk.
struct Site {
    dir: TempDir,
}

impl Site {
    fn novo() -> Self {
        Self {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }

    fn raiz(&self) -> &Path {
        self.dir.path()
    }

    fn escreve(&self, rel: &str, conteudo: &str) {
        let p = self.dir.path().join(rel);
        std::fs::create_dir_all(p.parent().expect("pai")).expect("mkdir");
        std::fs::write(p, conteudo).expect("write");
    }

    fn caminho(&self, rel: &str) -> String {
        self.dir.path().join(rel).to_string_lossy().into_owned()
    }

    fn entrada(&self, rel: &str, html: &str) -> CheckInput {
        CheckInput {
            path: self.caminho(rel),
            content: html.to_string(),
            project_root: self.raiz().to_path_buf(),
            disk: None,
            overlays: HashMap::new(),
            lang: "html".into(),
        }
    }

    /// The adapter alone: findings with their `Alvo`, severity not decided yet.
    fn mede(&self, rel: &str, html: &str) -> Verificacao {
        Html.verificar(&self.entrada(rel, html))
    }
}

/// Sorted names of the findings of one class.
fn nomes(v: &Verificacao, classe: &str) -> Vec<String> {
    let mut n: Vec<String> = v
        .achados
        .iter()
        .filter(|f| f.classe == classe)
        .map(|f| f.nome.clone())
        .collect();
    n.sort();
    n
}

/// Nothing found and nothing left out of reach.
fn limpo(v: &Verificacao) {
    assert!(v.achados.is_empty(), "achados inesperados: {:?}", v.achados);
    assert_eq!(v.nao_medido, None, "nada deveria ficar fora do alcance");
}

/// The findings of `classe` are exactly `esperados`.
fn ve(v: &Verificacao, classe: &str, esperados: &[&str]) {
    assert_eq!(nomes(v, classe), esperados, "{:?}", v.achados);
}

/// The reason something was not measured, or "" when everything was.
fn motivo(v: &Verificacao) -> String {
    v.nao_medido.clone().unwrap_or_default()
}

/// `check` on a virtual project (no disk): the policy and the verdict, not the resolution.
fn julga(path: &str, html: &str) -> CheckOutput {
    check(&CheckInput {
        path: path.into(),
        content: html.into(),
        project_root: "/home/u/proj".into(),
        disk: None,
        overlays: HashMap::new(),
        lang: "html".into(),
    })
}

// ============================================================================ despacho

#[test]
fn html_deixa_de_responder_nao_implementado() {
    let out = julga("/home/u/proj/site/a.html", &pagina("<p>oi</p>"));
    assert_eq!(out.verdict, "allow");
    assert_eq!(out.motivo, None);
    assert_eq!(out.lang, "html");
}

#[test]
fn documento_vazio_e_limpo() {
    let s = Site::novo();
    limpo(&s.mede("a.html", ""));
}

// ============================================================================ invariantes

#[test]
fn erro_de_sintaxe_do_html_e_dito_e_nao_vira_allow_calado() {
    let s = Site::novo();
    let v = s.mede("a.html", "<html><body><div class=\"a\" <<>> =\"x\"></body>");
    assert!(
        motivo(&v).contains("erro de sintaxe"),
        "um documento que o parser não leu inteiro não pode ser `allow` limpo: {v:?}"
    );
}

#[test]
fn tag_nao_fechada_tambem_e_dita() {
    let s = Site::novo();
    let v = s.mede("a.html", "<html><body><img src=\"a.png\"");
    assert!(motivo(&v).contains("erro de sintaxe"), "{v:?}");
}

#[test]
fn maior_que_solto_no_texto_nao_e_erro_de_sintaxe() {
    // `>` sem `<` no texto é HTML válido; a gramática o marca como ERROR, mas nenhuma tag se perde
    let s = Site::novo();
    limpo(&s.mede("a.html", &pagina("<p>a -> b, x >= 3 e 5 > 2 =></p>")));
}

#[test]
fn documento_grande_nao_tem_custo_quadratico() {
    let s = Site::novo();
    let mut corpo = String::new();
    for i in 0..30_000 {
        corpo.push_str(&format!(
            "<p id=\"id{i}\"><a href=\"#id{}\">x</a></p>\n",
            i + 1
        ));
    }
    for i in 0..200 {
        corpo.push_str(&format!(
            "<a href=\"#falta{i}\">f</a><img src=\"nada/{i}.png\">\n"
        ));
    }
    let t0 = std::time::Instant::now();
    let v = s.mede("a.html", &pagina(&corpo));
    let dt = t0.elapsed();
    assert_eq!(
        nomes(&v, "ancora").len(),
        201,
        "200 ausentes + o último link, para id30000"
    );
    assert_eq!(nomes(&v, "recurso").len(), 200);
    assert!(dt.as_secs() < 20, "{dt:?}");
}

#[test]
fn saida_e_deterministica_entre_chamadas() {
    let s = Site::novo();
    let mut corpo = String::new();
    for i in 0..3_000 {
        corpo.push_str(&format!("<p id=\"secao{i}\"></p>"));
    }
    corpo.push_str("<a href=\"#secao9999\">x</a><a href=\"#sec\">y</a>");
    let html = pagina(&corpo);
    let a = s.mede("a.html", &html);
    let b = s.mede("a.html", &html);
    let ver = |v: &Verificacao| {
        v.achados
            .iter()
            .map(|f| (f.nome.clone(), f.sugestao.clone(), f.reais.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(ver(&a), ver(&b), "mesma entrada, mesma saída");
}

#[test]
fn documento_acima_do_teto_nao_e_lido_e_diz_por_que() {
    let s = Site::novo();
    let grande = pagina(&"<p>x</p>".repeat(700_000));
    assert!(grande.len() > super::TETO_DO_DOCUMENTO);
    let v = s.mede("a.html", &format!("{grande}<a href=\"#falta\">f</a>"));
    assert!(v.achados.is_empty());
    assert!(motivo(&v).contains("acima do limite"), "{:?}", v.nao_medido);
}

#[test]
fn script_acima_do_teto_nao_e_relido_e_avisa_na_ancora() {
    let s = Site::novo();
    let js = "var x = 1;\n".repeat(60_000);
    assert!(js.len() > super::script::TETO_DO_SCRIPT);
    let v = s.mede("a.html", &com_script(&js, "<a href=\"#falta\">f</a>"));
    assert!(motivo(&v).contains("script inline"), "{:?}", v.nao_medido);
    ve(&v, "ancora", &["falta"]);
    assert_eq!(
        v.achados[0].alvo,
        Alvo::Incerto,
        "um script não lido pode criar o id"
    );
    assert!(
        v.achados[0].sugestao.contains("script"),
        "um script não lido pode criar o id: {}",
        v.achados[0].sugestao
    );
}
