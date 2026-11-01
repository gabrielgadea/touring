// touring-quality:allow-secrets — the SRI digests below are PUBLISHED reference values (Python
// `hashlib` outputs of a public string), high-entropy by nature and not credentials.
//! Tests of the network resolver. The logic is exercised with a scripted [`Transporte`]; the
//! transport itself ([`HttpPlano`]) and the end-to-end path (resolver → cache → layer 1) with a
//! fake CDN listening on 127.0.0.1.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use super::*;
use crate::vgp::CheckInput;
use crate::vgp::lang::cdn::Cdn;

const CORPO: &[u8] = b"console.log('vgp');\n";
const DIGESTS: [&str; 3] = [
    "sha256-jJeNPzyBu7gnnlj9LK0KVdAOTaOJExcIQEy3ACbmvQo=",
    "sha384-OcXUqEjFYy6VsN4S7pR544agBokSr1UOfFPWsq3YHKuFRIZmN07hNDLa0v+gTmsg",
    "sha512-SL+vzu/Xz7QEpNABHjCoZwllqNaGYfOWbiwBtY8cjsqLeLBVwlWZR38Ul784ZGQn8/VrBGqgK4J5w0Ub8qkvuw==",
];
const AGORA: u64 = 1_800_000_000;

fn resp(status: u16, corpo: &[u8]) -> Resposta {
    Resposta {
        status,
        location: None,
        corpo: corpo.to_vec(),
        truncado: false,
    }
}

fn redireciona(status: u16, destino: &str) -> Resposta {
    Resposta {
        location: Some(destino.into()),
        ..resp(status, b"")
    }
}

/// A transport that answers from a script and remembers every URL it was asked for. A URL
/// outside the script answers 404, like a CDN would.
struct Roteiro {
    respostas: HashMap<String, Result<Resposta, String>>,
    pedidos: Mutex<Vec<String>>,
}

impl Roteiro {
    fn novo(itens: Vec<(&str, Result<Resposta, String>)>) -> Self {
        Self {
            respostas: itens.into_iter().map(|(u, r)| (u.to_string(), r)).collect(),
            pedidos: Mutex::new(Vec::new()),
        }
    }

    fn pedidos(&self) -> Vec<String> {
        self.pedidos.lock().unwrap().clone()
    }
}

impl Transporte for Roteiro {
    fn get(&self, url: &str, _limite: usize) -> Result<Resposta, String> {
        self.pedidos.lock().unwrap().push(url.to_string());
        self.respostas
            .get(url)
            .cloned()
            .unwrap_or_else(|| Ok(resp(404, b"")))
    }
}

fn api(registro: &str, pacote: &str, spec: &str) -> String {
    format!("https://data.jsdelivr.com/v1/packages/{registro}/{pacote}/resolved?specifier={spec}")
}

fn versao_json(v: &str) -> Result<Resposta, String> {
    Ok(resp(
        200,
        format!(r#"{{"type":"npm","name":"x","version":"{v}"}}"#).as_bytes(),
    ))
}

// ------------------------------------------------------------------ resolver_url (roteiro)

#[test]
fn url_exata_que_existe_grava_existe_status_e_os_tres_digests_sem_consultar_a_api() {
    let url = "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js";
    let t = Roteiro::novo(vec![(url, Ok(resp(200, CORPO)))]);
    let r = resolver_url(&t, url, AGORA);
    assert_eq!(r.verdict, Veredito::Existe);
    assert_eq!(r.status, Some(200));
    assert_eq!(r.resolvida, None, "versão exata não resolve nada");
    assert_eq!(r.integrity, DIGESTS.to_vec());
    assert_eq!(r.verificado_em, Some(AGORA));
    assert_eq!(t.pedidos(), vec![url.to_string()]);
}

#[test]
fn faixa_resolve_pela_api_do_jsdelivr_e_busca_o_arquivo_na_versao_exata() {
    let t = Roteiro::novo(vec![
        (&api("npm", "bootstrap", "4.3"), versao_json("4.3.1")),
        (
            "https://cdn.jsdelivr.net/npm/bootstrap@4.3.1/dist/css/b.css",
            Ok(resp(200, CORPO)),
        ),
    ]);
    let r = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/bootstrap@4.3/dist/css/b.css",
        AGORA,
    );
    assert_eq!(r.verdict, Veredito::Existe);
    assert_eq!(r.resolvida.as_deref(), Some("4.3.1"));
    assert_eq!(
        r.integrity,
        DIGESTS.to_vec(),
        "o digest é do arquivo resolvido"
    );
    assert_eq!(
        t.pedidos(),
        vec![
            api("npm", "bootstrap", "4.3"),
            "https://cdn.jsdelivr.net/npm/bootstrap@4.3.1/dist/css/b.css".to_string()
        ]
    );
}

