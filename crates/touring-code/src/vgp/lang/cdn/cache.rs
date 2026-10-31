//! The local CDN cache — what the NETWORK resolver learned about each URL, and the only thing
//! the pure layer 1 reads (VL0 §3: no network on the hook path).
//!
//! **Path**: `$TOURING_VGP_CDN_CACHE` when set, else `~/.claude/touring/vgp/cdn_cache.json`
//! ([`caminho_padrao`]). **Format** (version 1), one record per URL, written by
//! [`super::resolvedor`]:
//!
//! ```json
//! { "formato": 1,
//!   "urls": { "https://cdn.jsdelivr.net/npm/bootstrap@4.3/dist/css/bootstrap.min.css": {
//!       "verdict": "existe", "status": 200, "resolvida": "4.3.1",
//!       "integrity": ["sha256-…", "sha384-…", "sha512-…"], "verificado_em": 1791600000 } } }
//! ```
//!
//! `verdict` is `existe` | `404` | `http_erro` | `indeterminado` (a timeout is NEVER a 404);
//! `resolvida` is the exact version a floating specifier (`@4.3`, `@latest`, none) resolved to;
//! `causa` splits a 404 into a missing version and a missing file; `integrity` are the SRI
//! digests of the bytes served; `verificado_em` is unix seconds. The reader also accepts the flat
//! shape the corpus generator (touring-4f) wrote — a bare `{url: {status, verdict}}` map — so
//! the semeado corpus cache works unchanged.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::SystemTime;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Environment variable that overrides [`caminho_padrao`].
pub const VAR_AMBIENTE: &str = "TOURING_VGP_CDN_CACHE";

/// Version of the written format.
pub const FORMATO: u32 = 1;

/// A `404` record older than this is not trusted to deny: a version published after the lookup
/// would be denied for as long as the stale record lived. Records without a timestamp (the flat
/// corpus cache) are trusted.
pub const TTL_404_S: u64 = 24 * 3600;

/// What the resolver concluded about one URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Veredito {
    /// The URL (at its resolved version) answered 2xx.
    #[serde(rename = "existe")]
    Existe,
    /// HTTP 404: the version or the file does not exist.
    #[serde(rename = "404")]
    NaoExiste,
    /// Another HTTP status (5xx, 403…): the CDN did not answer the question.
    #[serde(rename = "http_erro")]
    HttpErro,
    /// No answer (timeout, DNS, TLS) — and any verdict string this reader does not know.
    #[serde(rename = "indeterminado", other)]
    Indeterminado,
}

impl Veredito {
    /// The word the cache file uses for this verdict.
    pub fn rotulo(self) -> &'static str {
        match self {
            Self::Existe => "existe",
            Self::NaoExiste => "404",
            Self::HttpErro => "http_erro",
            Self::Indeterminado => "indeterminado",
        }
    }
}

/// Why a URL answered 404.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Causa {
    /// The package has no such version.
    #[serde(rename = "versao_inexistente")]
    VersaoInexistente,
    /// The version exists but does not contain the file.
    #[serde(rename = "arquivo_inexistente")]
    ArquivoInexistente,
    /// A cause this reader does not know.
    #[serde(rename = "outra", other)]
    Outra,
}

/// One URL's record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registro {
    /// The conclusion.
    pub verdict: Veredito,
    /// HTTP status of the deciding request, when there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The exact version a floating specifier resolved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolvida: Option<String>,
    /// Why a 404 is a 404.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causa: Option<Causa>,
    /// SRI digests (`sha256-…`, `sha384-…`, `sha512-…`) of the bytes the CDN served.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrity: Vec<String>,
    /// Unix seconds of the lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verificado_em: Option<u64>,
    /// The transport error, for `indeterminado`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub erro: Option<String>,
    /// For a `.css` file the resolver downloaded: the classes it defines (`None` = not collected,
    /// `Some(vec![])` = a sheet with no class). Lets layer 1 read a CDN sheet without the network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classes: Option<Vec<String>>,
    /// `[class…=…]` selectors of that sheet: (operator, value).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub padroes: Vec<(String, String)>,
    /// `@import` targets of that sheet, as written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
}

