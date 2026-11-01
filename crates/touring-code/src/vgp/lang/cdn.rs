//! Transversal check `cdn` / `cdn_pin` (VCDN, VL0 §4): the CDN URLs of an html document or of a
//! JS/TS `import`, judged against a LOCAL cache.
//!
//! Layer 1 is pure (VL0 §3): no daemon, no network, no process. A `404` therefore cannot be
//! observed here — it is READ from the cache ([`cache`], default `~/.claude/touring/vgp/cdn_cache.json`)
//! that the network side ([`resolvedor`]) filled earlier. What the check concludes, per URL:
//!
//! | situation | result |
//! |---|---|
//! | cache says `404` (fresh) | `cdn`, [`Alvo::Externo`] — deny by policy §5.1 |
//! | `integrity` of the tag ≠ the digest the cache holds (strongest algorithm, as the browser) | `cdn` (exact version: `Externo`; floating: `Incerto`) |
//! | version not exact (`@4.3`, `@latest`, none, a `/gh/` branch) | `cdn_pin` — advisory, with the exact version the cache resolved |
//! | URL not in the cache / `indeterminado` / `http_erro` / a `404` older than 24 h | **not measured**, the reason names [`resolvedor::preencher`] |
//!
//! "Not measured" is never "clean": the reason travels in [`Verificacao::nao_medido`]. A timeout
//! is never a `404`. URLs are read from `src`/`href` attributes, inline module `import`s,
//! `<style>` text, `type=importmap` scripts and the `import`/`export … from`/`import()`/`.src =`
//! forms of JS, TS and TSX — never from comments or running text.

pub mod cache;
pub(crate) mod documento;
mod hash;
pub mod resolvedor;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::SystemTime;

use regex::Regex;
use tree_sitter::Node;

use self::cache::{Cache, Causa, Registro, Veredito};
use self::documento::{Bloco, Documento};
use super::{Transversal, Verificacao, parse};
use crate::vgp::politica::Alvo;
use crate::vgp::{CheckInput, Finding};

/// How the layer-1 reason names the network resolver.
const RESOLVEDOR: &str = "vgp::lang::cdn::resolvedor::preencher";

/// The hosts the check knows (a cheap substring test gates the whole parse).
const HOSTS: [&str; 3] = ["cdn.jsdelivr.net", "unpkg.com", "cdnjs.cloudflare.com"];

/// SRI algorithms, weakest first — the browser keeps only the strongest one present.
const ALGORITMOS: [&str; 3] = ["sha256", "sha384", "sha512"];

/// How many known-good versions a `404` teaches.
const MAX_VERSOES_REAIS: usize = 5;

/// CDN URLs anywhere in free text (a `<style>` body, an import map).
static URL_EM_TEXTO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:https?:)?//(?:cdn\.jsdelivr\.net|unpkg\.com|cdnjs\.cloudflare\.com)/[^\s'"()<>;\\]+"#,
    )
    .expect("regex constante")
});

/// The `cdn` / `cdn_pin` transversal check.
pub struct Cdn;

/// Which CDN serves the URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Servidor {
    /// `cdn.jsdelivr.net` (`/npm/` and `/gh/`).
    Jsdelivr,
    /// `unpkg.com`.
    Unpkg,
    /// `cdnjs.cloudflare.com` (`/ajax/libs/<lib>/<version>/<file>`).
    Cdnjs,
}

/// Which registry a jsDelivr/unpkg package comes from (cdnjs carries `Npm` and ignores it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origem {
    /// npm.
    Npm,
    /// GitHub (`/gh/user/repo@tag`).
    Gh,
}

/// A CDN URL taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlCdn {
    /// The CDN.
    pub servidor: Servidor,
    /// The registry.
    pub origem: Origem,
    /// `name`, `@scope/name`, `user/repo`, or the cdnjs library.
    pub pacote: String,
    /// The version specifier as written (`4.3.1`, `4.3`, `latest`, `^4`); `None` when absent.
    pub versao: Option<String>,
    /// The file path after the package (may be empty).
    pub arquivo: String,
}

/// `name@version` split at the LAST `@` after the first character (`@scope` is not a version).
fn separa(segmento: &str) -> (&str, Option<&str>) {
    match segmento.rfind('@') {
        Some(i) if i > 0 => (
            &segmento[..i],
            Some(&segmento[i + 1..]).filter(|v| !v.is_empty()),
        ),
        _ => (segmento, None),
    }
}