#[test]
fn sem_versao_pede_latest_e_o_intervalo_vai_codificado_na_query() {
    let t = Roteiro::novo(vec![
        (&api("npm", "jquery", "latest"), versao_json("3.7.1")),
        (&api("npm", "lodash", "%5E4.17.0"), versao_json("4.17.21")),
        (
            "https://cdn.jsdelivr.net/npm/jquery@3.7.1/dist/jquery.min.js",
            Ok(resp(200, CORPO)),
        ),
        (
            "https://cdn.jsdelivr.net/npm/lodash@4.17.21/lodash.js",
            Ok(resp(200, CORPO)),
        ),
    ]);
    let a = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/jquery/dist/jquery.min.js",
        AGORA,
    );
    assert_eq!(a.resolvida.as_deref(), Some("3.7.1"));
    let b = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/lodash@^4.17.0/lodash.js",
        AGORA,
    );
    assert_eq!(b.resolvida.as_deref(), Some("4.17.21"));
}

#[test]
fn escopo_e_gh_usam_o_caminho_certo_da_api() {
    let t = Roteiro::novo(vec![
        (&api("npm", "@scope/pkg", "1"), versao_json("1.4.0")),
        (&api("gh", "user/repo", "main"), versao_json("v2.0.0")),
        (
            "https://cdn.jsdelivr.net/npm/@scope/pkg@1.4.0/dist/x.js",
            Ok(resp(200, CORPO)),
        ),
        (
            "https://cdn.jsdelivr.net/gh/user/repo@v2.0.0/dist/x.js",
            Ok(resp(200, CORPO)),
        ),
    ]);
    let a = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/@scope/pkg@1/dist/x.js",
        AGORA,
    );
    assert_eq!(a.resolvida.as_deref(), Some("1.4.0"));
    let b = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/gh/user/repo@main/dist/x.js",
        AGORA,
    );
    assert_eq!(b.resolvida.as_deref(), Some("v2.0.0"));
    assert_eq!(a.verdict, Veredito::Existe);
    assert_eq!(b.verdict, Veredito::Existe);
}

