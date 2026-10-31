//! Class `recurso` (VL0 §4): a LOCAL `src` / `href` that does not exist on disk.
//!
//! References are resolved the way a browser opening the file would: relative to the document's
//! directory (or to a local `<base href>`), with the `?query` and `#fragment` dropped and `%XX`
//! decoded. A root-relative reference (`/css/a.css`) depends on where the page is served from, which
//! the file cannot say: it is looked for in every ancestor directory of the document up to the
//! project root, in the usual web roots (`public/`, `static/`, `dist/`…) and as an absolute path,
//! and a miss is `Alvo::Incerto`. A relative miss is `Alvo::Projeto`: the disk decides, the file
//! is the project's and is absent. Whether it may ever `deny` is the POLICY's call, not this
//! label's: `recurso` is in `CLASSES_CONSULTIVAS` (the next write creates the file — the
//! `index.html` goes out before the `style.css` it links — and spec §5.1 reserves the "runs now"
//! deny for symbol classes), so it is a `warn` even in `/tmp/`.
//!
//! Out of scope: absolute URLs (`http:`, `data:`, `mailto:`…, protocol-relative `//host`) — a CDN
//! URL is `cdn`'s, not this class's — and templated values, which are counted as not measured.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use super::Saida;
use super::documento::{Documento, Elemento};
use super::texto::{decodificar_percentual, mais_proximos, tem_template};
use crate::vgp::politica::Alvo;
use crate::vgp::{CheckInput, Finding, is_inline};

/// Web roots a root-relative URL commonly maps onto, tried under each ancestor of the document.
const RAIZES_WEB: &[&str] = &[
    "public", "static", "dist", "build", "out", "www", "wwwroot", "htdocs", "site", "_site",
    "docs", "web",
];

/// `<a href>` / `<area href>` count only when the target looks like a FILE (a route such as
/// `/about` or `docs/` is the server's business, not the disk's).
const EXTENSOES_DE_ARQUIVO: &[&str] = &[
    "html", "htm", "xhtml", "pdf", "css", "js", "mjs", "json", "xml", "txt", "md", "csv", "png",
    "jpg", "jpeg", "gif", "svg", "webp", "avif", "ico", "bmp", "mp3", "mp4", "webm", "ogg", "wav",
    "m4a", "woff", "woff2", "ttf", "otf", "eot", "zip", "gz", "doc", "docx", "xls", "xlsx", "ppt",
    "pptx", "odt", "epub",
];

/// `<link rel>` values whose `href` is a file the page loads (not a page address such as
/// `canonical`, `alternate`, `next`).
const RELS_DE_ARQUIVO: &[&str] = &[
    "stylesheet",
    "icon",
    "apple-touch-icon",
    "apple-touch-icon-precomposed",
    "manifest",
    "preload",
    "prefetch",
    "modulepreload",
    "mask-icon",
];

/// A local reference found in the document.
struct Referencia {
    /// As written, without `?query` / `#fragment`.
    escrito: String,
    /// 1-based line of the attribute.
    linha: usize,
    /// `img src`, `link href`…
    origem: String,
}

/// What one attribute value is.
enum Valor {
    Local(String),
    /// Templated or computed: cannot be resolved from the file.
    Dinamico,
    /// Absolute URL, fragment, empty: not this class's business.
    Ignorado,
}

/// Report every local reference of `doc` that resolves to nothing.
pub(super) fn verificar(input: &CheckInput, doc: &Documento, saida: &mut Saida) {
    let (referencias, dinamicas) = colher(doc);
    if dinamicas > 0 {
        saida.nao_medido(format!(
            "{dinamicas} referência(s) com valor dinâmico ou de template não resolvida(s)"
        ));
    }
    if referencias.is_empty() {
        return;
    }
    let Some(mut disco) = Disco::de(input, doc) else {
        saida.nao_medido(format!(
            "caminho `{}` sem diretório: {} referência(s) local(is) não resolvida(s)",
            input.path,
            referencias.len()
        ));
        return;
    };
    if let Some(motivo) = disco.motivo_de_nao_medir.take() {
        saida.nao_medido(motivo);
        return;
    }
    for (escrito, grupo) in agrupar(&referencias) {
        if let Resolucao::Ausente { alvo, resolvido } = disco.resolver(escrito) {
            let vizinhos = disco.vizinhos(&resolvido);
            saida.achado(achado(escrito, &grupo, alvo, &resolvido, vizinhos));
        }
    }
}