/// `(package, version, file)` out of what follows `/npm/`, `/gh/` or the unpkg root.
fn pacote_versao_arquivo(resto: &str, origem: Origem) -> Option<(String, Option<String>, String)> {
    let mut segmentos = resto.split('/');
    let primeiro = segmentos.next()?;
    let (nome, versao) = match origem {
        Origem::Npm if primeiro.starts_with('@') => {
            let (n, v) = separa(segmentos.next()?);
            (format!("{primeiro}/{n}"), v)
        }
        Origem::Npm => {
            let (n, v) = separa(primeiro);
            (n.to_string(), v)
        }
        Origem::Gh => {
            let (n, v) = separa(segmentos.next()?);
            (format!("{primeiro}/{n}"), v)
        }
    };
    if nome.is_empty() || nome.ends_with('/') || nome.starts_with('/') {
        return None;
    }
    let arquivo = segmentos.collect::<Vec<_>>().join("/");
    Some((nome, versao.map(str::to_string), arquivo))
}

fn parse_jsdelivr(caminho: &str) -> Option<UrlCdn> {
    let (registro, resto) = caminho.split_once('/')?;
    let origem = match registro {
        "npm" => Origem::Npm,
        "gh" => Origem::Gh,
        _ => return None,
    };
    let (pacote, versao, arquivo) = pacote_versao_arquivo(resto, origem)?;
    Some(UrlCdn {
        servidor: Servidor::Jsdelivr,
        origem,
        pacote,
        versao,
        arquivo,
    })
}

fn parse_unpkg(caminho: &str) -> Option<UrlCdn> {
    if matches!(caminho.split('/').next(), Some("browse" | "-")) {
        return None;
    }
    let (pacote, versao, arquivo) = pacote_versao_arquivo(caminho, Origem::Npm)?;
    Some(UrlCdn {
        servidor: Servidor::Unpkg,
        origem: Origem::Npm,
        pacote,
        versao,
        arquivo,
    })
}

fn parse_cdnjs(caminho: &str) -> Option<UrlCdn> {
    let mut segmentos = caminho.strip_prefix("ajax/libs/")?.split('/');
    let pacote = segmentos.next().filter(|s| !s.is_empty())?;
    let versao = segmentos.next().filter(|s| !s.is_empty())?;
    Some(UrlCdn {
        servidor: Servidor::Cdnjs,
        origem: Origem::Npm,
        pacote: pacote.to_string(),
        versao: Some(versao.to_string()),
        arquivo: segmentos.collect::<Vec<_>>().join("/"),
    })
}