#[test]
fn especificador_que_nenhuma_versao_satisfaz_e_404_versao_inexistente() {
    for api_resp in [
        Ok(resp(200, br#"{"type":"npm","name":"x","version":null}"#)),
        Ok(resp(404, b"")),
    ] {
        let t = Roteiro::novo(vec![(&api("npm", "a", "99"), api_resp)]);
        let r = resolver_url(&t, "https://cdn.jsdelivr.net/npm/a@99/x.js", AGORA);
        assert_eq!(r.verdict, Veredito::NaoExiste);
        assert_eq!(r.status, Some(404));
        assert_eq!(r.causa, Some(Causa::VersaoInexistente));
        assert_eq!(
            t.pedidos().len(),
            1,
            "sem versão resolvida não busca arquivo"
        );
    }
}

#[test]
fn api_fora_do_ar_deixa_a_url_indeterminada_nunca_404() {
    let t = Roteiro::novo(vec![(&api("npm", "a", "4"), Ok(resp(503, b"")))]);
    let r = resolver_url(&t, "https://cdn.jsdelivr.net/npm/a@4/x.js", AGORA);
    assert_eq!(r.verdict, Veredito::Indeterminado);
    assert!(r.erro.unwrap().contains("503"));

    let t = Roteiro::novo(vec![(&api("npm", "a", "4"), Err("timeout".into()))]);
    let r = resolver_url(&t, "https://cdn.jsdelivr.net/npm/a@4/x.js", AGORA);
    assert_eq!(r.verdict, Veredito::Indeterminado);
    assert_eq!(r.erro.as_deref(), Some("timeout"));
}

#[test]
fn resposta_ilegivel_da_api_e_indeterminada_nunca_404() {
    // a captive portal / error page answering 200 must not turn into "no such version"
    for corpo in [&b"<html>erro</html>"[..], b"{}", br#"{"version": 4}"#] {
        let t = Roteiro::novo(vec![(&api("npm", "a", "4"), Ok(resp(200, corpo)))]);
        let r = resolver_url(&t, "https://cdn.jsdelivr.net/npm/a@4/x.js", AGORA);
        assert_eq!(r.verdict, Veredito::Indeterminado, "{corpo:?}");
        assert!(r.erro.is_some());
        assert_eq!(r.causa, None);
    }
}

#[test]
fn pagina_html_no_lugar_do_arquivo_nao_e_existe_nem_gera_digest() {
    let url = "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js";
    let t = Roteiro::novo(vec![(
        url,
        Ok(resp(200, b"  \n<!DOCTYPE html><html>login</html>")),
    )]);
    let r = resolver_url(&t, url, AGORA);
    assert_eq!(r.verdict, Veredito::Indeterminado);
    assert!(r.integrity.is_empty());
    assert!(r.erro.unwrap().contains("HTML"));
    // ... mas um .html legítimo segue valendo
    let pagina = "https://cdnjs.cloudflare.com/ajax/libs/lib/1.0.0/index.html";
    let t = Roteiro::novo(vec![(pagina, Ok(resp(200, b"<html></html>")))]);
    assert_eq!(resolver_url(&t, pagina, AGORA).verdict, Veredito::Existe);
}

#[test]
fn arquivo_404_com_versao_existente_e_arquivo_inexistente() {
    let url = "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/nao_existe.js";
    let t = Roteiro::novo(vec![(
        "https://data.jsdelivr.com/v1/packages/npm/pkg@1.2.3",
        Ok(resp(200, b"{}")),
    )]);
    let r = resolver_url(&t, url, AGORA);
    assert_eq!(r.verdict, Veredito::NaoExiste);
    assert_eq!(r.status, Some(404));
    assert_eq!(r.causa, Some(Causa::ArquivoInexistente));
    assert!(r.integrity.is_empty());
}

#[test]
fn versao_404_na_listagem_e_versao_inexistente_e_listagem_quebrada_nao_adivinha() {
    let url = "https://cdn.jsdelivr.net/npm/pkg@9.9.9/dist/x.js";
    let sem_versao = Roteiro::novo(vec![]);
    assert_eq!(
        resolver_url(&sem_versao, url, AGORA).causa,
        Some(Causa::VersaoInexistente)
    );
    let quebrada = Roteiro::novo(vec![(
        "https://data.jsdelivr.com/v1/packages/npm/pkg@9.9.9",
        Ok(resp(403, b"")),
    )]);
    let r = resolver_url(&quebrada, url, AGORA);
    assert_eq!(r.verdict, Veredito::NaoExiste, "o 404 do arquivo é 404");
    assert_eq!(r.causa, None, "sem listagem não se sabe a causa");
}

#[test]
fn outros_status_sao_http_erro_e_falha_de_transporte_e_indeterminado() {
    let url = "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js";
    let t = Roteiro::novo(vec![(url, Ok(resp(503, b"")))]);
    let r = resolver_url(&t, url, AGORA);
    assert_eq!((r.verdict, r.status), (Veredito::HttpErro, Some(503)));

    let t = Roteiro::novo(vec![(url, Err("conexão recusada".into()))]);
    let r = resolver_url(&t, url, AGORA);
    assert_eq!((r.verdict, r.status), (Veredito::Indeterminado, None));
    assert_eq!(r.erro.as_deref(), Some("conexão recusada"));
}

#[test]
fn corpo_truncado_existe_mas_sem_integrity() {
    let url = "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js";
    let grande = Resposta {
        truncado: true,
        ..resp(200, b"prefixo")
    };
    let t = Roteiro::novo(vec![(url, Ok(grande))]);
    let r = resolver_url(&t, url, AGORA);
    assert_eq!(r.verdict, Veredito::Existe);
    assert!(
        r.integrity.is_empty(),
        "um digest de prefixo seria um digest falso"
    );
}

#[test]
fn unpkg_resolve_pelo_redirect_relativo_ou_absoluto() {
    for destino in ["/a@4.7.1/x.js", "https://unpkg.com/a@4.7.1/x.js"] {
        let t = Roteiro::novo(vec![
            ("https://unpkg.com/a@4/x.js", Ok(redireciona(302, destino))),
            ("https://unpkg.com/a@4.7.1/x.js", Ok(resp(200, CORPO))),
        ]);
        let r = resolver_url(&t, "https://unpkg.com/a@4/x.js", AGORA);
        assert_eq!(r.verdict, Veredito::Existe, "{destino}");
        assert_eq!(r.resolvida.as_deref(), Some("4.7.1"), "{destino}");
        assert_eq!(r.integrity, DIGESTS.to_vec());
    }
}

#[test]
fn unpkg_redirect_para_url_sem_versao_exata_ou_sem_location_e_indeterminado() {
    let t = Roteiro::novo(vec![(
        "https://unpkg.com/a@4/x.js",
        Ok(redireciona(302, "/a@5/x.js")),
    )]);
    let r = resolver_url(&t, "https://unpkg.com/a@4/x.js", AGORA);
    assert_eq!(r.verdict, Veredito::Indeterminado);
    assert!(r.erro.unwrap().contains("sem versão exata"));

    let t = Roteiro::novo(vec![("https://unpkg.com/a@4/x.js", Ok(resp(302, b"")))]);
    let r = resolver_url(&t, "https://unpkg.com/a@4/x.js", AGORA);
    assert_eq!(r.verdict, Veredito::Indeterminado);
    assert!(r.erro.unwrap().contains("Location"));
}

#[test]
fn unpkg_pacote_inexistente_e_404_e_resposta_estranha_e_indeterminada() {
    let t = Roteiro::novo(vec![]);
    let r = resolver_url(&t, "https://unpkg.com/nao-existe@4/x.js", AGORA);
    assert_eq!(r.verdict, Veredito::NaoExiste);
    assert_eq!(r.causa, Some(Causa::VersaoInexistente));

    let t = Roteiro::novo(vec![("https://unpkg.com/a@4/x.js", Ok(resp(500, b"")))]);
    let r = resolver_url(&t, "https://unpkg.com/a@4/x.js", AGORA);
    assert_eq!(r.verdict, Veredito::Indeterminado);
}

#[test]
fn cdnjs_leva_a_versao_no_caminho_e_nao_consulta_api() {
    let url = "https://cdnjs.cloudflare.com/ajax/libs/animate.css/4.1.1/animate.min.css";
    let t = Roteiro::novo(vec![(url, Ok(resp(200, CORPO)))]);
    let r = resolver_url(&t, url, AGORA);
    assert_eq!(r.verdict, Veredito::Existe);
    assert_eq!(t.pedidos(), vec![url.to_string()]);
}

#[test]
fn url_fora_do_alcance_nao_vai_a_rede() {
    let t = Roteiro::novo(vec![]);
    let r = resolver_url(&t, "https://example.com/x.js", AGORA);
    assert_eq!(r.verdict, Veredito::Indeterminado);
    assert!(r.erro.unwrap().contains("fora do alcance"));
    assert!(t.pedidos().is_empty());
}

#[test]
fn css_baixado_grava_as_classes_os_padroes_e_os_imports_e_so_css() {
    let css = "https://cdn.jsdelivr.net/npm/lib@1.2.3/dist/lib.css";
    let corpo = b"@import \"base.css\"; .b { } .a:hover { } [class^=\"x-\"] { }";
    let t = Roteiro::novo(vec![(css, Ok(resp(200, corpo)))]);
    let r = resolver_url(&t, css, AGORA);
    assert_eq!(r.verdict, Veredito::Existe);
    assert_eq!(r.classes, Some(vec!["a".to_string(), "b".to_string()]));
    assert_eq!(r.padroes, vec![("^=".to_string(), "x-".to_string())]);
    assert_eq!(r.imports, vec!["base.css".to_string()]);

    // um .js não carrega classes: `None` diz "não coletado", nunca "folha sem classes"
    let js = "https://cdn.jsdelivr.net/npm/lib@1.2.3/dist/lib.js";
    let t = Roteiro::novo(vec![(js, Ok(resp(200, b".b { }")))]);
    assert_eq!(resolver_url(&t, js, AGORA).classes, None);

    // truncado: sem digest e sem classes (um prefixo de folha seria um fato falso)
    let t = Roteiro::novo(vec![(
        css,
        Ok(Resposta {
            truncado: true,
            ..resp(200, b".b { }")
        }),
    )]);
    let r = resolver_url(&t, css, AGORA);
    assert_eq!((r.classes, r.integrity.is_empty()), (None, true));
}

// ------------------------------------------------------------------ preencher

#[test]
fn preencher_so_consulta_o_que_o_cache_nao_responde() {
    let novo = "https://cdn.jsdelivr.net/npm/novo@1.0.0/x.js";
    let bom = "https://cdn.jsdelivr.net/npm/bom@1.0.0/x.js";
    let morto = "https://cdn.jsdelivr.net/npm/morto@1.0.0/x.js";
    let lento = "https://cdn.jsdelivr.net/npm/lento@1.0.0/x.js";
    let vencido = "https://cdn.jsdelivr.net/npm/vencido@1.0.0/x.js";
    let mut cache = Cache::default();
    cache.inserir(bom, Registro::novo(Veredito::Existe));
    cache.inserir(
        morto,
        Registro {
            verificado_em: Some(AGORA - 10),
            ..Registro::novo(Veredito::NaoExiste)
        },
    );
    cache.inserir(lento, Registro::novo(Veredito::Indeterminado));
    cache.inserir(
        vencido,
        Registro {
            verificado_em: Some(AGORA - super::super::cache::TTL_404_S - 1),
            ..Registro::novo(Veredito::NaoExiste)
        },
    );
    let t = Roteiro::novo(vec![
        (novo, Ok(resp(200, CORPO))),
        (lento, Ok(resp(200, CORPO))),
        (vencido, Ok(resp(200, CORPO))),
    ]);
    let urls: Vec<String> = [
        novo,
        bom,
        morto,
        lento,
        vencido,
        novo,
        "https://example.com/a.js",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let resumo = preencher(&t, &urls, &mut cache, AGORA, false);
    assert_eq!(
        resumo,
        Resumo {
            consultadas: 3,
            puladas: 2,
            existem: 3,
            nao_existem: 0,
            sem_veredito: 0
        }
    );
    assert_eq!(
        t.pedidos().len(),
        3,
        "duplicata e URL fora do alcance não vão à rede"
    );
    assert_eq!(cache.buscar(lento).unwrap().verdict, Veredito::Existe);
    assert_eq!(cache.buscar(vencido).unwrap().verdict, Veredito::Existe);
    assert!(cache.buscar("https://example.com/a.js").is_none());
}

#[test]
fn preencher_com_refazer_ignora_o_cache_e_conta_cada_desfecho() {
    let a = "https://cdn.jsdelivr.net/npm/a@1.0.0/x.js";
    let b = "https://cdn.jsdelivr.net/npm/b@1.0.0/x.js";
    let c = "https://cdn.jsdelivr.net/npm/c@1.0.0/x.js";
    let mut cache = Cache::default();
    cache.inserir(a, Registro::novo(Veredito::Existe));
    let t = Roteiro::novo(vec![
        (a, Ok(resp(200, CORPO))),
        (b, Ok(resp(404, b""))),
        (c, Err("timeout".into())),
    ]);
    let urls: Vec<String> = [a, b, c].iter().map(|s| s.to_string()).collect();
    let resumo = preencher(&t, &urls, &mut cache, AGORA, true);
    assert_eq!(
        (resumo.consultadas, resumo.puladas),
        (3, 0),
        "refazer não pula nada"
    );
    assert_eq!(
        (resumo.existem, resumo.nao_existem, resumo.sem_veredito),
        (1, 1, 1)
    );
    assert_eq!(cache.buscar(c).unwrap().erro.as_deref(), Some("timeout"));
}

// ------------------------------------------------------------------ pedaços puros

#[test]
fn codifica_query_preserva_so_os_nao_reservados() {
    for (entrada, esperado) in [
        ("latest", "latest"),
        ("4.x", "4.x"),
        ("~1.2", "~1.2"),
        ("^4.3.0", "%5E4.3.0"),
        (">=1 <2", "%3E%3D1%20%3C2"),
        ("a/b?c", "a%2Fb%3Fc"),
    ] {
        assert_eq!(codifica_query(entrada), esperado, "{entrada}");
    }
}

#[test]
fn interpreta_status_location_corpo_e_truncamento() {
    let r = interpreta(
        b"HTTP/1.1 302 Found\r\nLOCATION: /a@1.0.0/x.js\r\nContent-Length: 5\r\n\r\nhello",
        3,
    )
    .unwrap();
    assert_eq!(r.status, 302);
    assert_eq!(r.location.as_deref(), Some("/a@1.0.0/x.js"));
    assert_eq!(r.corpo, b"hel");
    assert!(r.truncado);
    let inteiro = interpreta(b"HTTP/1.1 200 OK\r\n\r\nhello", 100).unwrap();
    assert!(!inteiro.truncado);
    assert_eq!(inteiro.corpo, b"hello");
}

#[test]
fn interpreta_recusa_o_que_nao_decodifica_ou_nao_entende() {
    assert!(
        interpreta(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
            10
        )
        .unwrap_err()
        .contains("chunked")
    );
    assert!(
        interpreta(b"HTTP/1.1 200 OK\r\nX: y", 10)
            .unwrap_err()
            .contains("cabeçalho")
    );
    assert!(
        interpreta(b"lixo\r\n\r\n", 10)
            .unwrap_err()
            .contains("status")
    );
}

#[test]
fn host_e_caminho_separa_o_alvo_do_pedido() {
    assert_eq!(
        host_e_caminho("https://cdn.jsdelivr.net/npm/a@1/x.js?v=1"),
        Some(("cdn.jsdelivr.net", "/npm/a@1/x.js?v=1"))
    );
    assert_eq!(host_e_caminho("//unpkg.com"), Some(("unpkg.com", "/")));
    assert_eq!(host_e_caminho("/relativa"), None);
}

#[test]
fn http_plano_sem_destino_nao_finge_tls() {
    let e = HttpPlano::novo()
        .get("https://cdn.jsdelivr.net/npm/a@1.0.0/x.js", 10)
        .unwrap_err();
    assert!(e.contains("TLS"), "{e}");
}

// ------------------------------------------------------------------ servidor HTTP falso local

#[derive(Clone)]
struct Rota {
    status: u16,
    cabecalhos: Vec<(String, String)>,
    corpo: Vec<u8>,
    atraso: Duration,
}

impl Rota {
    fn ok(corpo: &[u8]) -> Self {
        Self {
            status: 200,
            cabecalhos: Vec::new(),
            corpo: corpo.to_vec(),
            atraso: Duration::ZERO,
        }
    }

    fn com_status(status: u16) -> Self {
        Self {
            status,
            ..Self::ok(b"")
        }
    }
}

struct Falso {
    endereco: String,
    parar: Arc<AtomicBool>,
    pedidos: Arc<Mutex<Vec<String>>>,
    fio: Option<JoinHandle<()>>,
}

impl Falso {
    fn sobe(rotas: Vec<(&str, Rota)>) -> Self {
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let endereco = ouvinte.local_addr().unwrap().to_string();
        let rotas: Arc<HashMap<String, Rota>> =
            Arc::new(rotas.into_iter().map(|(p, r)| (p.to_string(), r)).collect());
        let parar = Arc::new(AtomicBool::new(false));
        let pedidos = Arc::new(Mutex::new(Vec::new()));
        let (p, ped) = (Arc::clone(&parar), Arc::clone(&pedidos));
        let fio = thread::spawn(move || {
            for conexao in ouvinte.incoming() {
                if p.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(conexao) = conexao else { continue };
                let (rotas, ped) = (Arc::clone(&rotas), Arc::clone(&ped));
                thread::spawn(move || atende(conexao, &rotas, &ped));
            }
        });
        Self {
            endereco,
            parar,
            pedidos,
            fio: Some(fio),
        }
    }

    /// A transport that sends every CDN host of the check to this server.
    fn transporte(&self, prazo: Duration) -> HttpPlano {
        let destinos = [
            "cdn.jsdelivr.net",
            "data.jsdelivr.com",
            "unpkg.com",
            "cdnjs.cloudflare.com",
        ]
        .iter()
        .map(|h| (h.to_string(), self.endereco.clone()))
        .collect();
        HttpPlano { prazo, destinos }
    }

    fn pedidos(&self) -> Vec<String> {
        self.pedidos.lock().unwrap().clone()
    }
}

impl Drop for Falso {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.endereco);
        if let Some(fio) = self.fio.take() {
            let _ = fio.join();
        }
    }
}

fn atende(mut conexao: TcpStream, rotas: &HashMap<String, Rota>, pedidos: &Mutex<Vec<String>>) {
    let _ = conexao.set_read_timeout(Some(Duration::from_secs(5)));
    let mut lido = Vec::new();
    let mut buffer = [0u8; 1024];
    while !lido.windows(4).any(|w| w == b"\r\n\r\n") {
        match conexao.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => lido.extend_from_slice(&buffer[..n]),
        }
    }
    let texto = String::from_utf8_lossy(&lido).to_string();
    let alvo = texto.split_whitespace().nth(1).unwrap_or("/").to_string();
    pedidos.lock().unwrap().push(alvo.clone());
    let rota = rotas
        .get(&alvo)
        .cloned()
        .unwrap_or_else(|| Rota::com_status(404));
    thread::sleep(rota.atraso);
    let mut cabecalho = format!(
        "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
        rota.status,
        rota.corpo.len()
    );
    for (nome, valor) in &rota.cabecalhos {
        cabecalho.push_str(&format!("{nome}: {valor}\r\n"));
    }
    cabecalho.push_str("\r\n");
    let _ = conexao.write_all(cabecalho.as_bytes());
    let _ = conexao.write_all(&rota.corpo);
}