/// Every local reference of the document, plus how many dynamic ones were skipped.
fn colher(doc: &Documento) -> (Vec<Referencia>, usize) {
    let mut referencias = Vec::new();
    let mut dinamicas = 0;
    for el in &doc.elementos {
        for (atributo, valor) in valores_de_recurso(el) {
            let linha = el.atributo(atributo).map_or(el.linha, |a| a.linha);
            match classificar(&valor, el) {
                Valor::Local(escrito) => referencias.push(Referencia {
                    escrito,
                    linha,
                    origem: format!("{} {atributo}", el.tag),
                }),
                Valor::Dinamico => dinamicas += 1,
                Valor::Ignorado => {}
            }
        }
    }
    (referencias, dinamicas)
}

/// The attributes of `el` that name a file the page loads.
fn atributos_de_recurso(el: &Elemento) -> &'static [&'static str] {
    match el.tag.as_str() {
        "script" | "iframe" | "embed" | "audio" | "track" | "frame" => &["src"],
        "img" | "source" => &["src", "srcset"],
        "video" => &["src", "poster"],
        "object" => &["data"],
        "input"
            if el
                .valor("type")
                .is_some_and(|t| t.eq_ignore_ascii_case("image")) =>
        {
            &["src"]
        }
        "link" if link_de_arquivo(el) => &["href"],
        "a" | "area" => &["href"],
        _ => &[],
    }
}

/// The (attribute, value) pairs of `el` that name a file the page loads — one pair per URL of a
/// `srcset`.
fn valores_de_recurso(el: &Elemento) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for &nome in atributos_de_recurso(el) {
        let Some(v) = el.valor(nome) else { continue };
        if nome == "srcset" {
            out.extend(candidatos_de_srcset(v).into_iter().map(|c| (nome, c)));
        } else {
            out.push((nome, v.to_string()));
        }
    }
    out
}

fn link_de_arquivo(el: &Elemento) -> bool {
    el.valor("rel").is_some_and(|rel| {
        rel.split_ascii_whitespace()
            .any(|t| RELS_DE_ARQUIVO.contains(&t.to_ascii_lowercase().as_str()))
    })
}

/// The URLs of a `srcset` (`a.png 1x, b.png 2x`): each candidate's URL ends at the first blank,
/// so a `data:` URL with a comma stays whole.
fn candidatos_de_srcset(valor: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut resto = valor;
    loop {
        resto = resto.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if resto.is_empty() {
            return out;
        }
        let fim = resto.find(char::is_whitespace).unwrap_or(resto.len());
        let token = &resto[..fim];
        out.push(token.trim_end_matches(',').to_string());
        let depois = &resto[fim..];
        // a URL that ends with a comma has no descriptor; otherwise the descriptor runs to the next comma
        resto = if token.ends_with(',') {
            depois
        } else {
            &depois[depois.find(',').unwrap_or(depois.len())..]
        };
    }
}

fn classificar(valor: &str, el: &Elemento) -> Valor {
    let escrito: String = valor
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .trim()
        .to_string();
    // a browser reads `\` as `/` in a URL; the name keeps what was written
    let v = escrito.replace('\\', "/");
    if v.is_empty() {
        return Valor::Ignorado;
    }
    // `#frag` points INSIDE the document (class `ancora`, which counts its own templated ones);
    // `#{x}` is Pug/Ruby interpolation, not a fragment
    if v.starts_with('#') && !v.starts_with("#{") {
        return Valor::Ignorado;
    }
    if tem_template(&v) {
        return Valor::Dinamico;
    }
    if v.starts_with("//") || tem_esquema(&v) {
        return Valor::Ignorado;
    }
    let caminho = v.split(['?', '#']).next().unwrap_or("");
    if caminho.is_empty() {
        return Valor::Ignorado;
    }
    if matches!(el.tag.as_str(), "a" | "area") && !parece_arquivo(caminho) {
        return Valor::Ignorado;
    }
    let nome = escrito.split(['?', '#']).next().unwrap_or("");
    Valor::Local(nome.to_string())
}

