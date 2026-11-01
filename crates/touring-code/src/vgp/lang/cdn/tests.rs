//! Tests of the `cdn` / `cdn_pin` transversal check (VCDN). The pure core is exercised with an
//! in-memory [`Cache`] and a fixed clock; the policy end of it through [`crate::vgp::julgar`].

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use super::cache::{Cache, Causa, Registro, Veredito};
use super::*;
use crate::vgp::CheckInput;
use crate::vgp::lang::{Adaptador, Verificacao};
use crate::vgp::politica::Alvo;

const REACT_404: &str = "https://cdn.jsdelivr.net/npm/react@18.3.99/umd/react.production.min.js";
const REACT_OK: &str = "https://cdn.jsdelivr.net/npm/react@18.3.1/umd/react.production.min.js";
const BOOT_PIN: &str = "https://cdn.jsdelivr.net/npm/bootstrap@4.3/dist/css/bootstrap.min.css";
const BOOT_OK: &str = "https://cdn.jsdelivr.net/npm/bootstrap@4.3.1/dist/css/bootstrap.min.css";
const BOOT_404: &str = "https://cdn.jsdelivr.net/npm/bootstrap@4.3.99/dist/css/bootstrap.min.css";

fn agora() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000)
}

fn entrada(lang: &str, content: &str) -> CheckInput {
    CheckInput {
        path: "/home/u/proj/src/pagina".into(),
        content: content.into(),
        project_root: "/home/u/proj".into(),
        disk: None,
        overlays: HashMap::new(),
        lang: lang.into(),
    }
}

/// An html page whose `<head>` holds `corpo` — `corpo` starts at line 4.
fn pagina(corpo: &str) -> String {
    format!("<!DOCTYPE html>\n<html>\n<head>\n{corpo}\n</head>\n<body></body>\n</html>\n")
}

fn existe() -> Registro {
    Registro {
        status: Some(200),
        ..Registro::novo(Veredito::Existe)
    }
}

fn nao_existe() -> Registro {
    Registro {
        status: Some(404),
        ..Registro::novo(Veredito::NaoExiste)
    }
}

fn cache_com(itens: &[(&str, Registro)]) -> Cache {
    let mut c = Cache::vazio_de(Some("/home/u/.claude/touring/vgp/cdn_cache.json".into()));
    for (url, r) in itens {
        c.inserir(url, r.clone());
    }
    c
}

fn medir(lang: &str, content: &str, cache: &Cache) -> Verificacao {
    Cdn.medir_com(&entrada(lang, content), cache, agora())
}

/// (classe, nome, linha) of every finding, in order.
fn achados(v: &Verificacao) -> Vec<(String, String, usize)> {
    v.achados
        .iter()
        .map(|f| (f.classe.clone(), f.nome.clone(), f.linha))
        .collect()
}

fn tupla(classe: &str, nome: &str, linha: usize) -> (String, String, usize) {
    (classe.into(), nome.into(), linha)
}

// ------------------------------------------------------------------ UrlCdn::parse

#[test]
fn parse_jsdelivr_npm_separa_pacote_versao_e_arquivo() {
    let u = UrlCdn::parse(BOOT_PIN).unwrap();
    assert_eq!(u.servidor, Servidor::Jsdelivr);
    assert_eq!(u.origem, Origem::Npm);
    assert_eq!(u.pacote, "bootstrap");
    assert_eq!(u.versao.as_deref(), Some("4.3"));
    assert_eq!(u.arquivo, "dist/css/bootstrap.min.css");
}

#[test]
fn parse_pacote_com_escopo_e_sem_versao() {
    let u = UrlCdn::parse(
        "https://cdn.jsdelivr.net/npm/@fortawesome/fontawesome-free@6.2.0/css/all.min.css",
    )
    .unwrap();
    assert_eq!(u.pacote, "@fortawesome/fontawesome-free");
    assert_eq!(u.versao.as_deref(), Some("6.2.0"));
    assert_eq!(u.arquivo, "css/all.min.css");

    let sem = UrlCdn::parse("https://cdn.jsdelivr.net/npm/jquery/dist/jquery.min.js").unwrap();
    assert_eq!(sem.pacote, "jquery");
    assert_eq!(sem.versao, None);
    assert_eq!(sem.arquivo, "dist/jquery.min.js");

    let escopo_sem = UrlCdn::parse("https://cdn.jsdelivr.net/npm/@scope/pkg/dist/x.js").unwrap();
    assert_eq!(escopo_sem.pacote, "@scope/pkg");
    assert_eq!(escopo_sem.versao, None);
    assert_eq!(escopo_sem.arquivo, "dist/x.js");
}