/// A pre-release or build suffix: non-empty dot/dash separated alphanumerics.
fn identificadores_validos(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

/// Exactly three numeric, non-empty components.
fn nucleo_semver(nucleo: &str) -> bool {
    let partes: Vec<&str> = nucleo.split('.').collect();
    partes.len() == 3
        && partes
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Is `v` a complete semantic version (`MAJOR.MINOR.PATCH[-pre][+build]`)?
fn semver_exata(v: &str) -> bool {
    let (sem_build, build) = v.split_once('+').map_or((v, None), |(a, b)| (a, Some(b)));
    let (nucleo, pre) = sem_build
        .split_once('-')
        .map_or((sem_build, None), |(a, b)| (a, Some(b)));
    build.is_none_or(identificadores_validos)
        && pre.is_none_or(identificadores_validos)
        && nucleo_semver(nucleo)
}

/// A git commit id written in full or abbreviated (7–40 hex digits).
fn e_commit(v: &str) -> bool {
    (7..=40).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_hexdigit())
}

impl UrlCdn {
    /// Take a CDN URL apart. `None` for anything that is not a jsDelivr (`/npm/`, `/gh/`), unpkg
    /// or cdnjs (`/ajax/libs/`) package URL. The scheme may be `https`, `http` or absent
    /// (`//host/…`); query and fragment are ignored.
    pub fn parse(url: &str) -> Option<Self> {
        let canonica = cache::normaliza(url);
        let resto = canonica.strip_prefix("https://")?;
        let (host, caminho) = resto.split_once('/')?;
        match host {
            "cdn.jsdelivr.net" => parse_jsdelivr(caminho),
            "unpkg.com" => parse_unpkg(caminho),
            "cdnjs.cloudflare.com" => parse_cdnjs(caminho),
            _ => None,
        }
    }

    /// Does the URL pin ONE immutable release? npm: a full semver. `/gh/`: a semver tag
    /// (`v1.2.3` too) or a commit id — never a branch. cdnjs always (the path carries it).
    pub fn versao_exata(&self) -> bool {
        if self.servidor == Servidor::Cdnjs {
            return true;
        }
        let Some(v) = self.versao.as_deref() else {
            return false;
        };
        match self.origem {
            Origem::Npm => semver_exata(v),
            Origem::Gh => semver_exata(v.strip_prefix('v').unwrap_or(v)) || e_commit(v),
        }
    }

    fn montar(&self, versao: Option<&str>) -> String {
        let com_arroba = |pacote: &str| match versao {
            Some(v) => format!("{pacote}@{v}"),
            None => pacote.to_string(),
        };
        let base = match (self.servidor, self.origem) {
            (Servidor::Jsdelivr, Origem::Npm) => {
                format!("https://cdn.jsdelivr.net/npm/{}", com_arroba(&self.pacote))
            }
            (Servidor::Jsdelivr, Origem::Gh) => {
                format!("https://cdn.jsdelivr.net/gh/{}", com_arroba(&self.pacote))
            }
            (Servidor::Unpkg, _) => format!("https://unpkg.com/{}", com_arroba(&self.pacote)),
            (Servidor::Cdnjs, _) => format!(
                "https://cdnjs.cloudflare.com/ajax/libs/{}/{}",
                self.pacote,
                versao.unwrap_or_default()
            ),
        };
        if self.arquivo.is_empty() {
            base
        } else {
            format!("{base}/{}", self.arquivo)
        }
    }

    /// The same URL with `versao` as its version — `https`, no query, no fragment.
    pub fn com_versao(&self, versao: &str) -> String {
        self.montar(Some(versao))
    }

    /// The URL rebuilt from its parts with the version it already has.
    pub fn url_canonica(&self) -> String {
        self.montar(self.versao.as_deref())
    }

    /// The CDN's display name.
    fn nome_do_servidor(&self) -> &'static str {
        match self.servidor {
            Servidor::Jsdelivr => "jsDelivr",
            Servidor::Unpkg => "unpkg",
            Servidor::Cdnjs => "cdnjs",
        }
    }
}

// ---------------------------------------------------------------------------- integrity (SRI)

fn rank_do_algoritmo(nome: &str) -> Option<usize> {
    ALGORITMOS.iter().position(|a| a.eq_ignore_ascii_case(nome))
}

/// base64 and base64url, with or without padding, compare equal.
fn normaliza_b64(b64: &str) -> String {
    b64.trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            outro => outro,
        })
        .collect()
}

/// `(algorithm rank, normalised digest)` of every valid token of an `integrity` value. Tokens
/// with an unknown algorithm are dropped — the browser ignores them — and `?options` is cut.
fn tokens_sri(texto: &str) -> Vec<(usize, String)> {
    texto
        .split_whitespace()
        .filter_map(|token| {
            let token = token.split('?').next()?;
            let (algoritmo, b64) = token.split_once('-')?;
            Some((rank_do_algoritmo(algoritmo)?, normaliza_b64(b64)))
        })
        .collect()
}

/// The strongest SRI algorithm the tag carries, if it carries any valid token.
fn algoritmo_mais_forte(tag: &str) -> Option<&'static str> {
    let forte = tokens_sri(tag).into_iter().map(|(r, _)| r).max()?;
    ALGORITMOS.get(forte).copied()
}

/// Does the tag's `integrity` match the digests the cache holds? Like a browser: only the
/// strongest algorithm present counts, and any of its hashes may match. `None` = cannot judge
/// (no valid token in the tag, or the cache has no digest of that algorithm).
pub(crate) fn integrity_confere(tag: &str, do_cache: &[String]) -> Option<bool> {
    let da_tag = tokens_sri(tag);
    let forte = da_tag.iter().map(|(r, _)| *r).max()?;
    let esperados: Vec<String> = tokens_sri(&do_cache.join(" "))
        .into_iter()
        .filter(|(r, _)| *r == forte)
        .map(|(_, b)| b)
        .collect();
    if esperados.is_empty() {
        return None;
    }
    Some(
        da_tag
            .iter()
            .filter(|(r, _)| *r == forte)
            .any(|(_, b)| esperados.contains(b)),
    )
}