/// `http:`, `data:`, `mailto:`, `javascript:`, `C:`… — a scheme is a letter followed by letters,
/// digits, `+`, `-`, `.` and a colon.
fn tem_esquema(v: &str) -> bool {
    let mut cs = v.chars();
    cs.next().is_some_and(|c| c.is_ascii_alphabetic())
        && v.split_once(':').is_some_and(|(esq, _)| {
            esq.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        })
}

fn parece_arquivo(caminho: &str) -> bool {
    Path::new(caminho)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSOES_DE_ARQUIVO.contains(&e.to_ascii_lowercase().as_str()))
}

/// References grouped by what is written, in order of first appearance.
fn agrupar(referencias: &[Referencia]) -> Vec<(&str, Vec<&Referencia>)> {
    let mut indice: HashMap<&str, usize> = HashMap::new();
    let mut grupos: Vec<(&str, Vec<&Referencia>)> = Vec::new();
    for r in referencias {
        let i = *indice.entry(r.escrito.as_str()).or_insert_with(|| {
            grupos.push((r.escrito.as_str(), Vec::new()));
            grupos.len() - 1
        });
        grupos[i].1.push(r);
    }
    grupos
}

/// How a reference resolves.
enum Resolucao {
    Existe,
    Ausente { alvo: Alvo, resolvido: PathBuf },
}

/// The file system as the document sees it.
struct Disco<'a> {
    /// Directory references resolve against (the document's, or a local `<base href>`'s).
    base: PathBuf,
    /// Directory of the document itself — where ancestors are counted from.
    dir_doc: PathBuf,
    raiz: &'a Path,
    overlays: &'a HashMap<String, String>,
    cache: HashMap<PathBuf, bool>,
    /// Sorted names inside each directory listed for "você quis dizer" (`None`: no such directory).
    listagens: HashMap<PathBuf, Option<Vec<String>>>,
    /// Set when the whole class is unmeasurable for this document (remote `<base href>`).
    motivo_de_nao_medir: Option<String>,
}

impl<'a> Disco<'a> {
    /// `None` for a virtual path (`<string>`, `<stdin>`) or one with no parent directory.
    fn de(input: &'a CheckInput, doc: &Documento) -> Option<Self> {
        if is_inline(&input.path) {
            return None;
        }
        let caminho = Path::new(&input.path);
        let absoluto = if caminho.is_absolute() {
            caminho.to_path_buf()
        } else {
            input.project_root.join(caminho)
        };
        let dir_doc = absoluto.parent()?.to_path_buf();
        let (base, motivo) = base_do_documento(doc, &dir_doc);
        Some(Self {
            base,
            dir_doc,
            raiz: &input.project_root,
            overlays: &input.overlays,
            cache: HashMap::new(),
            listagens: HashMap::new(),
            motivo_de_nao_medir: motivo,
        })
    }

    fn existe(&mut self, p: &Path) -> bool {
        let p = normalizar(p);
        if let Some(&b) = self.cache.get(&p) {
            return b;
        }
        let b = p.exists() || self.overlays.contains_key(p.to_string_lossy().as_ref());
        self.cache.insert(p, b);
        b
    }