#[test]
fn parse_jsdelivr_gh_unpkg_e_cdnjs() {
    let gh = UrlCdn::parse("https://cdn.jsdelivr.net/gh/user/repo@v1.2.3/dist/x.js").unwrap();
    assert_eq!((gh.servidor, gh.origem), (Servidor::Jsdelivr, Origem::Gh));
    assert_eq!(gh.pacote, "user/repo");
    assert_eq!(gh.versao.as_deref(), Some("v1.2.3"));
    assert_eq!(gh.arquivo, "dist/x.js");

    let un = UrlCdn::parse("https://unpkg.com/maplibre-gl@4.7.1/dist/maplibre-gl.css").unwrap();
    assert_eq!((un.servidor, un.origem), (Servidor::Unpkg, Origem::Npm));
    assert_eq!(un.pacote, "maplibre-gl");
    assert_eq!(un.versao.as_deref(), Some("4.7.1"));

    let solto = UrlCdn::parse("https://unpkg.com/piii").unwrap();
    assert_eq!(
        (solto.pacote.as_str(), solto.versao.clone()),
        ("piii", None)
    );
    assert_eq!(solto.arquivo, "");

    let js = UrlCdn::parse(
        "https://cdnjs.cloudflare.com/ajax/libs/bootstrap-select/1.13.1/css/bootstrap-select.css",
    )
    .unwrap();
    assert_eq!(js.servidor, Servidor::Cdnjs);
    assert_eq!(js.pacote, "bootstrap-select");
    assert_eq!(js.versao.as_deref(), Some("1.13.1"));
    assert_eq!(js.arquivo, "css/bootstrap-select.css");
}

#[test]
fn parse_aceita_esquema_relativo_http_e_ignora_query_e_fragmento() {
    for url in [
        "//cdn.jsdelivr.net/npm/a@1.0.0/dist/x.js",
        "http://cdn.jsdelivr.net/npm/a@1.0.0/dist/x.js",
        "https://CDN.jsdelivr.net/npm/a@1.0.0/dist/x.js?v=3#frag",
    ] {
        let u = UrlCdn::parse(url).unwrap_or_else(|| panic!("{url}"));
        assert_eq!(u.pacote, "a", "{url}");
        assert_eq!(u.versao.as_deref(), Some("1.0.0"), "{url}");
        assert_eq!(u.arquivo, "dist/x.js", "{url}");
    }
}

#[test]
fn parse_recusa_o_que_nao_e_um_pacote_de_cdn() {
    for url in [
        "https://fonts.googleapis.com/css?family=Roboto",
        "https://cdn.example.com/npm/a@1.0.0/x.js",
        "https://cdn.jsdelivr.net/",
        "https://cdn.jsdelivr.net",
        "https://cdn.jsdelivr.net/combine/npm/a,npm/b",
        "https://cdn.jsdelivr.net/npm/",
        "https://unpkg.com/",
        "https://unpkg.com/browse/react@18/",
        "https://cdnjs.cloudflare.com/ajax/libs/jquery",
        "https://cdnjs.cloudflare.com/polyfill/v3/polyfill.min.js",
        "/local/x.js",
        "x.js",
        "",
    ] {
        assert!(UrlCdn::parse(url).is_none(), "{url:?}");
    }
}

// ------------------------------------------------------------------ versao_exata

fn exata(url: &str) -> bool {
    UrlCdn::parse(url).unwrap().versao_exata()
}

#[test]
fn versao_exata_npm_e_semver_completo() {
    for v in [
        "4.3.1",
        "0.0.1",
        "1.0.0-rc.1",
        "2.0.0+build.5",
        "1.0.0-beta+exp.sha.5114f85",
    ] {
        assert!(
            exata(&format!("https://cdn.jsdelivr.net/npm/a@{v}/x.js")),
            "{v} é exata"
        );
    }
}

#[test]
fn versao_nao_exata_e_range_tag_ou_ausente() {
    for v in [
        "4", "4.3", "^4.3.0", "~4.3", "4.x", "*", "latest", "next", "beta", ">=1", "1.2.3.4",
        "1.2.", "1.a.3",
    ] {
        assert!(
            !exata(&format!("https://cdn.jsdelivr.net/npm/a@{v}/x.js")),
            "{v} não é exata"
        );
    }
    assert!(!exata("https://cdn.jsdelivr.net/npm/a/x.js"), "sem versão");
    assert!(!exata("https://unpkg.com/a"), "unpkg sem versão");
    assert!(!exata("https://unpkg.com/a@4/x.js"), "unpkg range");
    assert!(exata("https://unpkg.com/a@4.7.1/x.js"));
}