const PRAZO_CURTO: Duration = Duration::from_millis(400);

#[test]
fn servidor_falso_url_exata_traz_status_e_digests_pela_rede_de_verdade() {
    let falso = Falso::sobe(vec![("/npm/pkg@1.2.3/dist/x.js", Rota::ok(CORPO))]);
    let t = falso.transporte(Duration::from_secs(5));
    let r = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js",
        AGORA,
    );
    assert_eq!(r.verdict, Veredito::Existe);
    assert_eq!(r.integrity, DIGESTS.to_vec());
    assert_eq!(
        falso.pedidos(),
        vec!["/npm/pkg@1.2.3/dist/x.js".to_string()]
    );
}

#[test]
fn servidor_falso_faixa_resolve_pela_api_e_unpkg_pelo_redirect() {
    let mut redireciona = Rota::com_status(302);
    redireciona
        .cabecalhos
        .push(("Location".into(), "/a@4.7.1/x.js".into()));
    let falso = Falso::sobe(vec![
        (
            "/v1/packages/npm/bootstrap/resolved?specifier=4.3",
            Rota::ok(br#"{"version":"4.3.1"}"#),
        ),
        ("/npm/bootstrap@4.3.1/dist/b.css", Rota::ok(CORPO)),
        ("/a@4/x.js", redireciona),
        ("/a@4.7.1/x.js", Rota::ok(CORPO)),
    ]);
    let t = falso.transporte(Duration::from_secs(5));
    let a = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/bootstrap@4.3/dist/b.css",
        AGORA,
    );
    assert_eq!(
        (a.verdict, a.resolvida.as_deref()),
        (Veredito::Existe, Some("4.3.1"))
    );
    let b = resolver_url(&t, "https://unpkg.com/a@4/x.js", AGORA);
    assert_eq!(
        (b.verdict, b.resolvida.as_deref()),
        (Veredito::Existe, Some("4.7.1"))
    );
}