    fn resolver(&mut self, escrito: &str) -> Resolucao {
        let escrito = escrito.replace('\\', "/");
        let escrito = escrito.as_str();
        let variantes: Vec<String> = std::iter::once(escrito.to_string())
            .chain(decodificar_percentual(escrito))
            .collect();
        if let Some(rel) = escrito.strip_prefix('/') {
            return self.resolver_da_raiz(rel, &variantes);
        }
        for v in &variantes {
            if self.existe(&self.base.join(v)) {
                return Resolucao::Existe;
            }
        }
        Resolucao::Ausente {
            alvo: Alvo::Projeto,
            resolvido: self.base.join(escrito),
        }
    }

    /// The names in the missing file's directory closest to it and — when that directory itself
    /// does not exist — the directory. Each directory is listed once per document.
    fn vizinhos(&mut self, resolvido: &Path) -> Vizinhos {
        let (Some(dir), Some(nome)) = (
            resolvido.parent(),
            resolvido.file_name().and_then(|n| n.to_str()),
        ) else {
            return Vizinhos::default();
        };
        let lista = self
            .listagens
            .entry(dir.to_path_buf())
            .or_insert_with(|| listar(dir));
        match lista {
            Some(nomes) => Vizinhos {
                parecidos: mais_proximos(nome, nomes.iter().map(String::as_str), 5),
                dir_inexistente: None,
            },
            None => Vizinhos {
                parecidos: Vec::new(),
                dir_inexistente: Some(dir.to_path_buf()),
            },
        }
    }

    /// `/x/y`: where the page is served from is unknown, so try every plausible root — the
    /// ancestors of the document (and the usual web roots under each), then the absolute path.
    /// A candidate root whose first segment (`/_next` of `/_next/static/a.js`) does not exist is
    /// dropped after ONE check, shared by every reference that starts with it.
    fn resolver_da_raiz(&mut self, rel: &str, variantes: &[String]) -> Resolucao {
        let raizes = self.raizes_candidatas();
        for v in variantes {
            let v = v.trim_start_matches('/');
            let (primeiro, resto) = v.split_once('/').unwrap_or((v, ""));
            for raiz in &raizes {
                if !resto.is_empty() && !self.existe(&raiz.join(primeiro)) {
                    continue;
                }
                if self.existe(&raiz.join(v)) {
                    return Resolucao::Existe;
                }
            }
        }
        Resolucao::Ausente {
            alvo: Alvo::Incerto,
            resolvido: self.raiz.join(rel),
        }
    }

    /// Every directory a root-relative URL may be served from.
    fn raizes_candidatas(&self) -> Vec<PathBuf> {
        let mut raizes = Vec::new();
        for a in self.ancestrais() {
            raizes.extend(RAIZES_WEB.iter().map(|w| a.join(w)));
            raizes.push(a);
        }
        raizes.push(PathBuf::from("/"));
        raizes
    }

    /// The document's directory and each parent up to the project root (only the project root
    /// besides, when the document lives outside it).
    fn ancestrais(&self) -> Vec<PathBuf> {
        if !self.dir_doc.starts_with(self.raiz) {
            return vec![self.dir_doc.clone(), self.raiz.to_path_buf()];
        }
        self.dir_doc
            .ancestors()
            .take_while(|a| a.starts_with(self.raiz))
            .map(Path::to_path_buf)
            .collect()
    }
}

/// The directory relative references resolve against: the document's, or the one a local
/// `<base href>` names; a remote base makes the class unmeasurable.
fn base_do_documento(doc: &Documento, dir_doc: &Path) -> (PathBuf, Option<String>) {
    let Some(href) = doc
        .elementos
        .iter()
        .filter(|e| e.tag == "base")
        .find_map(|e| e.valor("href").map(str::trim).filter(|v| !v.is_empty()))
    else {
        return (dir_doc.to_path_buf(), None);
    };
    if href.starts_with("//") || tem_esquema(href) {
        return (
            dir_doc.to_path_buf(),
            Some(format!(
                "documento com `<base href=\"{href}\">` externo: referências relativas não resolvidas"
            )),
        );
    }
    let caminho = href.split(['?', '#']).next().unwrap_or("");
    let diretorio = if caminho.ends_with('/') || caminho.is_empty() {
        caminho
    } else {
        caminho.rsplit_once('/').map_or("", |(d, _)| d)
    };
    (dir_doc.join(diretorio), None)
}