impl Registro {
    /// A record with a verdict and nothing else.
    pub fn novo(verdict: Veredito) -> Self {
        Self {
            verdict,
            status: None,
            resolvida: None,
            causa: None,
            integrity: Vec::new(),
            verificado_em: None,
            erro: None,
            classes: None,
            padroes: Vec::new(),
            imports: Vec::new(),
        }
    }

    /// Is this a 404 old enough that it must be looked up again before it can deny?
    pub fn e_404_vencido(&self, agora: SystemTime) -> bool {
        if self.verdict != Veredito::NaoExiste {
            return false;
        }
        let Some(quando) = self.verificado_em else {
            return false;
        };
        let agora_s = agora
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        agora_s.saturating_sub(quando) > TTL_404_S
    }
}

/// Canonical cache key: `https`, lower-case host, no query and no fragment. `//host/x` and
/// `http://host/x` are the same resource as `https://host/x` on every CDN of the check.
pub fn normaliza(url: &str) -> String {
    let u = url.trim();
    let sem_esquema = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"))
        .or_else(|| u.strip_prefix("//"))
        .unwrap_or(u);
    let corte = sem_esquema.find(['?', '#']).unwrap_or(sem_esquema.len());
    let host_e_caminho = &sem_esquema[..corte];
    let (host, caminho) = match host_e_caminho.find('/') {
        Some(i) => (&host_e_caminho[..i], &host_e_caminho[i..]),
        None => (host_e_caminho, ""),
    };
    format!("https://{}{}", host.to_ascii_lowercase(), caminho)
}

/// The URLs the resolver has looked up.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cache {
    urls: BTreeMap<String, Registro>,
    origem: Option<PathBuf>,
}

/// The on-disk envelope of format 1.
#[derive(Serialize, Deserialize)]
struct Envelope {
    formato: u32,
    urls: BTreeMap<String, Registro>,
}

impl Cache {
    /// An empty cache that remembers the file it would be read from.
    pub fn vazio_de(origem: Option<PathBuf>) -> Self {
        Self {
            urls: BTreeMap::new(),
            origem,
        }
    }

    /// Parse either format 1 or the flat corpus map.
    pub fn de_json(texto: &str) -> Result<Self, String> {
        let valor: serde_json::Value =
            serde_json::from_str(texto).map_err(|e| format!("JSON inválido: {e}"))?;
        let objeto = valor
            .as_object()
            .ok_or_else(|| "o cache deve ser um objeto JSON".to_string())?;
        let urls = if objeto.get("urls").is_some_and(|u| u.is_object()) {
            let envelope: Envelope = serde_json::from_value(valor)
                .map_err(|e| format!("cache formato {FORMATO} malformado: {e}"))?;
            envelope.urls
        } else {
            // flat map written by the corpus generator: a malformed record is dropped, not fatal
            objeto
                .iter()
                .filter_map(|(url, v)| {
                    let registro: Registro = serde_json::from_value(v.clone()).ok()?;
                    Some((url.clone(), registro))
                })
                .collect()
        };
        Ok(Self { urls, origem: None })
    }

    /// Format 1, stable key order.
    pub fn para_json(&self) -> String {
        let envelope = Envelope {
            formato: FORMATO,
            urls: self.urls.clone(),
        };
        serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string())
    }

    /// Read `caminho`. A missing file is an EMPTY cache (nothing resolved yet); an unreadable or
    /// malformed one is an error, so the caller can say why nothing was measured.
    pub fn carregar(caminho: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(caminho) {
            Ok(texto) => {
                let mut cache =
                    Self::de_json(&texto).map_err(|e| format!("{}: {e}", caminho.display()))?;
                cache.origem = Some(caminho.to_path_buf());
                Ok(cache)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::vazio_de(Some(caminho.to_path_buf())))
            }
            Err(e) => Err(format!("{}: {e}", caminho.display())),
        }
    }

    /// Write format 1 atomically (temp file in the same directory, then rename), so a hook that
    /// reads while the resolver writes sees the old or the new file, never half of one.
    pub fn guardar(&self, caminho: &Path) -> Result<(), String> {
        let pasta = caminho
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(pasta).map_err(|e| format!("{}: {e}", pasta.display()))?;
        let nome = caminho
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("cdn_cache.json");
        let tmp = pasta.join(format!(".{nome}.{}.tmp", std::process::id()));
        std::fs::write(&tmp, self.para_json()).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, caminho).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", caminho.display())
        })
    }

    /// The record of `url`: the exact key first, then its canonical form ([`normaliza`]).
    pub fn buscar(&self, url: &str) -> Option<&Registro> {
        self.urls
            .get(url)
            .or_else(|| self.urls.get(&normaliza(url)))
    }

    /// Store `registro` under the canonical key of `url`.
    pub fn inserir(&mut self, url: &str, registro: Registro) {
        self.urls.insert(normaliza(url), registro);
    }

    /// Every record, in key order.
    pub fn registros(&self) -> impl Iterator<Item = (&str, &Registro)> {
        self.urls.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// How many URLs are recorded.
    pub fn len(&self) -> usize {
        self.urls.len()
    }

    /// Is the cache empty?
    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }

    /// The file this cache was read from (or would be written to).
    pub fn origem(&self) -> Option<&Path> {
        self.origem.as_deref()
    }
}