/// The cache's own token for the strongest algorithm of `tag` — what the tag should say.
fn digest_certo(tag: &str, do_cache: &[String]) -> Option<String> {
    let forte = tokens_sri(tag).into_iter().map(|(r, _)| r).max()?;
    do_cache
        .iter()
        .find(|t| {
            t.split_once('-')
                .and_then(|(a, _)| rank_do_algoritmo(a))
                .is_some_and(|r| r == forte)
        })
        .cloned()
}

// ---------------------------------------------------------------------------- where URLs live

/// One CDN URL in the document.
struct Ocorrencia {
    /// As written (kept as the finding's name, scheme and all).
    url: String,
    /// 1-based line.
    linha: usize,
    /// The `integrity` of the same tag.
    integrity: Option<String>,
}

/// What was read out of a document: the URLs, and the parts that could not be read.
#[derive(Default)]
struct Achadas {
    itens: Vec<Ocorrencia>,
    notas: Vec<String>,
}

fn tem_host_de_cdn(texto: &str) -> bool {
    HOSTS.iter().any(|h| texto.contains(h))
}

/// Every CDN URL of `texto`, with the line it starts on.
fn urls_no_texto(texto: &str, linha_base: usize, out: &mut Vec<Ocorrencia>) {
    for m in URL_EM_TEXTO.find_iter(texto) {
        if UrlCdn::parse(m.as_str()).is_some() {
            out.push(Ocorrencia {
                url: m.as_str().to_string(),
                linha: linha_base + texto[..m.start()].matches('\n').count(),
                integrity: None,
            });
        }
    }
}

/// The text of a `"…"`/`'…'` JS string literal without escapes (an escaped URL is not read).
fn literal<'a>(no: Node, fonte: &'a str) -> Option<&'a str> {
    if no.kind() != "string" {
        return None;
    }
    let bruto = no.utf8_text(fonte.as_bytes()).ok()?;
    let interior = bruto.get(1..bruto.len().checked_sub(1)?)?;
    (!interior.contains('\\')).then_some(interior)
}

/// The string node a statement hands a URL through: `import … from "u"`, `export … from "u"`,
/// `import("u")`, `x.src = "u"` / `x.href = "u"`.
fn string_candidata<'t>(no: Node<'t>, fonte: &str) -> Option<Node<'t>> {
    match no.kind() {
        "import_statement" | "export_statement" => no.child_by_field_name("source"),
        "call_expression" => {
            if no.child_by_field_name("function")?.kind() != "import" {
                return None;
            }
            no.child_by_field_name("arguments")?.named_child(0)
        }
        "assignment_expression" => {
            let esquerda = no.child_by_field_name("left")?;
            if esquerda.kind() != "member_expression" {
                return None;
            }
            let propriedade = esquerda.child_by_field_name("property")?;
            let nome = propriedade.utf8_text(fonte.as_bytes()).ok()?;
            matches!(nome, "src" | "href").then(|| no.child_by_field_name("right"))?
        }
        _ => None,
    }
}

/// CDN URLs in JS/TS/TSX `fonte`, whose first line is `linha_base` of the whole document.
fn ocorrencias_js(
    lang: &str,
    fonte: &str,
    linha_base: usize,
    out: &mut Vec<Ocorrencia>,
) -> Result<(), String> {
    let arvore = parse(lang, fonte)?;
    let mut cursor = arvore.walk();
    loop {
        let no = cursor.node();
        if let Some(string) = string_candidata(no, fonte)
            && let Some(url) = literal(string, fonte)
            && UrlCdn::parse(url).is_some()
        {
            out.push(Ocorrencia {
                url: url.to_string(),
                linha: linha_base + string.start_position().row,
                integrity: None,
            });
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Ok(());
            }
        }
    }
}

