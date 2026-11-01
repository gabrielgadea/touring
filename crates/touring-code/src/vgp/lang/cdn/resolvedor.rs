//! The NETWORK side of the CDN check: look URLs up on jsDelivr / unpkg / cdnjs and fill the
//! cache ([`super::cache`]) that the pure layer 1 reads. Layer 1 NEVER calls this — a hook must
//! not wait on the network (VL0 §3); a command or a background job does, and the layer-1 reason
//! for an unmeasured URL names [`preencher`].
//!
//! What it learns per URL ([`resolver_url`]):
//! 1. a floating specifier (`@4.3`, `@latest`, none) is resolved to an exact version — jsDelivr:
//!    `GET data.jsdelivr.com/v1/packages/{npm|gh}/{pkg}/resolved?specifier=…` → `version`;
//!    unpkg: the 301/302 `Location` of the URL itself (unpkg documents that "URLs that do not
//!    specify a fully resolved package version number redirect to one that does");
//! 2. the file at the exact version is fetched: 2xx → `existe` + the SRI digests of the bytes;
//!    404 → `404` + the cause (missing version vs missing file, from the jsDelivr package listing);
//!    other status → `http_erro`; no answer → `indeterminado` — **a timeout is never a 404**.
//!
//! The transport is a trait ([`Transporte`]): `touring-code` carries no HTTP client, so the
//! HTTPS binding (the workspace already has `reqwest`) belongs to whoever wires this to a
//! command. [`HttpPlano`] speaks plain `http://` over `std::net` — enough for the tests, which
//! route the CDN hosts to a fake local server, and nothing else.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::cache::{Cache, Causa, Registro, Veredito, normaliza};
use super::hash;
use super::{Origem, Servidor, UrlCdn};
use crate::vgp::lang::classe_css;

/// Largest body hashed for the SRI digests; a bigger file is `existe` without `integrity`.
pub const LIMITE_CORPO: usize = 8 * 1024 * 1024;

/// How long the default transport waits for one request.
pub const PRAZO_PADRAO: Duration = Duration::from_secs(5);

/// One HTTP answer, redirects NOT followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resposta {
    /// The HTTP status code.
    pub status: u16,
    /// The `Location` header, for 3xx.
    pub location: Option<String>,
    /// The body, cut at the limit the caller passed.
    pub corpo: Vec<u8>,
    /// Did the body exceed the limit (so `corpo` is only a prefix)?
    pub truncado: bool,
}

/// The one network capability the resolver needs. An implementation must NOT follow redirects
/// (unpkg's `Location` is the answer) and must give up after its own deadline, returning `Err`.
pub trait Transporte {
    /// `GET url`, keeping at most `limite_corpo` bytes of the body.
    fn get(&self, url: &str, limite_corpo: usize) -> Result<Resposta, String>;
}

/// Unix seconds now — the `verificado_em` stamp.
pub fn agora_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Percent-encode everything but the RFC 3986 unreserved characters (a semver range such as
/// `^4.3.0` or `>=1 <2` must survive a query string).
fn codifica_query(valor: &str) -> String {
    let mut out = String::with_capacity(valor.len());
    for b in valor.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `version` out of the jsDelivr resolve answer: `Ok(Some(v))`, `Ok(None)` for an explicit
/// `null` (nothing satisfies the specifier). Anything that is not that JSON — an HTML error page
/// served with 200, a captive portal — is `Err`: an unreadable answer is never "no such version".
fn versao_do_json(corpo: &[u8]) -> Result<Option<String>, String> {
    let json: serde_json::Value = serde_json::from_slice(corpo)
        .map_err(|_| "data.jsdelivr.com respondeu algo que não é JSON".to_string())?;
    match json.get("version") {
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(v)) => Ok(Some(v.clone())),
        _ => Err("a resposta de data.jsdelivr.com não tem `version`".to_string()),
    }
}

/// The jsDelivr registry segment (`npm` | `gh`) of a package.
fn registro_jsdelivr(origem: Origem) -> &'static str {
    match origem {
        Origem::Npm => "npm",
        Origem::Gh => "gh",
    }
}

/// The exact version a floating `u` resolves to right now. `Ok(None)` means nothing satisfies
/// the specifier (the package or the range does not exist); `Err` means the lookup itself
/// failed (no answer, unexpected status) and the URL stays `indeterminado`.
fn resolver_versao(t: &dyn Transporte, u: &UrlCdn) -> Result<Option<String>, String> {
    match u.servidor {
        Servidor::Jsdelivr => versao_jsdelivr(t, u),
        Servidor::Unpkg => versao_unpkg(t, u),
        // cdnjs carries the (exact) version in the path: there is nothing to resolve
        Servidor::Cdnjs => Ok(u.versao.clone()),
    }
}