/// The cache path from the environment: `var` (the value of [`VAR_AMBIENTE`]) when non-empty,
/// else `<home>/.claude/touring/vgp/cdn_cache.json`. Pure over its inputs, so the table of
/// cases needs no process-global `setenv`.
pub fn caminho_padrao_de(var: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(v) = var.filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(v));
    }
    let home = home.filter(|h| !h.is_empty())?;
    Some(Path::new(home).join(".claude/touring/vgp/cdn_cache.json"))
}

/// [`caminho_padrao_de`] over the real environment.
pub fn caminho_padrao() -> Option<PathBuf> {
    caminho_padrao_de(
        std::env::var_os(VAR_AMBIENTE).as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// What the process remembers of the last read: the daemon checks every edit, so the file is
/// re-parsed only when its (mtime, length) changes.
struct Memo {
    caminho: PathBuf,
    assinatura: Option<(SystemTime, u64)>,
    cache: Arc<Cache>,
    erro: Option<String>,
}

static MEMO: LazyLock<Mutex<Option<Memo>>> = LazyLock::new(|| Mutex::new(None));

/// The cache at the default path, memoized by (mtime, length). The second element is why it
/// could not be read (the cache is then empty), never a silent empty.
pub fn padrao() -> (Arc<Cache>, Option<String>) {
    let Some(caminho) = caminho_padrao() else {
        return (
            Arc::new(Cache::default()),
            Some("sem HOME nem TOURING_VGP_CDN_CACHE: caminho do cache indefinido".into()),
        );
    };
    let assinatura = std::fs::metadata(&caminho)
        .ok()
        .and_then(|m| Some((m.modified().ok()?, m.len())));
    let mut memo = MEMO.lock();
    if let Some(m) = memo.as_ref()
        && m.caminho == caminho
        && m.assinatura == assinatura
    {
        return (Arc::clone(&m.cache), m.erro.clone());
    }
    let (cache, erro) = match Cache::carregar(&caminho) {
        Ok(c) => (c, None),
        Err(e) => (Cache::vazio_de(Some(caminho.clone())), Some(e)),
    };
    let cache = Arc::new(cache);
    *memo = Some(Memo {
        caminho,
        assinatura,
        cache: Arc::clone(&cache),
        erro: erro.clone(),
    });
    (cache, erro)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The shape touring-4f wrote into `corpus_semeado/cdn_cache.json` (copied, trimmed).
    const PLANO_4F: &str = r#"{
 "https://cdn.jsdelivr.net/npm/bootstrap@4.3.1/dist/css/bootstrap.min.css": {
  "status": 200, "url": "https://cdn.jsdelivr.net/npm/bootstrap@4.3.1/dist/css/bootstrap.min.css", "verdict": "existe"},
 "https://cdn.jsdelivr.net/npm/bootstrap@4.3.99/dist/css/bootstrap.min.css": {
  "status": 404, "url": "https://cdn.jsdelivr.net/npm/bootstrap@4.3.99/dist/css/bootstrap.min.css", "verdict": "404"},
 "https://cdn.jsdelivr.net/npm/lenta@1/x.js": {"status": null, "url": "u", "verdict": "indeterminado", "erro": "TimeoutError"}
}"#;

    #[test]
    fn le_o_formato_plano_do_corpus() {
        let c = Cache::de_json(PLANO_4F).unwrap();
        assert_eq!(c.len(), 3);
        let r = c
            .buscar("https://cdn.jsdelivr.net/npm/bootstrap@4.3.99/dist/css/bootstrap.min.css")
            .unwrap();
        assert_eq!(r.verdict, Veredito::NaoExiste);
        assert_eq!(r.status, Some(404));
        let lenta = c
            .buscar("https://cdn.jsdelivr.net/npm/lenta@1/x.js")
            .unwrap();
        assert_eq!(lenta.verdict, Veredito::Indeterminado);
        assert_eq!(lenta.status, None);
    }

    #[test]
    fn formato_um_vai_e_volta_com_todos_os_campos() {
        let mut c = Cache::default();
        c.inserir(
            "https://cdn.jsdelivr.net/npm/a@1/x.js",
            Registro {
                verdict: Veredito::Existe,
                status: Some(200),
                resolvida: Some("1.2.3".into()),
                causa: None,
                integrity: vec!["sha256-AAA=".into()],
                verificado_em: Some(1_791_600_000),
                erro: None,
                classes: Some(vec!["a".into(), "b".into()]),
                padroes: vec![("^=".into(), "x-".into())],
                imports: vec!["base.css".into()],
            },
        );
        c.inserir(
            "https://cdn.jsdelivr.net/npm/a@9/x.js",
            Registro {
                causa: Some(Causa::VersaoInexistente),
                ..Registro::novo(Veredito::NaoExiste)
            },
        );
        let json = c.para_json();
        assert!(json.contains("\"formato\": 1"), "{json}");
        assert!(json.contains("\"versao_inexistente\""), "{json}");
        let volta = Cache::de_json(&json).unwrap();
        assert_eq!(volta.urls, c.urls);
    }

    #[test]
    fn veredito_e_causa_desconhecidos_nao_derrubam_o_cache() {
        let c = Cache::de_json(
            r#"{"formato":1,"urls":{"https://unpkg.com/a@1.0.0/x.js":
                {"verdict":"talvez","causa":"marte"}}}"#,
        )
        .unwrap();
        let r = c.buscar("https://unpkg.com/a@1.0.0/x.js").unwrap();
        assert_eq!(r.verdict, Veredito::Indeterminado);
        assert_eq!(r.causa, Some(Causa::Outra));
    }

    #[test]
    fn json_invalido_e_erro_com_motivo() {
        assert!(Cache::de_json("{").unwrap_err().contains("JSON inválido"));
        assert!(Cache::de_json("[]").unwrap_err().contains("objeto"));
        assert!(
            Cache::de_json(r#"{"formato":1,"urls":{"u":{"verdict":42}}}"#)
                .unwrap_err()
                .contains("malformado")
        );
    }

    #[test]
    fn registro_malformado_no_formato_plano_e_descartado_sem_perder_os_outros() {
        let c = Cache::de_json(
            r#"{"https://unpkg.com/a@1.0.0/x.js":{"verdict":"existe"},
                "https://unpkg.com/b@1.0.0/x.js":"lixo"}"#,
        )
        .unwrap();
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn normaliza_unifica_esquema_host_query_e_fragmento() {
        let base = "https://cdn.jsdelivr.net/npm/a@1.0.0/x.js";
        for variante in [
            "https://cdn.jsdelivr.net/npm/a@1.0.0/x.js",
            "http://cdn.jsdelivr.net/npm/a@1.0.0/x.js",
            "//cdn.jsdelivr.net/npm/a@1.0.0/x.js",
            "https://CDN.JSDELIVR.NET/npm/a@1.0.0/x.js",
            "https://cdn.jsdelivr.net/npm/a@1.0.0/x.js?v=3",
            "https://cdn.jsdelivr.net/npm/a@1.0.0/x.js#frag",
            "  https://cdn.jsdelivr.net/npm/a@1.0.0/x.js  ",
        ] {
            assert_eq!(normaliza(variante), base, "{variante}");
        }
        // o caminho é sensível a caixa (npm distingue `Leaflet` de `leaflet`)
        assert_eq!(
            normaliza("https://cdn.jsdelivr.net/npm/Leaflet@1/x.js"),
            "https://cdn.jsdelivr.net/npm/Leaflet@1/x.js"
        );
        assert_eq!(normaliza("https://unpkg.com"), "https://unpkg.com");
    }

    #[test]
    fn buscar_acha_pela_chave_exata_e_pela_canonica() {
        let mut c = Cache::default();
        c.inserir(
            "//cdn.jsdelivr.net/npm/a@1.0.0/x.js?v=1",
            Registro::novo(Veredito::Existe),
        );
        assert!(
            c.buscar("https://cdn.jsdelivr.net/npm/a@1.0.0/x.js")
                .is_some()
        );
        assert!(c.buscar("//cdn.jsdelivr.net/npm/a@1.0.0/x.js#a").is_some());
        assert!(
            c.buscar("https://cdn.jsdelivr.net/npm/a@1.0.1/x.js")
                .is_none()
        );
        // a chave exata do formato plano (com o esquema original) vence sem passar pela canônica
        let plano =
            Cache::de_json(r#"{"http://unpkg.com/a@1.0.0/x.js":{"verdict":"404"}}"#).unwrap();
        assert!(plano.buscar("http://unpkg.com/a@1.0.0/x.js").is_some());
    }

    #[test]
    fn caminho_padrao_obedece_a_variavel_e_cai_no_home() {
        let var = Some(OsStr::new("/x/y.json"));
        let home = Some(OsStr::new("/home/u"));
        assert_eq!(
            caminho_padrao_de(var, home),
            Some(PathBuf::from("/x/y.json"))
        );
        assert_eq!(
            caminho_padrao_de(None, home),
            Some(PathBuf::from("/home/u/.claude/touring/vgp/cdn_cache.json"))
        );
        assert_eq!(
            caminho_padrao_de(Some(OsStr::new("")), home),
            Some(PathBuf::from("/home/u/.claude/touring/vgp/cdn_cache.json")),
            "variável vazia não vale"
        );
        assert_eq!(caminho_padrao_de(None, None), None);
        assert_eq!(caminho_padrao_de(None, Some(OsStr::new(""))), None);
    }

    #[test]
    fn carregar_arquivo_ausente_e_cache_vazio_e_malformado_e_erro() {
        let dir = tempfile::tempdir().unwrap();
        let ausente = dir.path().join("nao_existe.json");
        let c = Cache::carregar(&ausente).unwrap();
        assert!(c.is_empty());
        assert_eq!(c.origem(), Some(ausente.as_path()));

        let ruim = dir.path().join("ruim.json");
        std::fs::write(&ruim, "{ nao e json").unwrap();
        let e = Cache::carregar(&ruim).unwrap_err();
        assert!(
            e.contains("ruim.json") && e.contains("JSON inválido"),
            "{e}"
        );
    }

    #[test]
    fn guardar_e_atomico_cria_a_pasta_e_nao_deixa_temporario() {
        let dir = tempfile::tempdir().unwrap();
        let alvo = dir.path().join("a/b/cache.json");
        let mut c = Cache::default();
        c.inserir(
            "https://unpkg.com/a@1.0.0/x.js",
            Registro::novo(Veredito::Existe),
        );
        c.guardar(&alvo).unwrap();
        let lido = Cache::carregar(&alvo).unwrap();
        assert_eq!(lido.len(), 1);
        let restos: Vec<_> = std::fs::read_dir(alvo.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(restos.is_empty(), "sobrou temporário: {restos:?}");
        // sobrescrever também é atômico
        c.inserir(
            "https://unpkg.com/b@1.0.0/x.js",
            Registro::novo(Veredito::NaoExiste),
        );
        c.guardar(&alvo).unwrap();
        assert_eq!(Cache::carregar(&alvo).unwrap().len(), 2);
    }

    #[test]
    fn so_o_404_com_carimbo_antigo_vence() {
        let agora = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        let mk = |v, t| Registro {
            verificado_em: t,
            ..Registro::novo(v)
        };
        let velho = 2_000_000_000 - TTL_404_S - 1;
        let fresco = 2_000_000_000 - TTL_404_S + 1;
        assert!(mk(Veredito::NaoExiste, Some(velho)).e_404_vencido(agora));
        assert!(!mk(Veredito::NaoExiste, Some(fresco)).e_404_vencido(agora));
        assert!(
            !mk(Veredito::NaoExiste, None).e_404_vencido(agora),
            "sem carimbo (corpus plano) o 404 vale"
        );
        assert!(
            !mk(Veredito::Existe, Some(velho)).e_404_vencido(agora),
            "só o 404 vence: um `existe` antigo não nega nada"
        );
    }
}