/// Lexical normalization (`a/../b` → `b`), so overlay keys compare as written by the hook.
fn normalizar(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            outro => out.push(outro.as_os_str()),
        }
    }
    out
}

fn achado(
    escrito: &str,
    grupo: &[&Referencia],
    alvo: Alvo,
    resolvido: &Path,
    vizinhos: Vizinhos,
) -> Finding {
    let primeira = grupo[0];
    let Vizinhos {
        parecidos: reais,
        dir_inexistente,
    } = vizinhos;
    let mut sugestao = format!(
        "recurso local inexistente: `{escrito}` ({}) — resolvido como `{}`",
        primeira.origem,
        resolvido.display()
    );
    if grupo.len() > 1 {
        sugestao.push_str(&format!(" (+{} outra(s) referência(s))", grupo.len() - 1));
    }
    match (reais.first(), dir_inexistente) {
        (Some(proximo), _) => {
            let prefixo = escrito.rsplit_once('/').map_or("", |(d, _)| d);
            let sugerido = if prefixo.is_empty() {
                proximo.clone()
            } else {
                format!("{prefixo}/{proximo}")
            };
            sugestao.push_str(&format!(". Você quis dizer `{sugerido}`?"));
        }
        (None, Some(dir)) => {
            sugestao.push_str(&format!(". O diretório `{}` não existe", dir.display()))
        }
        (None, None) => {}
    }
    Finding::candidato("recurso", escrito, primeira.linha, alvo, sugestao, reais)
}

/// What the directory of a missing file offers.
#[derive(Default)]
struct Vizinhos {
    /// Names in the directory closest to the missing one.
    parecidos: Vec<String>,
    /// Set when the directory itself does not exist.
    dir_inexistente: Option<PathBuf>,
}

/// The sorted names inside `dir`, or `None` when it cannot be listed.
fn listar(dir: &Path) -> Option<Vec<String>> {
    let mut nomes: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    nomes.sort_unstable();
    Some(nomes)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn srcset_separa_candidatos_sem_quebrar_data_url() {
        assert_eq!(
            candidatos_de_srcset("a.png 1x, b.png 2x"),
            vec!["a.png", "b.png"]
        );
        assert_eq!(
            candidatos_de_srcset("data:image/png;base64,AAAA 1x, c.png"),
            vec!["data:image/png;base64,AAAA", "c.png"]
        );
        // a vírgula separa candidatos só depois de espaço ou no fim da URL (WHATWG, "srcset
        // attribute"): `a.png,b.png` é UMA URL, `a.png, b.png` e `a.png,` não
        assert_eq!(candidatos_de_srcset("a.png,b.png"), vec!["a.png,b.png"]);
        assert_eq!(candidatos_de_srcset("a.png, b.png"), vec!["a.png", "b.png"]);
        assert_eq!(candidatos_de_srcset("a.png,"), vec!["a.png"]);
        assert!(candidatos_de_srcset("  ").is_empty());
    }

    #[test]
    fn normalizar_resolve_pontos_lexicamente() {
        assert_eq!(
            normalizar(Path::new("/p/site/pages/../css/a.css")),
            PathBuf::from("/p/site/css/a.css")
        );
        assert_eq!(normalizar(Path::new("/p/./a")), PathBuf::from("/p/a"));
    }

    #[test]
    fn esquema_so_com_letra_inicial_e_dois_pontos() {
        assert!(tem_esquema("https://x"));
        assert!(tem_esquema("mailto:a@b"));
        assert!(tem_esquema("data:image/png;base64,AA"));
        assert!(tem_esquema("javascript:void(0)"));
        assert!(
            !tem_esquema("img/a:b.png"),
            "dois-pontos depois de barra não é esquema"
        );
        assert!(!tem_esquema("./a.png"));
        assert!(!tem_esquema("1http:x"));
    }
}