#[test]
fn servidor_falso_404_com_causa_pela_listagem() {
    let falso = Falso::sobe(vec![("/v1/packages/npm/pkg@1.2.3", Rota::ok(b"{}"))]);
    let t = falso.transporte(Duration::from_secs(5));
    let r = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js",
        AGORA,
    );
    assert_eq!((r.verdict, r.status), (Veredito::NaoExiste, Some(404)));
    assert_eq!(r.causa, Some(Causa::ArquivoInexistente));
}

#[test]
fn servidor_falso_5xx_e_http_erro() {
    let falso = Falso::sobe(vec![("/npm/pkg@1.2.3/dist/x.js", Rota::com_status(502))]);
    let t = falso.transporte(Duration::from_secs(5));
    let r = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js",
        AGORA,
    );
    assert_eq!((r.verdict, r.status), (Veredito::HttpErro, Some(502)));
}

#[test]
fn servidor_falso_que_nao_responde_a_tempo_e_indeterminado_nunca_404() {
    let lenta = Rota {
        atraso: Duration::from_millis(1500),
        ..Rota::ok(CORPO)
    };
    let falso = Falso::sobe(vec![("/npm/pkg@1.2.3/dist/x.js", lenta)]);
    let t = falso.transporte(PRAZO_CURTO);
    let inicio = Instant::now();
    let r = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js",
        AGORA,
    );
    assert!(
        inicio.elapsed() < Duration::from_millis(1400),
        "respeitou o prazo"
    );
    assert_eq!((r.verdict, r.status), (Veredito::Indeterminado, None));
    assert!(r.erro.is_some());
    assert!(r.integrity.is_empty());
}