#[test]
fn versao_exata_gh_e_tag_ou_commit_nunca_branch() {
    for v in [
        "v1.2.3",
        "1.2.3",
        "a1b2c3d",
        "0123456789abcdef0123456789abcdef01234567",
    ] {
        assert!(
            exata(&format!("https://cdn.jsdelivr.net/gh/u/r@{v}/x.js")),
            "{v}"
        );
    }
    for v in ["main", "master", "dev", "v1", "1.2", "abc"] {
        assert!(
            !exata(&format!("https://cdn.jsdelivr.net/gh/u/r@{v}/x.js")),
            "{v}"
        );
    }
    assert!(!exata("https://cdn.jsdelivr.net/gh/u/r/x.js"), "gh sem tag");
}

#[test]
fn cdnjs_leva_a_versao_no_caminho_e_nunca_e_pin() {
    assert!(exata(
        "https://cdnjs.cloudflare.com/ajax/libs/animate.css/4.1.1/animate.min.css"
    ));
}

#[test]
fn com_versao_remonta_a_url_com_a_versao_exata() {
    let casos = [
        (BOOT_PIN, "4.3.1", BOOT_OK),
        (
            "https://cdn.jsdelivr.net/npm/jquery/dist/jquery.min.js",
            "3.7.1",
            "https://cdn.jsdelivr.net/npm/jquery@3.7.1/dist/jquery.min.js",
        ),
        (
            "https://cdn.jsdelivr.net/npm/@scope/pkg@1/dist/x.js",
            "1.4.0",
            "https://cdn.jsdelivr.net/npm/@scope/pkg@1.4.0/dist/x.js",
        ),
        (
            "https://cdn.jsdelivr.net/gh/u/r@main/dist/x.js",
            "v2.0.0",
            "https://cdn.jsdelivr.net/gh/u/r@v2.0.0/dist/x.js",
        ),
        (
            "https://unpkg.com/piii",
            "1.0.0",
            "https://unpkg.com/piii@1.0.0",
        ),
        (
            "//unpkg.com/a@4/x.js?q=1",
            "4.7.1",
            "https://unpkg.com/a@4.7.1/x.js",
        ),
    ];
    for (url, v, esperado) in casos {
        assert_eq!(UrlCdn::parse(url).unwrap().com_versao(v), esperado, "{url}");
    }
}

// ------------------------------------------------------------------ cdn: 404