fn versao_jsdelivr(t: &dyn Transporte, u: &UrlCdn) -> Result<Option<String>, String> {
    let especificador = u.versao.as_deref().unwrap_or("latest");
    let url = format!(
        "https://data.jsdelivr.com/v1/packages/{}/{}/resolved?specifier={}",
        registro_jsdelivr(u.origem),
        u.pacote,
        codifica_query(especificador)
    );
    let r = t.get(&url, 64 * 1024)?;
    match r.status {
        200..=299 => versao_do_json(&r.corpo),
        404 => Ok(None),
        outro => Err(format!("data.jsdelivr.com respondeu HTTP {outro}")),
    }
}

fn versao_unpkg(t: &dyn Transporte, u: &UrlCdn) -> Result<Option<String>, String> {
    let r = t.get(&u.url_canonica(), 1024)?;
    match r.status {
        301 | 302 | 303 | 307 | 308 => versao_do_redirecionamento(r.location).map(Some),
        404 => Ok(None),
        outro => Err(format!("unpkg respondeu HTTP {outro} ao resolver a versão")),
    }
}

/// The exact version in the `Location` an unpkg redirect points to.
fn versao_do_redirecionamento(location: Option<String>) -> Result<String, String> {
    let destino = location.ok_or_else(|| "unpkg redirecionou sem `Location`".to_string())?;
    let absoluta = if destino.starts_with('/') {
        format!("https://unpkg.com{destino}")
    } else {
        destino
    };
    UrlCdn::parse(&absoluta)
        .filter(UrlCdn::versao_exata)
        .and_then(|d| d.versao)
        .ok_or_else(|| format!("unpkg redirecionou para uma URL sem versão exata: {absoluta}"))
}

/// Why a 404 is a 404, from the jsDelivr package listing at the exact version: the listing
/// itself 404s → no such version; 200 → the version exists, so the FILE does not.
fn classificar_404(t: &dyn Transporte, u: &UrlCdn) -> Option<Causa> {
    if u.servidor != Servidor::Jsdelivr {
        return None;
    }
    let url = format!(
        "https://data.jsdelivr.com/v1/packages/{}/{}@{}",
        registro_jsdelivr(u.origem),
        u.pacote,
        u.versao.as_deref()?
    );
    match t.get(&url, 1024) {
        Ok(r) if r.status == 404 => Some(Causa::VersaoInexistente),
        Ok(r) if (200..300).contains(&r.status) => Some(Causa::ArquivoInexistente),
        _ => None,
    }
}

/// Look one URL up. Never panics, never returns `Err`: a failed lookup IS the record
/// (`indeterminado` + the transport error), so the caller can store and report it.
pub fn resolver_url(t: &dyn Transporte, url: &str, agora: u64) -> Registro {
    let mut reg = Registro::novo(Veredito::Indeterminado);
    reg.verificado_em = Some(agora);
    let Some(u) = UrlCdn::parse(url) else {
        reg.erro = Some("URL fora do alcance do resolvedor (jsDelivr, unpkg, cdnjs)".into());
        return reg;
    };
    // the URL whose bytes decide existence and digests: the exact-version form
    let exata = if u.versao_exata() {
        u.url_canonica()
    } else {
        match resolver_versao(t, &u) {
            Ok(Some(versao)) => {
                let exata = u.com_versao(&versao);
                reg.resolvida = Some(versao);
                exata
            }
            Ok(None) => {
                reg.verdict = Veredito::NaoExiste;
                reg.status = Some(404);
                reg.causa = Some(Causa::VersaoInexistente);
                return reg;
            }
            Err(e) => {
                reg.erro = Some(e);
                return reg;
            }
        }
    };
    registrar_arquivo(t, &mut reg, &exata, &u);
    reg
}

/// Does `corpo` look like an HTML page? A script or a stylesheet that starts with `<` is an error
/// page or a captive portal answering 200, not the file.
fn parece_html(corpo: &[u8]) -> bool {
    corpo
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'<')
}

/// Fold a 2xx answer into `reg`: `existe` and the digests, unless the body is not the file.
fn registrar_2xx(reg: &mut Registro, r: &Resposta, exata: &str) {
    let e_html = exata.ends_with(".html") || exata.ends_with(".htm");
    if !e_html && parece_html(&r.corpo) {
        reg.erro = Some("200 com uma página HTML no lugar do arquivo (portal cativo?)".into());
        return;
    }
    reg.verdict = Veredito::Existe;
    reg.status = Some(r.status);
    if !r.truncado {
        reg.integrity = hash::sri(&r.corpo);
        registrar_classes(reg, r, exata);
    }
}