#[test]
fn conexao_recusada_e_indeterminada() {
    let porta_morta = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().to_string()
    };
    let t = HttpPlano {
        prazo: PRAZO_CURTO,
        destinos: [("cdn.jsdelivr.net".to_string(), porta_morta)].into(),
    };
    let r = resolver_url(
        &t,
        "https://cdn.jsdelivr.net/npm/pkg@1.2.3/dist/x.js",
        AGORA,
    );
    assert_eq!(r.verdict, Veredito::Indeterminado);
    assert!(r.erro.unwrap().contains("conexão"));
}

#[test]
fn do_resolvedor_a_camada_um_o_cache_gravado_vira_achado_sem_rede() {
    // end to end: the resolver learns a 404 from a fake CDN, the cache goes through the disk,
    // and the PURE layer 1 — no transport anywhere in sight — denies the document.
    let url404 = "https://cdn.jsdelivr.net/npm/react@18.3.99/umd/react.production.min.js";
    let falso = Falso::sobe(vec![(
        "/v1/packages/npm/react@18.3.99",
        Rota::com_status(404),
    )]);
    let t = falso.transporte(Duration::from_secs(5));
    let mut cache = Cache::default();
    let agora_real = agora_unix();
    let resumo = preencher(&t, &[url404.to_string()], &mut cache, agora_real, false);
    assert_eq!((resumo.consultadas, resumo.nao_existem), (1, 1));

    let dir = tempfile::tempdir().unwrap();
    let arquivo = dir.path().join("cdn_cache.json");
    cache.guardar(&arquivo).unwrap();
    let lido = Cache::carregar(&arquivo).unwrap();

    let html = format!("<html>\n<head>\n<script src=\"{url404}\"></script>\n</head>\n</html>\n");
    let input = CheckInput {
        path: "/home/u/proj/index.html".into(),
        content: html,
        project_root: "/home/u/proj".into(),
        disk: None,
        overlays: HashMap::new(),
        lang: "html".into(),
    };
    let v = Cdn.medir_com(&input, &lido, SystemTime::now());
    assert_eq!(v.achados.len(), 1);
    assert_eq!(
        (v.achados[0].classe.as_str(), v.achados[0].linha),
        ("cdn", 3)
    );
    assert!(
        v.achados[0].sugestao.contains("versão `18.3.99`"),
        "a causa gravada pelo resolvedor chega à sugestão: {}",
        v.achados[0].sugestao
    );
}