#[test]
fn cdn_404_confirmado_no_cache_e_achado_externo_na_linha_certa() {
    let html = pagina(&format!(r#"<script src="{REACT_404}"></script>"#));
    let v = medir("html", &html, &cache_com(&[(REACT_404, nao_existe())]));
    assert_eq!(achados(&v), vec![tupla("cdn", REACT_404, 4)]);
    assert_eq!(v.achados[0].alvo, Alvo::Externo);
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
}

#[test]
fn cdn_404_pela_politica_nega_em_arquivo_comum() {
    struct CdnFixo(Cache);
    impl Transversal for CdnFixo {
        fn nome(&self) -> &'static str {
            "cdn"
        }
        fn langs(&self) -> &'static [&'static str] {
            &["html"]
        }
        fn verificar(&self, input: &CheckInput) -> Verificacao {
            Cdn.medir_com(input, &self.0, agora())
        }
    }
    struct Limpo;
    impl Adaptador for Limpo {
        fn lang(&self) -> &'static str {
            "html"
        }
        fn verificar(&self, _: &CheckInput) -> Verificacao {
            Verificacao::medido(Vec::new())
        }
    }
    let html = pagina(&format!(r#"<script src="{REACT_404}"></script>"#));
    let mut input = entrada("html", &html);
    let cdn = CdnFixo(cache_com(&[(REACT_404, nao_existe())]));
    let (veredito, fs, _) =
        crate::vgp::julgar(&input, &Limpo, &[&cdn], crate::vgp::Contexto::Escrita);
    assert_eq!(
        veredito, "deny",
        "404 confirmado nega (§5.1), mesmo em src/"
    );
    assert_eq!(fs[0].severidade, "deny");

    // delta: o disco já tinha a mesma URL quebrada → pré-existente, não é a edição que a introduz
    input.disk = Some(html.clone());
    let (veredito, fs, _) =
        crate::vgp::julgar(&input, &Limpo, &[&cdn], crate::vgp::Contexto::Escrita);
    assert_eq!(veredito, "warn");
    assert!(
        fs[0].sugestao.starts_with("pré-existente"),
        "{}",
        fs[0].sugestao
    );
}

#[test]
fn cdn_existente_no_cache_nao_acha_nada_e_mede() {
    let html = pagina(&format!(r#"<script src="{REACT_OK}"></script>"#));
    let v = medir("html", &html, &cache_com(&[(REACT_OK, existe())]));
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.is_none());
}

#[test]
fn cdn_404_ensina_as_versoes_que_o_cache_sabe_que_existem() {
    let html = pagina(&format!(r#"<link rel="stylesheet" href="{BOOT_404}">"#));
    let cache = cache_com(&[(BOOT_404, nao_existe()), (BOOT_OK, existe())]);
    let v = medir("html", &html, &cache);
    assert_eq!(v.achados.len(), 1);
    assert_eq!(v.achados[0].reais, vec!["4.3.1".to_string()]);
    assert!(
        v.achados[0].sugestao.contains("4.3.1"),
        "{}",
        v.achados[0].sugestao
    );
    assert!(
        v.achados[0].sugestao.contains("404"),
        "{}",
        v.achados[0].sugestao
    );
}

#[test]
fn cdn_404_diz_se_faltou_a_versao_ou_o_arquivo_quando_o_cache_sabe() {
    let html = pagina(&format!(r#"<script src="{REACT_404}"></script>"#));
    let versao = Registro {
        causa: Some(Causa::VersaoInexistente),
        ..nao_existe()
    };
    let v = medir("html", &html, &cache_com(&[(REACT_404, versao)]));
    assert!(
        v.achados[0].sugestao.contains("versão `18.3.99`"),
        "{}",
        v.achados[0].sugestao
    );

    let arquivo = Registro {
        causa: Some(Causa::ArquivoInexistente),
        ..nao_existe()
    };
    let v = medir("html", &html, &cache_com(&[(REACT_404, arquivo)]));
    assert!(
        v.achados[0]
            .sugestao
            .contains("umd/react.production.min.js"),
        "{}",
        v.achados[0].sugestao
    );
}

#[test]
fn url_repetida_gera_um_achado_na_primeira_linha() {
    let html = pagina(&format!(
        "<script src=\"{REACT_404}\"></script>\n<script src=\"{REACT_404}\"></script>"
    ));
    let v = medir("html", &html, &cache_com(&[(REACT_404, nao_existe())]));
    assert_eq!(achados(&v), vec![tupla("cdn", REACT_404, 4)]);
}

#[test]
fn esquema_relativo_e_achado_pelo_nome_como_foi_escrito() {
    let rel = "//cdn.jsdelivr.net/npm/react@18.3.99/umd/react.production.min.js";
    let html = pagina(&format!(r#"<script src="{rel}"></script>"#));
    // o cache guarda a forma canônica (https); a busca normaliza
    let v = medir("html", &html, &cache_com(&[(REACT_404, nao_existe())]));
    assert_eq!(achados(&v), vec![tupla("cdn", rel, 4)]);
}

#[test]
fn cdn_404_vencido_nao_nega_pede_reconfirmacao() {
    let velho = Registro {
        verificado_em: Some(1_000_000_000),
        ..nao_existe()
    };
    let html = pagina(&format!(r#"<script src="{REACT_404}"></script>"#));
    let v = medir("html", &html, &cache_com(&[(REACT_404, velho)]));
    assert!(v.achados.is_empty(), "um 404 vencido não pode negar");
    let motivo = v.nao_medido.expect("vencido = não medido");
    assert!(
        motivo.contains("24 h") && motivo.contains(REACT_404),
        "{motivo}"
    );
    assert!(motivo.contains("resolvedor"), "{motivo}");
}

// ------------------------------------------------------------------ cache: o que falta, falta dito

#[test]
fn url_fora_do_cache_e_nao_medido_e_o_motivo_nomeia_o_resolvedor() {
    let html = pagina(&format!(r#"<script src="{REACT_OK}"></script>"#));
    let v = medir("html", &html, &cache_com(&[]));
    assert!(v.achados.is_empty());
    let motivo = v.nao_medido.expect("fora do cache = não medido");
    assert!(motivo.contains(REACT_OK), "{motivo}");
    assert!(
        motivo.contains("cdn_cache.json"),
        "o motivo diz QUAL cache: {motivo}"
    );
    assert!(
        motivo.contains("vgp::lang::cdn::resolvedor::preencher"),
        "o motivo nomeia o resolvedor: {motivo}"
    );
}

#[test]
fn varias_urls_fora_do_cache_viram_um_motivo_com_a_contagem() {
    let html = pagina(&format!(
        "<script src=\"{REACT_OK}\"></script>\n<link href=\"{BOOT_OK}\" rel=\"stylesheet\">"
    ));
    let v = medir("html", &html, &cache_com(&[]));
    let motivo = v.nao_medido.unwrap();
    assert!(motivo.contains("2 URL"), "{motivo}");
}

#[test]
fn veredito_indeterminado_ou_http_erro_no_cache_e_nao_medido_nunca_404() {
    for registro in [
        Registro {
            erro: Some("TimeoutError".into()),
            ..Registro::novo(Veredito::Indeterminado)
        },
        Registro {
            status: Some(503),
            ..Registro::novo(Veredito::HttpErro)
        },
    ] {
        let html = pagina(&format!(r#"<script src="{REACT_404}"></script>"#));
        let v = medir("html", &html, &cache_com(&[(REACT_404, registro)]));
        assert!(v.achados.is_empty());
        let motivo = v.nao_medido.expect("sem veredito = não medido");
        assert!(motivo.contains(REACT_404), "{motivo}");
    }
}

#[test]
fn achado_e_nao_medido_convivem_o_motivo_viaja_ao_lado() {
    let html = pagina(&format!(
        "<script src=\"{REACT_404}\"></script>\n<script src=\"{REACT_OK}\"></script>"
    ));
    let v = medir("html", &html, &cache_com(&[(REACT_404, nao_existe())]));
    assert_eq!(achados(&v), vec![tupla("cdn", REACT_404, 4)]);
    assert!(v.nao_medido.unwrap().contains(REACT_OK));
}

#[test]
fn sem_url_de_cdn_nao_ha_o_que_medir_nem_motivo() {
    let html = pagina(
        r#"<script src="app.js"></script><link href="https://fonts.googleapis.com/css?family=Roboto">"#,
    );
    let v = medir("html", &html, &cache_com(&[]));
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.is_none());
}

// ------------------------------------------------------------------ cdn_pin

#[test]
fn cdn_pin_range_traz_a_versao_exata_que_o_cache_resolveu() {
    let html = pagina(&format!(r#"<link rel="stylesheet" href="{BOOT_PIN}">"#));
    let registro = Registro {
        resolvida: Some("4.3.1".into()),
        ..existe()
    };
    let v = medir("html", &html, &cache_com(&[(BOOT_PIN, registro)]));
    assert_eq!(achados(&v), vec![tupla("cdn_pin", BOOT_PIN, 4)]);
    assert!(v.nao_medido.is_none());
    let f = &v.achados[0];
    assert!(f.sugestao.contains("4.3.1"), "{}", f.sugestao);
    assert!(
        f.sugestao.contains(BOOT_OK),
        "a URL já fixada: {}",
        f.sugestao
    );
    assert_eq!(f.reais, vec!["4.3.1".to_string()]);
}

#[test]
fn cdn_pin_vale_mesmo_fora_do_cache_e_diz_que_nao_resolveu() {
    let html = pagina(&format!(r#"<link rel="stylesheet" href="{BOOT_PIN}">"#));
    let v = medir("html", &html, &cache_com(&[]));
    assert_eq!(achados(&v), vec![tupla("cdn_pin", BOOT_PIN, 4)]);
    assert!(v.nao_medido.unwrap().contains("resolvedor"));
}

#[test]
fn cdn_pin_cobre_latest_tag_sem_versao_e_range_com_circunflexo() {
    let urls = [
        "https://cdn.jsdelivr.net/npm/vue@latest/dist/vue.global.js",
        "https://cdn.jsdelivr.net/npm/jquery/dist/jquery.min.js",
        "https://cdn.jsdelivr.net/npm/lodash@^4.17.0/lodash.js",
        "https://cdn.jsdelivr.net/npm/@scope/pkg@beta/x.js",
        "https://unpkg.com/preact/dist/preact.min.js",
        "https://cdn.jsdelivr.net/gh/user/repo@main/dist/x.js",
        "https://cdn.jsdelivr.net/gh/user/repo/dist/x.js",
    ];
    for url in urls {
        let html = pagina(&format!(r#"<script src="{url}"></script>"#));
        let cache = cache_com(&[(url, existe())]);
        let v = medir("html", &html, &cache);
        assert_eq!(achados(&v), vec![tupla("cdn_pin", url, 4)], "{url}");
    }
}

#[test]
fn versao_exata_nao_e_pin() {
    for url in [
        BOOT_OK,
        "https://cdn.jsdelivr.net/npm/a@1.0.0-rc.1/x.js",
        "https://cdnjs.cloudflare.com/ajax/libs/animate.css/4.1.1/animate.min.css",
        "https://cdn.jsdelivr.net/gh/user/repo@v1.2.3/x.js",
        "https://unpkg.com/maplibre-gl@4.7.1/dist/maplibre-gl.js",
    ] {
        let html = pagina(&format!(r#"<script src="{url}"></script>"#));
        let v = medir("html", &html, &cache_com(&[(url, existe())]));
        assert!(v.achados.is_empty(), "{url}: {:?}", achados(&v));
    }
}

#[test]
fn pin_que_da_404_e_cdn_e_nao_cdn_pin() {
    let pin_morto = "https://cdn.jsdelivr.net/npm/bootstrap@99/dist/css/bootstrap.min.css";
    let html = pagina(&format!(r#"<link rel="stylesheet" href="{pin_morto}">"#));
    let v = medir("html", &html, &cache_com(&[(pin_morto, nao_existe())]));
    assert_eq!(achados(&v), vec![tupla("cdn", pin_morto, 4)]);
}

#[test]
fn cdn_pin_e_consultivo_pela_politica_mesmo_com_alvo_externo() {
    let html = pagina(&format!(r#"<link rel="stylesheet" href="{BOOT_PIN}">"#));
    let v = medir("html", &html, &cache_com(&[(BOOT_PIN, existe())]));
    assert_eq!(v.achados[0].alvo, Alvo::Externo);
    let sev = crate::vgp::politica::severidade(&v.achados[0].classe, v.achados[0].alvo, true);
    assert_eq!(sev, "warn");
}

// ------------------------------------------------------------------ integrity

fn tag_com_integrity(url: &str, integrity: &str) -> String {
    let mut tag = String::from("<script src=\"");
    tag.push_str(url);
    tag.push_str("\" integrity=\"");
    tag.push_str(integrity);
    tag.push_str("\" crossorigin=\"anonymous\"></script>");
    pagina(&tag)
}

fn com_digests(url: &str, digests: &[&str]) -> Cache {
    cache_com(&[(
        url,
        Registro {
            integrity: digests.iter().map(|d| d.to_string()).collect(),
            ..existe()
        },
    )])
}

#[test]
fn integrity_que_bate_com_o_cache_nao_acha_nada() {
    let html = tag_com_integrity(REACT_OK, "sha384-AAAA");
    let v = medir(
        "html",
        &html,
        &com_digests(REACT_OK, &["sha256-XXXX", "sha384-AAAA"]),
    );
    assert!(v.achados.is_empty(), "{:?}", achados(&v));
    assert!(v.nao_medido.is_none());
}

#[test]
fn integrity_que_nao_bate_em_versao_exata_e_cdn_externo() {
    let html = tag_com_integrity(REACT_OK, "sha384-ERRADO");
    let v = medir("html", &html, &com_digests(REACT_OK, &["sha384-AAAA"]));
    assert_eq!(achados(&v), vec![tupla("cdn", REACT_OK, 4)]);
    assert_eq!(v.achados[0].alvo, Alvo::Externo, "versão exata é imutável");
    assert!(
        v.achados[0].sugestao.contains("integrity"),
        "{}",
        v.achados[0].sugestao
    );
    assert!(
        v.achados[0].sugestao.contains("sha384-AAAA"),
        "ensina o digest certo"
    );
}

#[test]
fn integrity_que_nao_bate_em_versao_movel_e_incerto() {
    let html = tag_com_integrity(BOOT_PIN, "sha384-ERRADO");
    let registro = Registro {
        integrity: vec!["sha384-AAAA".into()],
        resolvida: Some("4.3.1".into()),
        ..existe()
    };
    let v = medir("html", &html, &cache_com(&[(BOOT_PIN, registro)]));
    let cdn: Vec<_> = v.achados.iter().filter(|f| f.classe == "cdn").collect();
    assert_eq!(cdn.len(), 1);
    assert_eq!(
        cdn[0].alvo,
        Alvo::Incerto,
        "o conteúdo de um pin móvel muda"
    );
}

#[test]
fn integrity_usa_so_o_algoritmo_mais_forte_como_o_navegador() {
    // o sha256 do tag está errado mas o sha384 (mais forte) bate → o navegador aceita
    let html = tag_com_integrity(REACT_OK, "sha256-ERRADO sha384-AAAA");
    let cache = com_digests(REACT_OK, &["sha256-CERTO", "sha384-AAAA"]);
    assert!(medir("html", &html, &cache).achados.is_empty());
    // o sha384 do tag está errado e o sha256 bate → o navegador rejeita (sha256 é ignorado)
    let html = tag_com_integrity(REACT_OK, "sha256-CERTO sha384-ERRADO");
    let v = medir("html", &html, &cache);
    assert_eq!(achados(&v), vec![tupla("cdn", REACT_OK, 4)]);
}

#[test]
fn integrity_com_varios_hashes_do_mesmo_algoritmo_aceita_qualquer_um() {
    let html = tag_com_integrity(REACT_OK, "sha384-OUTRO sha384-AAAA");
    let v = medir("html", &html, &com_digests(REACT_OK, &["sha384-AAAA"]));
    assert!(v.achados.is_empty());
}

#[test]
fn integrity_base64url_e_sem_padding_equivalem_ao_base64() {
    let html = tag_com_integrity(REACT_OK, "sha256-ab_c-d");
    let v = medir("html", &html, &com_digests(REACT_OK, &["sha256-ab/c+d="]));
    assert!(v.achados.is_empty(), "{:?}", achados(&v));
}

#[test]
fn integrity_sem_o_algoritmo_no_cache_e_nao_medido_nunca_achado() {
    let html = tag_com_integrity(REACT_OK, "sha512-ZZZZ");
    let v = medir("html", &html, &com_digests(REACT_OK, &["sha256-XXXX"]));
    assert!(v.achados.is_empty());
    let motivo = v.nao_medido.unwrap();
    assert!(
        motivo.contains("sha512") && motivo.contains(REACT_OK),
        "{motivo}"
    );
}

#[test]
fn integrity_sem_digests_no_cache_ou_com_algoritmo_desconhecido_nao_acha_nada() {
    let html = tag_com_integrity(REACT_OK, "sha384-AAAA");
    let sem = cache_com(&[(REACT_OK, existe())]);
    assert!(medir("html", &html, &sem).achados.is_empty());
    // md5 não é algoritmo de SRI: o navegador ignora o token, logo não há o que comparar
    let html = tag_com_integrity(REACT_OK, "md5-AAAA");
    assert!(
        medir("html", &html, &com_digests(REACT_OK, &["sha384-AAAA"]))
            .achados
            .is_empty()
    );
}

#[test]
fn integrity_confere_tabela() {
    let c = |tag: &str, cache: &[&str]| {
        let cache: Vec<String> = cache.iter().map(|s| s.to_string()).collect();
        integrity_confere(tag, &cache)
    };
    assert_eq!(c("sha384-A", &["sha384-A"]), Some(true));
    assert_eq!(c("sha384-A", &["sha384-B"]), Some(false));
    assert_eq!(c("sha384-A", &["sha256-A"]), None, "sem o algoritmo");
    assert_eq!(c("", &["sha384-A"]), None, "sem tokens");
    assert_eq!(
        c("sha384-A?opt=1", &["sha384-A"]),
        Some(true),
        "?opções são ignoradas"
    );
    assert_eq!(
        c("sha256-A sha512-B", &["sha256-A", "sha512-C"]),
        Some(false)
    );
}

// ------------------------------------------------------------------ onde as URLs moram

#[test]
fn link_e_script_e_qualquer_src_ou_href_valem_comentario_e_texto_nao() {
    let html = pagina(&format!(
        "<link rel=\"stylesheet\" href=\"{BOOT_404}\">\n\
         <script src=\"{REACT_404}\"></script>\n\
         <!-- <script src=\"https://cdn.jsdelivr.net/npm/morto@9.9.9/x.js\"></script> -->\n\
         <a href=\"https://unpkg.com/lib@1.0.99/x.js\">baixar</a>\n\
         <p>veja https://cdn.jsdelivr.net/npm/texto@9.9.9/x.js no texto</p>"
    ));
    let cache = cache_com(&[
        (BOOT_404, nao_existe()),
        (REACT_404, nao_existe()),
        ("https://unpkg.com/lib@1.0.99/x.js", nao_existe()),
        (
            "https://cdn.jsdelivr.net/npm/morto@9.9.9/x.js",
            nao_existe(),
        ),
        (
            "https://cdn.jsdelivr.net/npm/texto@9.9.9/x.js",
            nao_existe(),
        ),
    ]);
    let v = medir("html", &html, &cache);
    assert_eq!(
        achados(&v),
        vec![
            tupla("cdn", BOOT_404, 4),
            tupla("cdn", REACT_404, 5),
            tupla("cdn", "https://unpkg.com/lib@1.0.99/x.js", 7),
        ]
    );
}

#[test]
fn atributos_e_tags_em_caixa_alta_ou_com_aspas_simples_tambem_valem() {
    let html = pagina(&format!("<SCRIPT SRC='{REACT_404}'></SCRIPT>"));
    let v = medir("html", &html, &cache_com(&[(REACT_404, nao_existe())]));
    assert_eq!(achados(&v), vec![tupla("cdn", REACT_404, 4)]);
}

#[test]
fn script_inline_com_import_de_url_e_style_com_import_e_url_valem() {
    let lodash = "https://cdn.jsdelivr.net/npm/lodash-es@4.17.99/lodash.js";
    let fonte = "https://cdnjs.cloudflare.com/ajax/libs/animate.css/4.1.99/animate.min.css";
    let html = pagina(&format!(
        "<script type=\"module\">\nimport _ from \"{lodash}\";\n</script>\n\
         <style>\n@import url('{fonte}');\n</style>"
    ));
    let cache = cache_com(&[(lodash, nao_existe()), (fonte, nao_existe())]);
    let v = medir("html", &html, &cache);
    assert_eq!(
        achados(&v),
        vec![tupla("cdn", lodash, 5), tupla("cdn", fonte, 8)]
    );
}

#[test]
fn importmap_aponta_urls_de_cdn_e_script_de_dados_nao_e_js() {
    let lodash = "https://cdn.jsdelivr.net/npm/lodash-es@4.17.99/+esm";
    let html = pagina(&format!(
        "<script type=\"importmap\">\n{{\"imports\": {{\"lodash\": \"{lodash}\"}}}}\n</script>\n\
         <script type=\"application/ld+json\">{{\"url\": \"https://unpkg.com/x@9.9.9/a.js\"}}</script>"
    ));
    let cache = cache_com(&[
        (lodash, nao_existe()),
        ("https://unpkg.com/x@9.9.9/a.js", nao_existe()),
    ]);
    let v = medir("html", &html, &cache);
    assert_eq!(achados(&v), vec![tupla("cdn", lodash, 5)]);
}

#[test]
fn js_import_estatico_dinamico_reexport_e_atribuicao_de_src() {
    let u = |p: &str| format!("https://cdn.jsdelivr.net/npm/{p}@1.0.99/dist/x.js");
    let fonte = format!(
        "import a from \"{}\";\n\
         export * from '{}';\n\
         const m = await import(\"{}\");\n\
         const s = document.createElement('script');\n\
         s.src = \"{}\";\n\
         import b from \"react\";\n\
         import c from \"https://esm.sh/c\";\n\
         const solto = \"{}\";\n",
        u("a"),
        u("b"),
        u("c"),
        u("d"),
        u("nao_e_import")
    );
    let cache = cache_com(&[
        (&u("a"), nao_existe()),
        (&u("b"), nao_existe()),
        (&u("c"), nao_existe()),
        (&u("d"), nao_existe()),
        (&u("nao_e_import"), nao_existe()),
    ]);
    for lang in ["javascript", "typescript", "tsx"] {
        let v = medir(lang, &fonte, &cache);
        assert_eq!(
            achados(&v),
            vec![
                tupla("cdn", &u("a"), 1),
                tupla("cdn", &u("b"), 2),
                tupla("cdn", &u("c"), 3),
                tupla("cdn", &u("d"), 5),
            ],
            "{lang}"
        );
    }
}

#[test]
fn js_pin_e_fora_do_cache_seguem_as_mesmas_regras_do_html() {
    let pin = "https://cdn.jsdelivr.net/npm/lodash-es@4/lodash.js";
    let fonte = format!("import _ from \"{pin}\";\n");
    let v = medir("typescript", &fonte, &cache_com(&[]));
    assert_eq!(achados(&v), vec![tupla("cdn_pin", pin, 1)]);
    assert!(v.nao_medido.unwrap().contains("resolvedor"));
}

#[test]
fn linguagem_sem_url_de_cdn_possivel_nao_mede_nada() {
    let v = medir("python", &format!("x = \"{REACT_404}\"\n"), &cache_com(&[]));
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.is_none());
}

#[test]
fn registro_do_transversal_cobre_as_quatro_linguagens() {
    let mut langs = Cdn.langs().to_vec();
    langs.sort_unstable();
    assert_eq!(langs, vec!["html", "javascript", "tsx", "typescript"]);
    assert_eq!(Cdn.nome(), "cdn");
}