/// A downloaded `.css` file: record the classes it defines, so layer 1 can read the sheet from
/// the cache (`classe_css`) instead of the network.
fn registrar_classes(reg: &mut Registro, r: &Resposta, exata: &str) {
    if exata
        .split(['?', '#'])
        .next()
        .is_some_and(|p| p.ends_with(".css"))
        && let Ok(texto) = std::str::from_utf8(&r.corpo)
        && let Some(folha) = classe_css::ler_folha(texto)
    {
        reg.classes = Some(folha.classes);
        reg.padroes = folha.padroes;
        reg.imports = folha.imports;
    }
}

/// Fetch the file at the exact URL and fold the answer into `reg`.
fn registrar_arquivo(t: &dyn Transporte, reg: &mut Registro, exata: &str, original: &UrlCdn) {
    match t.get(exata, LIMITE_CORPO) {
        Err(e) => reg.erro = Some(e),
        Ok(r) if (200..300).contains(&r.status) => registrar_2xx(reg, &r, exata),
        Ok(r) if r.status == 404 => {
            reg.verdict = Veredito::NaoExiste;
            reg.status = Some(404);
            let no_exato = UrlCdn::parse(exata);
            reg.causa = classificar_404(t, no_exato.as_ref().unwrap_or(original));
        }
        Ok(r) => {
            reg.verdict = Veredito::HttpErro;
            reg.status = Some(r.status);
        }
    }
}

/// What one [`preencher`] pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Resumo {
    /// URLs the resolver asked the network about.
    pub consultadas: usize,
    /// URLs skipped: already recorded with a verdict that is still good.
    pub puladas: usize,
    /// Of the consulted ones, how many came back `existe`.
    pub existem: usize,
    /// ... `404`.
    pub nao_existem: usize,
    /// ... `http_erro` or `indeterminado` (look again later).
    pub sem_veredito: usize,
}

/// Is the cached record final enough to skip the network? `existe` and a fresh `404` are;
/// `indeterminado`, `http_erro` and an expired `404` must be looked up again.
fn registro_bom(r: &Registro, agora: u64) -> bool {
    match r.verdict {
        Veredito::Existe => true,
        Veredito::NaoExiste => !r.e_404_vencido(UNIX_EPOCH + Duration::from_secs(agora)),
        Veredito::HttpErro | Veredito::Indeterminado => false,
    }
}

/// Resolve every CDN URL in `urls` that the cache does not already answer, and record the
/// result in `cache` (the caller then [`Cache::guardar`]s). `refazer` ignores the cache.
/// URLs that are not jsDelivr/unpkg/cdnjs package URLs are ignored.
pub fn preencher(
    t: &dyn Transporte,
    urls: &[String],
    cache: &mut Cache,
    agora: u64,
    refazer: bool,
) -> Resumo {
    let mut resumo = Resumo::default();
    let mut vistas = HashSet::new();
    for url in urls {
        if UrlCdn::parse(url).is_none() || !vistas.insert(normaliza(url)) {
            continue;
        }
        if !refazer && cache.buscar(url).is_some_and(|r| registro_bom(r, agora)) {
            resumo.puladas += 1;
            continue;
        }
        let registro = resolver_url(t, url, agora);
        resumo.consultadas += 1;
        match registro.verdict {
            Veredito::Existe => resumo.existem += 1,
            Veredito::NaoExiste => resumo.nao_existem += 1,
            Veredito::HttpErro | Veredito::Indeterminado => resumo.sem_veredito += 1,
        }
        cache.inserir(url, registro);
    }
    resumo
}

/// Plain-`http://` transport over `std::net`, for tests and tools. `destinos` maps a host to
/// the `ip:port` it is served from (the tests send `https://cdn.jsdelivr.net/…` to a fake local
/// server); a host without a mapping is dialled on port 80. It does not speak TLS, follow
/// redirects or decode `Transfer-Encoding: chunked`.
#[derive(Debug, Clone)]
pub struct HttpPlano {
    /// Total time one request may take (connect, send and read).
    pub prazo: Duration,
    /// host → `ip:port`.
    pub destinos: HashMap<String, String>,
}

impl HttpPlano {
    /// A transport with the default deadline and no host overrides.
    pub fn novo() -> Self {
        Self {
            prazo: PRAZO_PADRAO,
            destinos: HashMap::new(),
        }
    }