fn escanear_script(doc: &Documento, bloco: &Bloco, achadas: &mut Achadas) {
    let tipo = doc
        .elemento_de(bloco)
        .atributo("type")
        .map(|t| t.valor.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if tipo == "importmap" {
        urls_no_texto(&bloco.texto, bloco.linha, &mut achadas.itens);
    } else if documento::e_script_js(&tipo)
        && let Err(e) = ocorrencias_js("javascript", &bloco.texto, bloco.linha, &mut achadas.itens)
    {
        achadas
            .notas
            .push(format!("script inline da linha {}: {e}", bloco.linha));
    }
}

fn ocorrencias_html(fonte: &str) -> Result<Achadas, String> {
    let doc = documento::ler(fonte)?;
    let mut achadas = Achadas::default();
    for elemento in &doc.elementos {
        let integrity = elemento.atributo("integrity").map(|a| a.valor.clone());
        for atributo in &elemento.atributos {
            let valor = atributo.valor.trim();
            if matches!(atributo.nome.as_str(), "src" | "href") && UrlCdn::parse(valor).is_some() {
                achadas.itens.push(Ocorrencia {
                    url: valor.to_string(),
                    linha: atributo.linha,
                    integrity: integrity.clone(),
                });
            }
        }
    }
    for bloco in &doc.scripts {
        escanear_script(&doc, bloco, &mut achadas);
    }
    for bloco in &doc.estilos {
        urls_no_texto(&bloco.texto, bloco.linha, &mut achadas.itens);
    }
    Ok(achadas)
}

/// Every CDN URL occurrence of the document, in line order.
fn ocorrencias(input: &CheckInput) -> Result<Achadas, String> {
    let mut achadas = match input.lang.as_str() {
        "html" => ocorrencias_html(&input.content)?,
        "javascript" | "typescript" | "tsx" => {
            let mut a = Achadas::default();
            ocorrencias_js(&input.lang, &input.content, 1, &mut a.itens)?;
            a
        }
        _ => Achadas::default(),
    };
    achadas.itens.sort_by_key(|o| o.linha);
    Ok(achadas)
}

/// The CDN URLs of `input`, as written — what a command hands to [`resolvedor::preencher`] to
/// fill the cache for this document.
pub fn urls_do_conteudo(input: &CheckInput) -> Vec<String> {
    if !tem_host_de_cdn(&input.content) {
        return Vec::new();
    }
    ocorrencias(input)
        .map(|a| a.itens.into_iter().map(|o| o.url).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------- the judgement

/// Numeric key of a version string for newest-first ordering.
fn chave_versao(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// Exact versions of the SAME package file that the cache knows to exist, newest first.
fn versoes_reais(cache: &Cache, u: &UrlCdn) -> Vec<String> {
    let mut versoes: Vec<String> = cache
        .registros()
        .filter(|(_, r)| r.verdict == Veredito::Existe)
        .filter_map(|(url, r)| {
            let outra = UrlCdn::parse(url)?;
            let mesma = outra.servidor == u.servidor
                && outra.origem == u.origem
                && outra.pacote == u.pacote
                && outra.arquivo == u.arquivo;
            if !mesma {
                return None;
            }
            if outra.versao_exata() {
                outra.versao
            } else {
                r.resolvida.clone()
            }
        })
        .filter(|v| Some(v.as_str()) != u.versao.as_deref())
        .collect();
    versoes.sort_by_key(|v| std::cmp::Reverse(chave_versao(v)));
    versoes.dedup();
    versoes.truncate(MAX_VERSOES_REAIS);
    versoes
}

fn achado_404(oc: &Ocorrencia, u: &UrlCdn, registro: &Registro, cache: &Cache) -> Finding {
    let reais = versoes_reais(cache, u);
    let versao = u.versao.as_deref().unwrap_or("latest");
    let detalhe = match registro.causa {
        Some(Causa::VersaoInexistente) => format!(
            "a versão `{versao}` de `{}` não existe no {}",
            u.pacote,
            u.nome_do_servidor()
        ),
        Some(Causa::ArquivoInexistente) => {
            format!(
                "o arquivo `{}` não existe em `{}@{versao}`",
                u.arquivo, u.pacote
            )
        }
        _ => format!(
            "a versão `{versao}` de `{}` ou o arquivo `{}` não existe no {}",
            u.pacote,
            u.arquivo,
            u.nome_do_servidor()
        ),
    };
    let ensina = if reais.is_empty() {
        String::new()
    } else {
        format!(" — versões que existem (no cache): {}", reais.join(", "))
    };
    Finding::candidato(
        "cdn",
        &oc.url,
        oc.linha,
        Alvo::Externo,
        format!("HTTP 404 em {}: {detalhe}{ensina}", oc.url),
        reais,
    )
}

fn achado_pin(oc: &Ocorrencia, u: &UrlCdn, resolvida: Option<&str>) -> Finding {
    let escrita = match &u.versao {
        Some(v) => format!("`@{v}`"),
        None => "sem versão".to_string(),
    };
    let fixar = match resolvida {
        Some(v) => format!("; fixe `@{v}`: {}", u.com_versao(v)),
        None => "; fixe a versão exata (o cache ainda não a resolveu)".to_string(),
    };
    Finding::candidato(
        "cdn_pin",
        &oc.url,
        oc.linha,
        Alvo::Externo,
        format!(
            "versão não exata ({escrita}) em {}: o {} serve o que ela resolver HOJE e o conteúdo \
             muda sem aviso{fixar}",
            oc.url,
            u.nome_do_servidor()
        ),
        resolvida.map(str::to_string).into_iter().collect(),
    )
}

fn achado_integrity(oc: &Ocorrencia, u: &UrlCdn, registro: &Registro, tag: &str) -> Finding {
    let alvo = if u.versao_exata() {
        Alvo::Externo
    } else {
        Alvo::Incerto
    };
    let certo = digest_certo(tag, &registro.integrity)
        .map(|d| format!("; o digest que o cache registrou é `{d}`"))
        .unwrap_or_default();
    Finding::candidato(
        "cdn",
        &oc.url,
        oc.linha,
        alvo,
        format!(
            "o `integrity` do tag não confere com o arquivo que o {} serve em {}: o navegador \
             recusa o recurso{certo}",
            u.nome_do_servidor(),
            oc.url
        ),
        Vec::new(),
    )
}

/// Why a record carries no verdict worth acting on.
fn sem_veredito(oc: &Ocorrencia, registro: &Registro) -> String {
    let detalhe = registro
        .erro
        .clone()
        .or_else(|| registro.status.map(|s| format!("HTTP {s}")))
        .unwrap_or_else(|| "sem detalhe".to_string());
    format!(
        "CDN: {} sem veredito no cache ({detalhe}); `{RESOLVEDOR}` consulta de novo",
        oc.url
    )
}

/// The verdicts about every occurrence of one document, accumulated.
struct Avaliacao<'c> {
    cache: &'c Cache,
    agora: SystemTime,
    achados: Vec<Finding>,
    /// (class, name) already reported: a URL repeated in the page is one finding.
    vistos: HashSet<(String, String)>,
    /// URLs the cache knows nothing about.
    fora: Vec<String>,
    /// Other things that stayed unmeasured, one reason each.
    outros: Vec<String>,
}

impl<'c> Avaliacao<'c> {
    fn novo(cache: &'c Cache, agora: SystemTime) -> Self {
        Self {
            cache,
            agora,
            achados: Vec::new(),
            vistos: HashSet::new(),
            fora: Vec::new(),
            outros: Vec::new(),
        }
    }

    fn emitir(&mut self, achado: Finding) {
        if self
            .vistos
            .insert((achado.classe.clone(), achado.nome.clone()))
        {
            self.achados.push(achado);
        }
    }

    fn avisar_pin(&mut self, oc: &Ocorrencia, u: &UrlCdn, resolvida: Option<&str>) {
        if !u.versao_exata() {
            self.emitir(achado_pin(oc, u, resolvida));
        }
    }

    fn avaliar(&mut self, oc: &Ocorrencia) {
        let Some(u) = UrlCdn::parse(&oc.url) else {
            return;
        };
        let cache = self.cache;
        let registro = cache.buscar(&oc.url);
        // a 404 dominates: the floating-version advice is moot when nothing resolves
        if let Some(r) = registro.filter(|r| r.verdict == Veredito::NaoExiste) {
            self.avaliar_404(oc, &u, r);
            return;
        }
        self.avisar_pin(oc, &u, registro.and_then(|r| r.resolvida.as_deref()));
        match registro {
            None => self.fora.push(oc.url.clone()),
            Some(r) => self.avaliar_registro(oc, &u, r),
        }
    }

    fn avaliar_404(&mut self, oc: &Ocorrencia, u: &UrlCdn, registro: &Registro) {
        if registro.e_404_vencido(self.agora) {
            self.outros.push(format!(
                "CDN: o 404 de {} no cache tem mais de 24 h e não pode negar; `{RESOLVEDOR}` o reconfirma",
                oc.url
            ));
            self.avisar_pin(oc, u, None);
        } else {
            self.emitir(achado_404(oc, u, registro, self.cache));
        }
    }

    fn avaliar_registro(&mut self, oc: &Ocorrencia, u: &UrlCdn, registro: &Registro) {
        match registro.verdict {
            Veredito::Existe => self.conferir_integrity(oc, u, registro),
            Veredito::HttpErro | Veredito::Indeterminado => {
                self.outros.push(sem_veredito(oc, registro));
            }
            Veredito::NaoExiste => {}
        }
    }

    fn conferir_integrity(&mut self, oc: &Ocorrencia, u: &UrlCdn, registro: &Registro) {
        let Some(tag) = &oc.integrity else { return };
        let Some(forte) = algoritmo_mais_forte(tag) else {
            return;
        };
        match integrity_confere(tag, &registro.integrity) {
            Some(true) => {}
            Some(false) => self.emitir(achado_integrity(oc, u, registro, tag)),
            None => self.outros.push(format!(
                "CDN: integrity de {}: o cache não tem o digest {forte} dessa URL; `{RESOLVEDOR}` o grava",
                oc.url
            )),
        }
    }

    /// Why part of the document stayed unmeasured, or `None` when all of it was.
    fn motivo(&self, notas: &[String]) -> Option<String> {
        let mut partes = Vec::new();
        if let Some(primeira) = self.fora.first() {
            let onde = self.cache.origem().map_or_else(
                || "(cache sem arquivo)".to_string(),
                |p| p.display().to_string(),
            );
            partes.push(format!(
                "CDN: {} URL(s) fora do cache `{onde}` (ex.: {primeira}); a camada 1 não usa rede — \
                 `{RESOLVEDOR}` as consulta e grava no cache",
                self.fora.len()
            ));
        }
        partes.extend(self.outros.iter().cloned());
        partes.extend(notas.iter().cloned());
        (!partes.is_empty()).then(|| partes.join("; "))
    }
}

impl Cdn {
    /// The pure core: judge the CDN URLs of `input` against `cache` at time `agora`. Findings are
    /// `cdn` / `cdn_pin`; whatever the cache could not answer comes back in
    /// [`Verificacao::nao_medido`], naming [`resolvedor::preencher`].
    pub fn medir_com(&self, input: &CheckInput, cache: &Cache, agora: SystemTime) -> Verificacao {
        if !tem_host_de_cdn(&input.content) {
            return Verificacao::medido(Vec::new());
        }
        let achadas = match ocorrencias(input) {
            Ok(a) => a,
            Err(motivo) => return Verificacao::nao_medido(format!("CDN: {motivo}")),
        };
        let mut avaliacao = Avaliacao::novo(cache, agora);
        for oc in &achadas.itens {
            avaliacao.avaliar(oc);
        }
        let nao_medido = avaliacao.motivo(&achadas.notas);
        Verificacao {
            achados: avaliacao.achados,
            nao_medido,
        }
    }

    /// [`Cdn::medir_com`] against the default cache ([`cache::padrao`]) and the real clock. An
    /// unreadable cache file is reported, never silently treated as empty.
    pub fn medir(&self, input: &CheckInput) -> Verificacao {
        if !tem_host_de_cdn(&input.content) {
            return Verificacao::medido(Vec::new());
        }
        let (cache, erro) = cache::padrao();
        let mut v = self.medir_com(input, &cache, SystemTime::now());
        if let Some(erro) = erro {
            let aviso = format!("CDN: cache ilegível — {erro}");
            v.nao_medido = Some(match v.nao_medido {
                Some(outro) => format!("{aviso}; {outro}"),
                None => aviso,
            });
        }
        v
    }
}

impl Transversal for Cdn {
    fn nome(&self) -> &'static str {
        "cdn"
    }

    fn langs(&self) -> &'static [&'static str] {
        &["html", "javascript", "typescript", "tsx"]
    }

    /// [`Cdn::medir`]: the findings and the reason for whatever stayed unmeasured.
    fn verificar(&self, input: &CheckInput) -> Verificacao {
        self.medir(input)
    }
}