    /// Where to dial for `host` of `url`: the override, else port 80 for `http://`.
    fn destino(&self, host: &str, url: &str) -> Result<SocketAddr, String> {
        let destino = match self.destinos.get(host) {
            Some(d) => d.clone(),
            None if url.starts_with("https://") => {
                return Err(format!(
                    "HttpPlano não fala TLS e `{host}` não tem destino: use um Transporte HTTPS"
                ));
            }
            None => format!("{host}:80"),
        };
        destino
            .to_socket_addrs()
            .map_err(|e| format!("DNS {destino}: {e}"))?
            .next()
            .ok_or_else(|| format!("sem endereço para {destino}"))
    }

    /// Read to EOF within the deadline (`Connection: close`); the head and the body arrive
    /// together. Stops early once `teto` bytes are in.
    fn ler_tudo(
        &self,
        tcp: &mut TcpStream,
        inicio: Instant,
        teto: usize,
    ) -> Result<Vec<u8>, String> {
        let mut bruto = Vec::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let resta = self.prazo.saturating_sub(inicio.elapsed());
            if resta.is_zero() {
                return Err(format!("prazo de {:?} esgotado", self.prazo));
            }
            tcp.set_read_timeout(Some(resta))
                .map_err(|e| e.to_string())?;
            match tcp.read(&mut buffer) {
                Ok(0) => return Ok(bruto),
                Ok(n) => {
                    bruto.extend_from_slice(&buffer[..n]);
                    if bruto.len() > teto {
                        return Ok(bruto);
                    }
                }
                Err(e) => return Err(format!("leitura: {e}")),
            }
        }
    }
}

impl Default for HttpPlano {
    fn default() -> Self {
        Self::novo()
    }
}

/// `(host, path-with-query)` of an absolute or `//host/…` URL.
fn host_e_caminho(url: &str) -> Option<(&str, &str)> {
    let resto = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("//"))?;
    match resto.find('/') {
        Some(i) => Some((&resto[..i], &resto[i..])),
        None => Some((resto, "/")),
    }
}

impl Transporte for HttpPlano {
    fn get(&self, url: &str, limite_corpo: usize) -> Result<Resposta, String> {
        let inicio = Instant::now();
        let (host, caminho) =
            host_e_caminho(url).ok_or_else(|| format!("URL absoluta esperada: {url}"))?;
        let endereco = self.destino(host, url)?;
        let mut tcp = TcpStream::connect_timeout(&endereco, self.prazo)
            .map_err(|e| format!("conexão com {endereco}: {e}"))?;
        let escrita = self
            .prazo
            .saturating_sub(inicio.elapsed())
            .max(Duration::from_millis(1));
        tcp.set_write_timeout(Some(escrita))
            .map_err(|e| e.to_string())?;
        let pedido = format!(
            "GET {caminho} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: touring-vgp-cdn/1\r\n\
             Accept-Encoding: identity\r\nConnection: close\r\n\r\n"
        );
        tcp.write_all(pedido.as_bytes())
            .map_err(|e| format!("envio a {endereco}: {e}"))?;
        let bruto = self
            .ler_tudo(&mut tcp, inicio, limite_corpo.saturating_add(64 * 1024))
            .map_err(|e| format!("{endereco}: {e}"))?;
        interpreta(&bruto, limite_corpo)
    }
}

/// Parse `status line + headers + body` as read from a `Connection: close` response.
fn interpreta(bruto: &[u8], limite_corpo: usize) -> Result<Resposta, String> {
    let fim_cabecalho = bruto
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "resposta HTTP sem fim de cabeçalho".to_string())?;
    let cabecalho = String::from_utf8_lossy(&bruto[..fim_cabecalho]);
    let mut linhas = cabecalho.lines();
    let status = linhas
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| "linha de status HTTP ilegível".to_string())?;
    let cabecalhos: Vec<(&str, &str)> = linhas
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim(), v.trim()))
        .collect();
    let valor_de = |nome: &str| {
        cabecalhos
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(nome))
            .map(|(_, v)| *v)
    };
    if valor_de("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        return Err("HttpPlano não decodifica Transfer-Encoding: chunked".into());
    }
    let corpo = &bruto[fim_cabecalho + 4..];
    Ok(Resposta {
        status,
        location: valor_de("location").map(str::to_string),
        corpo: corpo[..corpo.len().min(limite_corpo)].to_vec(),
        truncado: corpo.len() > limite_corpo,
    })
}

#[cfg(test)]
mod tests;
